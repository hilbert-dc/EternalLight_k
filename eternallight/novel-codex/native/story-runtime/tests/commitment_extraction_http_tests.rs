use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use novel_codex_story_core::*;
use novel_codex_story_runtime::router;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const AUTHOR: &str = "extraction-author-token-1234567890123456";
const CANDIDATE: &str = "extraction-candidate-token-123456789012";
const CAPABILITY: &str = "extraction-capability-token-1234567890";

async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    actor: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {actor}"))
        .header("x-novel-task-capability", CAPABILITY)
        .header("content-type", "application/json");
    if method == "POST"
        && actor == CANDIDATE
        && uri.ends_with("/commitment-extractions")
        && let Some(payload) = body.as_ref()
    {
        let thread = payload["threadId"].as_str().unwrap();
        let key = payload["idempotencyKey"].as_str().unwrap();
        request = request
            .header("x-novel-tool-name", "submit_commitment_extraction")
            .header("x-novel-tool-call", format!("fixture-{key}"))
            .header("x-novel-turn", "extraction-turn")
            .header("x-novel-root-thread", thread)
            .header("x-novel-caller-thread", thread)
            .header(
                "x-novel-request-hash",
                format!("{:x}", Sha256::digest(payload.to_string().as_bytes())),
            );
    }
    if method == "GET" && actor == CANDIDATE && uri.contains("/task-source?") {
        let thread = uri
            .split("threadId=")
            .nth(1)
            .unwrap()
            .split('&')
            .next()
            .unwrap();
        request = request
            .header("x-novel-tool-name", "read_task_source")
            .header("x-novel-root-thread", thread)
            .header("x-novel-caller-thread", thread)
            .header("x-novel-turn", "extraction-turn")
            .header("x-novel-tool-call", "fixture-read-extraction-source");
    }
    let response = app
        .clone()
        .author_fixture_request(
            request
                .body(body.map_or_else(Body::empty, |value| Body::from(value.to_string())))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

async fn read_author_requirements(app: &Router, base: &str, task: &Value) -> Value {
    let id = task["id"].as_str().unwrap();
    let thread = task["threadId"].as_str().unwrap();
    let (status, context) = call(app, "GET", &format!("{base}/context?taskId={id}&threadId={thread}&projectRevision={}&purpose=planner&byteBudget=60000",task["inputRevision"]), CANDIDATE, None).await;
    assert_eq!(status, StatusCode::OK, "{context}");
    let entry = context["taskSources"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["sectionPath"] == "/authorRequirementsSource")
        .unwrap();
    let mut cursor = String::new();
    let mut content = String::new();
    loop {
        let (status, page) = call(app, "GET", &format!("{base}/tasks/{id}/task-source?threadId={thread}&expectedRevision={}&sourceId={}{cursor}",task["inputRevision"],entry["sourceId"].as_str().unwrap()), CANDIDATE, None).await;
        assert_eq!(status, StatusCode::OK, "{page}");
        assert_eq!(page["sourceId"], entry["sourceId"]);
        assert_eq!(page["contentHash"], entry["contentHash"]);
        assert_eq!(page["byteRange"]["start"], content.len());
        let part = page["payloadPart"].as_str().unwrap();
        assert!(part.len() <= 2000);
        assert!(page.to_string().len() <= 6500);
        let result = json!({"content":[{"type":"text","text":page.to_string()}]});
        let item = json!({"type":"function_call_output","call_id":"x".repeat(160),"output":result.to_string()});
        let tokens = eternallight_engine_core::dispatch("tokens.count", json!(item.to_string()))
            .unwrap()
            .as_u64()
            .unwrap();
        assert!(tokens <= 9000);
        content.push_str(part);
        assert_eq!(page["byteRange"]["end"], content.len());
        let Some(next) = page["nextCursor"].as_str() else {
            assert_eq!(page["byteRange"]["total"], content.len());
            assert_eq!(page["endOfInputFromCursor"], true);
            break;
        };
        let next = format!("&cursor={next}");
        assert_ne!(cursor, next);
        cursor = next;
    }
    assert_eq!(entry["payloadBytes"], content.len());
    assert_eq!(
        entry["contentHash"],
        format!("{:x}", Sha256::digest(content.as_bytes()))
    );
    serde_json::from_str(&content).unwrap()
}

#[tokio::test]
async fn conversation_extraction_uses_bound_candidate_output_and_an_author_only_report() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("承诺提案.sqlite3");
    let project = ProjectId::parse("extraction-http").unwrap();
    let branch = BranchId::parse("main").unwrap();
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    repo.create_project(
        ProjectRecord {
            project_id: project.clone(),
            title: "港城".into(),
            logline: String::new(),
            created_at: 1,
            updated_at: 1,
        },
        branch.clone(),
        SnapshotId::parse("initial").unwrap(),
    )
    .unwrap();
    for (chapter, body) in [(1, "白塔的灯将再亮。"), (2, "未来的秘密不可读取。")]
    {
        repo.save_candidate(SaveCandidateRequest {
            project_id: project.clone(),
            branch_id: branch.clone(),
            object_id: ObjectId::parse(format!("draft-chapter-{chapter}")).unwrap(),
            object_kind: "draft".into(),
            revision_id: RevisionId::parse(format!("source-{chapter}")).unwrap(),
            commit_id: CommitId::parse(format!("save-{chapter}")).unwrap(),
            idempotency_key: IdempotencyKey::parse(format!("save-{chapter}")).unwrap(),
            request_digest: body.into(),
            content_digest: body.into(),
            content_json: json!({"chapterNumber":chapter,"title":body,"body":body}).to_string(),
            expected_project_revision: 0,
            expected_snapshot_id: repo
                .branch_head(&project, &branch)
                .unwrap()
                .unwrap()
                .snapshot_id,
            next_snapshot_id: SnapshotId::parse(format!("snapshot-{chapter}")).unwrap(),
            expected_candidate_revision_id: None,
            chapter_projection: Some(ChapterProjection {
                chapter_id: format!("chapter-{chapter}"),
                number: chapter,
                title: body.into(),
            }),
            task_output: None,
            linked_impact_ids: vec![],
        })
        .unwrap();
    }
    drop(repo);
    let app = router(task_fixture::state(
        path.clone(),
        AUTHOR.into(),
        CANDIDATE.into(),
    ));
    assert_eq!(
        call(
            &app,
            "GET",
            "/api/projects/extraction-http/chapters/chapter-1/commitment-extractions",
            AUTHOR,
            /*body*/ None
        )
        .await,
        (StatusCode::OK, Value::Null)
    );
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        + 600;
    let create = json!({"operation":"advisory","inputRevision":0,"threadId":"discussion","target":{"type":"project"},"authorIntent":"提取并保存第一章的伏笔候选",
        "capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":expires});
    let (status, discussion) = call(
        &app,
        "POST",
        "/api/projects/extraction-http/tasks",
        AUTHOR,
        Some(create.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{discussion}");
    let root = discussion["id"].as_str().unwrap();
    let actions = format!("/api/projects/extraction-http/tasks/{root}/actions");
    let action_input = json!({"taskId":root,"threadId":"discussion","expectedRevision":0,"expectedIntentRevision":0,"idempotencyKey":"extract-request","operation":"extractCommitments",
        "target":{"chapterId":"chapter-1","chapterNumber":1,"sourceRevisionId":"source-1"},"instruction":"提取有原文证据的开放承诺候选"});
    let (status, action) = call(
        &app,
        "POST",
        &actions,
        CANDIDATE,
        Some(action_input.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{action}");
    assert_eq!(
        call(&app, "POST", &actions, CANDIDATE, Some(action_input))
            .await
            .1,
        action
    );
    let action_id = action["id"].as_str().unwrap();
    assert_eq!(
        call(
            &app,
            "POST",
            &format!("/api/projects/extraction-http/tasks/{root}/complete"),
            AUTHOR,
            Some(json!({"threadId":"discussion","summary":"已经登记提取任务"}))
        )
        .await
        .0,
        StatusCode::OK
    );
    settle_fixture::settle(&path, "extraction-http", root);
    let (status, approved) = call(
        &app,
        "POST",
        &format!("{actions}/{action_id}/approve"),
        AUTHOR,
        Some(json!({"expectedIntentRevision":0})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{approved}");
    let mut create = create;
    create["operation"] = json!("extractCommitments");
    create["threadId"] = json!("extraction");
    create["target"] = action["target"].clone();
    create["target"]["requestedByTaskId"] = json!(root);
    create["target"]["requestedActionId"] = json!(action_id);
    let (status, task) = call(
        &app,
        "POST",
        "/api/projects/extraction-http/tasks",
        AUTHOR,
        Some(create),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    let id = task["id"].as_str().unwrap();
    let context_uri = format!(
        "/api/projects/extraction-http/context?taskId={id}&threadId=extraction&projectRevision=0&purpose=critic&budget=24&byteBudget=60000"
    );
    let (status, pack) = call(&app, "GET", &context_uri, CANDIDATE, /*body*/ None).await;
    assert_eq!(status, StatusCode::OK, "{pack}");
    let text_sources: Vec<_> = pack["taskSources"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|source| source["sectionPath"] == "/relevantText")
        .collect();
    assert_eq!(text_sources.len(), 1);
    let source = text_sources[0];
    assert_eq!(source["chapterNumbers"], json!([1]));
    assert_eq!(pack["relevantText"]["sourceId"], source["sourceId"]);
    assert_eq!(pack["relevantText"]["textDelivered"], false);
    let source_uri = format!(
        "/api/projects/extraction-http/tasks/{id}/task-source?threadId=extraction&expectedRevision=0&sourceId={}",
        source["sourceId"].as_str().unwrap()
    );
    let mut cursor = Value::Null;
    let mut text = String::new();
    loop {
        let uri = cursor
            .as_str()
            .map(|cursor| format!("{source_uri}&cursor={cursor}"))
            .unwrap_or_else(|| source_uri.clone());
        let (status, page) = call(&app, "GET", &uri, CANDIDATE, None).await;
        assert_eq!(status, StatusCode::OK, "{page}");
        assert_eq!(page["contentHash"], source["contentHash"]);
        assert_eq!(page["byteRange"]["start"], text.len());
        text.push_str(page["payloadPart"].as_str().unwrap());
        assert_eq!(page["byteRange"]["end"], text.len());
        assert!(text.len() <= source["payloadBytes"].as_u64().unwrap() as usize);
        cursor = page["nextCursor"].clone();
        if cursor.is_null() {
            assert_eq!(page["endOfInputFromCursor"], true);
            assert_eq!(page["byteRange"]["total"], text.len());
            break;
        }
    }
    assert_eq!(
        format!("{:x}", Sha256::digest(text.as_bytes())),
        source["contentHash"].as_str().unwrap()
    );
    let visible_text: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(visible_text.as_array().unwrap().len(), 1);
    assert_eq!(visible_text[0]["sourceRevisionId"], "source-1");
    assert!(!text.contains("未来的秘密不可读取"));
    assert!(!pack.to_string().contains("未来的秘密不可读取"));
    let submit = "/api/projects/extraction-http/chapters/chapter-1/commitment-extractions";
    let input = json!({"taskId":id,"threadId":"extraction","expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":"save-extraction",
        "payload":{"chapterId":"chapter-1","sourceRevisionId":"source-1","examinedRanges":[{"start":0,"end":8}],"candidates":[{
            "id":"lamp","kind":"foreshadowing","summary":"灯会再亮","relatedEntityIds":[],"plotThreadId":null,"characterArcId":null,
            "setupEvidenceRefs":[{"objectId":"draft-chapter-1","revisionId":"source-1","quote":"白塔的灯","start":0,"end":4}],
            "fulfillmentCriteria":"后续出现再次亮灯的证据","targetWindow":{"kind":"open"},"status":"open","fulfillmentEvidenceRefs":[],"decisionReason":null}]}});
    assert_eq!(
        call(&app, "POST", submit, AUTHOR, Some(input.clone()))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    for (field, expected_message) in [
        (
            "plotThreadId",
            "candidates[0].plotThreadId: use null or a non-empty ID",
        ),
        (
            "characterArcId",
            "candidates[0].characterArcId: use null or a non-empty ID",
        ),
        (
            "sceneId",
            "candidates[0].setupEvidenceRefs[0]: unexpected field; only objectId, revisionId, quote, start, end are allowed",
        ),
        ("start", "invalid narrative commitment or source evidence"),
    ] {
        let mut invalid = input.clone();
        invalid["idempotencyKey"] = json!(format!("invalid-{field}"));
        match field {
            "plotThreadId" | "characterArcId" => {
                invalid["payload"]["candidates"][0][field] = json!("")
            }
            "sceneId" => {
                invalid["payload"]["candidates"][0]["setupEvidenceRefs"][0][field] = Value::Null
            }
            "start" => {
                invalid["payload"]["candidates"][0]["setupEvidenceRefs"][0][field] = json!(1)
            }
            _ => unreachable!(),
        }
        let (status, rejected) = call(&app, "POST", submit, CANDIDATE, Some(invalid)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{rejected}");
        assert_eq!(
            rejected["error"]["message"],
            format!("InvalidContent({expected_message:?})")
        );
        assert_eq!(
            call(&app, "GET", submit, AUTHOR, /*body*/ None).await.1,
            Value::Null
        );
        assert_eq!(
            SqliteStoryRepository::open(&path)
                .unwrap()
                .task(&project, id)
                .unwrap()
                .unwrap()
                .output_refs_json,
            "[]"
        );
    }
    let (status, snapshot) = call(
        &app,
        "POST",
        "/api/projects/extraction-http/tasks/recovery-snapshot",
        AUTHOR,
        Some(json!({"taskIds":[id]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{snapshot}");
    assert_eq!(snapshot["complete"], true);
    let rejected: Vec<Value> = snapshot["operations"].as_array().unwrap().iter().map(|operation|
        json!({"tool_call_id":operation["tool_call_id"],"state":operation["state"],"receipt_ref":operation["receipt_ref"]})
    ).collect();
    assert_eq!(
        rejected,
        json!([
            {"tool_call_id":"fixture-invalid-plotThreadId","state":"rejected","receipt_ref":null},
            {"tool_call_id":"fixture-invalid-characterArcId","state":"rejected","receipt_ref":null},
            {"tool_call_id":"fixture-invalid-sceneId","state":"rejected","receipt_ref":null},
            {"tool_call_id":"fixture-invalid-start","state":"rejected","receipt_ref":null}
        ])
        .as_array()
        .unwrap()
        .clone()
    );
    let (status, saved) = call(&app, "POST", submit, CANDIDATE, Some(input.clone())).await;
    assert_eq!(status, StatusCode::CREATED, "{saved}");
    assert_eq!(saved["candidateCount"], 1);
    assert_eq!(
        call(&app, "POST", submit, CANDIDATE, Some(input.clone()))
            .await
            .1,
        saved
    );
    // Every checkpoint is a complete replacement, not an append operation.
    let mut two = input.clone();
    two["idempotencyKey"] = json!("save-two-proposals");
    two["expectedCandidateRevisionId"] = saved["revisionId"].clone();
    let mut extra = two["payload"]["candidates"][0].clone();
    extra["id"] = json!("lamp-second-proposal");
    two["payload"]["candidates"]
        .as_array_mut()
        .unwrap()
        .push(extra);
    let (status, saved_two) = call(&app, "POST", submit, CANDIDATE, Some(two.clone())).await;
    assert_eq!(status, StatusCode::CREATED, "{saved_two}");
    assert_eq!(saved_two["candidateCount"], 2);
    let (_, report_two) = call(&app, "GET", submit, AUTHOR, /*body*/ None).await;
    assert_eq!(report_two["revisionId"], saved_two["revisionId"]);
    assert_eq!(report_two["payload"], two["payload"]);
    assert_eq!(report_two["reportCurrent"], true);
    let original_uri = format!(
        "{submit}?reportRevisionId={}",
        saved["revisionId"].as_str().unwrap()
    );
    let (status, original_report) = call(&app, "GET", &original_uri, AUTHOR, None).await;
    assert_eq!(status, StatusCode::OK, "{original_report}");
    assert_eq!(original_report["revisionId"], saved["revisionId"]);
    assert_eq!(original_report["payload"], input["payload"]);
    assert_eq!(original_report["reportCurrent"], false);
    assert_eq!(original_report["sourceCurrent"], true);
    assert_eq!(original_report["snapshotId"], report_two["snapshotId"]);
    assert_eq!(
        original_report["projectRevision"],
        report_two["projectRevision"]
    );
    assert_eq!(
        call(
            &app,
            "GET",
            &format!(
                "{submit}?reportRevisionId={}",
                saved_two["revisionId"].as_str().unwrap()
            ),
            AUTHOR,
            None,
        )
        .await,
        (StatusCode::OK, report_two.clone())
    );
    for uri in [
        format!("{submit}?reportRevisionId=unknown-report"),
        format!("{submit}?reportRevisionId=source-1"),
        original_uri.replace("chapter-1", "chapter-2"),
    ] {
        assert_eq!(
            call(&app, "GET", &uri, AUTHOR, None).await,
            (StatusCode::OK, Value::Null)
        );
    }
    let (status, foreign) = call(
        &app,
        "GET",
        &original_uri.replace("extraction-http", "foreign-project"),
        AUTHOR,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{foreign}");
    assert_eq!(foreign["error"]["code"], "novel_scope_required");
    for revision in ["", "%20", "bad%0Aid"] {
        let (status, rejected) = call(
            &app,
            "GET",
            &format!("{submit}?reportRevisionId={revision}"),
            AUTHOR,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{rejected}");
        assert_eq!(rejected["error"]["code"], "invalid_report_revision_id");
    }
    assert_eq!(
        call(&app, "GET", &original_uri, CANDIDATE, None).await.0,
        StatusCode::UNAUTHORIZED
    );
    let stale_choice = json!({"reportRevisionId":original_report["revisionId"],"proposalId":"lamp",
        "baseSnapshotId":original_report["snapshotId"],"expectedRevision":original_report["projectRevision"],"idempotencyKey":"choose-old-report"});
    assert_ne!(
        call(
            &app,
            "POST",
            &format!("{submit}/establish"),
            AUTHOR,
            Some(stale_choice)
        )
        .await
        .0,
        StatusCode::CREATED
    );
    assert_eq!(call(&app, "GET", submit, AUTHOR, None).await.1, report_two);
    // Replaying an older successful write returns that exact saved revision's count.
    assert_eq!(
        call(&app, "POST", submit, CANDIDATE, Some(input.clone())).await,
        (StatusCode::CREATED, saved)
    );
    assert_eq!(
        call(&app, "GET", submit, AUTHOR, /*body*/ None).await.1,
        report_two
    );
    let mut corrected = input;
    corrected["idempotencyKey"] = json!("save-corrected-report");
    corrected["expectedCandidateRevisionId"] = saved_two["revisionId"].clone();
    let (status, saved) = call(&app, "POST", submit, CANDIDATE, Some(corrected.clone())).await;
    assert_eq!(status, StatusCode::CREATED, "{saved}");
    assert_eq!(saved["candidateCount"], 1);
    let (_, report_corrected) = call(&app, "GET", submit, AUTHOR, /*body*/ None).await;
    assert_eq!(report_corrected["revisionId"], saved["revisionId"]);
    assert_eq!(report_corrected["payload"], corrected["payload"]);
    assert_eq!(
        call(&app, "POST", submit, CANDIDATE, Some(two)).await,
        (StatusCode::CREATED, saved_two)
    );
    assert_eq!(
        call(&app, "GET", submit, AUTHOR, /*body*/ None).await.1,
        report_corrected
    );
    assert_eq!(
        call(&app, "GET", submit, CANDIDATE, /*body*/ None).await.0,
        StatusCode::UNAUTHORIZED
    );
    let (status, report) = call(&app, "GET", submit, AUTHOR, /*body*/ None).await;
    assert_eq!(status, StatusCode::OK, "{report}");
    assert_eq!(report["authority"], "candidateProposalBatch");
    assert_eq!(report["reportCurrent"], true);
    assert_eq!(report["coverage"]["semanticCompleteness"], "unknown");
    let (status, completed) = call(
        &app,
        "POST",
        &format!("/api/projects/extraction-http/tasks/{id}/complete"),
        CANDIDATE,
        Some(
            json!({"threadId":"extraction","expectedRevision":0,"summary":"已保存一项开放承诺提案"}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    assert_eq!(completed["status"], "completed");
    let work_id = format!("work-{root}");
    let work = SqliteStoryRepository::open(&path)
        .unwrap()
        .work_task(&project, &work_id)
        .unwrap();
    assert_eq!(work.status, "running");
    assert_eq!(work.steps.last().unwrap().status, "completed");
    assert_eq!(
        work.steps.last().unwrap().output_refs[0]["kind"],
        "commitmentExtraction"
    );
    assert_eq!(
        work.steps
            .last()
            .unwrap()
            .output_refs
            .as_array()
            .unwrap()
            .last()
            .unwrap()["revisionId"],
        saved["revisionId"]
    );
    assert_ne!(call(&app,"POST",submit,CANDIDATE,Some(json!({"taskId":id,"threadId":"extraction","expectedRevision":0,"idempotencyKey":"late","payload":{}}))).await.0,StatusCode::CREATED);
    // A completed extraction is a saved candidate, not the final Work review.
    // Close that real obligation through the current Native decision transport.
    settle_fixture::settle(&path, "extraction-http", id);
    let settled_work = SqliteStoryRepository::open(&path)
        .unwrap()
        .work_task(&project, &work_id)
        .unwrap();
    let base = "/api/projects/extraction-http";
    let (_, workspace) = call(
        &app,
        "GET",
        &format!("{base}/creative-workspace"),
        AUTHOR,
        None,
    )
    .await;
    let (status, next) = call(&app, "POST", &format!("{base}/work-tasks/{work_id}/decision"), AUTHOR,
        Some(json!({"expectedWorkVersion":settled_work.version,"expectedRevision":workspace["revision"],"expectedSourceDigest":workspace["sourceDigest"]}))).await;
    assert_eq!(status, StatusCode::OK, "{next}");
    assert_eq!(next["stopReason"], Value::Null);
    assert_eq!(next["workTaskId"], work_id);
    assert_eq!(next["threadId"], "discussion");
    assert_eq!(next["originTaskId"], root);
    assert_eq!(next["step"]["operation"], "advisory");
    let mut decision_target = next["step"]["target"].clone();
    decision_target["resumeStepId"] = next["step"]["id"].clone();
    let (status, decision) = call(&app, "POST", &format!("{base}/tasks"), AUTHOR,
        Some(json!({"operation":"advisory","threadId":next["threadId"],"inputRevision":next["step"]["inputRevision"],
            "target":decision_target,"authorIntent":discussion["inputManifest"]["authorIntent"],
            "capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":expires}))).await;
    assert_eq!(status, StatusCode::CREATED, "{decision}");
    let decision_id = decision["id"].as_str().unwrap();
    assert_eq!(decision["threadId"], "discussion");
    let requirements = read_author_requirements(&app, base, &decision).await;
    assert!(
        requirements
            .to_string()
            .contains("提取并保存第一章的伏笔候选")
    );
    let (status, state) = call(
        &app,
        "GET",
        &format!(
            "{base}/tasks/{decision_id}/work-decision?threadId=discussion&expectedRevision={}",
            decision["inputRevision"]
        ),
        CANDIDATE,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{state}");
    assert_eq!(state["canFinish"], true, "{state}");
    assert_eq!(state["deliveryReviewRequired"], true);
    let review = json!({"expectedWorkVersion":state["version"],"expectedSourceDigest":state["sourceDigest"],
        "goalStatus":"fulfilled","goalAssessment":"已按作者要求提取并保存第一章的开放承诺候选，未采用为正式承诺",
        "checkedScope":"第一章原文与实际保存的候选报告；未判断全书质量或采用候选",
        "unresolved":[],"issueResolutions":[],"authorDecisions":[],
        "evidence":[{"taskId":id,"revisionId":saved["revisionId"]}]});
    let (status, final_task) = call(&app, "POST", &format!("{base}/tasks/{decision_id}/complete"), CANDIDATE,
        Some(json!({"threadId":"discussion","expectedRevision":decision["inputRevision"],"summary":"第一章承诺候选已保存并核对","deliveryReview":review}))).await;
    assert_eq!(status, StatusCode::OK, "{final_task}");
    assert_eq!(final_task["status"], "completed");
    settle_fixture::settle(&path, "extraction-http", decision_id);
    let finished = SqliteStoryRepository::open(&path)
        .unwrap()
        .work_task(&project, &work_id)
        .unwrap();
    assert_eq!(finished.status, "completed");
    assert!(finished.steps.iter().all(|step| step.status == "completed"));
    let (status, finished_http) = call(
        &app,
        "GET",
        &format!("{base}/work-tasks/{work_id}"),
        AUTHOR,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{finished_http}");
    assert_eq!(finished_http["deliveryReport"]["deliveryReview"], review);
    assert_eq!(
        finished_http["deliveryReport"]["unsettledAttemptIds"],
        json!([])
    );

    let (_, report) = call(&app, "GET", submit, AUTHOR, /*body*/ None).await;
    let establish_uri = format!("{submit}/establish");
    let choice = json!({"reportRevisionId":saved["revisionId"],"proposalId":"lamp","baseSnapshotId":report["snapshotId"],
        "expectedRevision":report["projectRevision"],"idempotencyKey":"author-choice"});
    assert_eq!(
        call(
            &app,
            "POST",
            &establish_uri,
            CANDIDATE,
            Some(choice.clone())
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    let (status, established) =
        call(&app, "POST", &establish_uri, AUTHOR, Some(choice.clone())).await;
    assert_eq!(status, StatusCode::CREATED, "{established}");
    assert_eq!(
        call(&app, "POST", &establish_uri, AUTHOR, Some(choice.clone())).await,
        (status, established.clone())
    );
    assert_eq!(established["confirmationState"], "candidate");
    let (_, current_report) = call(
        &app,
        "GET",
        "/api/projects/extraction-http/chapters/chapter-1/commitment-extractions",
        AUTHOR,
        /*body*/ None,
    )
    .await;
    assert_eq!(
        current_report["established"],
        json!([{ "proposalId": "lamp", "commitmentId": established["commitmentId"] }])
    );
    let commitment_uri = format!(
        "/api/projects/extraction-http/commitments/{}",
        established["commitmentId"].as_str().unwrap()
    );
    let (status, independent) = call(&app, "GET", &commitment_uri, AUTHOR, /*body*/ None).await;
    assert_eq!(status, StatusCode::OK, "{independent}");
    assert_eq!(independent["confirmationCommitId"], Value::Null);
    assert_eq!(independent["authorControls"]["projectRevision"], 0);
    assert_eq!(
        independent["extractionOrigin"],
        json!({"chapterId":"chapter-1","reportRevisionId":saved["revisionId"],"proposalId":"lamp"})
    );
    let (status, review) = call(
        &app,
        "POST",
        &format!("{commitment_uri}/reviews"),
        AUTHOR,
        Some(json!({"revisionId":established["revisionId"],"idempotencyKey":"review-established"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{review}");
    let (status, accepted) = call(&app,"POST",&format!("/api/projects/extraction-http/reviews/{}/accept",review["id"].as_str().unwrap()),AUTHOR,
        Some(json!({"expectedRevision":report["projectRevision"],"idempotencyKey":"accept-established","selections":{}}))).await;
    assert_eq!(status, StatusCode::OK, "{accepted}");
    let (_, confirmed) = call(&app, "GET", &commitment_uri, AUTHOR, /*body*/ None).await;
    assert_eq!(confirmed["confirmationCommitId"], accepted["commitId"]);
    assert_eq!(
        confirmed["authorControls"],
        json!({"projectRevision":1,"undoAvailable":true})
    );
    assert_eq!(
        call(&app, "GET", &commitment_uri, AUTHOR, /*body*/ None)
            .await
            .1["confirmationState"],
        "confirmed"
    );
    assert_eq!(
        call(&app, "POST", &establish_uri, AUTHOR, Some(choice))
            .await
            .1,
        established
    );
    let (status, undone) = call(
        &app,
        "POST",
        &format!(
            "/api/projects/extraction-http/commits/{}/undo",
            accepted["commitId"].as_str().unwrap()
        ),
        AUTHOR,
        Some(json!({"expectedRevision":1,"idempotencyKey":"undo-established"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{undone}");
    let (_, withdrawn) = call(&app, "GET", &commitment_uri, AUTHOR, /*body*/ None).await;
    assert_eq!(withdrawn["confirmationCommitId"], Value::Null);
    assert_eq!(withdrawn["authorControls"]["projectRevision"], 2);
    assert_eq!(withdrawn["sourceCurrent"], true);
    assert_eq!(withdrawn["authorControls"]["undoAvailable"], false);
    assert_eq!(
        call(&app, "GET", &commitment_uri, AUTHOR, /*body*/ None)
            .await
            .1["confirmationState"],
        "candidate"
    );
    let mut adjusted = withdrawn["payload"].clone();
    adjusted["summary"] = json!("作者重新决定这条伏笔");
    adjusted["status"] = json!("abandoned");
    adjusted["decisionReason"] = json!("转为氛围描写，不再承诺后文解释");
    adjusted["targetWindow"] =
        json!({"kind":"chapterRange","fromChapterId":"chapter-1","throughChapterId":"chapter-2"});
    let edit_request = json!({"payload":adjusted,"extractionOrigin":withdrawn["extractionOrigin"],"expectedRevision":2,
        "baseSnapshotId":withdrawn["snapshotId"],"expectedCandidateRevisionId":withdrawn["revisionId"],"idempotencyKey":"edit-established"});
    assert_eq!(
        call(
            &app,
            "POST",
            &commitment_uri,
            CANDIDATE,
            Some(edit_request.clone())
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    let (status, edited) = call(
        &app,
        "POST",
        &commitment_uri,
        AUTHOR,
        Some(edit_request.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{edited}");
    assert_eq!(
        call(&app, "POST", &commitment_uri, AUTHOR, Some(edit_request)).await,
        (status, edited.clone())
    );
    let (_, edited_detail) = call(&app, "GET", &commitment_uri, AUTHOR, /*body*/ None).await;
    assert_eq!(edited_detail["payload"], adjusted);
    assert_eq!(
        edited_detail["extractionOrigin"],
        withdrawn["extractionOrigin"]
    );
    assert_eq!(edited_detail["confirmationState"], "candidate");
    let (status, second_review) = call(
        &app,
        "POST",
        &format!("{commitment_uri}/reviews"),
        AUTHOR,
        Some(json!({"revisionId":edited["revisionId"],"idempotencyKey":"review-again"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, second_accept) = call(
        &app,
        "POST",
        &format!(
            "/api/projects/extraction-http/reviews/{}/accept",
            second_review["id"].as_str().unwrap()
        ),
        AUTHOR,
        Some(json!({"expectedRevision":2,"idempotencyKey":"accept-again","selections":{}})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{second_accept}");
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    repo.save_candidate(SaveCandidateRequest {
        project_id: project.clone(),
        branch_id: branch.clone(),
        object_id: ObjectId::parse("draft-chapter-1").unwrap(),
        object_kind: "draft".into(),
        revision_id: RevisionId::parse("source-changed").unwrap(),
        commit_id: CommitId::parse("save-changed").unwrap(),
        idempotency_key: IdempotencyKey::parse("save-changed").unwrap(),
        request_digest: "changed-request".into(),
        content_digest: "changed-content".into(),
        content_json:
            json!({"chapterNumber":1,"title":"白塔的灯将再亮。","body":"白塔的灯不再亮。"})
                .to_string(),
        expected_project_revision: 3,
        expected_snapshot_id: repo
            .branch_head(&project, &branch)
            .unwrap()
            .unwrap()
            .snapshot_id,
        next_snapshot_id: SnapshotId::parse("snapshot-changed").unwrap(),
        expected_candidate_revision_id: Some(RevisionId::parse("source-1").unwrap()),
        chapter_projection: None,
        task_output: None,
        linked_impact_ids: vec![],
    })
    .unwrap();
    drop(repo);
    let (_, stale) = call(&app, "GET", &commitment_uri, AUTHOR, /*body*/ None).await;
    assert_eq!(stale["sourceCurrent"], false);
    assert_eq!(stale["confirmationCommitId"], second_accept["commitId"]);
    assert_eq!(stale["authorControls"]["undoAvailable"], false);
    let (status, _) = call(
        &app,
        "POST",
        &format!(
            "/api/projects/extraction-http/commits/{}/undo",
            second_accept["commitId"].as_str().unwrap()
        ),
        AUTHOR,
        Some(json!({"expectedRevision":3,"idempotencyKey":"undo-stale"})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(
        call(&app, "GET", &commitment_uri, AUTHOR, /*body*/ None)
            .await
            .1,
        stale
    );
    let repo = SqliteStoryRepository::open(&path).unwrap();
    assert_eq!(
        repo.object_heads(&project)
            .unwrap()
            .into_iter()
            .find(|head| head.object_id.as_str() == "draft-chapter-1")
            .unwrap()
            .candidate_revision_id
            .unwrap()
            .as_str(),
        "source-changed"
    );
}
#[path = "support/author_task_fixture.rs"]
mod task_fixture;
use task_fixture::AuthorTaskFixture;
#[path = "support/settle_task_fixture.rs"]
mod settle_fixture;
