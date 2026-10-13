use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use novel_codex_story_runtime::router;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[path = "support/author_task_fixture.rs"]
mod task_fixture;
use task_fixture::AuthorTaskFixture;

const AUTHOR: &str = "scene-mapping-author-123456789012345";
const CANDIDATE: &str = "scene-mapping-candidate-123456789012";
const CAPABILITY: &str = "scene-mapping-capability-1234567890";

async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    actor: &str,
    body: Value,
) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {actor}"))
        .header("x-novel-task-capability", CAPABILITY)
        .header("content-type", "application/json");
    if method == "GET" && actor == CANDIDATE && uri.contains("/task-source?") {
        request = request
            .header("x-novel-tool-name", "read_source_content")
            .header("x-novel-root-thread", "assessSceneOutcome")
            .header("x-novel-caller-thread", "assessSceneOutcome")
            .header("x-novel-turn", "mapping-assessment-turn")
            .header("x-novel-tool-call", "mapping-read-assessment-source");
    }
    let response = app
        .clone()
        .author_fixture_request(
            request
                .body(if body.is_null() {
                    Body::empty()
                } else {
                    Body::from(body.to_string())
                })
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let value = serde_json::from_slice(&bytes).unwrap_or_else(|error| {
        panic!(
            "{method} {uri}: {status}: {error}: {}",
            String::from_utf8_lossy(&bytes)
        )
    });
    (status, value)
}

async fn read_scene_links(app: &Router, base: &str, task: &Value, context: &Value) -> Value {
    let path = "/operationContext/scenePlanDraftLinks";
    let sources: Vec<_> = context["taskSources"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|entry| entry["sectionPath"] == path)
        .collect();
    assert_eq!(sources.len(), 1, "{context}");
    let source = sources[0];
    assert_eq!(
        context.pointer(path).unwrap()["sourceId"],
        source["sourceId"]
    );
    assert_eq!(context.pointer(path).unwrap()["textDelivered"], false);
    let endpoint = format!(
        "{base}/tasks/{}/task-source?threadId={}&expectedRevision={}&sourceId={}",
        task["id"].as_str().unwrap(),
        task["threadId"].as_str().unwrap(),
        task["inputRevision"],
        source["sourceId"].as_str().unwrap()
    );
    let mut suffix = String::new();
    let mut text = String::new();
    loop {
        let (status, page) = call(
            app,
            "GET",
            &format!("{endpoint}{suffix}"),
            CANDIDATE,
            Value::Null,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{page}");
        assert_eq!(page["sourceId"], source["sourceId"]);
        assert_eq!(page["contentHash"], source["contentHash"]);
        assert!(page.to_string().len() <= 6500);
        assert_eq!(page["byteRange"]["start"], text.len());
        let part = page["payloadPart"].as_str().unwrap();
        assert!(part.len() <= 2000);
        text.push_str(part);
        assert_eq!(page["byteRange"]["end"], text.len());
        assert_eq!(page["byteRange"]["total"], source["payloadBytes"]);
        assert!(text.len() <= source["payloadBytes"].as_u64().unwrap() as usize);
        if let Some(cursor) = page["nextCursor"].as_str() {
            assert!(!part.is_empty());
            assert_eq!(page["endOfInputFromCursor"], false);
            let next = format!("&cursor={}", cursor.replace(':', "%3A"));
            assert_ne!(suffix, next);
            suffix = next;
        } else {
            assert_eq!(page["endOfInputFromCursor"], true);
            assert_eq!(
                text.len(),
                source["payloadBytes"].as_u64().unwrap() as usize
            );
            break;
        }
    }
    assert_eq!(
        format!("{:x}", Sha256::digest(text.as_bytes())),
        source["contentHash"].as_str().unwrap()
    );
    serde_json::from_str(&text).unwrap()
}

async fn workspace(app: &Router, base: &str) -> Value {
    let (status, value) = call(
        app,
        "GET",
        &format!("{base}/creative-workspace"),
        AUTHOR,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{value}");
    value
}

#[tokio::test]
async fn planned_complete_requires_real_mapping_not_all_planned_scenes_or_realized_effects() {
    let directory = tempfile::tempdir().unwrap();
    let app = router(task_fixture::state(
        directory.path().join("mapping.sqlite3"),
        AUTHOR.into(),
        CANDIDATE.into(),
    ));
    let (status, created) = call(
        &app,
        "POST",
        "/api/projects",
        AUTHOR,
        json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"映射来源"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let base = format!(
        "/api/projects/{}",
        created["project"]["id"].as_str().unwrap()
    );
    let scenes = json!([
        {"id":"scene-one","goal":"收到密函","beats":[],"expectedEffects":[{"id":"effect-one","dimension":"plot","description":"发现秘密","commitmentRefs":[]}]},
        {"id":"scene-two","goal":"决定出发","beats":[],"expectedEffects":[{"id":"effect-two","dimension":"plot","description":"作出决定","commitmentRefs":[]}]}
    ]);
    let (status, plan) = call(&app, "POST", &format!("{base}/artifacts/plan"), AUTHOR,
        json!({"expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":"plan",
        "payload":{"chapters":[{"chapterPlanId":"chapter-plan","chapterNumber":1,"goal":"密函","scenes":scenes}]}})).await;
    assert_eq!(status, StatusCode::CREATED, "{plan}");
    let (status, review) = call(&app, "POST", &format!("{base}/reviews"), AUTHOR,
        json!({"artifactKind":"plan","revisionId":plan["revisionId"],"proposals":[],"selections":{},"idempotencyKey":"review"})).await;
    assert_eq!(status, StatusCode::CREATED, "{review}");
    let (status, accepted) = call(
        &app,
        "POST",
        &format!("{base}/reviews/{}/accept", review["id"].as_str().unwrap()),
        AUTHOR,
        json!({"expectedRevision":0,"selections":{},"idempotencyKey":"accept"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{accepted}");
    let current = workspace(&app, &base).await;
    let (status, materialized) = call(&app, "POST", &format!("{base}/chapters/materialize-from-plan"), AUTHOR,
        json!({"chapterPlanId":"chapter-plan","expectedRevision":current["revision"],"idempotencyKey":"chapter"})).await;
    assert_eq!(status, StatusCode::OK, "{materialized}");
    let chapter = materialized["chapterId"].as_str().unwrap();
    let current = workspace(&app, &base).await;
    let mut payload = json!({"chapterId":chapter,"chapterNumber":1,"title":"密函","body":"林夕拾起密函。🌊","completeness":"chapterComplete",
        "sourcePlanRefs":{"chapterPlanId":"chapter-plan","planRevisionId":plan["revisionId"],"localOnly":false,"lineageStatus":"current","outlineRevisionId":null}});
    // Ordinary Author body-only saving remains valid even for a planned chapter.
    let (status, author_save) = call(&app, "POST", &format!("{base}/drafts/checkpoints"), AUTHOR,
        json!({"payload":payload,"expectedRevision":current["revision"],"expectedCandidateRevisionId":null,"idempotencyKey":"author-body"})).await;
    assert_eq!(status, StatusCode::CREATED, "{author_save}");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let (status, batch) = call(&app, "POST", &format!("{base}/modeling-batches"), AUTHOR,
        json!({"objectId":format!("draft-{chapter}"),"sourceRevisionId":author_save["revisionId"],"expectedRevision":current["revision"],"extractorVersion":"unmapped-assessment"})).await;
    assert_eq!(status, StatusCode::CREATED, "{batch}");
    let create = |operation: &str, target: Value| {
        json!({"operation":operation,"target":target,"inputRevision":current["revision"],"threadId":operation,
        "authorIntent":"只检查原文与规划，不猜场景归属。","capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":now+600})
    };
    let (status, model) = call(
        &app,
        "POST",
        &format!("{base}/tasks"),
        AUTHOR,
        create("modelKnowledge", json!({"batchId":batch["id"]})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{model}");
    let evidence = json!({"quote":"密函"});
    let (status, publication) = call(&app, "POST", &format!("{base}/modeling-batches/{}/publish",batch["id"].as_str().unwrap()), CANDIDATE,
        json!({"taskId":model["id"],"threadId":"modelKnowledge","expectedRevision":current["revision"],
            "entities":[{"id":"letter-event","kind":"event","displayName":"拾起密函","evidence":evidence},
                {"id":"linxi","kind":"character","displayName":"林夕","evidence":{"quote":"林夕"}}],
            "assertions":[{"kind":"event","subjectId":"letter-event","predicate":"pickedUp","storyTime":null,
                "value":{"participants":[{"role":"actor","entityId":"linxi"}],"preconditions":[],"effects":[],"untilExclusive":null},"evidence":evidence}]})).await;
    assert_eq!(status, StatusCode::OK, "{publication}");
    assert_eq!(
        call(
            &app,
            "POST",
            &format!("{base}/tasks/{}/complete", model["id"].as_str().unwrap()),
            CANDIDATE,
            json!({"threadId":"modelKnowledge","summary":"原文事件已建模，但没有规划场景归属。"})
        )
        .await
        .0,
        StatusCode::OK
    );
    let before_assessment = workspace(&app, &base).await;
    let (status, assessment) = call(&app, "POST", &format!("{base}/tasks"), AUTHOR, create("assessSceneOutcome",
        json!({"chapterId":chapter,"chapterNumber":1,"draftRevisionId":author_save["revisionId"],"planRevisionId":plan["revisionId"],"knowledgeBatchId":batch["id"],
            "inputSnapshotId":before_assessment["headSnapshotId"],"intentGeneration":0}))).await;
    assert_eq!(status, StatusCode::CREATED, "{assessment}");
    let (status, context) = call(&app, "GET", &format!("{base}/context?taskId={}&threadId=assessSceneOutcome&projectRevision={}&purpose=critic&budget=24&byteBudget=60000",
        assessment["id"].as_str().unwrap(),current["revision"].as_i64().unwrap()), CANDIDATE, Value::Null).await;
    assert_eq!(status, StatusCode::OK, "{context}");
    let mapping = read_scene_links(&app, &base, &assessment, &context).await;
    let links = mapping.as_array().unwrap();
    assert_eq!(links.len(), 2);
    assert_eq!(
        links
            .iter()
            .map(|link| link["sceneId"].clone())
            .collect::<Vec<_>>(),
        vec![json!("scene-one"), json!("scene-two")]
    );
    assert!(links.iter().all(
        |link| link["actualSceneGroupIds"] == json!([]) && link["actualEventRefs"] == json!([])
    ));
    let mut report = assessment["target"].clone();
    report.as_object_mut().unwrap().retain(|key, _| {
        [
            "chapterId",
            "planRevisionId",
            "draftRevisionId",
            "knowledgeBatchId",
            "inputSnapshotId",
            "intentGeneration",
        ]
        .contains(&key.as_str())
    });
    report["assessments"] = json!(links.iter().map(|link| json!({"sceneId":link["sceneId"],"verdict":"insufficientEvidence",
        "rationale":"正文没有显式绑定该规划场景，不能借用整章事件。","recommendation":"repairDraft",
        "effectAssessments":link["plannedExpectedEffects"].as_array().unwrap().iter().map(|effect| json!({"effectId":effect["id"],
            "verdict":"insufficientEvidence","rationale":"无合法场景事件。","actualEventRefs":[],"commitmentAssessments":[]})).collect::<Vec<_>>()
    })).collect::<Vec<_>>());
    let submit = |report: &Value, key: &str| {
        json!({"taskId":assessment["id"],"threadId":"assessSceneOutcome","expectedRevision":current["revision"],
        "expectedCandidateRevisionId":null,"idempotencyKey":key,"payload":report})
    };
    let assessment_uri = format!("{base}/chapters/{chapter}/scene-assessments");
    let mut missing = report.clone();
    missing["assessments"][0]
        .as_object_mut()
        .unwrap()
        .remove("verdict");
    assert!(
        call(
            &app,
            "POST",
            &assessment_uri,
            CANDIDATE,
            submit(&missing, "missing-verdict")
        )
        .await
        .0
        .is_client_error()
    );
    let mut borrowed = report.clone();
    borrowed["assessments"][0]["verdict"] = json!("realized");
    borrowed["assessments"][0]["effectAssessments"][0]["verdict"] = json!("realized");
    borrowed["assessments"][0]["effectAssessments"][0]["actualEventRefs"] =
        json!([{"modelingBatchId":batch["id"],"assertionIndex":0}]);
    assert!(
        call(
            &app,
            "POST",
            &assessment_uri,
            CANDIDATE,
            submit(&borrowed, "borrowed-event")
        )
        .await
        .0
        .is_client_error()
    );
    let (status, saved_report) = call(
        &app,
        "POST",
        &assessment_uri,
        CANDIDATE,
        submit(&report, "insufficient"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{saved_report}");
    let (status, completed_report) = call(&app, "POST", &format!("{base}/tasks/{}/complete",assessment["id"].as_str().unwrap()), CANDIDATE,
        json!({"threadId":"assessSceneOutcome","summary":"仅保存缺少场景映射的修复建议，不修改正文。"})).await;
    assert_eq!(status, StatusCode::OK, "{completed_report}");
    let (status, read_report) = call(&app, "GET", &assessment_uri, AUTHOR, Value::Null).await;
    assert_eq!(status, StatusCode::OK, "{read_report}");
    assert_eq!(
        read_report["assessment"]["assessments"],
        report["assessments"]
    );
    assert_eq!(
        workspace(&app, &base).await["chapters"],
        before_assessment["chapters"]
    );
    let current = workspace(&app, &base).await;
    let (status, task) = call(&app, "POST", &format!("{base}/tasks"), AUTHOR,
        json!({"operation":"generateChapter","target":{"type":"chapter","chapterId":chapter,"chapterNumber":1,
            "readingChapter":1,"sceneOrder":1,"scope":{"type":"wholeChapter"}},"inputRevision":current["revision"],
            "threadId":"writer","authorIntent":"只写收到密函，可以偏离未写阶段，不正式采用。",
            "capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":now+600})).await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    let request = |payload: &Value, head: &Value, key: &str| {
        json!({"taskId":task["id"],"threadId":"writer",
        "expectedRevision":current["revision"],"expectedCandidateRevisionId":head,"idempotencyKey":key,"payload":payload})
    };
    let before = workspace(&app, &base).await;
    for (key, groups) in [
        ("body-only", Value::Null),
        (
            "unmapped",
            json!([{"id":"one","sceneId":null,"body":payload["body"],"locked":false}]),
        ),
        (
            "foreign",
            json!([{"id":"one","sceneId":"other-chapter-scene","body":payload["body"],"locked":false}]),
        ),
        (
            "empty-mapping",
            json!([{"id":"one","sceneId":"scene-one","body":" ","locked":false}]),
        ),
    ] {
        let mut bad = payload.clone();
        if !groups.is_null() {
            bad["sceneGroups"] = groups;
        }
        let (status, failure) = call(
            &app,
            "POST",
            &format!("{base}/drafts/checkpoints"),
            CANDIDATE,
            request(&bad, &author_save["revisionId"], key),
        )
        .await;
        assert!(status.is_client_error(), "{failure}");
        let after = workspace(&app, &base).await;
        assert_eq!(
            json!({"chapters":after["chapters"],"snapshot":after["headSnapshotId"],"commits":after["commits"]}),
            json!({"chapters":before["chapters"],"snapshot":before["headSnapshotId"],"commits":before["commits"]})
        );
    }
    payload["completeness"] = json!("partial");
    let (status, partial) = call(
        &app,
        "POST",
        &format!("{base}/drafts/checkpoints"),
        CANDIDATE,
        request(&payload, &author_save["revisionId"], "partial"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{partial}");
    let finish = format!("{base}/tasks/{}/complete", task["id"].as_str().unwrap());
    assert!(
        call(
            &app,
            "POST",
            &finish,
            CANDIDATE,
            json!({"threadId":"writer","summary":"尚未完成"})
        )
        .await
        .0
        .is_client_error()
    );
    let groups = json!([
        {"id":"one","sceneId":"scene-one","body":"林夕拾起密函。🌊","locked":false},
        {"id":"transition","sceneId":null,"body":"远处天色渐暗。","locked":false}
    ]);
    payload["body"] = json!("林夕拾起密函。🌊\n\n---\n\n远处天色渐暗。");
    payload["sceneGroups"] = groups.clone();
    payload["completeness"] = json!("chapterComplete");
    let (status, saved) = call(
        &app,
        "POST",
        &format!("{base}/drafts/checkpoints"),
        CANDIDATE,
        request(&payload, &partial["revisionId"], "mapped"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{saved}");
    let (status, completed) = call(
        &app,
        "POST",
        &finish,
        CANDIDATE,
        json!({"threadId":"writer","summary":"正文已保存，第二场景尚未写入，不宣称效果已实现。"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    let final_state = workspace(&app, &base).await;
    assert_eq!(
        final_state["chapters"][0]["candidate"]["sceneGroups"],
        groups
    );
    assert_eq!(final_state["chapters"][0]["accepted"], Value::Null);
    assert_eq!(completed["status"], "completed");
}
