use novel_codex_story_core::BranchId;
use novel_codex_story_core::ChapterProjection;
use novel_codex_story_core::CommitId;
use novel_codex_story_core::CreateTaskRequest;
use novel_codex_story_core::IdError;
use novel_codex_story_core::IdempotencyKey;
use novel_codex_story_core::ObjectId;
use novel_codex_story_core::ProjectId;
use novel_codex_story_core::ProjectRecord;
use novel_codex_story_core::RevisionId;
use novel_codex_story_core::SaveCandidateRequest;
use novel_codex_story_core::SnapshotId;
use novel_codex_story_core::SqliteStoryRepository;
use novel_codex_story_core::TaskOutputBinding;

fn parse<T>(value: &str, parser: impl FnOnce(String) -> Result<T, IdError>) -> T {
    parser(value.to_owned()).unwrap()
}

#[test]
fn task_bound_draft_output_and_completion_are_one_authority_chain() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let project_id = parse("project-task", ProjectId::parse);
    repository
        .create_project(
            ProjectRecord {
                project_id: project_id.clone(),
                title: "Task".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            parse("main", BranchId::parse),
            parse("snapshot-0", SnapshotId::parse),
        )
        .unwrap();
    let task = repository
        .create_task(CreateTaskRequest {
            task_id: "task-1".into(),
            project_id: project_id.clone(),
            thread_id: "thread-1".into(),
            operation: "generateChapter".into(),
            input_revision: 0,
            model: Some("model".into()),
            target_json: r#"{"type":"chapter","chapterId":"chapter-1"}"#.into(),
            input_manifest_json: r#"{"expectedCandidateRevisionId":null}"#.into(),
            capability_hash: "a".repeat(64),
            capability_expires_at: 100,
        })
        .unwrap();
    assert_eq!(task.status, "queued");

    repository
        .save_candidate(SaveCandidateRequest {
            project_id: project_id.clone(),
            branch_id: parse("main", BranchId::parse),
            object_id: parse("draft-chapter-1", ObjectId::parse),
            object_kind: "draft".into(),
            revision_id: parse("draft-1", RevisionId::parse),
            commit_id: parse("candidate-1", CommitId::parse),
            idempotency_key: parse("save-1", IdempotencyKey::parse),
            request_digest: "digest".into(),
            expected_project_revision: 0,
            expected_snapshot_id: parse("snapshot-0", SnapshotId::parse),
            next_snapshot_id: parse("snapshot-1", SnapshotId::parse),
            expected_candidate_revision_id: None,
            content_digest: "content".into(),
            content_json: r#"{"chapterId":"chapter-1","body":"chapter"}"#.into(),
            chapter_projection: Some(ChapterProjection {
                chapter_id: "chapter-1".into(),
                number: 1,
                title: "One".into(),
            }),
            task_output: Some(TaskOutputBinding {
                task_id: "task-1".into(),
                thread_id: "thread-1".into(),
                output_kind: "draft".into(),
                completeness: Some("chapterComplete".into()),
            }),
            linked_impact_ids: Vec::new(),
        })
        .unwrap();
    let completed = repository
        .complete_task(&project_id, "task-1", "thread-1", "done".into(), 10)
        .unwrap();

    assert_eq!(completed.status, "completed");
    assert_eq!(completed.delivery_status, "ready_for_review");
    assert!(completed.output_refs_json.contains("draft-1"));
    assert_eq!(completed.capability_revoked_at, Some(10));
}
