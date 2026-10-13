use crate::{
    BranchId, ChapterProjection, CommitId, IdempotencyKey, ObjectId, ProjectId, ProjectRecord,
    RepositoryError, RevisionId, SaveCandidateRequest, SnapshotId, SqliteStoryRepository,
};

fn draft_request(
    revision: &str,
    base_snapshot: &str,
    next_snapshot: &str,
    parent_revision: Option<&str>,
    chapter_number: i64,
    title: &str,
) -> SaveCandidateRequest {
    SaveCandidateRequest {
        project_id: ProjectId::parse("project-trace").unwrap(),
        branch_id: BranchId::parse("main").unwrap(),
        object_id: ObjectId::parse("draft-chapter-1").unwrap(),
        object_kind: "draft".into(),
        revision_id: RevisionId::parse(revision).unwrap(),
        commit_id: CommitId::parse(format!("commit-{revision}")).unwrap(),
        idempotency_key: IdempotencyKey::parse(format!("request-{revision}")).unwrap(),
        request_digest: format!("digest-{revision}"),
        expected_project_revision: 0,
        expected_snapshot_id: SnapshotId::parse(base_snapshot).unwrap(),
        next_snapshot_id: SnapshotId::parse(next_snapshot).unwrap(),
        expected_candidate_revision_id: parent_revision.map(|id| RevisionId::parse(id).unwrap()),
        content_digest: format!("content-{revision}"),
        content_json: serde_json::json!({
            "chapterId": "chapter-1", "chapterNumber": chapter_number,
            "title": title, "body": "林夕记下证词。"
        })
        .to_string(),
        chapter_projection: Some(ChapterProjection {
            chapter_id: "chapter-1".into(),
            number: chapter_number,
            title: title.into(),
        }),
        task_output: None,
        linked_impact_ids: Vec::new(),
    }
}

fn repository() -> SqliteStoryRepository {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let project_id = ProjectId::parse("project-trace").unwrap();
    repository
        .create_project(
            ProjectRecord {
                project_id: project_id.clone(),
                title: "Trace".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            BranchId::parse("main").unwrap(),
            SnapshotId::parse("snapshot-0").unwrap(),
        )
        .unwrap();
    repository
}

#[test]
fn historical_trace_keeps_frozen_chapter_number_after_reordering() {
    let mut repository = repository();
    let project_id = ProjectId::parse("project-trace").unwrap();
    repository
        .save_candidate(draft_request(
            "revision-1",
            "snapshot-0",
            "snapshot-1",
            None,
            1,
            "旧章名",
        ))
        .unwrap();
    repository
        .save_candidate(draft_request(
            "revision-2",
            "snapshot-1",
            "snapshot-2",
            Some("revision-1"),
            2,
            "新章名",
        ))
        .unwrap();

    let old_snapshot = SnapshotId::parse("snapshot-1").unwrap();
    let new_snapshot = SnapshotId::parse("snapshot-2").unwrap();
    assert_eq!(
        repository
            .trace_manifest(&project_id, Some(&old_snapshot))
            .unwrap()
            .sources[0]
            .chapter,
        1
    );
    assert_eq!(
        repository
            .trace_manifest(&project_id, Some(&new_snapshot))
            .unwrap()
            .sources[0]
            .chapter,
        2
    );
    assert_eq!(repository.chapters(&project_id).unwrap()[0].number, 2);
    assert_eq!(repository.chapters(&project_id).unwrap()[0].title, "新章名");
}

#[test]
fn colliding_chapter_renumber_does_not_advance_snapshot_or_directory() {
    let mut repository = repository();
    let project_id = ProjectId::parse("project-trace").unwrap();
    repository
        .save_candidate(draft_request(
            "revision-1",
            "snapshot-0",
            "snapshot-1",
            None,
            1,
            "第一章",
        ))
        .unwrap();
    let mut second = draft_request("revision-2", "snapshot-1", "snapshot-2", None, 2, "第二章");
    second.object_id = ObjectId::parse("draft-chapter-2").unwrap();
    second.content_json = serde_json::json!({
        "chapterId": "chapter-2", "chapterNumber": 2,
        "title": "第二章", "body": "另一章。"
    })
    .to_string();
    second.chapter_projection = Some(ChapterProjection {
        chapter_id: "chapter-2".into(),
        number: 2,
        title: "第二章".into(),
    });
    repository.save_candidate(second).unwrap();

    let colliding = draft_request(
        "revision-3",
        "snapshot-2",
        "snapshot-3",
        Some("revision-1"),
        2,
        "碰撞章",
    );
    assert_eq!(
        repository.save_candidate(colliding),
        Err(RepositoryError::InvalidChapter)
    );
    assert_eq!(
        repository
            .branch_head(&project_id, &BranchId::parse("main").unwrap())
            .unwrap()
            .unwrap()
            .snapshot_id,
        SnapshotId::parse("snapshot-2").unwrap()
    );
    assert_eq!(repository.chapters(&project_id).unwrap()[0].number, 1);
}

use crate::{ProjectRevisionRepository};
