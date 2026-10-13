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
async fn model_direct_verification_requires_real_reads_and_delivers_without_critic_task() {
    direct_verification(false).await;
    direct_verification(true).await;
}

async fn direct_verification(relevant: bool) {
    let r = Runtime::new().await;
    let (root, uri) = r.start().await;
    let first = r.next_task(&root, &uri, "coordinator").await;
    // A method plan is optional. Keep this fixture focused on the receipt-to-action contract.
    let initial = r.decision_status(&root, &first).await;
    r.candidate(&format!("{root}/tasks/{}/work-plan", first["id"].as_str().unwrap()),
        json!({"taskId":first["id"],"threadId":first["threadId"],"expectedRevision":first["inputRevision"],
            "expectedPlanVersion":initial["workPlan"]["version"],"idempotencyKey":"no-optional-method",
            "plan":{"intentMode":"executeGoals","goals":[]}})).await;
    let queued = r.choose_with_acceptance(&root,&first,"writeNextChapter",Some(json!({"requirements":[{"id":"whole-chapter","text":"林夕调查灯光"}],"length":{"requirement":"无篇幅限制","minCharacters":null,"maxCharacters":null},"verification":{"method":"direct","reason":"简单章节由主模型直接核对实际正文"}}))).await;
    assert_eq!(queued["decisionStatus"], r.decision_status(&root, &first).await);
    assert_eq!(queued["decisionStatus"]["deliveryReviewRequired"], false);
    r.complete_and_advance(&root, &uri, &first).await;
    let writer = r.next_task(&root, &uri, "writer").await;
    let saved = r.save_writer(&root, &writer, BODY, "direct-draft").await;
    r.complete_and_advance(&root, &uri, &writer).await;
    let decision = r.next_task(&root, &uri, "coordinator").await;
    let first_page = r.ok("GET", &format!("{root}/tasks/{}/work-decision?threadId={}&expectedRevision={}", decision["id"].as_str().unwrap(), decision["threadId"].as_str().unwrap(), decision["inputRevision"]), CANDIDATE, None).await;
    assert!(first_page.to_string().len() <= 6500, "the existing response bound must not grow");
    assert_eq!(first_page["coverage"]["complete"], true, "all compact choices and requirements for one chapter fit together");
    assert!(first_page["pagination"]["nextCursor"].is_null());
    assert!(first_page["readyActions"].as_array().unwrap().iter().any(|a| a["operation"] == "verifyChapter"), "a simple current chapter check must be visible on the first compact action page");
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
    let locator = &action["currentDraftSource"];
    assert_eq!(locator["revisionId"], saved["revisionId"]);
    assert_eq!(locator["contentRead"], false);
    assert_eq!(locator["read"]["tool"], "read_source_content");
    assert_eq!(locator["read"]["arguments"]["expectedRevision"], decision["inputRevision"]);
    if relevant {
        read_draft_source(&r, &root, &decision, &saved["revisionId"], true).await;
    } else {
        // No directory scan or guessed cursor: consume the real action affordance.
        let source = locator["read"]["arguments"]["sourceId"].as_str().unwrap();
        let base = format!("{root}/tasks/{}/task-source-content?threadId={}&expectedRevision={}&sourceId={source}",
            decision["id"].as_str().unwrap(), decision["threadId"].as_str().unwrap(), decision["inputRevision"]);
        let mut cursor = String::new();
        loop {
            let page = r.ok("GET", &format!("{base}{cursor}"), CANDIDATE, None).await;
            let Some(next) = page["nextCursor"].as_str() else { break; };
            cursor = format!("&cursor={next}");
        }
    }

    let mut wrong_group = request.clone();
    wrong_group["chapterReview"]["outcomes"][0]["evidence"][0]["groupId"] = json!("other-chapter-group");
    let (status, rejected) = r.call("POST", &endpoint, CANDIDATE, Some(wrong_group)).await;
    assert!(!status.is_success());
    assert!(rejected.to_string().contains("evidence_group_not_in_candidate"), "{rejected}");
    assert!(rejected.to_string().contains("/chapterReview/outcomes/0/evidence/0/groupId"), "{rejected}");
    assert!(!rejected.to_string().contains("other-chapter-group"), "unknown input identity is not echoed");
    assert_eq!(r.decision_status(&root, &decision).await["version"], read["version"]);
    let mut forged = request.clone();
    forged["chapterReview"]["outcomes"][0]["evidence"][0]["quote"] = json!("不存在的正文");
    let (status, rejected) = r.call("POST", &endpoint, CANDIDATE, Some(forged)).await;
    assert!(!status.is_success());
    assert!(rejected.to_string().contains("evidence_quote_not_found"), "{rejected}");
    assert!(rejected.to_string().contains("/chapterReview/outcomes/0/evidence/0"), "{rejected}");
    assert!(rejected.to_string().contains(saved["revisionId"].as_str().unwrap()), "{rejected}");
    let receipt = r.candidate(&endpoint, request.clone()).await;
    assert_eq!(receipt["verificationSaved"], true);
    assert_eq!(r.candidate(&endpoint, request.clone()).await, receipt);
    let mut changed = request;
    changed["chapterReview"]["outcomes"][0]["reason"] = json!("changed");
    let (status, _) = r.call("POST", &endpoint, CANDIDATE, Some(changed)).await;
    assert!(!status.is_success());
    let state = &receipt["decisionStatus"];
    assert!(state["version"].as_i64().unwrap() > read["version"].as_i64().unwrap());
    let wire_status = r.ok("GET", &format!("{root}/tasks/{}/work-decision?threadId={}&expectedRevision={}", decision["id"].as_str().unwrap(), decision["threadId"].as_str().unwrap(), decision["inputRevision"]), CANDIDATE, None).await;
    assert_eq!(state, &wire_status, "same authoritative projection without test-side source hydration");
    assert_eq!(
        state["requirements"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["kind"] == "revisionOutcome")
            .unwrap()["assessment"]["verdict"],
        "passed"
    );
    let finish = state["readyActions"].as_array().unwrap().iter().find(|a| a["operation"] == "finishChapter").unwrap();
    let finish_request = json!({"taskId":decision["id"],"threadId":decision["threadId"],"expectedRevision":decision["inputRevision"],
        "expectedWorkVersion":state["version"],"expectedSourceDigest":state["sourceDigest"],
        "actionKey":finish["actionKey"],"instruction":"根据已保存核验登记结章","idempotencyKey":"finish-from-receipt"});
    let mut stale = finish_request.clone();
    stale["expectedWorkVersion"] = read["version"].clone();
    let (status, rejected) = r.call("POST", &endpoint, CANDIDATE, Some(stale)).await;
    assert!(!status.is_success(), "old pre-verification CAS must remain invalid");
    assert!(rejected.get("decisionStatus").is_none());
    let finished = r.candidate(&endpoint, finish_request.clone()).await;
    assert_eq!(r.candidate(&endpoint, finish_request).await, finished);
    assert_eq!(
        finished["chapterReviewReceipt"]["assessmentMethod"],
        "direct"
    );
    let selected = &finished["decisionStatus"];
    assert_eq!(selected["completionRegistration"]["operation"], "finishChapter");
    assert_eq!(selected["completionRegistration"]["status"], "registered");
    assert_eq!(selected["completionRegistration"]["nextRequiredOperation"], "complete_task");
    assert_eq!(selected["readyActions"], json!([]));
    assert_eq!(selected["coverage"]["availableActions"], 0);
    assert_eq!(selected["deliveryReviewRequired"], true);
    assert_eq!(r.get(&uri).await["completedChapters"], 0);
    let grant = r.get(&format!("{root}/tasks/{}/provider-budget", decision["id"].as_str().unwrap())).await;
    // Complete using only the returned current status; no status query follows finishChapter.
    r.candidate(&format!("{root}/tasks/{}/complete", decision["id"].as_str().unwrap()),
        json!({"threadId":decision["threadId"],"summary":"已核对保存稿及作者要求",
            "deliveryReview":{"expectedWorkVersion":selected["version"],"expectedSourceDigest":selected["sourceDigest"],
                "goalStatus":"fulfilled","goalAssessment":"本章正文与核验均已保存","checkedScope":"当前章节全文和作者原要求",
                "unresolved":[],"issueResolutions":[],"authorDecisions":[]}})).await;
    let charged = r.settle(&uri, &decision, &grant).await;
    r.post(&format!("{uri}/advance-task"),json!({"expectedVersion":charged["version"],"taskId":decision["id"]})).await;
    assert_eq!(r.get(&uri).await["completedChapters"], 1);
    let work = r.get(&format!("{root}/work-tasks/work-{}", uri.rsplit('/').next().unwrap())).await;
    assert!(work["deliveryReport"]["deliveryReview"]["evidence"].as_array().unwrap()
        .iter().any(|e| e["taskId"] == writer["id"] && e["revisionId"] == saved["revisionId"]),
        "Core must persist the current output binding when the model omits evidence: {work}");
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
