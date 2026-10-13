//! Public Runtime contracts: model selects verification, Native verifies provenance.
use serde_json::{Value, json};
#[path = "support/continuous_recovery.rs"]
mod support;
use support::{BODY, CANDIDATE, Runtime};

async fn read_draft(r: &Runtime, root: &str, task: &Value, revision: &Value) {
    read_draft_source(r, root, task, revision, false).await;
}

async fn read_draft_source(r: &Runtime, root: &str, task: &Value, revision: &Value, relevant: bool) {
    let pack=r.ok("GET",&format!("{root}/context?taskId={}&threadId={}&projectRevision={}&purpose=planner&byteBudget=60000",task["id"].as_str().unwrap(),task["threadId"].as_str().unwrap(),task["inputRevision"]),CANDIDATE,None).await;
    let directory = pack["taskSources"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["sectionPath"] == if relevant { "/relevantText" } else { "/taskWorkspace" })
        .unwrap();
    let base = format!(
        "{root}/tasks/{}/task-source-content?threadId={}&expectedRevision={}",
        task["id"].as_str().unwrap(),
        task["threadId"].as_str().unwrap(),
        task["inputRevision"]
    );
    let source = if relevant { directory.clone() } else {
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
    source.clone()
    };
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
async fn saved_verification_validity_is_independent_from_execution_lifecycle() {
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
    read_draft_source(&r, &root, &decision, &saved["revisionId"], false).await;
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

    let db = rusqlite::Connection::open(&r.path).unwrap();
    let work: String = db.query_row("SELECT work_task_id FROM story_direct_verifications_v1 WHERE revision_id=?1",[receipt["revisionId"].as_str().unwrap()],|row|row.get(0)).unwrap();
    for status in ["failed", "cancelled"] {
        db.execute("UPDATE story_tasks_v2 SET status=?1 WHERE task_id=?2",rusqlite::params![status,decision["id"].as_str().unwrap()]).unwrap();
        let projection = r.get(&format!("{root}/work-tasks/{work}")).await;
        let reports = projection["deliveryReport"]["chapterAcceptanceReports"].as_array().unwrap();
        let report = reports.iter().find(|v|v["reportRevisionId"]==receipt["revisionId"]).unwrap();
        assert_eq!(report["current"], true, "saved evidence must survive producer {status}: {report}");
        assert_ne!(projection["status"], "completed", "valid evidence alone must not complete Work");
        let (code,_) = r.call("POST",&format!("{root}/tasks/{}/complete",decision["id"].as_str().unwrap()),CANDIDATE,Some(json!({"threadId":decision["threadId"],"summary":"must not revive"}))).await;
        assert!(!code.is_success(), "report validity must not authorize a terminal task");
    }
    let workspace = r.get(&format!("{root}/creative-workspace")).await;
    let mut payload = workspace["chapters"][0]["candidate"].clone();
    payload["sceneGroups"][0]["body"] = json!("林夕离开了港口。");
    payload["body"] = json!("林夕离开了港口。");
    r.post(&format!("{root}/drafts/checkpoints"),json!({"expectedRevision":workspace["revision"],"expectedCandidateRevisionId":saved["revisionId"],"idempotencyKey":"author-replaced-draft","payload":payload})).await;
    let projection = r.get(&format!("{root}/work-tasks/{work}")).await;
    let reports = projection["deliveryReport"]["chapterAcceptanceReports"].as_array().unwrap();
    assert!(reports.iter().all(|v|v["reportRevisionId"]!=receipt["revisionId"] || v["current"]==false),"real draft changes must invalidate evidence");
}
