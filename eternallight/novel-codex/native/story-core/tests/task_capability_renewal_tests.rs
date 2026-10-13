use novel_codex_story_core::{
    BranchId, CommitId, CreateTaskRequest, IdempotencyKey, InitialWorkBudget, ObjectId, ProjectId,
    ProjectRecord, RenewTaskCapability, RevisionId, SaveCandidateRequest, SnapshotId,
    SqliteStoryRepository, TaskRecord, TaskUpdate,
};
use pretty_assertions::assert_eq;
use serde_json::json;

fn setup() -> (SqliteStoryRepository, TaskRecord) {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let project = ProjectId::parse("renew-project").unwrap();
    repository
        .create_project(
            ProjectRecord {
                project_id: project.clone(),
                title: "长期创作".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            BranchId::parse("main").unwrap(),
            SnapshotId::parse("initial").unwrap(),
        )
        .unwrap();
    let digest = repository.project_source_watermarks(&project).unwrap()["digest"]
        .as_str()
        .unwrap()
        .to_owned();
    let task = repository
        .create_budgeted_root_task(
            CreateTaskRequest {
                task_id: "root".into(),
                project_id: project,
                thread_id: "renew-thread".into(),
                operation: "advisory".into(),
                input_revision: 0,
                model: Some("test-model".into()),
                target_json: json!({"type":"project"}).to_string(),
                input_manifest_json: json!({"authorIntent":"分析当前小说"}).to_string(),
                capability_hash: "a".repeat(64),
                capability_expires_at: 1800,
            },
            &InitialWorkBudget {
                authorization_id: "renew-authority".into(),
                author_message_id: "renew-message".into(),
                expected_base_snapshot_id: "initial".into(),
                expected_source_digest: digest,
                max_requests: None,
                max_tokens: None,
                deadline_at: None,
            },
            /*now*/ 0,
        )
        .unwrap();
    (repository, task)
}

fn renew(
    repository: &mut SqliteStoryRepository,
    task: &TaskRecord,
    now: i64,
) -> Result<TaskRecord, novel_codex_story_core::RepositoryError> {
    repository.renew_task_capability(RenewTaskCapability {
        project_id: &task.project_id,
        task_id: &task.task_id,
        thread_id: &task.thread_id,
        capability_hash: &task.capability_hash,
        expected_expires_at: task.capability_expires_at,
        operation: &task.operation,
        target: &serde_json::from_str(&task.target_json).unwrap(),
        model: task.model.as_deref(),
        now,
    })
}

#[test]
fn long_task_crosses_original_expiry_without_replacing_authority_or_budget() {
    let (mut repository, original) = setup();
    let budget = repository
        .task_provider_budget(&original.project_id, &original.task_id, 0)
        .unwrap();
    let first = renew(&mut repository, &original, 1500).unwrap();
    let mut expected = original.clone();
    expected.capability_expires_at = 3300;
    assert_eq!(first, expected);
    let second = renew(&mut repository, &first, 3000).unwrap();
    expected.capability_expires_at = 4800;
    assert_eq!(second, expected);
    assert_eq!(
        repository
            .authorize_task_capability(
                &original.project_id,
                &original.task_id,
                &original.thread_id,
                0,
                &original.capability_hash,
                3500
            )
            .unwrap(),
        second
    );
    assert_eq!(
        repository
            .task_provider_budget(&original.project_id, &original.task_id, 3500)
            .unwrap(),
        budget
    );
    // A repeated stale renewal cannot overwrite the newer lease generation.
    assert!(renew(&mut repository, &first, 3200).is_err());
    assert_eq!(
        repository
            .task(&original.project_id, &original.task_id)
            .unwrap(),
        Some(second)
    );
}

#[test]
fn expiry_cancellation_and_changed_scope_cannot_be_renewed() {
    for change in [
        "expiry",
        "cancel",
        "thread",
        "model",
        "target",
        "operation",
        "secret",
    ] {
        let (mut repository, original) = setup();
        let mut requested = original.clone();
        match change {
            "cancel" => {
                repository
                    .update_task(
                        &original.project_id,
                        &original.task_id,
                        &original.thread_id,
                        TaskUpdate {
                            status: "cancelled".into(),
                            error: None,
                            result_summary: None,
                            now: 1400,
                        },
                    )
                    .unwrap();
            }
            "thread" => requested.thread_id = "other-thread".into(),
            "model" => requested.model = Some("other-model".into()),
            "target" => requested.target_json = json!({"type":"other"}).to_string(),
            "operation" => requested.operation = "generateBrief".into(),
            "secret" => requested.capability_hash = "b".repeat(64),
            "expiry" => {}
            _ => unreachable!(),
        }
        let before = repository
            .task(&original.project_id, &original.task_id)
            .unwrap();
        assert!(
            renew(
                &mut repository,
                &requested,
                if change == "expiry" { 1800 } else { 1500 }
            )
            .is_err(),
            "{change}"
        );
        assert_eq!(
            repository
                .task(&original.project_id, &original.task_id)
                .unwrap(),
            before
        );
    }
}

#[test]
fn paused_work_cannot_extend_a_live_task_capability() {
    let (mut repository, original) = setup();
    let work_id = format!("work-{}", original.task_id);
    let work = repository
        .work_task(&original.project_id, &work_id)
        .unwrap();
    repository
        .control_work_task(&original.project_id, &work_id, "pause", work.control_revision, 1400)
        .unwrap();
    let before = repository
        .task(&original.project_id, &original.task_id)
        .unwrap();
    assert!(renew(&mut repository, &original, 1500).is_err());
    assert_eq!(
        repository
            .task(&original.project_id, &original.task_id)
            .unwrap(),
        before
    );
}

#[test]
fn candidate_source_change_blocks_renewal_even_when_formal_project_revision_is_unchanged() {
    let (mut repository, original) = setup();
    repository
        .save_candidate(SaveCandidateRequest {
            project_id: original.project_id.clone(),
            branch_id: BranchId::parse("main").unwrap(),
            object_id: ObjectId::parse("artifact-brief").unwrap(),
            object_kind: "brief".into(),
            revision_id: RevisionId::parse("changed-brief").unwrap(),
            commit_id: CommitId::parse("changed-commit").unwrap(),
            idempotency_key: IdempotencyKey::parse("changed-save").unwrap(),
            request_digest: "change".into(),
            expected_project_revision: 0,
            expected_snapshot_id: SnapshotId::parse("initial").unwrap(),
            next_snapshot_id: SnapshotId::parse("changed-snapshot").unwrap(),
            expected_candidate_revision_id: None,
            content_digest: "changed-content".into(),
            content_json: json!({"title":"新故事"}).to_string(),
            chapter_projection: None,
            task_output: None,
            linked_impact_ids: Vec::new(),
        })
        .unwrap();
    assert!(renew(&mut repository, &original, 1500).is_err());
    assert_eq!(
        repository
            .task(&original.project_id, &original.task_id)
            .unwrap(),
        Some(original)
    );
}

use novel_codex_story_core::{InitialWorkBudgetRepository, ProjectRevisionRepository, TaskCapabilityRenewalRepository, TaskInputRepository, TaskProviderBudgetRepository, TasksRepository, WorkTasksRepository};
