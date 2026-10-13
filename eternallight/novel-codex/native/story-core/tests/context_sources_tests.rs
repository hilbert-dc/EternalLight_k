use novel_codex_story_core::*;
use serde_json::json;

#[path = "../examples/support/long_work_fixture.rs"]
mod fixture;

#[test]
fn source_summaries_exclude_future_and_do_not_keep_ready_after_edit() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let (project, snapshot) = fixture::build(&mut repository, 3);
    let sources = repository
        .context_sources(&project, ContextSourceScope::ThroughChapter(2))
        .unwrap();
    assert_eq!(sources.total_sources, 2);
    assert!(
        sources
            .sources
            .iter()
            .all(|source| source["status"] == "ready")
    );
    assert!(
        sources
            .sources
            .iter()
            .all(|source| source["sourceRevisionId"] != "revision-3")
    );
    let content = json!({"chapterNumber":2,"title":"第二章改稿","body":"新的港城正文"}).to_string();
    repository
        .save_candidate(SaveCandidateRequest {
            project_id: project.clone(),
            branch_id: BranchId::parse("main").unwrap(),
            object_id: ObjectId::parse("draft-chapter-2").unwrap(),
            object_kind: "draft".into(),
            revision_id: RevisionId::parse("revision-edited").unwrap(),
            commit_id: CommitId::parse("edit").unwrap(),
            idempotency_key: IdempotencyKey::parse("edit").unwrap(),
            request_digest: "edited".into(),
            content_digest: "edited".into(),
            content_json: content,
            expected_project_revision: 0,
            expected_snapshot_id: snapshot,
            next_snapshot_id: SnapshotId::parse("snapshot-edited").unwrap(),
            expected_candidate_revision_id: Some(RevisionId::parse("revision-2").unwrap()),
            chapter_projection: Some(ChapterProjection {
                chapter_id: "chapter-2".into(),
                number: 2,
                title: "第二章改稿".into(),
            }),
            task_output: None,
            linked_impact_ids: Vec::new(),
        })
        .unwrap();
    let sources = repository
        .context_sources(&project, ContextSourceScope::ThroughChapter(2))
        .unwrap();
    assert_eq!(sources.total_sources, 2);
    let edited = sources
        .sources
        .iter()
        .find(|source| source["objectId"] == "draft-chapter-2")
        .unwrap();
    assert_eq!(
        edited,
        &json!({"objectId":"draft-chapter-2","sourceRevisionId":"revision-edited",
        "batchId":null,"status":"model_pending","confirmationState":"candidate"})
    );
    let other = ProjectId::parse("other-project").unwrap();
    repository
        .create_project(
            ProjectRecord {
                project_id: other.clone(),
                title: "另一部作品".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            BranchId::parse("main").unwrap(),
            SnapshotId::parse("other-initial").unwrap(),
        )
        .unwrap();
    let isolated = repository
        .context_sources(&other, ContextSourceScope::Project)
        .unwrap();
    assert_eq!(isolated.total_sources, 0);
    assert!(isolated.sources.is_empty());
}

use novel_codex_story_core::{ContextSourcesRepository, ProjectRevisionRepository};
