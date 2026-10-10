use novel_codex_story_core::{
    InitialWorkBudget, BranchId, CreateConversationRequest, CreateTaskRequest, ProjectId, ProjectRecord, SnapshotId,
    SqliteStoryRepository,
};
use serde_json::json;

fn fixture() -> (SqliteStoryRepository, ProjectId) {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let project = ProjectId::parse("branch-project").unwrap();
    repository
        .create_project(
            ProjectRecord {
                project_id: project.clone(),
                title: "分支原文".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            BranchId::parse("main").unwrap(),
            SnapshotId::parse("initial").unwrap(),
        )
        .unwrap();
    (repository, project)
}
fn register(
    repository: &mut SqliteStoryRepository,
    project: &ProjectId,
    thread: &str,
    source: Option<&str>,
    boundary: Option<&str>,
) {
    repository
        .create_conversation(
            project,
            CreateConversationRequest {
                scope_id: None,
                thread_id: thread.into(),
                title: "讨论".into(),
                forked_from_id: source.map(str::to_owned),
                last_turn_id: boundary.map(str::to_owned),
            },
            1,
        )
        .unwrap();
}
fn author(
    repository: &mut SqliteStoryRepository,
    project: &ProjectId,
    thread: &str,
    turn: &str,
    text: &str,
) {
    let id = format!("task-{thread}-{turn}");
    let digest = repository.project_source_watermarks(&project).unwrap()["digest"].as_str().unwrap().to_owned();
    let task = repository
        .create_budgeted_root_task(CreateTaskRequest {
            task_id: id.clone(),
            project_id: project.clone(),
            thread_id: thread.into(),
            operation: "advisory".into(),
            input_revision: 0,
            model: None,
            target_json: json!({"type":"project"}).to_string(),
            input_manifest_json: json!({"authorIntent":text}).to_string(),
            capability_hash: "a".repeat(64),
            capability_expires_at: 1000,
        }, &InitialWorkBudget {
            authorization_id: format!("authority-{}", id.as_str()), author_message_id: format!("message-{}", id.as_str()),
            expected_base_snapshot_id: "initial".into(), expected_source_digest: digest,
            max_requests: None, max_tokens: None, deadline_at: None,
        }, 1)
        .unwrap();
    repository
        .bind_task_turn(project, &task.task_id, thread, turn)
        .unwrap();
}
fn messages(
    repository: &SqliteStoryRepository,
    project: &ProjectId,
    thread: &str,
    turns: &[&str],
) -> serde_json::Value {
    repository
        .thread_author_messages(
            project,
            thread,
            &turns
                .iter()
                .map(|turn| (*turn).to_owned())
                .collect::<Vec<_>>(),
        )
        .unwrap()["messages"]
        .clone()
}

#[test]
fn branch_restores_exact_author_messages_only_through_selected_source_turn() {
    let (mut repository, project) = fixture();
    register(&mut repository, &project, "source", None, None);
    author(&mut repository, &project, "source", "before", "第一条原文");
    author(
        &mut repository,
        &project,
        "source",
        "selected",
        "截至本条，保留空格  😀",
    );
    author(&mut repository, &project, "source", "after", "不能进入分支");
    register(
        &mut repository,
        &project,
        "branch",
        Some("source"),
        Some("selected"),
    );
    author(&mut repository, &project, "branch", "own", "分支独立问题");
    assert_eq!(
        messages(
            &repository,
            &project,
            "branch",
            &["before", "selected", "after", "own"]
        ),
        json!([
            {"turnId":"before","text":"第一条原文","imageAssetRevisionIds":[]}, {"turnId":"selected","text":"截至本条，保留空格  😀","imageAssetRevisionIds":[]}, {"turnId":"own","text":"分支独立问题","imageAssetRevisionIds":[]}
        ])
    );
    assert_eq!(
        messages(
            &repository,
            &ProjectId::parse("foreign").unwrap(),
            "branch",
            &["before", "own"]
        ),
        json!([])
    );
}

#[test]
fn nested_branches_preserve_all_ancestor_boundaries_including_inherited_selected_turn() {
    let (mut repository, project) = fixture();
    register(&mut repository, &project, "source", None, None);
    author(&mut repository, &project, "source", "before", "原文 A");
    author(&mut repository, &project, "source", "selected", "原文 B");
    author(&mut repository, &project, "source", "after", "源线程后续 C");
    register(
        &mut repository,
        &project,
        "branch",
        Some("source"),
        Some("selected"),
    );
    author(&mut repository, &project, "branch", "own", "分支 D");
    register(
        &mut repository,
        &project,
        "nested",
        Some("branch"),
        Some("own"),
    );
    assert_eq!(
        messages(
            &repository,
            &project,
            "nested",
            &["before", "selected", "after", "own"]
        ),
        json!([
            {"turnId":"before","text":"原文 A","imageAssetRevisionIds":[]}, {"turnId":"selected","text":"原文 B","imageAssetRevisionIds":[]}, {"turnId":"own","text":"分支 D","imageAssetRevisionIds":[]}
        ])
    );
    register(
        &mut repository,
        &project,
        "rewind",
        Some("branch"),
        Some("before"),
    );
    assert_eq!(
        messages(
            &repository,
            &project,
            "rewind",
            &["before", "selected", "after", "own"]
        ),
        json!([{"turnId":"before","text":"原文 A","imageAssetRevisionIds":[]}])
    );
}

#[test]
fn unbound_or_out_of_scope_branch_boundary_never_falls_back_to_engine_prompts() {
    let (mut repository, project) = fixture();
    register(&mut repository, &project, "source", None, None);
    author(&mut repository, &project, "source", "selected", "原文 A");
    author(&mut repository, &project, "source", "after", "后续 B");
    register(
        &mut repository,
        &project,
        "branch",
        Some("source"),
        Some("selected"),
    );
    register(
        &mut repository,
        &project,
        "invalid",
        Some("branch"),
        Some("after"),
    );
    assert_eq!(
        messages(&repository, &project, "invalid", &["selected", "after"]),
        json!([])
    );
    register(
        &mut repository,
        &project,
        "unknown",
        Some("source"),
        Some("unbound-review-turn"),
    );
    author(
        &mut repository,
        &project,
        "unknown",
        "own",
        "仍保留分支自己的问题",
    );
    assert_eq!(
        messages(
            &repository,
            &project,
            "unknown",
            &["selected", "after", "own"]
        ),
        json!([{"turnId":"own","text":"仍保留分支自己的问题","imageAssetRevisionIds":[]}])
    );
}

#[test]
fn identical_author_sends_remain_distinct_but_execution_context_is_not_a_message() {
    let (mut repository, project) = fixture();
    register(&mut repository, &project, "source", None, None);
    author(&mut repository, &project, "source", "first", "继续创作");
    author(&mut repository, &project, "source", "second", "继续创作");
    repository.create_task(CreateTaskRequest {
        task_id: "internal".into(), project_id: project.clone(), thread_id: "source".into(),
        operation: "advisory".into(), input_revision: 0, model: None,
        target_json: json!({"type":"project"}).to_string(),
        input_manifest_json: json!({"authorIntent":"继续创作"}).to_string(),
        capability_hash: "b".repeat(64), capability_expires_at: 1000,
    }).unwrap();
    repository.bind_task_turn(&project, "internal", "source", "automatic").unwrap();
    assert_eq!(messages(&repository, &project, "source", &["first","automatic","second"]),
        json!([{"turnId":"first","text":"继续创作","imageAssetRevisionIds":[]},{"turnId":"second","text":"继续创作","imageAssetRevisionIds":[]}]));
    // A page containing only an internal turn must not recreate the inherited request.
    assert_eq!(messages(&repository, &project, "source", &["automatic"]), json!([]));
    register(&mut repository, &project, "child", Some("source"), Some("automatic"));
    assert_eq!(messages(&repository, &project, "child", &["first","second","automatic"]),
        json!([{"turnId":"first","text":"继续创作","imageAssetRevisionIds":[]},{"turnId":"second","text":"继续创作","imageAssetRevisionIds":[]}]));
}
