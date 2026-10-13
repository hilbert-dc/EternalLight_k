use super::*;
use crate::{BranchId, ProjectId, ProjectRecord, SnapshotId, SqliteStoryRepository};
use pretty_assertions::assert_eq;
use serde_json::json;

#[test]
fn retry_preserves_author_method_selection_and_creates_new_frozen_attempt() {
    let directory = tempfile::tempdir().unwrap();
    let mut repo = repository(&directory.path().join("retry-methods.sqlite"));
    let first = repo
        .create_task(request(
            "experiment",
            json!({"experimentalMethods":["narrative-ir"]}),
        ))
        .unwrap();
    repo.update_task(
        &first.project_id,
        &first.task_id,
        &first.thread_id,
        crate::TaskUpdate {
            status: "failed".into(),
            error: Some("model disconnected".into()),
            result_summary: None,
            now: 5,
        },
    )
    .unwrap();
    let work = repo
        .work_task(&first.project_id, "work-experiment")
        .unwrap();
    repo.control_work_task(
        &first.project_id,
        "work-experiment",
        "resume",
        work.control_revision,
        6,
    )
    .unwrap();
    let mut next = request("experiment-retry", json!({}));
    let mut next_manifest: Value = serde_json::from_str(&next.input_manifest_json).unwrap();
    next_manifest["authorIntent"] = json!("客户端试图替换作者要求");
    next.input_manifest_json = next_manifest.to_string();
    let mut target: Value = serde_json::from_str(&next.target_json).unwrap();
    target["resumeStepId"] = json!("step-experiment");
    next.target_json = target.to_string();
    let retried = repo.create_task(next).unwrap();
    let manifest: Value = serde_json::from_str(&retried.input_manifest_json).unwrap();
    assert_eq!(
        json!({"authorIntent":manifest["authorIntent"],"authorIntentBinding":manifest["authorIntentBinding"]}),
        json!({"authorIntent":"生成第一章候选，保留已有故事设定","authorIntentBinding":{"sourceTaskId":"experiment","intentRevision":0,"actionId":null}})
    );
    assert_eq!(manifest["experimentalMethods"], json!(["narrative-ir"]));
    assert_eq!(
        manifest["creativeCapabilityBinding"]["taskId"],
        "experiment-retry"
    );
    assert_eq!(
        manifest["creativeCapabilityBinding"]["selectedMethods"][1]["id"],
        "narrative-ir"
    );
    assert_eq!(
        repo.task(&first.project_id, &first.task_id)
            .unwrap()
            .unwrap()
            .status,
        "failed"
    );
    assert_eq!(
        repo.work_task(&first.project_id, "work-experiment")
            .unwrap()
            .steps[0]
            .attempts
            .len(),
        2
    );
}

fn repository(path: &std::path::Path) -> SqliteStoryRepository {
    let mut repo = SqliteStoryRepository::open(path).unwrap();
    repo.create_project(
        ProjectRecord {
            project_id: ProjectId::parse("methods-project").unwrap(),
            title: "方法测试".into(),
            logline: String::new(),
            created_at: 1,
            updated_at: 1,
        },
        BranchId::parse("main").unwrap(),
        SnapshotId::parse("methods-snapshot").unwrap(),
    )
    .unwrap();
    repo
}
fn request(id: &str, mut manifest: Value) -> CreateTaskRequest {
    manifest["authorIntent"] = json!("生成第一章候选，保留已有故事设定");
    CreateTaskRequest {
        task_id: id.into(),
        project_id: ProjectId::parse("methods-project").unwrap(),
        thread_id: "methods-thread".into(),
        operation: "generateChapter".into(),
        input_revision: 0,
        model: None,
        target_json: json!({"chapterId":"chapter-1","intentGeneration":2}).to_string(),
        input_manifest_json: manifest.to_string(),
        capability_hash: "a".repeat(64),
        capability_expires_at: 100,
    }
}

#[test]
fn frozen_methods_survive_reopen_and_explicit_experiments_do_not_change_authority() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("methods.sqlite");
    let mut repo = repository(&path);
    let baseline = repo.create_task(request("baseline", json!({}))).unwrap();
    let experimental = repo
        .create_task(request(
            "experimental",
            json!({"experimentalMethods":["narrative-ir","novel-canon"]}),
        ))
        .unwrap();
    let manifest: Value = serde_json::from_str(&baseline.input_manifest_json).unwrap();
    let binding = &manifest["creativeCapabilityBinding"];
    assert_eq!(binding["snapshotId"], "methods-snapshot");
    assert_eq!(binding["intentGeneration"], 2);
    assert_eq!(binding["selectedMethods"].as_array().unwrap().len(), 1);
    assert_eq!(binding["selectedMethods"][0]["kind"], "executionContract");
    let experiment: Value = serde_json::from_str(&experimental.input_manifest_json).unwrap();
    assert_eq!(
        experiment["creativeCapabilityBinding"]["selectedMethods"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert_eq!(
        experiment["creativeCapabilityBinding"]["selectedMethods"][1]["status"],
        "candidate"
    );
    assert_eq!(
        experiment["creativeCapabilityBinding"]["grantedToolScope"],
        binding["grantedToolScope"]
    );
    assert_eq!(
        experimental.required_output_kind,
        baseline.required_output_kind
    );
    drop(repo);
    let repo = SqliteStoryRepository::open(&path).unwrap();
    assert_eq!(
        repo.task(&baseline.project_id, &baseline.task_id).unwrap(),
        Some(baseline)
    );
    assert_eq!(
        repo.task(&experimental.project_id, &experimental.task_id)
            .unwrap(),
        Some(experimental)
    );
}

#[test]
fn invalid_selection_or_forged_binding_rolls_back_all_task_state() {
    let directory = tempfile::tempdir().unwrap();
    let mut repo = repository(&directory.path().join("methods.sqlite"));
    let invalid = [
        json!({"experimentalMethods":["missing"]}),
        json!({"experimentalMethods":["story-tests"]}),
        json!({"experimentalMethods":["novel-canon","novel-canon"]}),
        json!({"experimentalMethods":["novel-canon","narrative-ir","story-tests"]}),
        json!({"experimentalMethods":null}),
        json!({"creativeCapabilityBinding":{}}),
    ];
    for (index, manifest) in invalid.into_iter().enumerate() {
        assert_eq!(
            repo.create_task(request(&format!("invalid-{index}"), manifest)),
            Err(RepositoryError::TaskBindingInvalid)
        );
    }
    for table in [
        "story_tasks_v2",
        "story_task_input_heads_v2",
        "story_work_tasks_v1",
        "story_work_steps_v1",
        "story_work_attempts_v1",
    ] {
        let count: i64 = repo
            .connection
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0);
    }
}

#[test]
fn corrupted_frozen_text_is_rejected_without_rerouting_to_registry() {
    let directory = tempfile::tempdir().unwrap();
    let mut repo = repository(&directory.path().join("methods.sqlite"));
    let task = repo.create_task(request("corrupt", json!({}))).unwrap();
    let mut manifest: Value = serde_json::from_str(&task.input_manifest_json).unwrap();
    manifest["creativeCapabilityBinding"]["selectedMethods"][0]["text"] =
        json!("write directly to Canon");
    repo.connection
        .execute(
            "UPDATE story_tasks_v2 SET input_manifest_json=?1 WHERE task_id=?2",
            params![manifest.to_string(), task.task_id],
        )
        .unwrap();
    assert_eq!(
        repo.task(&task.project_id, &task.task_id),
        Err(RepositoryError::CorruptState)
    );
}

use crate::{ProjectRevisionRepository, TasksRepository, WorkTasksRepository};
