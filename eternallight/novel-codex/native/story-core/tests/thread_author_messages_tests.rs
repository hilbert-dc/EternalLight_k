use novel_codex_story_core::{
    InitialWorkBudget, BranchId, CreateTaskRequest, ProjectId, ProjectRecord, SnapshotId, SqliteStoryRepository,
};
use pretty_assertions::assert_eq;
use serde_json::json;

#[test]
fn restored_author_history_is_persisted_bounded_and_project_thread_scoped() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("history.sqlite3");
    let project = ProjectId::parse("history-project").unwrap();
    let mut repository = SqliteStoryRepository::open(&path).unwrap();
    repository
        .create_project(
            ProjectRecord {
                project_id: project.clone(),
                title: "对话恢复".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            BranchId::parse("main").unwrap(),
            SnapshotId::parse("initial").unwrap(),
        )
        .unwrap();
    let digest = repository.project_source_watermarks(&project).unwrap()["digest"].as_str().unwrap().to_owned();
    let task = repository
        .create_budgeted_root_task(CreateTaskRequest {
            task_id: "author-task".into(),
            project_id: project.clone(),
            thread_id: "author-thread".into(),
            operation: "advisory".into(),
            input_revision: 0,
            model: None,
            target_json: json!({"type":"project"}).to_string(),
            input_manifest_json: json!({"authorIntent":"分析人物，不要修改正文"}).to_string(),
            capability_hash: "a".repeat(64),
            capability_expires_at: 1000,
        }, &InitialWorkBudget {
            authorization_id: format!("authority-{}", "author-task"), author_message_id: format!("message-{}", "author-task"),
            expected_base_snapshot_id: "initial".into(), expected_source_digest: digest,
            max_requests: None, max_tokens: None, deadline_at: None,
        }, 1)
        .unwrap();
    repository
        .bind_task_turn(&project, &task.task_id, "author-thread", "engine-turn")
        .unwrap();
    repository
        .append_task_intent(
            &project,
            &task.task_id,
            /*expected_intent_revision*/ 0,
            "保留结局",
            /*now*/ 2,
        )
        .unwrap();
    assert_eq!(repository.thread_author_messages(&project, "author-thread", &["engine-turn".into()]).unwrap(),
        json!({"threadId":"author-thread","messages":[{"turnId":"engine-turn","text":"分析人物，不要修改正文","imageAssetRevisionIds":[]}]}));
    repository
        .deliver_task_intent(
            &project,
            &task.task_id,
            /*intent_revision*/ 1,
            /*now*/ 3,
        )
        .unwrap();
    repository
        .complete_task(
            &project,
            &task.task_id,
            "author-thread",
            "完成分析".into(), None,
            /*now*/ 4,
        )
        .unwrap();
    drop(repository);
    let repository = SqliteStoryRepository::open(&path).unwrap();
    let turns = vec!["engine-turn".into(), "unbound-turn".into()];
    assert_eq!(
        repository
            .thread_author_messages(&project, "author-thread", &turns)
            .unwrap(),
        json!({"threadId":"author-thread","messages":[{"turnId":"engine-turn",
            "text":"分析人物，不要修改正文\n\n作者追加要求：\n保留结局","imageAssetRevisionIds":[]}]})
    );
    assert_eq!(
        repository
            .thread_author_messages(&project, "other-thread", &turns)
            .unwrap(),
        json!({"threadId":"other-thread","messages":[]})
    );
    assert_eq!(
        repository
            .thread_author_messages(
                &ProjectId::parse("other-project").unwrap(),
                "author-thread",
                &turns
            )
            .unwrap(),
        json!({"threadId":"author-thread","messages":[]})
    );
    assert!(
        repository
            .thread_author_messages(&project, "author-thread", &vec!["engine-turn".into(); 13])
            .is_err()
    );
}
