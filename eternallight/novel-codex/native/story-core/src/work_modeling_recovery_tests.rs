use crate::*;
use pretty_assertions::assert_eq;
use serde_json::json;

fn automatic_attempt() -> (SqliteStoryRepository, ProjectId, String) {
    let (mut repo, project) = crate::modeling_dispatch::tests::source_repository();
    repo.connection
        .execute(
            "UPDATE story_modeling_outbox_v1 SET first_dirty_ms=0,last_dirty_ms=0",
            [],
        )
        .unwrap();
    let action = repo
        .dispatch_modeling_intent(&project, 1000)
        .unwrap()
        .unwrap();
    let work = repo
        .work_task(&project, action["workTaskId"].as_str().unwrap())
        .unwrap();
    let mut target = work.steps[0].target.clone();
    let batch = target["batchId"].as_str().unwrap().to_owned();
    target["resumeStepId"] = json!(work.steps[0].id);
    repo.create_task(CreateTaskRequest {
        task_id: "model".into(),
        project_id: project.clone(),
        thread_id: "root".into(),
        operation: "modelKnowledge".into(),
        input_revision: 0,
        model: None,
        target_json: target.to_string(),
        input_manifest_json: "{}".into(),
        capability_hash: "a".repeat(64),
        capability_expires_at: i64::MAX,
    })
    .unwrap();
    (repo, project, batch)
}

fn failed_publication() -> (SqliteStoryRepository, ProjectId, String, WorkTask) {
    let (mut repo, project, batch) = automatic_attempt();
    repo.bind_recovery_operation(RecoveryOperationBinding {
        project_id: project.clone(),
        task_id: "model".into(),
        thread_id: "root".into(),
        turn_id: "turn".into(),
        call_id: "publish".into(),
        tool_name: "publish_knowledge_batch".into(),
        request_hash: "b".repeat(64),
        idempotency_ref: None,
        now: 2,
    })
    .unwrap();
    repo.publish_modeling_batch(PublishModelingBatch {
        project_id: &project,
        batch_id: &batch,
        task_id: "model",
        thread_id: "root",
        entities: &[],
        assertions: &[],
        now: 2,
    })
    .unwrap();
    repo.update_task(
        &project,
        "model",
        "root",
        TaskUpdate {
            status: "failed".into(),
            error: Some("provider_output_limit".into()),
            result_summary: None,
            now: 3,
        },
    )
    .unwrap();
    let work = repo.work_task_for_attempt(&project, "model").unwrap();
    (repo, project, batch, work)
}

#[test]
fn author_resume_recovers_committed_modeling_without_republication_or_a_new_attempt() {
    let (mut repo, project, batch, work) = failed_publication();
    assert_eq!(
        (work.status.as_str(), work.resume_block_reason.as_deref()),
        ("blocked", None)
    );
    let saved = repo.modeling_batch(&project, &batch).unwrap().as_json();
    let recovered = repo
        .control_work_task(&project, &work.id, "resume", work.version, 4)
        .unwrap();
    assert_eq!(
        (
            recovered.status.as_str(),
            recovered.steps[0].status.as_str(),
            recovered.steps[0].attempts.len()
        ),
        ("completed", "completed", 1)
    );
    let task = repo.task(&project, "model").unwrap().unwrap();
    assert_eq!(
        (
            task.status.as_str(),
            task.error.as_deref(),
            task.capability_revoked_at
        ),
        ("completed", Some("provider_output_limit"), Some(4))
    );
    assert_eq!(
        repo.modeling_batch(&project, &batch).unwrap().as_json(),
        saved
    );
    let operations: Vec<(String, String)> = repo
        .connection
        .prepare("SELECT tool_name,state FROM story_tool_operations_v1")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        operations,
        vec![("publish_knowledge_batch".into(), "committed".into())]
    );
    assert!(matches!(
        repo.control_work_task(&project, &work.id, "resume", work.version, 5),
        Err(RepositoryError::WorkTaskStale)
    ));
}

#[test]
fn unpublished_failure_can_resume_as_a_new_attempt_without_changing_the_source() {
    let (mut repo, project, batch) = automatic_attempt();
    repo.update_task(
        &project,
        "model",
        "root",
        TaskUpdate {
            status: "failed".into(),
            error: Some("provider_output_limit".into()),
            result_summary: None,
            now: 2,
        },
    )
    .unwrap();
    let work = repo.work_task_for_attempt(&project, "model").unwrap();
    let resumed = repo
        .control_work_task(&project, &work.id, "resume", work.version, 3)
        .unwrap();
    assert_eq!(
        (resumed.status.as_str(), resumed.steps[0].status.as_str()),
        ("running", "queued")
    );
    let mut target = resumed.steps[0].target.clone();
    target["resumeStepId"] = json!(resumed.steps[0].id);
    repo.create_task(CreateTaskRequest {
        task_id: "second".into(),
        project_id: project.clone(),
        thread_id: "second-root".into(),
        operation: "modelKnowledge".into(),
        input_revision: 0,
        model: None,
        target_json: target.to_string(),
        input_manifest_json: "{}".into(),
        capability_hash: "c".repeat(64),
        capability_expires_at: i64::MAX,
    })
    .unwrap();
    repo.publish_modeling_batch(PublishModelingBatch {
        project_id: &project,
        batch_id: &batch,
        task_id: "second",
        thread_id: "second-root",
        entities: &[],
        assertions: &[],
        now: 4,
    })
    .unwrap();
    repo.complete_task(
        &project,
        "second",
        "second-root",
        "已完成恢复建模".into(),
        5,
    )
    .unwrap();
    let completed = repo.work_task_for_attempt(&project, "second").unwrap();
    assert_eq!(
        (completed.status.as_str(), completed.steps[0].attempts.len()),
        ("completed", 2)
    );
    assert_eq!(
        repo.task(&project, "model").unwrap().unwrap().status,
        "failed"
    );
}

#[test]
fn accepted_unknown_write_cannot_be_recovered_as_completed_knowledge() {
    let (mut repo, project, _batch, work) = failed_publication();
    repo.connection
        .execute(
            "UPDATE story_tool_operations_v1 SET state='accepted',receipt_ref=NULL",
            [],
        )
        .unwrap();
    let before = repo.work_task_for_attempt(&project, "model").unwrap();
    assert_eq!(
        before.resume_block_reason.as_deref(),
        Some("pendingRecovery")
    );
    assert!(matches!(
        repo.control_work_task(&project, &work.id, "resume", work.version, 4),
        Err(RepositoryError::TaskInputStale)
    ));
    assert_eq!(
        repo.work_task_for_attempt(&project, "model").unwrap(),
        before
    );
    assert_eq!(
        repo.task(&project, "model").unwrap().unwrap().status,
        "failed"
    );
    assert_eq!(
        repo.connection
            .query_row("SELECT state FROM story_tool_operations_v1", [], |row| {
                row.get::<_, String>(0)
            })
            .unwrap(),
        "accepted"
    );
}

#[test]
fn stale_publication_does_not_bypass_author_resume_source_checks() {
    let (mut repo, project, _batch, work) = failed_publication();
    repo.connection
        .execute(
            "UPDATE story_workspaces_v2 SET revision=revision+1 WHERE project_id=?1",
            [project.as_str()],
        )
        .unwrap();
    let before = repo.work_task_for_attempt(&project, "model").unwrap();
    assert!(matches!(
        repo.control_work_task(&project, &work.id, "resume", before.version, 4),
        Err(RepositoryError::TaskInputStale)
    ));
    assert_eq!(
        repo.work_task_for_attempt(&project, "model").unwrap(),
        before
    );
}

#[test]
fn pausing_an_active_attempt_does_not_hide_an_unknown_write_on_resume() {
    let (mut repo, project, _batch) = automatic_attempt();
    repo.bind_recovery_operation(RecoveryOperationBinding {
        project_id: project.clone(),
        task_id: "model".into(),
        thread_id: "root".into(),
        turn_id: "turn".into(),
        call_id: "unknown".into(),
        tool_name: "publish_knowledge_batch".into(),
        request_hash: "b".repeat(64),
        idempotency_ref: None,
        now: 2,
    })
    .unwrap();
    let work = repo.work_task_for_attempt(&project, "model").unwrap();
    let paused = repo
        .control_work_task(&project, &work.id, "pause", work.version, 3)
        .unwrap();
    assert_eq!(
        paused.resume_block_reason.as_deref(),
        Some("pendingRecovery")
    );
    assert!(matches!(
        repo.control_work_task(&project, &work.id, "resume", paused.version, 4),
        Err(RepositoryError::TaskInputStale)
    ));
    assert_eq!(
        repo.work_task_for_attempt(&project, "model").unwrap(),
        paused
    );
}

#[test]
fn a_ready_batch_without_its_active_knowledge_head_cannot_satisfy_resume() {
    let (mut repo, project, batch, work) = failed_publication();
    repo.connection
        .execute(
            "DELETE FROM story_knowledge_heads_v2 WHERE batch_id=?1",
            [batch],
        )
        .unwrap();
    let before = repo.work_task_for_attempt(&project, "model").unwrap();
    assert_eq!(before.resume_block_reason.as_deref(), Some("inputChanged"));
    assert!(matches!(
        repo.control_work_task(&project, &work.id, "resume", before.version, 4),
        Err(RepositoryError::TaskInputStale)
    ));
    assert_eq!(
        repo.work_task_for_attempt(&project, "model").unwrap(),
        before
    );
}
