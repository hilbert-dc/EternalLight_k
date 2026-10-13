use novel_codex_story_core::BranchId;
use novel_codex_story_core::CommitId;
use novel_codex_story_core::CommitRequest;
use novel_codex_story_core::CommitStatus;
use novel_codex_story_core::IdempotencyKey;
use novel_codex_story_core::ObjectId;
use novel_codex_story_core::ProjectId;
use novel_codex_story_core::RepositoryError;
use novel_codex_story_core::RevisionId;
use novel_codex_story_core::RevisionRecord;
use novel_codex_story_core::SnapshotId;
use novel_codex_story_core::StoryRepository;
use novel_codex_story_core::UndoRequest;

fn id<T>(
    value: &str,
    parse: impl FnOnce(String) -> Result<T, novel_codex_story_core::IdError>,
) -> T {
    parse(value.to_owned()).unwrap()
}

fn project() -> ProjectId {
    id("project-lamp", ProjectId::parse)
}

fn branch() -> BranchId {
    id("branch-main", BranchId::parse)
}

fn snapshot(value: &str) -> SnapshotId {
    id(value, SnapshotId::parse)
}

fn repository() -> StoryRepository {
    let mut repository = StoryRepository::default();
    repository
        .create_branch(project(), branch(), snapshot("snapshot-0"))
        .unwrap();
    repository
}

fn first_commit() -> CommitRequest {
    CommitRequest {
        project_id: project(),
        branch_id: branch(),
        commit_id: id("commit-1", CommitId::parse),
        idempotency_key: id("request-1", IdempotencyKey::parse),
        request_digest: "digest-1".into(),
        base_snapshot_id: snapshot("snapshot-0"),
        next_snapshot_id: snapshot("snapshot-1"),
        revisions: vec![RevisionRecord {
            project_id: project(),
            object_id: id("chapter-1", ObjectId::parse),
            object_kind: "draft".into(),
            revision_id: id("revision-1", RevisionId::parse),
            parent_revision_id: None,
            content_digest: "content-1".into(),
            content_json: r#"{"body":"first"}"#.into(),
        }],
    }
}

#[test]
fn commit_advances_branch_and_replays_same_receipt() {
    let mut repository = repository();
    let request = first_commit();

    let receipt = repository.apply_commit(request.clone()).unwrap();
    let replay = repository.apply_commit(request).unwrap();

    assert_eq!(receipt, replay);
    assert_eq!(receipt.status, CommitStatus::Applied);
    assert_eq!(
        repository
            .branch_head(&project(), &branch())
            .unwrap()
            .members,
        std::collections::BTreeMap::from([(
            id("chapter-1", ObjectId::parse),
            id("revision-1", RevisionId::parse),
        )]),
    );
}

#[test]
fn object_kind_cannot_change_between_revisions() {
    let mut repository = repository();
    repository.apply_commit(first_commit()).unwrap();
    let mut next = first_commit();
    next.commit_id = id("commit-kind-change", CommitId::parse);
    next.idempotency_key = id("request-kind-change", IdempotencyKey::parse);
    next.base_snapshot_id = snapshot("snapshot-1");
    next.next_snapshot_id = snapshot("snapshot-kind-change");
    next.revisions[0].revision_id = id("revision-kind-change", RevisionId::parse);
    next.revisions[0].parent_revision_id = Some(id("revision-1", RevisionId::parse));
    next.revisions[0].object_kind = "outline".into();

    assert_eq!(
        repository.apply_commit(next),
        Err(RepositoryError::InvalidObjectKind)
    );
    assert_eq!(
        repository
            .branch_head(&project(), &branch())
            .unwrap()
            .snapshot_id,
        snapshot("snapshot-1")
    );
}

#[test]
fn stale_snapshot_and_changed_idempotent_payload_are_rejected() {
    let mut repository = repository();
    let request = first_commit();
    repository.apply_commit(request.clone()).unwrap();

    let mut changed = request;
    changed.request_digest = "different".into();
    assert_eq!(
        repository.apply_commit(changed),
        Err(RepositoryError::IdempotencyConflict),
    );

    let mut stale = first_commit();
    stale.commit_id = id("commit-2", CommitId::parse);
    stale.idempotency_key = id("request-2", IdempotencyKey::parse);
    stale.next_snapshot_id = snapshot("snapshot-2");
    assert_eq!(
        repository.apply_commit(stale),
        Err(RepositoryError::StaleSnapshot {
            expected: snapshot("snapshot-0"),
            actual: snapshot("snapshot-1"),
        }),
    );
}

#[test]
fn revision_parent_must_match_visible_object_revision() {
    let mut repository = repository();
    repository.apply_commit(first_commit()).unwrap();
    let request = CommitRequest {
        project_id: project(),
        branch_id: branch(),
        commit_id: id("commit-2", CommitId::parse),
        idempotency_key: id("request-2", IdempotencyKey::parse),
        request_digest: "digest-2".into(),
        base_snapshot_id: snapshot("snapshot-1"),
        next_snapshot_id: snapshot("snapshot-2"),
        revisions: vec![RevisionRecord {
            project_id: project(),
            object_id: id("chapter-1", ObjectId::parse),
            object_kind: "draft".into(),
            revision_id: id("revision-2", RevisionId::parse),
            parent_revision_id: None,
            content_digest: "content-2".into(),
            content_json: r#"{"body":"second"}"#.into(),
        }],
    };

    assert_eq!(
        repository.apply_commit(request),
        Err(RepositoryError::InvalidParent {
            object_id: id("chapter-1", ObjectId::parse),
            expected: Some(id("revision-1", RevisionId::parse)),
            actual: None,
        }),
    );
}

#[test]
fn undo_creates_compensating_snapshot_without_deleting_history() {
    let mut repository = repository();
    repository.apply_commit(first_commit()).unwrap();

    let receipt = repository
        .undo_latest(UndoRequest {
            project_id: project(),
            branch_id: branch(),
            commit_id: id("commit-undo-1", CommitId::parse),
            target_commit_id: id("commit-1", CommitId::parse),
            idempotency_key: id("request-undo-1", IdempotencyKey::parse),
            request_digest: "undo-digest-1".into(),
            base_snapshot_id: snapshot("snapshot-1"),
            next_snapshot_id: snapshot("snapshot-undo-1"),
        })
        .unwrap();

    assert_eq!(receipt.status, CommitStatus::Undone);
    assert!(
        repository
            .branch_head(&project(), &branch())
            .unwrap()
            .members
            .is_empty()
    );
}

#[test]
fn commit_rejects_revision_from_another_project_without_advancing_head() {
    let mut repository = repository();
    let mut request = first_commit();
    request.revisions[0].project_id = id("project-other", ProjectId::parse);

    assert_eq!(
        repository.apply_commit(request),
        Err(RepositoryError::CrossProjectReference),
    );
    assert_eq!(
        repository
            .branch_head(&project(), &branch())
            .unwrap()
            .snapshot_id,
        snapshot("snapshot-0"),
    );
}
