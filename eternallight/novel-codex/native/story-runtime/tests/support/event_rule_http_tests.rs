use super::{Role, call, prepare_chapter};
use axum::http::StatusCode;
use novel_codex_story_core::{
    BranchId, ConditionEvaluationControl, ConditionEvaluationStatus, ConditionEvaluationTarget,
    ProjectId, SqliteStoryRepository,
};
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[path = "condition_api_checks.rs"]
mod api_checks;
#[path = "condition_execution_checks.rs"]
mod execution_checks;
#[path = "condition_query_checks.rs"]
mod query_checks;

// Included by story_event_http_tests to share the actual router fixture.
#[tokio::test]
async fn adopted_rules_scan_unselected_counterclaims_and_preserve_historical_authority() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("rule-conditions.sqlite3");
    let (status, created) = call(
        &path,
        "POST",
        "/api/projects",
        json!({"title":"条件反证"}),
        Role::Author,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let project = created["project"]["id"].as_str().unwrap();
    let base = format!("/api/projects/{project}");
    let body = "甲持有信。";
    let evidence = json!({"quote":body,"start":0,"end":body.chars().count()});
    let entities = vec![
        json!({"id":"a","kind":"character","displayName":"甲","aliases":[],"evidence":evidence}),
        json!({"id":"letter","kind":"item","displayName":"信","aliases":[],"evidence":evidence}),
    ];
    let mut batches = Vec::new();
    let mut counter = Value::Null;
    for (number, value) in [(1, true), (3, false)] {
        let (_, batch, task) = prepare_chapter(&path, &base, number, body).await;
        let assertion = json!({"kind":"state","subjectId":"letter","predicate":"held","value":value,"stateSchema":{"cardinality":"single","valueType":{"type":"boolean"}},"validFrom":1,"validUntilExclusive":5,"validUntilEvidence":evidence,"evidence":evidence});
        // More than the display budget of same-direction source claims must not
        // hide the other batch's counterclaim in any public interpretation path.
        let assertions = vec![assertion.clone(); if value { 20 } else { 1 }];
        let (status,published) = call(&path,"POST",&format!("{base}/modeling-batches/{}/publish",batch["id"].as_str().unwrap()),json!({"taskId":task["id"],"threadId":"cross-event","expectedRevision":0,"entities":entities,"assertions":assertions}),Role::Candidate).await;
        assert_eq!(status, StatusCode::OK, "{published}");
        counter = assertion;
        batches.push(batch);
    }
    let (draft, batch, task) = prepare_chapter(&path, &base, /*number*/ 2, body).await;
    let mut event_entities = entities;
    event_entities.push(json!({"id":"delivery","kind":"event","displayName":"持信事件","aliases":[],"evidence":evidence}));
    let event = json!({"kind":"event","subjectId":"delivery","predicate":"transfer","storyTime":5,"value":{"participants":[{"role":"actor","entityId":"a"},{"role":"item","entityId":"letter"}],"preconditions":[{"sourceBatchId":batches[0]["id"],"assertionIndex":0}],"effects":[],"untilExclusive":null},"evidence":evidence});
    let (status,published) = call(&path,"POST",&format!("{base}/modeling-batches/{}/publish",batch["id"].as_str().unwrap()),json!({"taskId":task["id"],"threadId":"cross-event","expectedRevision":0,"entities":event_entities,"assertions":[event]}),Role::Candidate).await;
    assert_eq!(status, StatusCode::OK, "{published}");
    let (status,saved) = call(&path,"POST",&format!("{base}/artifacts/propertyRules"),json!({"expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":"condition-rules","payload":{
        "definitions":[{"predicate":"held","subjectKind":"item","schema":{"cardinality":"single","valueType":{"type":"boolean"}},"description":"是否持有","exceptions":[]}],
        "eventRules":[{"eventType":"transfer","description":"持有才能交出","exceptions":[],"condition":{"type":"all","conditions":[{"type":"state","role":"item","subjectKind":"item","predicate":"held","quantifier":"all","operator":"eq","operand":{"type":"boolean","value":true}},{"type":"state","role":"item","subjectKind":"item","predicate":"held","quantifier":"all","operator":"eq","operand":{"type":"boolean","value":true}}]}}]}}),Role::Author).await;
    assert_eq!(status, StatusCode::CREATED, "{saved}");
    let query = format!("{base}/knowledge/state?perspective=world&predicate=transfer&worldTime=7");
    let (status, candidate) = call(&path, "GET", &query, Value::Null, Role::Author).await;
    assert_eq!(status, StatusCode::OK, "{candidate}");
    assert_eq!(
        candidate["entries"][0]["eventConditionAssessment"]["status"],
        "noApplicableRule"
    );
    let (status,review) = call(&path,"POST",&format!("{base}/reviews"),json!({"artifactKind":"propertyRules","revisionId":saved["revisionId"],"proposals":[],"selections":{},"idempotencyKey":"condition-rule-review"}),Role::Author).await;
    assert_eq!(status, StatusCode::CREATED, "{review}");
    let (status, receipt) = call(
        &path,
        "POST",
        &format!("{base}/reviews/{}/accept", review["id"].as_str().unwrap()),
        json!({"expectedRevision":0,"idempotencyKey":"condition-rule-accept","selections":{}}),
        Role::Author,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{receipt}");
    let snapshot = SqliteStoryRepository::open(&path)
        .unwrap()
        .branch_head(
            &ProjectId::parse(project).unwrap(),
            &BranchId::parse("main").unwrap(),
        )
        .unwrap()
        .unwrap()
        .snapshot_id;
    let (status, state) = call(&path, "GET", &query, Value::Null, Role::Author).await;
    assert_eq!(status, StatusCode::OK, "{state}");
    assert_eq!(state["entries"].as_array().unwrap().len(), 1);
    let assessment = &state["entries"][0]["eventConditionAssessment"];
    assert_eq!(assessment["result"], "both");
    assert_eq!(assessment["coverage"]["complete"], true);
    assert_eq!(assessment["ruleRevisionId"], saved["revisionId"]);
    assert_eq!(assessment["worldTruthCertified"], false);
    let mut references = assessment["sourceClaims"].as_array().unwrap().clone();
    references.sort_by_key(|reference| reference["sourceBatchId"].as_str().unwrap().to_owned());
    let mut expected = batches
        .iter()
        .map(|batch| json!({"sourceBatchId":batch["id"],"assertionIndex":0}))
        .collect::<Vec<_>>();
    expected.sort_by_key(|reference| reference["sourceBatchId"].as_str().unwrap().to_owned());
    assert_eq!(references, expected);
    // Model queries normalize source quotes into an evidence dictionary. Condition
    // references must survive that path as references, not masquerade as quote anchors.
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        + 600;
    let (status,query_task) = call(&path,"POST",&format!("{base}/tasks"),json!({
        "operation":"advisory","inputRevision":1,"threadId":"condition-query","target":{"type":"project"},
        "authorIntent":"分析交信条件的反证","capabilityExpiresAt":expires,
        "capabilityHash":format!("{:x}",Sha256::digest(super::CAPABILITY.as_bytes()))
    }),Role::Author).await;
    assert_eq!(status, StatusCode::CREATED, "{query_task}");
    let (status,report) = call(&path,"POST",&format!("{base}/tasks/{}/query",query_task["id"].as_str().unwrap()),json!({
        "threadId":"condition-query","expectedRevision":1,
        "plan":{"question":"交信条件是否成立？","byteBudget":9000,"ranking":{"type":"literal"},
          "steps":[{"id":"condition","operation":{"type":"state","entityId":"letter","predicate":"transfer","perspective":"world","worldTime":7}}]}
    }),Role::Candidate).await;
    assert_eq!(status, StatusCode::OK, "{report}");
    assert!(report.to_string().len() <= 9000);
    assert_eq!(report["steps"][0]["results"].as_array().unwrap().len(), 1);
    let result = &report["steps"][0]["results"][0];
    assert_eq!(&result["eventConditionAssessment"], assessment);
    let quote_ref = result["assertion"]["evidenceRef"].as_str().unwrap();
    assert_eq!(report["evidence"][quote_ref]["quote"], body);
    assert_eq!(state["entries"][0]["confirmationState"], "candidate");
    let (status,draft_review) = call(&path,"POST",&format!("{base}/reviews"),json!({"artifactKind":"draft","revisionId":draft["revisionId"],"modelingBatchId":batch["id"],"proposals":[],"selections":{},"idempotencyKey":"condition-draft-review"}),Role::Author).await;
    assert_eq!(status, StatusCode::CREATED, "{draft_review}");
    assert_eq!(
        &draft_review["factProposals"][0]["storyEvent"]["conditionAssessment"],
        assessment
    );
    let (status, limited) = call(
        &path,
        "GET",
        &format!("{base}/knowledge/state?perspective=reader&readingChapter=3&predicate=transfer"),
        Value::Null,
        Role::Author,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{limited}");
    assert!(limited.get("eventConditionsPolicy").is_none());
    assert!(
        limited["entries"][0]
            .get("eventConditionAssessment")
            .is_none()
    );
    query_checks::verify(&path, project, &base, &batch, &draft).await;
    let (status, undone) = call(
        &path,
        "POST",
        &format!(
            "{base}/commits/{}/undo",
            receipt["commitId"].as_str().unwrap()
        ),
        json!({"expectedRevision":1,"idempotencyKey":"condition-undo"}),
        Role::Author,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{undone}");
    let (_, current) = call(&path, "GET", &query, Value::Null, Role::Author).await;
    assert_eq!(
        current["entries"][0]["eventConditionAssessment"]["status"],
        "noApplicableRule"
    );
    let historical_query = format!("{query}&snapshotId={snapshot}");
    let (_, historical) = call(&path, "GET", &historical_query, Value::Null, Role::Author).await;
    assert_eq!(
        &historical["entries"][0]["eventConditionAssessment"],
        assessment
    );
    let (status,review) = call(&path,"POST",&format!("{base}/reviews"),json!({"artifactKind":"propertyRules","revisionId":saved["revisionId"],"proposals":[],"selections":{},"idempotencyKey":"condition-rule-readopt-review"}),Role::Author).await;
    assert_eq!(status, StatusCode::CREATED, "{review}");
    let (status, readopted) = call(
        &path,
        "POST",
        &format!("{base}/reviews/{}/accept", review["id"].as_str().unwrap()),
        json!({"expectedRevision":2,"idempotencyKey":"condition-rule-readopt","selections":{}}),
        Role::Author,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{readopted}");
    let (status,pending) = call(&path,"POST",&format!("{base}/drafts/checkpoints"),json!({"payload":{"chapterNumber":4,"title":"未建模","body":"新的一天。"},"expectedRevision":3,"expectedCandidateRevisionId":null,"idempotencyKey":"condition-pending"}),Role::Author).await;
    assert_eq!(status, StatusCode::CREATED, "{pending}");
    let (status, incomplete) = call(&path, "GET", &query, Value::Null, Role::Author).await;
    assert_eq!(status, StatusCode::OK, "{incomplete}");
    assert_eq!(
        incomplete["entries"][0]["eventConditionAssessment"]["result"],
        "unknown"
    );
    assert_eq!(
        incomplete["entries"][0]["eventConditionAssessment"]["observedResult"],
        "both"
    );
    assert_eq!(
        incomplete["entries"][0]["eventConditionAssessment"]["coverage"]["sources"]["pendingSources"],
        1
    );
    let (status, historical) =
        call(&path, "GET", &historical_query, Value::Null, Role::Author).await;
    assert_eq!(status, StatusCode::OK, "{historical}");
    assert_eq!(
        &historical["entries"][0]["eventConditionAssessment"],
        assessment
    );
    // Persistent condition requests use the same published/approved source
    // fixture. No fabricated successful proof or client-provided binding hash.
    let project_id = ProjectId::parse(project).unwrap();
    let target = ConditionEvaluationTarget {
        snapshot_id: Some(snapshot.to_string()),
        source_batch_id: batch["id"].as_str().unwrap().into(),
        assertion_index: 0,
    };
    let mut repository = SqliteStoryRepository::open(&path).unwrap();
    // Missing-proof rejection is checked before the first audit in query_checks.
    repository.rebuild_entity_index(&project_id).unwrap();
    repository.rebuild_assertion_index(&project_id).unwrap();
    let audit = repository.start_knowledge_audit(&project_id).unwrap();
    loop {
        let progress = repository
            .advance_knowledge_audit(&project_id, &audit.id)
            .unwrap();
        if progress.status != "running" {
            assert_eq!(progress.verified_consistent, Some(true));
            break;
        }
    }
    api_checks::verify(&path, &base, &target).await;
    let job = repository
        .start_condition_evaluation(&project_id, target.clone())
        .unwrap();
    assert_eq!(
        repository
            .start_condition_evaluation(&project_id, target.clone())
            .unwrap(),
        job
    );
    let mut invalid = target.clone();
    invalid.snapshot_id = Some("not-owned".into());
    assert!(
        repository
            .start_condition_evaluation(&project_id, invalid)
            .is_err()
    );
    let mut pending = target.clone();
    pending.snapshot_id = None;
    assert!(
        repository
            .start_condition_evaluation(&project_id, pending)
            .is_err()
    );
    let mut non_event = target.clone();
    non_event.source_batch_id = batches[0]["id"].as_str().unwrap().into();
    assert!(
        repository
            .start_condition_evaluation(&project_id, non_event)
            .is_err()
    );
    let paused = repository
        .control_condition_evaluation(
            &project_id,
            &job.id,
            job.version,
            ConditionEvaluationControl::Pause,
        )
        .unwrap();
    assert_eq!(paused.status, ConditionEvaluationStatus::Paused);
    drop(repository);
    let mut repository = SqliteStoryRepository::open(&path).unwrap();
    assert_eq!(
        repository
            .condition_evaluation(&project_id, &job.id)
            .unwrap(),
        paused
    );
    assert!(
        repository
            .condition_evaluation(&ProjectId::parse("other-project").unwrap(), &job.id)
            .is_err()
    );
    assert!(
        repository
            .control_condition_evaluation(
                &project_id,
                &job.id,
                job.version,
                ConditionEvaluationControl::Resume
            )
            .is_err()
    );
    let resumed = repository
        .control_condition_evaluation(
            &project_id,
            &job.id,
            paused.version,
            ConditionEvaluationControl::Resume,
        )
        .unwrap();
    assert_eq!(resumed.status, ConditionEvaluationStatus::Queued);
    let cancelled = repository
        .control_condition_evaluation(
            &project_id,
            &job.id,
            resumed.version,
            ConditionEvaluationControl::Cancel,
        )
        .unwrap();
    assert_eq!(cancelled.status, ConditionEvaluationStatus::Cancelled);
    // Freeze all ranges before starting requests, so the capacity test doesn't
    // accidentally free slots by changing the snapshot generation between starts.
    let db = rusqlite::Connection::open(&path).unwrap();
    for index in 0..17 {
        let id = format!("condition-capacity-{index}");
        db.execute("INSERT INTO story_snapshots_v2(snapshot_id,project_id,branch_id,parent_snapshot_id) SELECT ?1,project_id,branch_id,snapshot_id FROM story_snapshots_v2 WHERE snapshot_id=?2",rusqlite::params![id,snapshot.to_string()]).unwrap();
        db.execute("INSERT INTO story_snapshot_members_v2(snapshot_id,object_id,revision_id) SELECT ?1,object_id,revision_id FROM story_snapshot_members_v2 WHERE snapshot_id=?2",rusqlite::params![id,snapshot.to_string()]).unwrap();
    }
    drop(db);
    let mut capacity_jobs = Vec::new();
    for index in 0..16 {
        let mut capacity_target = target.clone();
        capacity_target.snapshot_id = Some(format!("condition-capacity-{index}"));
        capacity_jobs.push(
            repository
                .start_condition_evaluation(&project_id, capacity_target)
                .unwrap(),
        );
    }
    let mut overflow = target.clone();
    overflow.snapshot_id = Some("condition-capacity-16".into());
    assert!(
        repository
            .start_condition_evaluation(&project_id, overflow.clone())
            .is_err()
    );
    // Input changes invalidate queued reservations. A new valid request must
    // retire those reservations instead of leaving the queue permanently full.
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute("INSERT INTO story_snapshots_v2(snapshot_id,project_id,branch_id,parent_snapshot_id) VALUES('condition-extra-snapshot',?1,'main',NULL)",[project]).unwrap();
    db.execute("INSERT INTO story_snapshot_members_v2(snapshot_id,object_id,revision_id) SELECT 'condition-extra-snapshot',object_id,revision_id FROM story_snapshot_members_v2 WHERE snapshot_id=?1",[snapshot.to_string()]).unwrap();
    drop(db);
    let replacement = repository
        .start_condition_evaluation(&project_id, overflow)
        .unwrap();
    assert_eq!(
        repository
            .condition_evaluation(&project_id, &capacity_jobs[0].id)
            .unwrap()
            .status,
        ConditionEvaluationStatus::Stale
    );
    repository
        .control_condition_evaluation(
            &project_id,
            &replacement.id,
            replacement.version,
            ConditionEvaluationControl::Cancel,
        )
        .unwrap();
    execution_checks::verify(
        &path,
        &project_id,
        &target,
        batches[0]["id"].as_str().unwrap(),
        batches[1]["id"].as_str().unwrap(),
    )
    .await;
    let barrier = std::sync::Barrier::new(2);
    let jobs = std::thread::scope(|scope| {
        let handles = (0..2)
            .map(|_| {
                scope.spawn(|| {
                    let mut repository = SqliteStoryRepository::open(&path).unwrap();
                    barrier.wait();
                    repository
                        .start_condition_evaluation(&project_id, target.clone())
                        .unwrap()
                })
            })
            .collect::<Vec<_>>();
        handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(jobs[0], jobs[1]);
    assert_ne!(jobs[0].id, job.id);
    let paused = repository
        .control_condition_evaluation(
            &project_id,
            &jobs[0].id,
            jobs[0].version,
            ConditionEvaluationControl::Pause,
        )
        .unwrap();
    counter["evidence"]["quote"] = json!("损坏来源。");
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute(
            "UPDATE story_modeling_batches_v2 SET assertions_json=?1 WHERE batch_id=?2",
            rusqlite::params![
                json!([counter]).to_string(),
                batches[1]["id"].as_str().unwrap()
            ],
        )
        .unwrap();
    let (status, rejected) = call(&path, "GET", &historical_query, Value::Null, Role::Author).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{rejected}");
    let stale = repository
        .condition_evaluation(&project_id, &paused.id)
        .unwrap();
    assert_eq!(stale.status, ConditionEvaluationStatus::Stale);
    let stale = repository
        .control_condition_evaluation(
            &project_id,
            &paused.id,
            paused.version,
            ConditionEvaluationControl::Resume,
        )
        .unwrap();
    assert_eq!(stale.status, ConditionEvaluationStatus::Stale);
    assert_eq!(
        SqliteStoryRepository::open(&path)
            .unwrap()
            .condition_evaluation(&project_id, &paused.id)
            .unwrap(),
        stale
    );
}
