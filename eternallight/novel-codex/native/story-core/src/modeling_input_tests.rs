use crate::*;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};

fn publication_binding(project: &ProjectId, call: &str) -> RecoveryOperationBinding {
    RecoveryOperationBinding {
        project_id: project.clone(),
        task_id: "model".into(),
        thread_id: "root".into(),
        turn_id: "turn".into(),
        call_id: call.into(),
        tool_name: "publish_knowledge_batch".into(),
        request_hash: "b".repeat(64),
        idempotency_ref: None,
        now: 2,
    }
}

fn operation_state(repo: &SqliteStoryRepository) -> String {
    repo.connection
        .query_row("SELECT state FROM story_tool_operations_v1", [], |row| {
            row.get(0)
        })
        .unwrap()
}

fn setup() -> (SqliteStoryRepository, ProjectId, String) {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    let project = ProjectId::parse("model-input").unwrap();
    repo.create_project(
        ProjectRecord {
            project_id: project.clone(),
            title: "输入契约".into(),
            logline: String::new(),
            created_at: 1,
            updated_at: 1,
        },
        BranchId::parse("main").unwrap(),
        SnapshotId::parse("initial").unwrap(),
    )
    .unwrap();
    repo.save_candidate(SaveCandidateRequest {
        project_id: project.clone(),
        branch_id: BranchId::parse("main").unwrap(),
        object_id: ObjectId::parse("draft-one").unwrap(),
        object_kind: "draft".into(),
        revision_id: RevisionId::parse("source").unwrap(),
        commit_id: CommitId::parse("save-source").unwrap(),
        idempotency_key: IdempotencyKey::parse("source").unwrap(),
        request_digest: "source".into(),
        content_digest: "source".into(),
        content_json: json!({"chapterNumber":1,"title":"信",
            "body":"🌙林夕藏信，准备作证。信"})
        .to_string(),
        expected_project_revision: 0,
        expected_snapshot_id: SnapshotId::parse("initial").unwrap(),
        next_snapshot_id: SnapshotId::parse("saved").unwrap(),
        expected_candidate_revision_id: None,
        chapter_projection: Some(ChapterProjection {
            chapter_id: "one".into(),
            number: 1,
            title: "信".into(),
        }),
        task_output: None,
        linked_impact_ids: vec![],
    })
    .unwrap();
    let batch = repo
        .begin_modeling_batch(BeginModelingBatch {
            project_id: &project,
            object_id: &ObjectId::parse("draft-one").unwrap(),
            source_revision_id: &RevisionId::parse("source").unwrap(),
            expected_revision: 0,
            extractor_version: "input-contract",
            now: 1,
        })
        .unwrap();
    repo.create_task(CreateTaskRequest {
        task_id: "model".into(),
        project_id: project.clone(),
        thread_id: "root".into(),
        operation: "modelKnowledge".into(),
        input_revision: 0,
        model: None,
        target_json: json!({"batchId":batch.id}).to_string(),
        input_manifest_json: "{}".into(),
        capability_hash: "a".repeat(64),
        capability_expires_at: i64::MAX,
    })
    .unwrap();
    (repo, project, batch.id)
}

#[test]
fn omitted_aliases_follow_the_optional_tool_contract_and_replay_canonically() {
    let (mut repo, project, batch) = setup();
    let entities = [json!({"id":"linxi","kind":"character","displayName":"林夕",
        "evidence":{"quote":"林夕","start":1,"end":3}})];
    let publish = |repo: &mut SqliteStoryRepository| {
        repo.publish_modeling_batch(PublishModelingBatch {
            project_id: &project,
            batch_id: &batch,
            task_id: "model",
            thread_id: "root",
            entities: &entities,
            assertions: &[],
            now: 2,
        })
    };
    let first = publish(&mut repo).unwrap().as_json();
    assert_eq!(
        first["entities"],
        json!([{ "id":"linxi","kind":"character","displayName":"林夕",
        "aliases":[],"evidence":{"quote":"林夕","start":1,"end":3}}])
    );
    assert_eq!(publish(&mut repo).unwrap().as_json(), first);
}

#[test]
fn quoted_knowledge_is_anchored_to_exact_bound_source_without_model_counting() {
    let (mut repo, project, batch) = setup();
    let entities =
        [json!({"id":"linxi","kind":"character","displayName":"林夕","evidence":{"quote":"林夕"}})];
    let assertions = [
        json!({"kind":"assertion","subjectId":"linxi","predicate":"intent",
        "value":"testify","evidence":{"quote":"准备作证"}}),
    ];
    let saved = repo
        .publish_modeling_batch(PublishModelingBatch {
            project_id: &project,
            batch_id: &batch,
            task_id: "model",
            thread_id: "root",
            entities: &entities,
            assertions: &assertions,
            now: 2,
        })
        .unwrap();
    assert_eq!(
        saved.entities[0]["evidence"],
        json!({"quote":"林夕","start":1,"end":3})
    );
    assert_eq!(
        saved.assertions[0]["evidence"],
        json!({"quote":"准备作证","start":6,"end":10})
    );
}

#[test]
fn ambiguous_wrong_and_partial_anchors_reject_without_publishing_any_claim() {
    let (mut repo, project, batch) = setup();
    for evidence in [
        json!({"quote":"信"}),
        json!({"quote":"林夕","start":2,"end":4}),
        json!({"quote":"林夕","start":1}),
        json!({"quote":"别处正文"}),
        json!({"quote":"林夕","sourceRevisionId":"forged"}),
    ] {
        let entities =
            [json!({"id":"linxi","kind":"character","displayName":"林夕","evidence":evidence})];
        assert!(matches!(
            repo.publish_modeling_batch(PublishModelingBatch {
                project_id: &project,
                batch_id: &batch,
                task_id: "model",
                thread_id: "root",
                entities: &entities,
                assertions: &[],
                now: 2
            }),
            Err(RepositoryError::InvalidContent(_))
        ));
        let saved = repo.modeling_batch(&project, &batch).unwrap();
        assert_eq!(
            (saved.status.as_str(), saved.entities, saved.assertions),
            ("queued", Vec::<Value>::new(), Vec::<Value>::new())
        );
    }
}

#[test]
fn changed_republication_is_rejected_durably_and_does_not_block_completion() {
    let (mut repo, project, batch) = setup();
    let original =
        [json!({"id":"linxi","kind":"character","displayName":"林夕","evidence":{"quote":"林夕"}})];
    let binding = |call: &str| RecoveryOperationBinding {
        project_id: project.clone(),
        task_id: "model".into(),
        thread_id: "root".into(),
        turn_id: "turn".into(),
        call_id: call.into(),
        tool_name: "publish_knowledge_batch".into(),
        request_hash: "b".repeat(64),
        idempotency_ref: None,
        now: 2,
    };
    repo.bind_recovery_operation(binding("first")).unwrap();
    let saved = repo
        .publish_modeling_batch(PublishModelingBatch {
            project_id: &project,
            batch_id: &batch,
            task_id: "model",
            thread_id: "root",
            entities: &original,
            assertions: &[],
            now: 2,
        })
        .unwrap()
        .as_json();
    repo.bind_recovery_operation(binding("changed")).unwrap();
    let changed = [
        json!({"id":"linxi","kind":"character","displayName":"林夕","aliases":["抄写员"],"evidence":{"quote":"林夕"}}),
    ];
    assert!(matches!(
        repo.publish_modeling_batch(PublishModelingBatch {
            project_id: &project,
            batch_id: &batch,
            task_id: "model",
            thread_id: "root",
            entities: &changed,
            assertions: &[],
            now: 3,
        }),
        Err(RepositoryError::IdempotencyConflict)
    ));
    assert_eq!(
        repo.modeling_batch(&project, &batch).unwrap().as_json(),
        saved
    );
    let states = repo
        .connection
        .prepare("SELECT call_id,state FROM story_tool_operations_v1 ORDER BY call_id")
        .unwrap()
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(
        states,
        vec![
            ("changed".into(), "rejected".into()),
            ("first".into(), "committed".into())
        ]
    );
    let mut completion = binding("complete");
    completion.tool_name = "complete_task".into();
    repo.bind_recovery_operation(completion).unwrap();
    assert_eq!(
        repo.complete_task(&project, "model", "root", "已发布候选知识".into(), 4)
            .unwrap()
            .status,
        "completed"
    );
}

#[test]
fn prepublication_limits_and_external_event_rejections_close_the_operation_ledger() {
    let character =
        json!({"id":"linxi","kind":"character","displayName":"林夕","evidence":{"quote":"林夕"}});
    let event =
        json!({"id":"hide","kind":"event","displayName":"藏信","evidence":{"quote":"藏信"}});
    let claim = json!({"kind":"event","subjectId":"hide","predicate":"hide","storyTime":1,
        "value":{"participants":[{"role":"actor","entityId":"linxi"}],
            "preconditions":[{"sourceBatchId":"absent-prior","assertionIndex":0}],
            "effects":[],"untilExclusive":null},"evidence":{"quote":"藏信"}});
    for (entities, assertions, message) in [
        (
            vec![character.clone(); 101],
            vec![],
            "too many knowledge or entity candidates",
        ),
        (
            vec![character, event],
            vec![claim],
            "event prerequisite must reference an exact non-event claim in an authorized prior chapter; use null for this batch",
        ),
    ] {
        let (mut repo, project, batch) = setup();
        repo.bind_recovery_operation(publication_binding(&project, "invalid"))
            .unwrap();
        let result = repo.publish_modeling_batch(PublishModelingBatch {
            project_id: &project,
            batch_id: &batch,
            task_id: "model",
            thread_id: "root",
            entities: &entities,
            assertions: &assertions,
            now: 2,
        });
        assert!(
            matches!(result, Err(RepositoryError::InvalidContent(ref detail)) if detail == message),
            "{:?}",
            result.err()
        );
        let saved = repo.modeling_batch(&project, &batch).unwrap();
        assert_eq!(
            (
                operation_state(&repo),
                saved.status,
                saved.entities,
                saved.assertions
            ),
            (
                "rejected".into(),
                "queued".into(),
                Vec::<Value>::new(),
                Vec::<Value>::new()
            )
        );
    }
}

#[test]
fn storage_failure_retains_uncertainty_and_replay_can_commit_after_rollback() {
    let (mut repo, project, batch) = setup();
    repo.bind_recovery_operation(publication_binding(&project, "storage"))
        .unwrap();
    repo.connection.execute_batch("CREATE TRIGGER fixture_storage_failure BEFORE INSERT ON story_entity_claims_v2 BEGIN SELECT RAISE(ABORT,'fixture storage failure'); END;").unwrap();
    let entities =
        [json!({"id":"linxi","kind":"character","displayName":"林夕","evidence":{"quote":"林夕"}})];
    let publish = |repo: &mut SqliteStoryRepository| {
        repo.publish_modeling_batch(PublishModelingBatch {
            project_id: &project,
            batch_id: &batch,
            task_id: "model",
            thread_id: "root",
            entities: &entities,
            assertions: &[],
            now: 2,
        })
    };
    assert!(matches!(
        publish(&mut repo),
        Err(RepositoryError::Storage(_))
    ));
    let saved = repo.modeling_batch(&project, &batch).unwrap();
    assert_eq!(
        (operation_state(&repo), saved.status, saved.entities),
        ("accepted".into(), "queued".into(), Vec::<Value>::new())
    );
    repo.connection
        .execute_batch("DROP TRIGGER fixture_storage_failure")
        .unwrap();
    assert_eq!(
        (publish(&mut repo).unwrap().status, operation_state(&repo)),
        ("ready".into(), "committed".into())
    );
}

#[test]
fn unknown_batch_identity_is_a_proven_rejection_not_an_unfinished_write() {
    let (mut repo, project, batch) = setup();
    repo.bind_recovery_operation(publication_binding(&project, "absent"))
        .unwrap();
    assert!(matches!(
        repo.publish_modeling_batch(PublishModelingBatch {
            project_id: &project,
            batch_id: "absent",
            task_id: "model",
            thread_id: "root",
            entities: &[],
            assertions: &[],
            now: 2,
        }),
        Err(RepositoryError::CandidateNotFound)
    ));
    assert_eq!(
        (
            operation_state(&repo),
            repo.modeling_batch(&project, &batch).unwrap().status
        ),
        ("rejected".into(), "queued".into())
    );
}
