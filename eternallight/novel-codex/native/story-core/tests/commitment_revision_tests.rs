use novel_codex_story_core::*;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};

fn project() -> ProjectId {
    ProjectId::parse("revision-project").unwrap()
}
fn branch() -> BranchId {
    BranchId::parse("main").unwrap()
}
fn content(id: &str) -> Value {
    json!({"payload":{"id":id,"kind":"suspense","summary":"灯能否归来","relatedEntityIds":[],"plotThreadId":null,"characterArcId":null,
        "setupEvidenceRefs":[{"objectId":"draft-one","revisionId":"source","quote":"灯将熄灭","start":0,"end":4}],
        "fulfillmentCriteria":"原文明确灯火归来","targetWindow":{"kind":"open"},"status":"open","fulfillmentEvidenceRefs":[],"decisionReason":null},"extractionOrigin":null})
}
fn save_request(
    repo: &SqliteStoryRepository,
    id: &str,
    object: &str,
    kind: &str,
    body: Value,
) -> SaveCandidateRequest {
    SaveCandidateRequest {
        project_id: project(),
        branch_id: branch(),
        object_id: ObjectId::parse(object).unwrap(),
        object_kind: kind.into(),
        revision_id: RevisionId::parse(id).unwrap(),
        commit_id: CommitId::parse(format!("commit-{id}")).unwrap(),
        idempotency_key: IdempotencyKey::parse(id).unwrap(),
        request_digest: body.to_string(),
        content_digest: body.to_string(),
        content_json: body.to_string(),
        expected_project_revision: repo.project_revision(&project()).unwrap(),
        expected_snapshot_id: repo
            .branch_head(&project(), &branch())
            .unwrap()
            .unwrap()
            .snapshot_id,
        next_snapshot_id: SnapshotId::parse(format!("snapshot-{id}")).unwrap(),
        expected_candidate_revision_id: None,
        chapter_projection: None,
        task_output: None,
        linked_impact_ids: vec![],
    }
}
fn setup() -> SqliteStoryRepository {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    repo.create_project(
        ProjectRecord {
            project_id: project(),
            title: "灯".into(),
            logline: String::new(),
            created_at: 1,
            updated_at: 1,
        },
        branch(),
        SnapshotId::parse("initial").unwrap(),
    )
    .unwrap();
    repo.save_candidate(save_request(
        &repo,
        "source",
        "draft-one",
        "draft",
        json!({"chapterNumber":1,"body":"灯将熄灭。灯火归来。"}),
    ))
    .unwrap();
    for id in ["lamp", "other"] {
        repo.save_candidate(save_request(
            &repo,
            &format!("base-{id}"),
            &format!("commitment-{id}"),
            "commitment",
            content(id),
        ))
        .unwrap();
    }
    repo
}
fn task(
    repo: &mut SqliteStoryRepository,
    id: &str,
    operation: &str,
    target: Value,
) -> Result<TaskRecord, RepositoryError> {
    repo.create_task(CreateTaskRequest {
        task_id: id.into(),
        project_id: project(),
        thread_id: format!("thread-{id}"),
        operation: operation.into(),
        input_revision: repo.project_revision(&project()).unwrap(),
        target_json: target.to_string(),
        input_manifest_json: r#"{"authorIntent":"调整叙事承诺"}"#.into(),
        model: None,
        capability_hash: "a".repeat(64),
        capability_expires_at: 100,
    })
}
fn target() -> Value {
    json!({"type":"commitment","commitmentId":"lamp","revisionId":"base-lamp"})
}
fn candidate(
    repo: &SqliteStoryRepository,
    task: &TaskRecord,
    id: &str,
    body: Value,
) -> SaveCandidateRequest {
    let mut request = save_request(repo, id, "commitment-lamp", "commitment", body);
    let manifest: Value = serde_json::from_str(&task.input_manifest_json).unwrap();
    request.expected_candidate_revision_id = manifest["expectedCandidateRevisionId"]
        .as_str()
        .map(|id| RevisionId::parse(id).unwrap());
    request.task_output = Some(TaskOutputBinding {
        task_id: task.task_id.clone(),
        thread_id: task.thread_id.clone(),
        output_kind: "commitment".into(),
        completeness: None,
    });
    request
}

#[test]
fn bound_revision_checkpoints_complete_without_accepting_and_keep_the_original_read_base() {
    let mut repo = setup();
    let discussion = task(
        &mut repo,
        "discussion",
        "advisory",
        json!({"type":"project"}),
    )
    .unwrap();
    assert_eq!(
        repo.task_commitment(&discussion.task_id, "lamp").unwrap()["revisionId"],
        json!("base-lamp")
    );
    let action = repo
        .record_task_action(
            &project(),
            "discussion",
            "adjust",
            TaskActionRequest {
                id: "adjust".into(),
                source_task_id: "discussion".into(),
                operation: "reviseCommitment".into(),
                target: target(),
                instruction: "重新表述灯火的承诺".into(),
                input_revision: 0,
                intent_revision: 0,
                status: "proposed".into(),
                result_task_id: None,
                error: None,
            },
        )
        .unwrap();
    repo.complete_task(
        &project(),
        "discussion",
        "thread-discussion",
        "登记候选".into(), None,
        /*now*/ 2,
    )
    .unwrap();
    repo.approve_task_action(&project(), "discussion", "adjust", 0, 2)
        .unwrap();
    let mut bound = action.target;
    bound["requestedByTaskId"] = json!("discussion");
    bound["requestedActionId"] = json!("adjust");
    let task = task(&mut repo, "revise", "reviseCommitment", bound).unwrap();
    assert_eq!(
        repo.complete_task(
            &project(),
            "revise",
            "thread-revise",
            String::new(), None,
            /*now*/ 3
        ),
        Err(RepositoryError::TaskOutputMissing)
    );
    let mut body = content("lamp");
    body["payload"]["summary"] = json!("灯火归来的悬念");
    let first = candidate(&repo, &task, "candidate-one", body.clone());
    let receipt = repo.save_candidate(first.clone()).unwrap();
    assert_eq!(repo.save_candidate(first).unwrap(), receipt);
    assert_eq!(
        repo.task_commitment("revise", "lamp").unwrap()["payload"],
        content("lamp")["payload"]
    );
    assert_eq!(
        repo.task_commitment("revise", "other"),
        Err(RepositoryError::TaskOutputForbidden)
    );
    body["payload"]["fulfillmentCriteria"] = json!("正文写明灯火重亮");
    let mut second = candidate(&repo, &task, "candidate-two", body);
    second.expected_candidate_revision_id = Some(receipt.revision_id);
    repo.save_candidate(second).unwrap();
    let complete = repo
        .complete_task(
            &project(),
            "revise",
            "thread-revise",
            "候选已保存".into(), None,
            /*now*/ 4,
        )
        .unwrap();
    assert_eq!(
        (complete.status.as_str(), complete.delivery_status.as_str()),
        ("completed", "ready_for_review")
    );
    let head = repo
        .object_heads(&project())
        .unwrap()
        .into_iter()
        .find(|head| head.object_id.as_str() == "commitment-lamp")
        .unwrap();
    assert_eq!(head.accepted_revision_id, None);
}

#[test]
fn invalid_targets_origins_evidence_and_other_operations_cannot_write() {
    let mut repo = setup();
    let mut invalid = target();
    invalid["revisionId"] = json!("missing");
    assert_eq!(
        task(&mut repo, "bad-target", "reviseCommitment", invalid),
        Err(RepositoryError::TaskInputStale)
    );
    let revision = task(&mut repo, "revise", "reviseCommitment", target()).unwrap();
    let before = repo.branch_head(&project(), &branch()).unwrap();
    let mut invalid = content("lamp");
    invalid["extractionOrigin"] =
        json!({"chapterId":"one","reportRevisionId":"report","proposalId":"fake"});
    assert_eq!(
        repo.save_candidate(candidate(&repo, &revision, "bad-origin", invalid)),
        Err(RepositoryError::TaskOutputForbidden)
    );
    let mut invalid = content("lamp");
    invalid["payload"]["setupEvidenceRefs"][0]["quote"] = json!("假引用");
    assert!(matches!(
        repo.save_candidate(candidate(&repo, &revision, "bad-evidence", invalid)),
        Err(RepositoryError::InvalidContent(_))
    ));
    let mut other = candidate(&repo, &revision, "bad-object", content("other"));
    other.object_id = ObjectId::parse("commitment-other").unwrap();
    assert!(repo.save_candidate(other).is_err());
    let discussion = task(&mut repo, "advisory", "advisory", json!({"type":"project"})).unwrap();
    assert_eq!(
        repo.save_candidate(candidate(
            &repo,
            &discussion,
            "bad-operation",
            content("lamp")
        )),
        Err(RepositoryError::TaskOutputForbidden)
    );
    let writer = task(
        &mut repo,
        "writer",
        "generateChapter",
        json!({"type":"chapter","chapterId":"two"}),
    )
    .unwrap();
    assert_eq!(
        repo.task_commitment(&writer.task_id, "lamp"),
        Err(RepositoryError::TaskOutputForbidden)
    );
    assert_eq!(repo.branch_head(&project(), &branch()).unwrap(), before);
}

#[test]
fn foreign_candidate_changes_invalidate_reads_and_new_writes_but_not_a_saved_receipt() {
    let mut repo = setup();
    let revision = task(&mut repo, "revise", "reviseCommitment", target()).unwrap();
    let request = candidate(&repo, &revision, "saved", content("lamp"));
    let receipt = repo.save_candidate(request.clone()).unwrap();
    let mut foreign = save_request(
        &repo,
        "foreign",
        "commitment-other",
        "commitment",
        content("other"),
    );
    foreign.expected_candidate_revision_id = Some(RevisionId::parse("base-other").unwrap());
    repo.save_candidate(foreign).unwrap();
    assert_eq!(
        repo.task_commitment("revise", "lamp"),
        Err(RepositoryError::TaskInputStale)
    );
    assert_eq!(repo.save_candidate(request).unwrap(), receipt);
    let next = candidate(&repo, &revision, "next", content("lamp"));
    assert_eq!(
        repo.save_candidate(next),
        Err(RepositoryError::TaskInputStale)
    );
}

#[test]
fn revising_a_confirmed_commitment_preserves_acceptance_and_cancel_revokes_further_work() {
    let mut repo = setup();
    let review = repo
        .create_review(CreateReviewRequest {
            review_id: "review-base".into(),
            project_id: project(),
            object_id: ObjectId::parse("commitment-lamp").unwrap(),
            object_kind: "commitment".into(),
            revision_id: RevisionId::parse("base-lamp").unwrap(),
            idempotency_key: IdempotencyKey::parse("review-base").unwrap(),
            request_digest: "review-base".into(),
            checks_json: "[]".into(),
            proposals_json: "[]".into(),
            modeling_batch_id: None,
            selections_json: "{}".into(),
        })
        .unwrap();
    repo.accept_review(AcceptReviewRequest {
        project_id: project(),
        branch_id: branch(),
        review_id: review.review_id,
        commit_id: CommitId::parse("accept-base").unwrap(),
        idempotency_key: IdempotencyKey::parse("accept-base").unwrap(),
        request_digest: "accept-base".into(),
        expected_project_revision: review.project_revision,
        next_snapshot_id: SnapshotId::parse("accepted-base").unwrap(),
        selections_json: "{}".into(),
    })
    .unwrap();
    let task = task(&mut repo, "revise", "reviseCommitment", target()).unwrap();
    repo.save_candidate(candidate(&repo, &task, "revised", content("lamp")))
        .unwrap();
    let head = repo
        .object_heads(&project())
        .unwrap()
        .into_iter()
        .find(|head| head.object_id.as_str() == "commitment-lamp")
        .unwrap();
    assert_eq!(
        (head.accepted_revision_id, head.candidate_revision_id),
        (
            Some(RevisionId::parse("base-lamp").unwrap()),
            Some(RevisionId::parse("revised").unwrap())
        )
    );
    let work = repo.work_task_for_attempt(&project(), "revise").unwrap();
    repo.control_work_task(&project(), &work.id, "cancel", work.control_revision, /*now*/ 5)
        .unwrap();
    assert_eq!(
        repo.task_commitment("revise", "lamp"),
        Err(RepositoryError::TaskNotWritable)
    );
    assert_eq!(
        repo.save_candidate(candidate(&repo, &task, "after-cancel", content("lamp"))),
        Err(RepositoryError::TaskNotWritable)
    );
}
