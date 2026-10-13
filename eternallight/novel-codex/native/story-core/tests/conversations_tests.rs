use novel_codex_story_core::{
    BranchId, CreateConversationRequest, ProjectId, ProjectRecord, RepositoryError, SnapshotId,
    SqliteStoryRepository, UpdateConversationRequest,
};
use pretty_assertions::assert_eq;

fn project(repository: &mut SqliteStoryRepository, name: &str) -> ProjectId {
    let id = ProjectId::parse(name).unwrap();
    repository
        .create_project(
            ProjectRecord {
                project_id: id.clone(),
                title: name.into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            BranchId::parse("main").unwrap(),
            SnapshotId::parse(format!("snapshot-{name}")).unwrap(),
        )
        .unwrap();
    id
}
fn create(id: &str) -> CreateConversationRequest {
    CreateConversationRequest {
        scope_id: None,
        thread_id: id.into(),
        title: "独立对话".into(),
        forked_from_id: None,
        last_turn_id: None,
    }
}
fn update(version: i64) -> UpdateConversationRequest {
    UpdateConversationRequest {
        expected_version: version,
        title: None,
        is_pinned: None,
        is_archived: None,
    }
}

#[test]
fn empty_conversations_survive_restart_without_creating_tasks_or_revising_content() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("conversations.sqlite3");
    let mut repository = SqliteStoryRepository::open(&path).unwrap();
    let a = project(&mut repository, "project-a");
    let b = project(&mut repository, "project-b");
    let revision = repository.project_revision(&a).unwrap();
    repository
        .conversation_store()
        .create_conversation(&a, create("thread-a"), 10)
        .unwrap();
    repository
        .conversation_store()
        .create_conversation(&a, create("thread-a2"), 11)
        .unwrap();
    repository
        .conversation_store()
        .create_conversation(&b, create("thread-b"), 12)
        .unwrap();
    assert_eq!(repository.project_revision(&a).unwrap(), revision);
    assert!(repository.tasks(&a).unwrap().is_empty());
    drop(repository);
    let mut repository = SqliteStoryRepository::open(&path).unwrap();
    assert_eq!(
        repository
            .conversation_store()
            .conversations(&a, false)
            .unwrap()
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>(),
        vec!["thread-a2", "thread-a"]
    );
    assert_eq!(
        repository
            .conversation_store()
            .conversations(&b, false)
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        repository.conversation_store().conversation(&b, "thread-a"),
        Err(RepositoryError::ConversationNotFound)
    );
}

#[test]
fn thread_ownership_is_unique_immutable_and_registration_is_idempotent_after_rename() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let a = project(&mut repository, "project-a");
    let b = project(&mut repository, "project-b");
    let original = repository
        .conversation_store()
        .create_conversation(&a, create("thread-a"), 10)
        .unwrap();
    assert_eq!(
        repository
            .conversation_store()
            .create_conversation(&a, create("thread-a"), 20)
            .unwrap(),
        original
    );
    assert_eq!(
        repository
            .conversation_store()
            .create_conversation(&b, create("thread-a"), 20),
        Err(RepositoryError::CrossProjectReference)
    );
    let mut rename = update(0);
    rename.title = Some("人物讨论".into());
    let renamed = repository
        .conversation_store()
        .update_conversation(&a, "thread-a", rename, 30)
        .unwrap();
    assert_eq!(
        repository
            .conversation_store()
            .create_conversation(&a, create("thread-a"), 40)
            .unwrap(),
        renamed
    );
    let mut changed_registration = create("thread-a");
    changed_registration.title = "另一个登记".into();
    assert_eq!(
        repository
            .conversation_store()
            .create_conversation(&a, changed_registration, 40),
        Err(RepositoryError::IdempotencyConflict)
    );
    assert!(
        repository
            .conversation_store()
            .conversations(&b, false)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn metadata_updates_use_optimistic_versions_and_archive_is_reversible() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let a = project(&mut repository, "project-a");
    repository
        .conversation_store()
        .create_conversation(&a, create("older"), 10)
        .unwrap();
    repository
        .conversation_store()
        .create_conversation(&a, create("newer"), 20)
        .unwrap();
    let mut pin = update(0);
    pin.is_pinned = Some(true);
    let pinned = repository
        .conversation_store()
        .update_conversation(&a, "older", pin.clone(), 30)
        .unwrap();
    assert_eq!(pinned.version, 1);
    assert_eq!(
        repository
            .conversation_store()
            .conversations(&a, false)
            .unwrap()[0]
            .id,
        "older"
    );
    assert_eq!(
        repository
            .conversation_store()
            .update_conversation(&a, "older", pin, 31),
        Err(RepositoryError::ConversationStale {
            expected: 0,
            actual: 1
        })
    );
    let mut archive = update(1);
    archive.is_archived = Some(true);
    let archived = repository
        .conversation_store()
        .update_conversation(&a, "older", archive, 32)
        .unwrap();
    assert_eq!(archived.version, 2);
    assert_eq!(
        repository
            .conversation_store()
            .conversations(&a, false)
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        repository
            .conversation_store()
            .conversations(&a, true)
            .unwrap()[0]
            .id,
        "older"
    );
    let mut restore = update(2);
    restore.is_archived = Some(false);
    repository
        .conversation_store()
        .update_conversation(&a, "older", restore, 29)
        .unwrap();
    assert_eq!(
        repository
            .conversation_store()
            .conversation(&a, "older")
            .unwrap()
            .updated_at,
        32
    );
    assert_eq!(
        repository
            .conversation_store()
            .conversations(&a, true)
            .unwrap()
            .len(),
        0
    );
    assert_eq!(repository.project_revision(&a).unwrap(), 0);
    assert!(repository.tasks(&a).unwrap().is_empty());
}

#[test]
fn branch_registration_requires_registered_same_project_source() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let a = project(&mut repository, "project-a");
    let b = project(&mut repository, "project-b");
    repository
        .conversation_store()
        .create_conversation(&a, create("source"), 10)
        .unwrap();
    let mut branch = create("branch");
    branch.forked_from_id = Some("source".into());
    branch.last_turn_id = Some("selected-turn".into());
    assert_eq!(
        repository
            .conversation_store()
            .create_conversation(&b, branch.clone(), 11),
        Err(RepositoryError::ConversationNotFound)
    );
    let record = repository
        .conversation_store()
        .create_conversation(&a, branch, 12)
        .unwrap();
    assert_eq!(record.forked_from_id.as_deref(), Some("source"));
    assert_eq!(record.last_turn_id.as_deref(), Some("selected-turn"));
    let mut invalid = create("no-source");
    invalid.last_turn_id = Some("selected-turn".into());
    assert!(matches!(
        repository
            .conversation_store()
            .create_conversation(&a, invalid, 12),
        Err(RepositoryError::InvalidContent(_))
    ));
    let mut invalid = create("no-boundary");
    invalid.forked_from_id = Some("source".into());
    assert!(matches!(
        repository
            .conversation_store()
            .create_conversation(&a, invalid, 12),
        Err(RepositoryError::InvalidContent(_))
    ));
}

#[test]
fn malformed_and_unknown_inputs_are_rejected_without_writes() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let a = project(&mut repository, "project-a");
    for id in ["", " thread", "thread\n", &"界".repeat(54)] {
        assert!(
            repository
                .conversation_store()
                .create_conversation(&a, create(id), 0)
                .is_err()
        );
    }
    for title in [
        "".into(),
        " ".into(),
        "line\nsecond".into(),
        "界".repeat(121),
    ] {
        let mut request = create("thread");
        request.title = title;
        assert!(
            repository
                .conversation_store()
                .create_conversation(&a, request, 0)
                .is_err()
        );
    }
    let mut valid = create("thread");
    valid.title = "界".repeat(120);
    repository
        .conversation_store()
        .create_conversation(&a, valid, 0)
        .unwrap();
    assert!(
        repository
            .conversation_store()
            .update_conversation(&a, "thread", update(0), 0)
            .is_err()
    );
    assert!(
        serde_json::from_value::<CreateConversationRequest>(
            serde_json::json!({"threadId":"thread","title":"标题","projectId":"forged"})
        )
        .is_err()
    );
    assert!(
        serde_json::from_value::<UpdateConversationRequest>(
            serde_json::json!({"expectedVersion":0,"threadId":"other","isPinned":true})
        )
        .is_err()
    );
    assert!(
        repository
            .conversation_store()
            .conversations(&ProjectId::parse("missing").unwrap(), false)
            .is_err()
    );
    assert_eq!(
        repository
            .conversation_store()
            .conversations(&a, false)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn concurrent_metadata_writers_cannot_overwrite_each_others_version() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("conversations.sqlite3");
    let mut repository = SqliteStoryRepository::open(&path).unwrap();
    let a = project(&mut repository, "project-a");
    repository
        .conversation_store()
        .create_conversation(&a, create("thread-a"), 10)
        .unwrap();
    drop(repository);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let writers = ["title-a", "title-b"].map(|title| {
        let path = path.clone();
        let a = a.clone();
        let barrier = barrier.clone();
        std::thread::spawn(move || {
            let mut repository = SqliteStoryRepository::open(path).unwrap();
            let mut request = update(0);
            request.title = Some(title.into());
            barrier.wait();
            repository
                .conversation_store()
                .update_conversation(&a, "thread-a", request, 11)
        })
    });
    let results = writers.map(|writer| writer.join().unwrap());
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(
                result,
                Err(RepositoryError::ConversationStale {
                    expected: 0,
                    actual: 1
                })
            ))
            .count(),
        1
    );
    let mut repository = SqliteStoryRepository::open(path).unwrap();
    assert_eq!(
        repository
            .conversation_store()
            .conversation(&a, "thread-a")
            .unwrap()
            .version,
        1
    );
}

use novel_codex_story_core::{ProjectRevisionRepository, TasksRepository};
