use novel_codex_story_core::BranchId;
use novel_codex_story_core::ChapterProjection;
use novel_codex_story_core::CommitId;
use novel_codex_story_core::CommitRequest;
use novel_codex_story_core::CommitStatus;
use novel_codex_story_core::IdempotencyKey;
use novel_codex_story_core::ObjectId;
use novel_codex_story_core::ProjectId;
use novel_codex_story_core::ProjectRecord;
use novel_codex_story_core::RepositoryError;
use novel_codex_story_core::RevisionId;
use novel_codex_story_core::RevisionRecord;
use novel_codex_story_core::SaveCandidateRequest;
use novel_codex_story_core::SearchQuery;
use novel_codex_story_core::SearchScope;
use novel_codex_story_core::SnapshotId;
use novel_codex_story_core::SqliteStoryRepository;
use novel_codex_story_core::UndoRequest;

fn parse<T>(
    value: &str,
    parser: impl FnOnce(String) -> Result<T, novel_codex_story_core::IdError>,
) -> T {
    parser(value.to_owned()).unwrap()
}

fn request() -> CommitRequest {
    CommitRequest {
        project_id: parse("project-1", ProjectId::parse),
        branch_id: parse("main", BranchId::parse),
        commit_id: parse("commit-1", CommitId::parse),
        idempotency_key: parse("request-1", IdempotencyKey::parse),
        request_digest: "digest-1".into(),
        base_snapshot_id: parse("snapshot-0", SnapshotId::parse),
        next_snapshot_id: parse("snapshot-1", SnapshotId::parse),
        revisions: vec![RevisionRecord {
            project_id: parse("project-1", ProjectId::parse),
            object_id: parse("chapter-1", ObjectId::parse),
            object_kind: "draft".into(),
            revision_id: parse("revision-1", RevisionId::parse),
            parent_revision_id: None,
            content_digest: "content-digest".into(),
            content_json: r#"{"body":"hello"}"#.into(),
        }],
    }
}

#[test]
fn durable_commit_replay_and_content_read_are_atomic() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("story.sqlite3");
    {
        let mut repository = SqliteStoryRepository::open(&path).unwrap();
        repository
            .create_branch(
                parse("project-1", ProjectId::parse),
                parse("main", BranchId::parse),
                parse("snapshot-0", SnapshotId::parse),
            )
            .unwrap();
        let receipt = repository.apply_commit(request()).unwrap();
        assert_eq!(receipt.status, CommitStatus::Applied);
    }
    let mut repository = SqliteStoryRepository::open(&path).unwrap();
    assert_eq!(
        repository.apply_commit(request()).unwrap().status,
        CommitStatus::Applied
    );
    assert_eq!(
        repository
            .revision_content(&parse("revision-1", RevisionId::parse))
            .unwrap(),
        Some(r#"{"body":"hello"}"#.into()),
    );
}

#[test]
fn historical_search_reads_immutable_revision_without_a_current_object_head() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let project_id = parse("project-1", ProjectId::parse);
    repository
        .create_project(
            ProjectRecord {
                project_id: project_id.clone(),
                title: "Search".into(),
                logline: String::new(),
                created_at: 10,
                updated_at: 10,
            },
            parse("main", BranchId::parse),
            parse("snapshot-0", SnapshotId::parse),
        )
        .unwrap();
    repository.apply_commit(request()).unwrap();

    let snapshot_id = parse("snapshot-1", SnapshotId::parse);
    let found = repository
        .search_documents(SearchQuery {
            project_id: &project_id,
            snapshot_id: Some(&snapshot_id),
            text: "hello",
            kind: Some("draft"),
            scope: SearchScope::All,
            limit: 10,
            cursor: None,
        })
        .unwrap();
    assert_eq!(found["results"][0]["sourceRevisionId"], "revision-1");
    assert_eq!(found["results"][0]["kind"], "draft");
    assert_eq!(found["results"][0]["title"], "draft");
}

#[test]
fn durable_commit_rejects_object_kind_change_without_advancing_head() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    repository
        .create_branch(
            parse("project-1", ProjectId::parse),
            parse("main", BranchId::parse),
            parse("snapshot-0", SnapshotId::parse),
        )
        .unwrap();
    repository.apply_commit(request()).unwrap();
    let mut next = request();
    next.commit_id = parse("commit-kind-change", CommitId::parse);
    next.idempotency_key = parse("request-kind-change", IdempotencyKey::parse);
    next.base_snapshot_id = parse("snapshot-1", SnapshotId::parse);
    next.next_snapshot_id = parse("snapshot-kind-change", SnapshotId::parse);
    next.revisions[0].revision_id = parse("revision-kind-change", RevisionId::parse);
    next.revisions[0].parent_revision_id = Some(parse("revision-1", RevisionId::parse));
    next.revisions[0].object_kind = "outline".into();

    assert_eq!(
        repository.apply_commit(next),
        Err(RepositoryError::InvalidObjectKind)
    );
    assert_eq!(
        repository
            .branch_head(
                &parse("project-1", ProjectId::parse),
                &parse("main", BranchId::parse)
            )
            .unwrap()
            .unwrap()
            .snapshot_id,
        parse("snapshot-1", SnapshotId::parse)
    );
}

#[test]
fn durable_undo_restores_previous_members_without_deleting_content() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    repository
        .create_branch(
            parse("project-1", ProjectId::parse),
            parse("main", BranchId::parse),
            parse("snapshot-0", SnapshotId::parse),
        )
        .unwrap();
    repository.apply_commit(request()).unwrap();
    let receipt = repository
        .undo_latest(UndoRequest {
            project_id: parse("project-1", ProjectId::parse),
            branch_id: parse("main", BranchId::parse),
            commit_id: parse("commit-undo", CommitId::parse),
            target_commit_id: parse("commit-1", CommitId::parse),
            idempotency_key: parse("request-undo", IdempotencyKey::parse),
            request_digest: "undo-digest".into(),
            base_snapshot_id: parse("snapshot-1", SnapshotId::parse),
            next_snapshot_id: parse("snapshot-undo", SnapshotId::parse),
        })
        .unwrap();
    assert_eq!(receipt.status, CommitStatus::Undone);
    assert!(
        repository
            .branch_head(
                &parse("project-1", ProjectId::parse),
                &parse("main", BranchId::parse),
            )
            .unwrap()
            .unwrap()
            .members
            .is_empty()
    );
    assert!(
        repository
            .revision_content(&parse("revision-1", RevisionId::parse))
            .unwrap()
            .is_some()
    );
}

#[test]
fn failed_commit_does_not_advance_durable_head() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    repository
        .create_branch(
            parse("project-1", ProjectId::parse),
            parse("main", BranchId::parse),
            parse("snapshot-0", SnapshotId::parse),
        )
        .unwrap();
    let mut invalid = request();
    invalid.revisions[0].content_json = "not-json".into();
    assert!(matches!(
        repository.apply_commit(invalid),
        Err(RepositoryError::Storage(_))
    ));
    assert_eq!(
        repository
            .branch_head(
                &parse("project-1", ProjectId::parse),
                &parse("main", BranchId::parse),
            )
            .unwrap()
            .unwrap()
            .snapshot_id,
        parse("snapshot-0", SnapshotId::parse),
    );
}

#[test]
fn project_and_initial_branch_are_created_in_one_transaction() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let project = ProjectRecord {
        project_id: parse("project-new", ProjectId::parse),
        title: "New story".into(),
        logline: "A durable beginning".into(),
        created_at: 10,
        updated_at: 10,
    };
    repository
        .create_project(
            project.clone(),
            parse("main", BranchId::parse),
            parse("snapshot-initial", SnapshotId::parse),
        )
        .unwrap();

    assert_eq!(repository.list_projects().unwrap(), vec![project.clone()]);
    assert_eq!(
        repository.project(&project.project_id).unwrap(),
        Some(project.clone())
    );
    assert_eq!(
        repository
            .branch_head(&project.project_id, &parse("main", BranchId::parse),)
            .unwrap()
            .unwrap()
            .snapshot_id,
        parse("snapshot-initial", SnapshotId::parse),
    );
}

fn repository_with_project() -> SqliteStoryRepository {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    repository
        .create_project(
            ProjectRecord {
                project_id: parse("project-candidate", ProjectId::parse),
                title: "Candidate".into(),
                logline: String::new(),
                created_at: 10,
                updated_at: 10,
            },
            parse("main", BranchId::parse),
            parse("snapshot-initial", SnapshotId::parse),
        )
        .unwrap();
    repository
}

fn candidate_request() -> SaveCandidateRequest {
    SaveCandidateRequest {
        project_id: parse("project-candidate", ProjectId::parse),
        branch_id: parse("main", BranchId::parse),
        object_id: parse("artifact-brief", ObjectId::parse),
        object_kind: "brief".into(),
        revision_id: parse("brief-revision-1", RevisionId::parse),
        commit_id: parse("candidate-commit-1", CommitId::parse),
        idempotency_key: parse("candidate-request-1", IdempotencyKey::parse),
        request_digest: "candidate-digest-1".into(),
        expected_project_revision: 0,
        expected_snapshot_id: parse("snapshot-initial", SnapshotId::parse),
        next_snapshot_id: parse("snapshot-candidate-1", SnapshotId::parse),
        expected_candidate_revision_id: None,
        content_digest: "brief-content-1".into(),
        content_json: r#"{"premise":"memory"}"#.into(),
        chapter_projection: None,
        task_output: None,
        linked_impact_ids: Vec::new(),
    }
}

#[test]
fn candidate_save_atomically_advances_snapshot_and_candidate_head() {
    let mut repository = repository_with_project();
    let request = candidate_request();
    let receipt = repository.save_candidate(request.clone()).unwrap();
    let mut replay_request = request;
    replay_request.revision_id = parse("ignored-revision", RevisionId::parse);
    replay_request.commit_id = parse("ignored-commit", CommitId::parse);
    replay_request.next_snapshot_id = parse("ignored-snapshot", SnapshotId::parse);
    let replay = repository.save_candidate(replay_request).unwrap();

    assert_eq!(receipt, replay);
    assert_eq!(
        repository
            .object_heads(&parse("project-candidate", ProjectId::parse))
            .unwrap()[0]
            .candidate_revision_id,
        Some(parse("brief-revision-1", RevisionId::parse)),
    );
    assert_eq!(
        repository
            .branch_head(
                &parse("project-candidate", ProjectId::parse),
                &parse("main", BranchId::parse),
            )
            .unwrap()
            .unwrap()
            .snapshot_id,
        parse("snapshot-candidate-1", SnapshotId::parse),
    );
}

#[test]
fn candidate_save_rejects_stale_candidate_without_partial_state() {
    let mut repository = repository_with_project();
    repository.save_candidate(candidate_request()).unwrap();
    let mut stale = candidate_request();
    stale.commit_id = parse("candidate-commit-2", CommitId::parse);
    stale.idempotency_key = parse("candidate-request-2", IdempotencyKey::parse);
    stale.request_digest = "candidate-digest-2".into();
    stale.revision_id = parse("brief-revision-2", RevisionId::parse);
    stale.expected_snapshot_id = parse("snapshot-candidate-1", SnapshotId::parse);
    stale.next_snapshot_id = parse("snapshot-candidate-2", SnapshotId::parse);

    assert!(matches!(
        repository.save_candidate(stale),
        Err(RepositoryError::StaleCandidate { .. })
    ));
    assert_eq!(
        repository
            .object_heads(&parse("project-candidate", ProjectId::parse))
            .unwrap()[0]
            .candidate_revision_id,
        Some(parse("brief-revision-1", RevisionId::parse)),
    );
}

#[test]
fn draft_candidate_creates_chapter_in_the_same_transaction() {
    let mut repository = repository_with_project();
    let mut request = candidate_request();
    request.object_id = parse("draft-chapter-1", ObjectId::parse);
    request.object_kind = "draft".into();
    request.revision_id = parse("draft-revision-1", RevisionId::parse);
    request.content_json = r#"{"chapterId":"chapter-1","body":"opening"}"#.into();
    request.chapter_projection = Some(ChapterProjection {
        chapter_id: "chapter-1".into(),
        number: 1,
        title: "Opening".into(),
    });

    repository.save_candidate(request).unwrap();

    let chapters = repository
        .chapters(&parse("project-candidate", ProjectId::parse))
        .unwrap();
    assert_eq!(chapters.len(), 1);
    assert_eq!(chapters[0].chapter_id, "chapter-1");
}

use novel_codex_story_core::{ProjectRevisionRepository, SearchIndexRepository};
