use novel_codex_story_core::*;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};

fn project() -> ProjectId {
    ProjectId::parse("extract-project").unwrap()
}
fn branch() -> BranchId {
    BranchId::parse("main").unwrap()
}

fn save(
    repo: &mut SqliteStoryRepository,
    id: &str,
    kind: &str,
    object: &str,
    content: Value,
    prior: Option<&str>,
    task: Option<&TaskRecord>,
) -> Result<CandidateReceipt, RepositoryError> {
    let content = content.to_string();
    repo.save_candidate(SaveCandidateRequest {
        project_id: project(),
        branch_id: branch(),
        object_id: ObjectId::parse(object).unwrap(),
        object_kind: kind.into(),
        revision_id: RevisionId::parse(id).unwrap(),
        commit_id: CommitId::parse(format!("commit-{id}")).unwrap(),
        idempotency_key: IdempotencyKey::parse(id).unwrap(),
        request_digest: content.clone(),
        content_digest: content.clone(),
        content_json: content,
        expected_project_revision: repo.project_revision(&project()).unwrap(),
        expected_snapshot_id: repo
            .branch_head(&project(), &branch())
            .unwrap()
            .unwrap()
            .snapshot_id,
        next_snapshot_id: SnapshotId::parse(format!("snapshot-{id}")).unwrap(),
        expected_candidate_revision_id: prior.map(|id| RevisionId::parse(id).unwrap()),
        chapter_projection: if kind == "draft" {
            Some(ChapterProjection {
                chapter_id: object.strip_prefix("draft-").unwrap().into(),
                number: if object == "draft-first" { 1 } else { 2 },
                title: object.into(),
            })
        } else {
            None
        },
        task_output: task.map(|task| TaskOutputBinding {
            task_id: task.task_id.clone(),
            thread_id: task.thread_id.clone(),
            output_kind: "commitmentExtraction".into(),
            completeness: None,
        }),
        linked_impact_ids: vec![],
    })
}

fn setup() -> (SqliteStoryRepository, TaskRecord) {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    repo.create_project(
        ProjectRecord {
            project_id: project(),
            title: "港城".into(),
            logline: String::new(),
            created_at: 1,
            updated_at: 1,
        },
        branch(),
        SnapshotId::parse("initial").unwrap(),
    )
    .unwrap();
    save(
        &mut repo,
        "source-first",
        "draft",
        "draft-first",
        json!({"chapterNumber":1,"body":"白塔的灯将再亮。"}),
        /*prior*/ None,
        /*task*/ None,
    )
    .unwrap();
    save(
        &mut repo,
        "source-second",
        "draft",
        "draft-second",
        json!({"chapterNumber":2,"body":"未来秘密揭晓了。"}),
        /*prior*/ None,
        /*task*/ None,
    )
    .unwrap();
    let task = repo
        .create_task(CreateTaskRequest {
            task_id: "extract-task".into(),
            project_id: project(),
            thread_id: "extract-thread".into(),
            operation: "extractCommitments".into(),
            input_revision: 0,
            model: None,
            target_json:
                json!({"chapterId":"first","chapterNumber":1,"sourceRevisionId":"source-first"})
                    .to_string(),
            input_manifest_json: "{}".into(),
            capability_hash: "e".repeat(/*n*/ 64),
            capability_expires_at: i64::MAX,
        })
        .unwrap();
    (repo, task)
}

fn content() -> Value {
    json!({"payload":{"chapterId":"first","sourceRevisionId":"source-first","examinedRanges":[{"start":0,"end":8}],
        "candidates":[{"id":"lamp","kind":"foreshadowing","summary":"灯再亮的期待","relatedEntityIds":[],"plotThreadId":null,"characterArcId":null,
            "setupEvidenceRefs":[{"objectId":"draft-first","revisionId":"source-first","quote":"白塔的灯","start":0,"end":4}],
            "fulfillmentCriteria":"后续原文呈现灯重新亮起","targetWindow":{"kind":"open"},"status":"open","fulfillmentEvidenceRefs":[],"decisionReason":null}]}})
}

#[test]
fn extraction_is_a_frozen_multi_candidate_output_not_an_established_commitment() {
    let (mut repo, task) = setup();
    assert_eq!(task.purpose, "critic");
    assert_eq!(
        task.required_output_kind,
        Some("commitmentExtraction".into())
    );
    assert_eq!(
        repo.complete_task(
            &project(),
            &task.task_id,
            &task.thread_id,
            String::new(),
            /*now*/ 5
        ),
        Err(RepositoryError::TaskOutputMissing)
    );
    let manifest: Value = serde_json::from_str(&task.input_manifest_json).unwrap();
    assert_eq!(manifest["outputObjectId"], "commitment-extraction-first");
    let mut report = content();
    let mut second = report["payload"]["candidates"][0].clone();
    second["id"] = json!("second");
    report["payload"]["candidates"]
        .as_array_mut()
        .unwrap()
        .push(second);
    let first = save(
        &mut repo,
        "output",
        "commitmentExtraction",
        "commitment-extraction-first",
        report.clone(),
        /*prior*/ None,
        Some(&task),
    )
    .unwrap();
    let replay = save(
        &mut repo,
        "output",
        "commitmentExtraction",
        "commitment-extraction-first",
        report,
        /*prior*/ None,
        Some(&task),
    )
    .unwrap();
    assert_eq!(first, replay);
    let output = repo
        .commitment_extraction(&project(), "first")
        .unwrap()
        .unwrap();
    assert_eq!(output["payload"]["candidates"].as_array().unwrap().len(), 2);
    assert_eq!(output["authority"], "candidateProposalBatch");
    assert_eq!(output["sourceCurrent"], true);
    assert!(
        repo.object_heads(&project())
            .unwrap()
            .iter()
            .all(|head| head.object_id.as_str() != "commitment-lamp")
    );
    let review = repo
        .create_review(CreateReviewRequest {
            review_id: "extraction-review".into(),
            project_id: project(),
            object_id: ObjectId::parse("commitment-extraction-first").unwrap(),
            object_kind: "commitmentExtraction".into(),
            revision_id: first.revision_id,
            idempotency_key: IdempotencyKey::parse("extraction-review").unwrap(),
            request_digest: "review".into(),
            checks_json: "[]".into(),
            proposals_json: "[]".into(),
            modeling_batch_id: None,
            selections_json: "{}".into(),
        })
        .unwrap();
    assert_eq!(
        repo.accept_review(AcceptReviewRequest {
            project_id: project(),
            branch_id: branch(),
            review_id: review.review_id,
            commit_id: CommitId::parse("forbidden-accept").unwrap(),
            idempotency_key: IdempotencyKey::parse("forbidden-accept").unwrap(),
            request_digest: "accept".into(),
            expected_project_revision: review.project_revision,
            next_snapshot_id: SnapshotId::parse("forbidden-accepted").unwrap(),
            selections_json: "{}".into(),
        }),
        Err(RepositoryError::TaskOutputForbidden)
    );
    repo.complete_task(
        &project(),
        &task.task_id,
        &task.thread_id,
        "提案已保存".into(),
        /*now*/ 6,
    )
    .unwrap();
    assert_eq!(
        repo.task(&project(), &task.task_id)
            .unwrap()
            .unwrap()
            .status,
        "completed"
    );
}

#[test]
fn invalid_ranges_and_source_or_authority_forgery_roll_back_atomically() {
    let (mut repo, task) = setup();
    let before = repo.branch_head(&project(), &branch()).unwrap();
    let mut cases = Vec::new();
    let mut report = content();
    report["payload"]["candidates"][0]["status"] = json!("fulfilled");
    cases.push(report);
    let mut report = content();
    report["payload"]["candidates"][0]["decisionReason"] = json!("模型擅自放弃");
    cases.push(report);
    let mut report = content();
    report["payload"]["candidates"][0]["setupEvidenceRefs"][0]["quote"] = json!("伪造摘录");
    cases.push(report);
    let mut report = content();
    report["payload"]["candidates"][0]["setupEvidenceRefs"][0] = json!({"objectId":"draft-second","revisionId":"source-second","quote":"未来秘密","start":0,"end":4});
    cases.push(report);
    let mut report = content();
    report["payload"]["sourceRevisionId"] = json!("source-second");
    cases.push(report);
    let mut report = content();
    report["payload"]["examinedRanges"] = json!([{"start":4,"end":8}]);
    cases.push(report);
    let mut report = content();
    report["payload"]["examinedRanges"] = json!([{"start":0,"end":4},{"start":3,"end":8}]);
    cases.push(report);
    let mut report = content();
    report["payload"]["examinedRanges"] = json!([{"start":0,"end":100}]);
    cases.push(report);
    let mut report = content();
    let entry = report["payload"]["candidates"][0].clone();
    report["payload"]["candidates"]
        .as_array_mut()
        .unwrap()
        .push(entry);
    cases.push(report);
    for (index, report) in cases.into_iter().enumerate() {
        assert!(
            save(
                &mut repo,
                &format!("invalid-{index}"),
                "commitmentExtraction",
                "commitment-extraction-first",
                report,
                /*prior*/ None,
                Some(&task)
            )
            .is_err()
        );
        assert_eq!(repo.branch_head(&project(), &branch()).unwrap(), before);
        assert_eq!(
            repo.task(&project(), &task.task_id)
                .unwrap()
                .unwrap()
                .output_refs_json,
            "[]"
        );
    }
    assert_eq!(
        save(
            &mut repo,
            "no-task",
            "commitmentExtraction",
            "commitment-extraction-first",
            content(),
            /*prior*/ None,
            /*task*/ None
        ),
        Err(RepositoryError::TaskOutputForbidden)
    );
    assert!(
        save(
            &mut repo,
            "wrong-object",
            "commitmentExtraction",
            "commitment-extraction-second",
            content(),
            /*prior*/ None,
            Some(&task)
        )
        .is_err()
    );
    assert_eq!(repo.branch_head(&project(), &branch()).unwrap(), before);
}

#[test]
fn source_edit_revokes_old_execution_without_rewriting_saved_proposals() {
    let (mut repo, task) = setup();
    save(
        &mut repo,
        "proposal",
        "commitmentExtraction",
        "commitment-extraction-first",
        content(),
        /*prior*/ None,
        Some(&task),
    )
    .unwrap();
    save(
        &mut repo,
        "edited",
        "draft",
        "draft-first",
        json!({"chapterNumber":1,"body":"作者删除了旧伏笔。"}),
        Some("source-first"),
        /*task*/ None,
    )
    .unwrap();
    assert_eq!(
        repo.complete_task(
            &project(),
            &task.task_id,
            &task.thread_id,
            String::new(),
            /*now*/ 6
        ),
        Err(RepositoryError::TaskInputStale)
    );
    let report = repo
        .commitment_extraction(&project(), "first")
        .unwrap()
        .unwrap();
    assert_eq!(report["sourceCurrent"], false);
    assert_eq!(
        report["payload"]["candidates"][0]["setupEvidenceRefs"][0]["quote"],
        "白塔的灯"
    );
}

#[test]
fn empty_extraction_is_valid_and_restart_recovers_only_saved_output() {
    let (mut repo, task) = setup();
    let mut empty = content();
    empty["payload"]["candidates"] = json!([]);
    save(
        &mut repo,
        "empty",
        "commitmentExtraction",
        "commitment-extraction-first",
        empty,
        /*prior*/ None,
        Some(&task),
    )
    .unwrap();
    assert_eq!(
        repo.recover_orphaned_tasks(/*now*/ 9).unwrap(),
        vec![task.task_id.clone()]
    );
    let completed = repo.task(&project(), &task.task_id).unwrap().unwrap();
    assert_eq!(completed.status, "completed");
    assert!(completed.capability_revoked_at.is_some());
    assert_eq!(
        repo.commitment_extraction(&project(), "first")
            .unwrap()
            .unwrap()["coverage"]["semanticCompleteness"],
        "unknown"
    );
}

fn establishment(repo: &SqliteStoryRepository, key: &str) -> EstablishCommitment {
    EstablishCommitment {
        project_id: project(),
        origin: CommitmentExtractionOrigin {
            chapter_id: "first".into(),
            report_revision_id: "proposal".into(),
            proposal_id: "lamp".into(),
        },
        expected_project_revision: repo.project_revision(&project()).unwrap(),
        base_snapshot_id: repo
            .branch_head(&project(), &branch())
            .unwrap()
            .unwrap()
            .snapshot_id,
        idempotency_key: IdempotencyKey::parse(key).unwrap(),
    }
}

#[test]
fn author_establishment_has_one_stable_identity_and_replays_the_original_receipt() {
    let (mut repo, task) = setup();
    save(
        &mut repo,
        "proposal",
        "commitmentExtraction",
        "commitment-extraction-first",
        content(),
        /*prior*/ None,
        Some(&task),
    )
    .unwrap();
    let request = establishment(&repo, "establish-once");
    let snapshot = request.base_snapshot_id.clone();
    let saved = repo.establish_commitment(request).unwrap();
    let mut replay = establishment(&repo, "establish-once");
    replay.base_snapshot_id = snapshot;
    let repeated = repo.establish_commitment(replay).unwrap();
    assert_eq!(
        (repeated.commitment_id, repeated.receipt),
        (saved.commitment_id.clone(), saved.receipt.clone())
    );
    let read = repo
        .narrative_commitment(&project(), &saved.commitment_id, /*snapshot*/ None)
        .unwrap();
    assert_eq!(read["confirmationState"], "candidate");
    assert_eq!(
        repo.commitment_extraction(&project(), "first")
            .unwrap()
            .unwrap()["established"],
        json!([{ "proposalId": "lamp", "commitmentId": saved.commitment_id }])
    );
    assert_eq!(
        read["extractionOrigin"],
        json!({"chapterId":"first","reportRevisionId":"proposal","proposalId":"lamp"})
    );
    let before = repo.branch_head(&project(), &branch()).unwrap();
    assert!(
        repo.establish_commitment(establishment(&repo, "duplicate-choice"))
            .is_err()
    );
    assert_eq!(repo.branch_head(&project(), &branch()).unwrap(), before);
    assert_eq!(
        repo.object_heads(&project())
            .unwrap()
            .iter()
            .filter(|head| head.object_kind == "commitment")
            .count(),
        1
    );
}

#[test]
fn obsolete_or_foreign_report_selection_cannot_establish_an_author_object() {
    let (mut repo, task) = setup();
    save(
        &mut repo,
        "proposal",
        "commitmentExtraction",
        "commitment-extraction-first",
        content(),
        /*prior*/ None,
        Some(&task),
    )
    .unwrap();
    let obsolete = establishment(&repo, "old-choice");
    save(
        &mut repo,
        "edited",
        "draft",
        "draft-first",
        json!({"chapterNumber":1,"body":"作者删除了旧伏笔。"}),
        Some("source-first"),
        /*task*/ None,
    )
    .unwrap();
    let before = repo.branch_head(&project(), &branch()).unwrap();
    assert!(repo.establish_commitment(obsolete).is_err());
    assert_eq!(
        repo.establish_commitment(establishment(&repo, "stale-source"))
            .err(),
        Some(RepositoryError::TaskInputStale)
    );
    for (field, value) in [
        ("chapter", "second"),
        ("proposal", "unknown"),
        ("snapshot", "initial"),
    ] {
        let mut request = establishment(&repo, field);
        match field {
            "chapter" => request.origin.chapter_id = value.into(),
            "proposal" => request.origin.proposal_id = value.into(),
            "snapshot" => request.base_snapshot_id = SnapshotId::parse(value).unwrap(),
            _ => unreachable!(),
        }
        assert!(repo.establish_commitment(request).is_err());
    }
    assert_eq!(repo.branch_head(&project(), &branch()).unwrap(), before);
    assert!(
        repo.object_heads(&project())
            .unwrap()
            .iter()
            .all(|head| head.object_kind != "commitment")
    );
}

#[test]
fn author_can_revise_established_content_but_cannot_delete_or_rebind_its_origin() {
    let (mut repo, task) = setup();
    save(
        &mut repo,
        "proposal",
        "commitmentExtraction",
        "commitment-extraction-first",
        content(),
        /*prior*/ None,
        Some(&task),
    )
    .unwrap();
    let saved = repo
        .establish_commitment(establishment(&repo, "choose"))
        .unwrap();
    let read = repo
        .narrative_commitment(&project(), &saved.commitment_id, /*snapshot*/ None)
        .unwrap();
    let object = format!("commitment-{}", saved.commitment_id);
    let mut revised =
        json!({"payload":read["payload"], "extractionOrigin":read["extractionOrigin"]});
    revised["payload"]["summary"] = json!("作者决定保留悬念，不再安排灯亮");
    revised["payload"]["status"] = json!("abandoned");
    revised["payload"]["decisionReason"] = json!("保持开放结局");
    save(
        &mut repo,
        "author-revised",
        "commitment",
        &object,
        revised.clone(),
        Some(saved.receipt.revision_id.as_str()),
        /*task*/ None,
    )
    .unwrap();
    let before = repo.branch_head(&project(), &branch()).unwrap();
    for (id, origin) in [
        ("drop-origin", Value::Null),
        (
            "rebind-origin",
            json!({"chapterId":"first","reportRevisionId":"proposal","proposalId":"other"}),
        ),
    ] {
        let mut forged = revised.clone();
        forged["extractionOrigin"] = origin;
        assert!(
            save(
                &mut repo,
                id,
                "commitment",
                &object,
                forged,
                Some("author-revised"),
                /*task*/ None
            )
            .is_err()
        );
    }
    assert_eq!(repo.branch_head(&project(), &branch()).unwrap(), before);
    assert_eq!(
        repo.narrative_commitment(&project(), &saved.commitment_id, /*snapshot*/ None)
            .unwrap()["payload"],
        revised["payload"]
    );
    assert_eq!(
        repo.narrative_commitment(
            &project(),
            &saved.commitment_id,
            Some(&saved.receipt.result_snapshot_id)
        )
        .unwrap()["payload"]["status"],
        "open"
    );
}

#[test]
fn replacing_a_report_preserves_established_origin_but_cannot_establish_the_old_selection() {
    let (mut repo, task) = setup();
    save(
        &mut repo,
        "proposal",
        "commitmentExtraction",
        "commitment-extraction-first",
        content(),
        /*prior*/ None,
        Some(&task),
    )
    .unwrap();
    let saved = repo
        .establish_commitment(establishment(&repo, "first-choice"))
        .unwrap();
    let next = repo
        .create_task(CreateTaskRequest {
            task_id: "second-extraction".into(),
            project_id: project(),
            thread_id: "second-thread".into(),
            operation: "extractCommitments".into(),
            input_revision: 0,
            model: None,
            target_json: task.target_json,
            input_manifest_json: "{}".into(),
            capability_hash: "e".repeat(/*n*/ 64),
            capability_expires_at: i64::MAX,
        })
        .unwrap();
    let mut report = content();
    report["payload"]["candidates"][0]["summary"] = json!("新分析仍需作者选择");
    save(
        &mut repo,
        "replacement",
        "commitmentExtraction",
        "commitment-extraction-first",
        report,
        Some("proposal"),
        Some(&next),
    )
    .unwrap();
    assert_eq!(
        repo.establish_commitment(establishment(&repo, "obsolete-report"))
            .err(),
        Some(RepositoryError::CandidateNotFound)
    );
    let read = repo
        .narrative_commitment(&project(), &saved.commitment_id, /*snapshot*/ None)
        .unwrap();
    assert_eq!(read["extractionOrigin"]["reportRevisionId"], "proposal");
    let catalog = repo
        .narrative_commitments_page(&project(), /*cursor*/ None, /*limit*/ 20)
        .unwrap();
    assert_eq!(catalog["data"].as_array().unwrap().len(), 1);
    assert_eq!(catalog["data"][0]["id"], saved.commitment_id);
    assert_eq!(catalog["data"][0]["summary"], read["payload"]["summary"]);
    let mut modified =
        json!({"payload":read["payload"],"extractionOrigin":read["extractionOrigin"]});
    modified["payload"]["summary"] = json!("作者继续维护原来的承诺");
    save(
        &mut repo,
        "after-report-replacement",
        "commitment",
        &format!("commitment-{}", saved.commitment_id),
        modified,
        Some(saved.receipt.revision_id.as_str()),
        /*task*/ None,
    )
    .unwrap();
    assert_eq!(
        repo.narrative_commitment(&project(), &saved.commitment_id, /*snapshot*/ None)
            .unwrap()["extractionOrigin"],
        read["extractionOrigin"]
    );
}
