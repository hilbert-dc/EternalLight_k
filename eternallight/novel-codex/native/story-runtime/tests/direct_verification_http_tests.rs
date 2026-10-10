//! Public Runtime contracts: model selects verification, Native verifies provenance.
use serde_json::{Value, json};
#[path = "support/continuous_recovery.rs"]
mod support;
use support::{BODY, CANDIDATE, Runtime};

async fn read_draft(r: &Runtime, root: &str, task: &Value, revision: &Value) {
    let pack=r.ok("GET",&format!("{root}/context?taskId={}&threadId={}&projectRevision={}&purpose=planner&byteBudget=60000",task["id"].as_str().unwrap(),task["threadId"].as_str().unwrap(),task["inputRevision"]),CANDIDATE,None).await;
    let directory = pack["taskSources"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["sectionPath"] == "/taskWorkspace")
        .unwrap();
    let base = format!(
        "{root}/tasks/{}/task-source-content?threadId={}&expectedRevision={}",
        task["id"].as_str().unwrap(),
        task["threadId"].as_str().unwrap(),
        task["inputRevision"]
    );
    let page = r
        .ok(
            "GET",
            &format!(
                "{base}&sourceId={}&view=workspace&chapterNumber=1",
                directory["sourceId"].as_str().unwrap()
            ),
            CANDIDATE,
            None,
        )
        .await;
    let source = page["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["revisionId"] == *revision)
        .unwrap_or_else(|| panic!("{page}"));
    let mut cursor = String::new();
    loop {
        let p = r
            .ok(
                "GET",
                &format!(
                    "{base}&sourceId={}{cursor}",
                    source["sourceId"].as_str().unwrap()
                ),
                CANDIDATE,
                None,
            )
            .await;
        let Some(next) = p["nextCursor"].as_str() else {
            break;
        };
        cursor = format!("&cursor={next}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn model_direct_verification_requires_real_reads_and_delivers_without_critic_task() {
    let r = Runtime::new().await;
    let (root, uri) = r.start().await;
    let first = r.next_task(&root, &uri, "coordinator").await;
    r.choose_with_acceptance(&root,&first,"writeNextChapter",Some(json!({"requirements":[{"id":"whole-chapter","text":"林夕调查灯光"}],"length":{"requirement":"无篇幅限制","minCharacters":null,"maxCharacters":null},"verification":{"method":"direct","reason":"简单章节由主模型直接核对实际正文"}}))).await;
    r.complete_and_advance(&root, &uri, &first).await;
    let writer = r.next_task(&root, &uri, "writer").await;
    let saved = r.save_writer(&root, &writer, BODY, "direct-draft").await;
    r.complete_and_advance(&root, &uri, &writer).await;
    let decision = r.next_task(&root, &uri, "coordinator").await;
    let read = r.decision_status(&root, &decision).await;
    let action = read["readyActions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["operation"] == "verifyChapter")
        .unwrap();
    assert!(
        read["readyActions"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|a| a["operation"] == "assessRevisionOutcome")
            .all(|a| a["required"] == false)
    );
    let workspace = r.get(&format!("{root}/creative-workspace")).await;
    let group = &workspace["chapters"][0]["candidate"]["sceneGroups"][0]["id"];
    let endpoint = format!(
        "{root}/tasks/{}/work-decision/actions",
        decision["id"].as_str().unwrap()
    );
    let request = json!({"taskId":decision["id"],"threadId":decision["threadId"],"expectedRevision":decision["inputRevision"],"expectedWorkVersion":read["version"],"expectedSourceDigest":read["sourceDigest"],"actionKey":action["actionKey"],"instruction":"直接核对正文与作者原要求","idempotencyKey":"direct-verification","chapterReview":{"outcomes":[{"id":"whole-chapter","status":"supported","reason":"原文呈现了人物观察灯光","evidence":[{"groupId":group,"quote":BODY}],"issue":null}],"uncoveredRequirements":[]}});
    let (status, _) = r
        .call("POST", &endpoint, CANDIDATE, Some(request.clone()))
        .await;
    assert!(!status.is_success());
    r.read_author_requirements(&root, &decision).await;
    let (status, _) = r
        .call("POST", &endpoint, CANDIDATE, Some(request.clone()))
        .await;
    assert!(!status.is_success(), "author read alone is insufficient");
    read_draft(&r, &root, &decision, &saved["revisionId"]).await;
    let mut forged = request.clone();
    forged["chapterReview"]["outcomes"][0]["evidence"][0]["quote"] = json!("不存在的正文");
    let (status, _) = r.call("POST", &endpoint, CANDIDATE, Some(forged)).await;
    assert!(!status.is_success());
    let receipt = r.candidate(&endpoint, request.clone()).await;
    assert_eq!(receipt["verificationSaved"], true);
    assert_eq!(r.candidate(&endpoint, request.clone()).await, receipt);
    let mut changed = request;
    changed["chapterReview"]["outcomes"][0]["reason"] = json!("changed");
    let (status, _) = r.call("POST", &endpoint, CANDIDATE, Some(changed)).await;
    assert!(!status.is_success());
    let state = r.decision_status(&root, &decision).await;
    assert_eq!(
        state["requirements"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["kind"] == "revisionOutcome")
            .unwrap()["assessment"]["verdict"],
        "passed"
    );
    let finished = r.choose(&root, &decision, "finishChapter").await;
    assert_eq!(
        finished["chapterReviewReceipt"]["assessmentMethod"],
        "direct"
    );
    let selected = r.decision_status(&root, &decision).await;
    assert_eq!(selected["completionRegistration"]["operation"], "finishChapter");
    assert_eq!(selected["completionRegistration"]["status"], "registered");
    assert_eq!(selected["completionRegistration"]["nextRequiredOperation"], "complete_task");
    assert_eq!(selected["readyActions"], json!([]));
    assert_eq!(selected["coverage"]["availableActions"], 0);
    assert_eq!(selected["deliveryReviewRequired"], true);
    assert_eq!(r.get(&uri).await["completedChapters"], 0);
    r.complete_and_advance(&root, &uri, &decision).await;
    assert_eq!(r.get(&uri).await["completedChapters"], 1);
    let db = rusqlite::Connection::open(&r.path).unwrap();
    let critics: i64 = db
        .query_row(
            "SELECT count(*) FROM story_tasks_v2 WHERE operation='assessRevisionOutcome'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(critics, 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn independent_contract_does_not_offer_direct_verification() {
    let r = Runtime::new().await;
    let (root, uri) = r.start().await;
    r.saved_chapter(&root, &uri, BODY).await;
    let decision = r.next_task(&root, &uri, "coordinator").await;
    let state = r.decision_status(&root, &decision).await;
    assert!(
        !state["readyActions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["operation"] == "verifyChapter")
    );
    assert!(
        state["readyActions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["operation"] == "assessRevisionOutcome" && a["required"] == true)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn direct_negative_report_blocks_delivery_and_changed_prose_invalidates_it() {
    let r = Runtime::new().await;
    let (root, uri) = r.start().await;
    let first = r.next_task(&root, &uri, "coordinator").await;
    r.choose_with_acceptance(&root,&first,"writeNextChapter",Some(json!({"requirements":[{"id":"whole-chapter","text":"林夕调查灯光"}],"length":{"requirement":"无篇幅限制","minCharacters":null,"maxCharacters":null},"verification":{"method":"direct","reason":"主模型直接检查"}}))).await;
    r.complete_and_advance(&root, &uri, &first).await;
    let writer = r.next_task(&root, &uri, "writer").await;
    let saved = r
        .save_writer(&root, &writer, BODY, "negative-direct-draft")
        .await;
    r.complete_and_advance(&root, &uri, &writer).await;
    let decision = r.next_task(&root, &uri, "coordinator").await;
    r.read_author_requirements(&root, &decision).await;
    read_draft(&r, &root, &decision, &saved["revisionId"]).await;
    let state = r.decision_status(&root, &decision).await;
    let action = state["readyActions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["operation"] == "verifyChapter")
        .unwrap();
    let workspace = r.get(&format!("{root}/creative-workspace")).await;
    let group = &workspace["chapters"][0]["candidate"]["sceneGroups"][0]["id"];
    let request = json!({"taskId":decision["id"],"threadId":decision["threadId"],"expectedRevision":decision["inputRevision"],"expectedWorkVersion":state["version"],"expectedSourceDigest":state["sourceDigest"],"actionKey":action["actionKey"],"instruction":"记录真实缺项","idempotencyKey":"negative-direct-check","chapterReview":{"outcomes":[{"id":"whole-chapter","status":"unmet","reason":"没有说明调查灯光的依据","evidence":[{"groupId":group,"quote":BODY}],"issue":{"issueId":"missing-origin","newIssue":{"summary":"缺少灯光来源"},"resolutionOf":[]}}],"uncoveredRequirements":[]}});
    r.candidate(
        &format!(
            "{root}/tasks/{}/work-decision/actions",
            decision["id"].as_str().unwrap()
        ),
        request,
    )
    .await;
    let state = r.decision_status(&root, &decision).await;
    assert_eq!(
        state["requirements"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["kind"] == "revisionOutcome")
            .unwrap()["assessment"]["verdict"],
        "notMet"
    );
    assert!(
        !state["readyActions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["operation"] == "finishChapter")
    );
    let report_uri = format!(
        "{root}/revision-outcomes/{}",
        writer["id"].as_str().unwrap()
    );
    let report = r.get(&report_uri).await;
    assert_eq!(report["state"], "current");
    assert_eq!(
        report["assessment"]["outcomes"][0]["issue"]["issueId"],
        "missing-origin"
    );
    r.post(&format!("{root}/drafts/checkpoints"),json!({"expectedRevision":r.get(&format!("{root}/creative-workspace")).await["revision"],"expectedCandidateRevisionId":saved["revisionId"],"idempotencyKey":"author-changes-current-draft","payload":{"chapterId":writer["target"]["chapterId"],"chapterNumber":1,"title":"作者修改","body":"正文已经由作者修改，旧核验不得继续交付。","completeness":"chapterComplete","sceneGroups":[]}})).await;
    assert_eq!(r.get(&report_uri).await["state"], "stale");
    assert_eq!(r.get(&uri).await["completedChapters"], 0);
}
