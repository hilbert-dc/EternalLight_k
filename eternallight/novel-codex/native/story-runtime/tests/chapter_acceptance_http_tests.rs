//! Real HTTP and SQLite; model decisions are scripted to test enforcement boundaries.
use serde_json::{Value, json};
use sha2::Digest;
#[path = "support/continuous_recovery.rs"]
mod support;
use support::{BODY, CANDIDATE, Runtime};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sourced_length_interpretation_can_be_corrected_without_certifying_existing_prose() {
    let r = Runtime::new().await;
    let (root, uri) = r.start().await;
    let coordinator = r.next_task(&root, &uri, "coordinator").await;
    let read = r.decision_status(&root, &coordinator).await;
    let choice = read["readyActions"].as_array().unwrap().iter()
        .find(|a| a["operation"] == "writeNextChapter").unwrap();
    let acceptance = json!({"requirements":[{"id":"whole-chapter","text":"呈现林夕调查港城灯光"}],
        "length":{"requirement":"模型自行估计至少20字符","minCharacters":20,"maxCharacters":null}});
    r.candidate(&format!("{root}/tasks/{}/work-decision/actions", coordinator["id"].as_str().unwrap()),
        json!({"taskId":coordinator["id"],"threadId":coordinator["threadId"],"expectedRevision":coordinator["inputRevision"],"expectedWorkVersion":read["version"],"expectedSourceDigest":read["sourceDigest"],"actionKey":choice["actionKey"],"chapterAcceptance":acceptance,"instruction":"起草章节","idempotencyKey":"initial-interpretation"})).await;
    r.complete_and_advance(&root, &uri, &coordinator).await;
    let writer = r.next_task(&root, &uri, "writer").await;
    r.save_writer(&root, &writer, BODY, "initial-prose").await;
    r.complete_and_advance(&root, &uri, &writer).await;
    let decision = r.next_task(&root, &uri, "coordinator").await;
    let read = r.decision_status(&root, &decision).await;
    assert_eq!(read["canFinish"], false);
    let repair = read["readyActions"].as_array().unwrap().iter()
        .find(|a| a["operation"] == "generateChapter").unwrap();
    let author = r.read_author_requirements(&root, &decision).await;
    let mut corrected = acceptance.clone();
    corrected["length"] = json!({"requirement":"作者未给出这一下限，撤销模型自行添加的20字符要求","minCharacters":null,"maxCharacters":null});
    let endpoint = format!("{root}/tasks/{}/work-decision/actions", decision["id"].as_str().unwrap());
    let mut request = json!({"taskId":decision["id"],"threadId":decision["threadId"],"expectedRevision":decision["inputRevision"],"expectedWorkVersion":read["version"],"expectedSourceDigest":read["sourceDigest"],"actionKey":repair["actionKey"],"chapterAcceptance":corrected,"instruction":"核对原作者要求后修正解释","idempotencyKey":"correct-interpretation"});
    let (code, _) = r.call("POST", &endpoint, CANDIDATE, Some(request.clone())).await;
    assert!(!code.is_success(), "unsourced change must remain rejected");
    corrected["interpretationCorrection"] = json!({"reason":"先前模型自行添加字数下限，原作者要求未规定这个数值","impact":"移除推断下限，仍检查完整章节和调查目标","sourceChecks":[{"sourceId":author["sourceId"],"contentHash":author["contentHash"],"quote":"林夕调查港城灯光"}]});
    request["chapterAcceptance"] = corrected.clone();
    for field in ["quote", "contentHash"] {
        let mut forged = request.clone();
        forged["chapterAcceptance"]["interpretationCorrection"]["sourceChecks"][0][field] = json!("fabricated");
        let (code, _) = r.call("POST", &endpoint, CANDIDATE, Some(forged)).await;
        assert!(!code.is_success(), "forged {field} must be rejected");
    }
    r.candidate(&endpoint, request).await;
    assert_eq!(r.get(&uri).await["completedChapters"], 0);
    r.complete_and_advance(&root, &uri, &decision).await;
    let replacement = r.next_task(&root, &uri, "writer").await;
    assert_eq!(replacement["target"]["chapterAcceptance"], corrected);
    assert_eq!(replacement["inputManifest"]["authorIntent"], support::author_intent(&r.get(&uri).await));
    r.save_writer(&root, &replacement, BODY, "same-prose-new-interpretation").await;
    r.complete_and_advance(&root, &uri, &replacement).await;
    // Numeric satisfaction is not author-goal satisfaction: the independent
    // reader can still reject omissions from the original requirements.
    r.assess_chapter_report(&root, &uri, "supported", "当前清单满足", &["原要求调查动机仍未呈现"]).await;
    let decision = r.next_task(&root, &uri, "coordinator").await;
    let read = r.decision_status(&root, &decision).await;
    let assessment = &read["requirements"].as_array().unwrap().iter()
        .find(|q| q["kind"] == "revisionOutcome").unwrap()["assessment"];
    assert_eq!(assessment["length"]["satisfied"], true);
    assert_eq!(assessment["verdict"], "notMet");
    assert_eq!(read["canFinish"], false);
    assert_eq!(r.get(&uri).await["completedChapters"], 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn maximal_frozen_standards_and_checks_keep_decision_status_bounded() {
    let r = Runtime::new().await;
    let (root, uri) = r.start().await;
    let coordinator = r.next_task(&root, &uri, "coordinator").await;
    let read = r.decision_status(&root, &coordinator).await;
    let choice = read["readyActions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["operation"] == "writeNextChapter")
        .unwrap();
    let requirements: Vec<Value> = (0..12)
        .map(|n| json!({"id":format!("criterion-{n}"),"text":format!("标准{n}：{}", "核".repeat(590))}))
        .collect();
    let acceptance = json!({"requirements":requirements,"length":{"requirement":"正文至少一个字符，按实际保存内容计量","minCharacters":1,"maxCharacters":null}});
    assert!(acceptance.to_string().len() > 20_000);
    r.candidate(&format!("{root}/tasks/{}/work-decision/actions",coordinator["id"].as_str().unwrap()),json!({"taskId":coordinator["id"],"threadId":coordinator["threadId"],"expectedRevision":coordinator["inputRevision"],"expectedWorkVersion":read["version"],"expectedSourceDigest":read["sourceDigest"],"actionKey":choice["actionKey"],"chapterAcceptance":acceptance,"instruction":"逐项执行冻结标准","idempotencyKey":"maximal-contract"})).await;
    r.complete_and_advance(&root, &uri, &coordinator).await;
    let writer = r.next_task(&root, &uri, "writer").await;
    r.save_writer(&root, &writer, BODY, "bounded-writer").await;
    r.complete_and_advance(&root, &uri, &writer).await;
    let critic = r
        .assess_chapter_reason(&root, &uri, "supported", &"理".repeat(96))
        .await;
    let coordinator = r.next_task(&root, &uri, "coordinator").await;
    let read = r.decision_status(&root, &coordinator).await;
    assert_eq!(read["chapterAcceptance"], acceptance);
    // decision_status consumes every individually bounded page before aggregating
    // arrays locally for assertions; an aggregate is never a tool response.
    let assessment = &read["requirements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["kind"] == "revisionOutcome")
        .unwrap()["assessment"];
    assert_eq!(assessment["verdict"], "passed");
    assert_eq!(assessment["checkCounts"]["total"], 12);
    assert_eq!(assessment["checkCounts"]["supported"], 12);
    assert_eq!(
        assessment["length"]["characterCount"],
        BODY.chars().filter(|c| !c.is_whitespace()).count()
    );
    assert!(assessment.get("chapterAcceptance").is_none());
    assert!(assessment.get("checks").is_none());
    let report = r
        .get(&format!(
            "{root}/revision-outcomes/{}",
            critic["target"]["writerTaskId"].as_str().unwrap()
        ))
        .await;
    assert_eq!(
        report["assessment"]["outcomes"][0]["reason"],
        "理".repeat(96)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn short_and_unresolved_prose_requires_repair_without_lowering_frozen_acceptance() {
    let r = Runtime::new().await;
    let (root, uri) = r.start().await;
    let coordinator = r.next_task(&root, &uri, "coordinator").await;
    let read = r.decision_status(&root, &coordinator).await;
    let choice = read["readyActions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["operation"] == "writeNextChapter")
        .unwrap();
    let acceptance = json!({"requirements":[{"id":"complete-chapter","text":"保存完整章节"},{"id":"deadline","text":"保留子时期限"}],
        "length":{"requirement":"起草前将作者篇幅要求理解为至少20个正文字符","minCharacters":20,"maxCharacters":null}});
    let select_uri = format!(
        "{root}/tasks/{}/work-decision/actions",
        coordinator["id"].as_str().unwrap()
    );
    let mut request = json!({"taskId":coordinator["id"],"threadId":coordinator["threadId"],"expectedRevision":coordinator["inputRevision"],
        "expectedWorkVersion":read["version"],"expectedSourceDigest":read["sourceDigest"],"actionKey":choice["actionKey"],
        "instruction":"按原要求起草，不降低标准","idempotencyKey":"first-writer"});
    let (code, _) = r
        .call("POST", &select_uri, CANDIDATE, Some(request.clone()))
        .await;
    assert!(!code.is_success());
    request["chapterAcceptance"] = acceptance.clone();
    r.candidate(&select_uri, request).await;
    r.complete_and_advance(&root, &uri, &coordinator).await;
    let writer = r.next_task(&root, &uri, "writer").await;
    let saved = r.save_writer(&root, &writer, BODY, "short").await;
    r.complete_and_advance(&root, &uri, &writer).await;
    let reviewer = r.next_task(&root, &uri, "coordinator").await;
    let read = r.decision_status(&root, &reviewer).await;
    assert!(
        read["readyActions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["operation"] == "generateChapter")
    );
    assert!(
        !read["readyActions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["operation"] == "finishChapter")
    );
    let finish = json!({"actionKey":"not-ready-finish"});
    assert_eq!(read["chapterAcceptance"], acceptance);
    let select_uri = format!(
        "{root}/tasks/{}/work-decision/actions",
        reviewer["id"].as_str().unwrap()
    );
    let mut request = json!({"taskId":reviewer["id"],"threadId":reviewer["threadId"],"expectedRevision":reviewer["inputRevision"],
        "expectedWorkVersion":read["version"],"expectedSourceDigest":read["sourceDigest"],"actionKey":finish["actionKey"],
        "instruction":"核对当前正文","idempotencyKey":"finish"});
    let source = r
        .source_check(&root, &reviewer, saved["revisionId"].as_str().unwrap())
        .await;
    let review = json!({"sourceRevisionId":saved["revisionId"],"assessmentRevisionId":"not-yet-assessed","checks":acceptance["requirements"].as_array().unwrap().iter()
        .map(|s|json!({"criterionId":s["id"],"issueId":null})).collect::<Vec<_>>(),"length":acceptance["length"]});
    let mut stale = review.clone();
    stale["sourceRevisionId"] = json!("old");
    let mut unresolved = review.clone();
    unresolved["checks"][0]["status"] = json!("unresolved");
    let mut weakened = review.clone();
    weakened["length"]["minCharacters"] = json!(1);
    for invalid in [Value::Null, review.clone(), stale, unresolved, weakened] {
        request["chapterReview"] = invalid;
        let (code, _) = r
            .call("POST", &select_uri, CANDIDATE, Some(request.clone()))
            .await;
        assert!(!code.is_success());
        assert_eq!(r.get(&uri).await["completedChapters"], 0);
    }
    let repair = read["readyActions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["operation"] == "generateChapter")
        .unwrap();
    let mut repair_request = request.clone();
    repair_request
        .as_object_mut()
        .unwrap()
        .remove("chapterReview");
    repair_request["actionKey"] = repair["actionKey"].clone();
    repair_request["idempotencyKey"] = json!("repair");
    repair_request["chapterAcceptance"] = acceptance.clone();
    let mut weakened = repair_request.clone();
    weakened["chapterAcceptance"]["length"]["minCharacters"] = json!(1);
    let (code, _) = r.call("POST", &select_uri, CANDIDATE, Some(weakened)).await;
    assert!(!code.is_success());
    r.candidate(&select_uri, repair_request).await;
    r.complete_and_advance(&root, &uri, &reviewer).await;
    let writer = r.next_task(&root, &uri, "repair-writer").await;
    let repaired = r
        .save_writer(
            &root,
            &writer,
            "林夕在子时前抵达码头，灯光照见收据上的罚银金额。",
            "repair-save",
        )
        .await;
    r.complete_and_advance(&root, &uri, &writer).await;
    r.assess_chapter(&root, &uri, "supported").await;
    let reviewer = r.next_task(&root, &uri, "coordinator").await;
    let read = r.decision_status(&root, &reviewer).await;
    let finish = read["readyActions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|action| action["operation"] == "finishChapter")
        .unwrap();
    let repaired_source = r
        .source_check(&root, &reviewer, repaired["revisionId"].as_str().unwrap())
        .await;
    let mut current_review = review.clone();
    current_review["sourceRevisionId"] = repaired["revisionId"].clone();
    current_review["assessmentRevisionId"] = read["requirements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["kind"] == "revisionOutcome")
        .unwrap()["assessment"]["reportRevisionId"]
        .clone();
    for invalid_source in [
        json!([]),
        json!([source]),
        json!([{"sourceId":repaired_source["sourceId"],"revisionId":repaired["revisionId"],"contentHash":repaired_source["contentHash"],"evidence":{"quote":"昨夜已经售出旧灯"}}]),
        json!([{"sourceId":repaired_source["sourceId"],"revisionId":repaired["revisionId"],"contentHash":"f".repeat(64),"evidence":repaired_source["evidence"]}]),
    ] {
        let mut invalid = current_review.clone();
        invalid["checks"][0]["sourceChecks"] = invalid_source;
        let request = json!({"taskId":reviewer["id"],"threadId":reviewer["threadId"],"expectedRevision":reviewer["inputRevision"],"expectedWorkVersion":read["version"],"expectedSourceDigest":read["sourceDigest"],"actionKey":finish["actionKey"],"instruction":"核对实际保存的当前稿","idempotencyKey":"unproven-review","chapterReview":invalid});
        let (code, _) = r
            .call(
                "POST",
                &format!(
                    "{root}/tasks/{}/work-decision/actions",
                    reviewer["id"].as_str().unwrap()
                ),
                CANDIDATE,
                Some(request),
            )
            .await;
        assert!(!code.is_success());
        assert_eq!(r.get(&uri).await["completedChapters"], 0);
    }
    let receipt = r.choose(&root, &reviewer, "finishChapter").await;
    assert_eq!(
        receipt["chapterReviewReceipt"]["revisionId"],
        repaired["revisionId"]
    );
    assert!(
        receipt["chapterReviewReceipt"]["characterCount"]
            .as_u64()
            .unwrap()
            >= 20
    );
    // Replay the original selected request; a registered finish is no longer a
    // ready action and must not be rediscovered as if it were new work.
    let same = r.candidate(&format!("{root}/tasks/{}/work-decision/actions",reviewer["id"].as_str().unwrap()),
        json!({"taskId":reviewer["id"],"threadId":reviewer["threadId"],"expectedRevision":reviewer["inputRevision"],
            "expectedWorkVersion":read["version"],"expectedSourceDigest":read["sourceDigest"],
            "actionKey":receipt["actionKey"],"instruction":"根据作者当前要求完成当前章节，不改其他章节",
            "idempotencyKey":"select-finishChapter"})).await;
    assert_eq!(receipt, same);
    let read = r.decision_status(&root, &reviewer).await;
    let mut changed = json!({"taskId":reviewer["id"],"threadId":reviewer["threadId"],"expectedRevision":reviewer["inputRevision"],
        "expectedWorkVersion":read["version"],"expectedSourceDigest":read["sourceDigest"],"actionKey":receipt["actionKey"],
        "instruction":"根据作者当前要求完成当前章节，不改其他章节","idempotencyKey":"select-finishChapter","chapterReview":review});
    changed["chapterReview"]["sourceRevisionId"] = repaired["revisionId"].clone();
    changed["chapterReview"]["checks"][0]["assessment"] = json!("不同评审");
    let (code, _) = r
        .call(
            "POST",
            &format!(
                "{root}/tasks/{}/work-decision/actions",
                reviewer["id"].as_str().unwrap()
            ),
            CANDIDATE,
            Some(changed),
        )
        .await;
    assert!(!code.is_success());
    let completed = r.complete_and_advance(&root, &uri, &reviewer).await;
    assert_eq!(completed["completedChapters"], 1);
    assert_eq!(completed["status"], "completed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn negative_independent_report_blocks_delivery_and_model_pause_commits_actual_state() {
    let r = Runtime::new().await;
    let (root, uri) = r.start().await;
    let (writer, _) = r
        .saved_chapter(
            &root,
            &uri,
            "林夕在子时前抵达码头，灯光照见收据上的罚银金额。",
        )
        .await;
    r.assess_chapter(&root, &uri, "unmet").await;
    let coordinator = r.next_task(&root, &uri, "repair-coordinator").await;
    let state = r.decision_status(&root, &coordinator).await;
    let assessment = state["requirements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|q| q["kind"] == "revisionOutcome")
        .unwrap();
    assert_eq!(assessment["assessment"]["verdict"], "notMet");
    assert_eq!(assessment["status"], "pending");
    let report_uri = format!(
        "{root}/tasks/{}/revision-outcome?threadId={}&expectedRevision={}&writerTaskId={}",
        coordinator["id"].as_str().unwrap(),
        coordinator["threadId"].as_str().unwrap(),
        coordinator["inputRevision"],
        writer["id"].as_str().unwrap()
    );
    let report = r.ok("GET", &report_uri, CANDIDATE, None).await;
    assert_eq!(report["state"], "current");
    assert_eq!(report["assessment"]["textDelivered"], false);
    let source = &report["taskSources"]["entries"][0];
    assert_eq!(
        source["sectionPath"],
        "/operationContext/revisionOutcomeReport"
    );
    let page = r
        .ok(
            "GET",
            &format!(
                "{root}/tasks/{}/task-source-content?threadId={}&expectedRevision={}&sourceId={}",
                coordinator["id"].as_str().unwrap(),
                coordinator["threadId"].as_str().unwrap(),
                coordinator["inputRevision"],
                source["sourceId"].as_str().unwrap()
            ),
            CANDIDATE,
            None,
        )
        .await;
    assert!(
        page["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["path"] == "/outcomes/0/status" && e["value"] == "unmet")
    );
    assert!(
        page["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["path"] == "/outcomes/0/evidence/0/quote" && e["value"].is_string())
    );
    assert!(
        !state["readyActions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["operation"] == "finishChapter")
    );
    let workspace = r.get(&format!("{root}/creative-workspace")).await;
    let foreign=r.post(&format!("{root}/tasks"),json!({"operation":"advisory","target":{"type":"project"},"inputRevision":workspace["revision"],"threadId":"different-work","authorIntent":"只讨论当前小说","capabilityHash":format!("{:x}",sha2::Sha256::digest(support::CAPABILITY.as_bytes())),"capabilityExpiresAt":r.get(&uri).await["deadlineAt"],"initialBudget":{"authorizationId":"foreign-read-budget","authorMessageId":"foreign-read-author","expectedBaseSnapshotId":workspace["headSnapshotId"],"expectedSourceDigest":workspace["sourceDigest"],"maxRequests":4,"maxTokens":4000,"deadlineAt":r.get(&uri).await["deadlineAt"]}})).await;
    let(code,_)=r.call("GET",&format!("{root}/tasks/{}/revision-outcome?threadId=different-work&expectedRevision={}&writerTaskId={}",foreign["id"].as_str().unwrap(),foreign["inputRevision"],writer["id"].as_str().unwrap()),CANDIDATE,None).await;
    assert!(!code.is_success());
    let finish =
        json!({"actionKey":"not-ready-finish","sourceRevisionId":assessment["sourceRevisionId"]});
    let mut request = json!({"taskId":coordinator["id"],"threadId":coordinator["threadId"],"expectedRevision":coordinator["inputRevision"],"expectedWorkVersion":state["version"],"expectedSourceDigest":state["sourceDigest"],"actionKey":finish["actionKey"],"instruction":"不能以报告存在代替通过","idempotencyKey":"rejected-finish"});
    let source = r
        .source_check(
            &root,
            &coordinator,
            finish["sourceRevisionId"].as_str().unwrap(),
        )
        .await;
    request["chapterReview"] = json!({"sourceRevisionId":finish["sourceRevisionId"],"assessmentRevisionId":"unpassed-report","checks":state["chapterAcceptance"]["requirements"].as_array().unwrap().iter().map(|s|json!({"criterionId":s["id"],"issueId":null})).collect::<Vec<_>>(),"length":state["chapterAcceptance"]["length"]});
    let (code, _) = r
        .call(
            "POST",
            &format!(
                "{root}/tasks/{}/work-decision/actions",
                coordinator["id"].as_str().unwrap()
            ),
            CANDIDATE,
            Some(request),
        )
        .await;
    assert!(!code.is_success());
    let before = r.get(&uri).await;
    let grant = r
        .get(&format!(
            "{root}/tasks/{}/provider-budget",
            coordinator["id"].as_str().unwrap()
        ))
        .await;
    let control_uri = format!(
        "{root}/tasks/{}/generation-control",
        coordinator["id"].as_str().unwrap()
    );
    let pause = json!({"taskId":coordinator["id"],"threadId":coordinator["threadId"],"expectedRevision":coordinator["inputRevision"],"runId":before["id"],"expectedVersion":before["version"],"expectedIntentRevision":0,"instruction":"先暂停原任务，保留本章修订要求","idempotencyKey":"model-pause"});
    let mut stale = pause.clone();
    stale["expectedVersion"] = json!(0);
    let (code, _) = r.call("POST", &control_uri, CANDIDATE, Some(stale)).await;
    assert!(!code.is_success());
    assert_eq!(r.get(&uri).await, before);
    let receipt = r.candidate(&control_uri, pause.clone()).await;
    assert_eq!(receipt["receipt"]["status"], "paused");
    let paused = r.get(&uri).await;
    assert_eq!(paused["status"], "paused");
    let project =
        novel_codex_story_core::ProjectId::parse(root.rsplit('/').next().unwrap()).unwrap();
    let request:novel_codex_story_core::ModelRunPauseRequest=serde_json::from_value(json!({"runId":pause["runId"],"expectedVersion":pause["expectedVersion"],"expectedIntentRevision":pause["expectedIntentRevision"],"instruction":pause["instruction"],"idempotencyKey":pause["idempotencyKey"]})).unwrap();
    let mut repo = novel_codex_story_core::SqliteStoryRepository::open(&r.path).unwrap();
    let task = repo
        .task(&project, coordinator["id"].as_str().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(
        repo.pause_generation_run_for_model(&task, &request)
            .unwrap(),
        receipt["receipt"]
    );
    drop(repo);
    assert_eq!(paused["completedChapters"], 0);
    assert_eq!(paused["targetChapters"], before["targetChapters"]);
    assert_eq!(paused["providerTokensUsed"], before["providerTokensUsed"]);
    r.settle(&uri, &coordinator, &grant).await;
    assert_eq!(r.get(&uri).await["status"], "paused");
    let next = r.post(&format!("{uri}/next"), json!({})).await;
    assert_ne!(next["action"]["kind"], "agentTask");
    let paused = r.get(&uri).await;
    r.post(
        &format!("{uri}/control"),
        json!({"expectedVersion":paused["version"],"command":"resume"}),
    )
    .await;
    let resumed = r.get(&uri).await;
    let mut repo = novel_codex_story_core::SqliteStoryRepository::open(&r.path).unwrap();
    assert!(matches!(
        repo.pause_generation_run_for_model(&task, &request),
        Err(novel_codex_story_core::RepositoryError::AuthoringRunStale)
    ));
    drop(repo);
    assert_eq!(r.get(&uri).await, resumed);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn generation_work_control_uses_control_revision_independently_of_background_progress() {
    let r = Runtime::new().await;
    let (root, uri) = r.start().await;
    let coordinator = r.next_task(&root, &uri, "coordinator").await;
    let run = r.get(&uri).await;
    let work_uri = format!("{root}/work-tasks/work-{}", run["id"].as_str().unwrap());
    let before = r.get(&work_uri).await;
    r.choose(&root, &coordinator, "generateBrief").await;
    let progressed = r.get(&work_uri).await;
    assert_eq!(r.get(&uri).await["version"], run["version"]);
    assert!(progressed["version"].as_i64().unwrap() > before["version"].as_i64().unwrap());
    assert_eq!(progressed["controlRevision"], before["controlRevision"]);
    let command = json!({"commandId":uuid::Uuid::new_v4().to_string(),
        "threadId":before["threadId"],"expectedControlRevision":before["controlRevision"],"command":"pause"});
    let outcome = r.post(&format!("{work_uri}/control"), command.clone()).await;
    assert_eq!(outcome["status"], "paused");
    assert_eq!(outcome["budget"], progressed["budget"]);
    let receipt_uri = format!("{work_uri}/control-commands/{}", command["commandId"].as_str().unwrap());
    let receipt = r.get(&receipt_uri).await;
    assert_eq!(receipt["receipt"]["status"], "applied");
    let paused_run = r.get(&uri).await;
    assert_eq!(paused_run["status"], "paused");
    let paused_work = r.get(&work_uri).await;
    let replay = r.post(&format!("{work_uri}/control"), command.clone()).await;
    assert_eq!(replay, outcome);
    assert_eq!(r.get(&receipt_uri).await, receipt);
    let mut stale = command;
    stale["commandId"] = json!(uuid::Uuid::new_v4().to_string());
    stale["command"] = json!("cancel");
    let (code, rejected) = r.call("POST", &format!("{work_uri}/control"), support::AUTHOR, Some(stale)).await;
    assert_eq!(code, axum::http::StatusCode::CONFLICT);
    assert_eq!(rejected["outcome"]["receipt"]["status"], "rejected");
    assert_eq!(rejected["outcome"]["receipt"]["reason"], "WorkTaskStale");
    assert_eq!(r.get(&uri).await, paused_run);
    assert_eq!(r.get(&work_uri).await, paused_work);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn actual_prior_read_changes_revoke_report_but_future_plan_and_later_chapters_do_not() {
    let r = Runtime::new().await;
    let (root, uri) = r.start_with_chapters(2).await;
    let (first, first_saved) = r
        .saved_chapter(&root, &uri, "期限已过，林夕缴纳罚银并留下第一份收据。")
        .await;
    r.finish_chapter(&root, &uri).await;
    let (second, _) = r
        .saved_chapter(&root, &uri, "林夕拿着昨日缴纳罚银的收据进入港城。")
        .await;
    r.assess_chapter_report_with_prior(
        &root,
        &uri,
        "supported",
        "已读前章收据后独立核对当前正文",
        &[],
        Some(1),
    )
    .await;
    let report_uri = format!(
        "{root}/revision-outcomes/{}",
        second["id"].as_str().unwrap()
    );
    assert_eq!(r.get(&report_uri).await["state"], "current");
    let coordinator = r.next_task(&root, &uri, "coordinator").await;
    let state = r.decision_status(&root, &coordinator).await;
    assert!(
        state["requirements"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|q| q["kind"] == "revisionOutcome")
            .all(|q| q["chapterId"] == second["target"]["chapterId"])
    );
    r.post(&format!("{root}/artifacts/plan"),json!({"expectedRevision":r.get(&format!("{root}/creative-workspace")).await["revision"],"expectedCandidateRevisionId":null,"idempotencyKey":"future-plan","payload":{"chapters":[{"chapterPlanId":"future","chapterNumber":16,"goal":"规划未来港城线索","scenes":[{"id":"future-scene","goal":"未来的线索","beats":["进入下一卷"],"expectedEffects":[]}]}]}})).await;
    assert_eq!(r.get(&report_uri).await["state"], "current");
    r.post(&format!("{root}/drafts/checkpoints"),json!({"expectedRevision":r.get(&format!("{root}/creative-workspace")).await["revision"],"expectedCandidateRevisionId":null,"idempotencyKey":"later-chapter","payload":{"chapterId":"chapter-later","chapterNumber":3,"title":"后章","body":"后章保存不修改当前稿和已读前文。","completeness":"chapterComplete","sceneGroups":[]}})).await;
    assert_eq!(r.get(&report_uri).await["state"], "current");
    r.post(&format!("{root}/drafts/checkpoints"),json!({"expectedRevision":r.get(&format!("{root}/creative-workspace")).await["revision"],"expectedCandidateRevisionId":first_saved["revisionId"],"idempotencyKey":"changed-required-prior","payload":{"chapterId":first["target"]["chapterId"],"chapterNumber":1,"title":"前文改动","body":"林夕没有缴纳罚银，也没有收据。","completeness":"chapterComplete","sceneGroups":[]}})).await;
    assert_eq!(r.get(&report_uri).await["state"], "stale");
    let work = r
        .get(&format!(
            "{root}/work-tasks/work-{}",
            r.get(&uri).await["id"].as_str().unwrap()
        ))
        .await;
    let assessment = work["deliveryReport"]["chapterAcceptanceReports"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["writerTaskId"] == second["id"])
        .unwrap();
    assert_eq!(assessment["current"], false);
    assert_eq!(assessment["verdict"], "stale");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn original_author_omission_blocks_supported_report_and_sourced_correction_is_replaceable() {
    let r = Runtime::new().await;
    let (root, uri) = r.start().await;
    let (_writer, _saved) = r.saved_chapter(&root, &uri, BODY).await;
    r.assess_chapter_report(
        &root,
        &uri,
        "supported",
        "模型清单满足，但原要求遗漏调查动机",
        &["原作者要求调查港城灯光，当前清单遗漏调查动机"],
    )
    .await;
    let decision = r.next_task(&root, &uri, "coordinator").await;
    let read = r.decision_status(&root, &decision).await;
    let pending = read["requirements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|q| q["kind"] == "revisionOutcome")
        .unwrap();
    assert_eq!(pending["assessment"]["verdict"], "notMet");
    assert_eq!(pending["assessment"]["uncoveredRequirementCount"], 1);
    assert_eq!(read["canFinish"], false);
    assert!(
        !read["readyActions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["operation"] == "finishChapter")
    );
    let repair = read["readyActions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["operation"] == "generateChapter")
        .unwrap();
    let author = r.read_author_requirements(&root, &decision).await;
    let mut corrected = read["chapterAcceptance"].clone();
    corrected["requirements"] =
        json!([{"id":"whole-chapter","text":"保存完整章节并呈现林夕调查港城灯光的动机"}]);
    corrected["interpretationCorrection"] = json!({"reason":"原作者要求中调查目标被初始模型清单遗漏","impact":"补回调查动机；保留篇幅和原授权范围","sourceChecks":[{"sourceId":author["sourceId"],"contentHash":author["contentHash"],"quote":"林夕调查港城灯光"}]});
    let mut request = json!({"taskId":decision["id"],"threadId":decision["threadId"],"expectedRevision":decision["inputRevision"],"expectedWorkVersion":read["version"],"expectedSourceDigest":read["sourceDigest"],"actionKey":repair["actionKey"],"chapterAcceptance":corrected,"instruction":"补回原作者调查目标并独立重核","idempotencyKey":"sourced-correction"});
    let mut fabricated = request.clone();
    fabricated["chapterAcceptance"]["interpretationCorrection"]["sourceChecks"][0]["quote"] =
        json!("没有读过的作者要求");
    let (code, _) = r
        .call(
            "POST",
            &format!(
                "{root}/tasks/{}/work-decision/actions",
                decision["id"].as_str().unwrap()
            ),
            CANDIDATE,
            Some(fabricated),
        )
        .await;
    assert!(!code.is_success());
    let receipt = r
        .candidate(
            &format!(
                "{root}/tasks/{}/work-decision/actions",
                decision["id"].as_str().unwrap()
            ),
            request.take(),
        )
        .await;
    assert!(receipt.is_object());
    assert_eq!(r.get(&uri).await["completedChapters"], 0);
    r.complete_and_advance(&root, &uri, &decision).await;
    let repair_writer = r.next_task(&root, &uri, "repair-writer").await;
    assert_eq!(repair_writer["target"]["chapterAcceptance"], corrected);
    assert_eq!(
        repair_writer["inputManifest"]["authorIntent"],
        support::author_intent(&r.get(&uri).await)
    );
}

use novel_codex_story_core::{GenerationModelControlRepository, TasksRepository};
