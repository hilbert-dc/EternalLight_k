use rusqlite::params;
use serde_json::json;

use crate::{
    BranchId, ProjectId, ProjectRecord, RepositoryError, RevisionId, SnapshotId,
    SqliteStoryRepository,
};

#[test]
fn draft_delta_tracks_unicode_ranges_and_scene_reordering_without_semantic_claims() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let project_id = ProjectId::parse("delta-project").unwrap();
    repository
        .create_project(
            ProjectRecord {
                project_id: project_id.clone(),
                title: "港城".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            BranchId::parse("main").unwrap(),
            SnapshotId::parse("delta-initial").unwrap(),
        )
        .unwrap();
    let before = json!({
        "chapterNumber": 1, "title": "白塔", "body": "林夕烧掉密函。",
        "sceneGroups": [
            {"sceneId": "opening", "body": "烧掉密函"},
            {"sceneId": "ending", "body": "夜色已深"}
        ]
    });
    let after = json!({
        "chapterNumber": 1, "title": "白塔", "body": "林夕藏起密函。",
        "sceneGroups": [
            {"sceneId": "ending", "body": "夜色已深"},
            {"sceneId": "opening", "body": "藏起密函"}
        ]
    });
    for (revision_id, object_id, content) in [
        ("delta-before", "draft-one", before),
        ("delta-after", "draft-one", after),
        ("delta-other", "draft-two", json!({"body":"另一章"})),
    ] {
        repository.connection.execute(
            "INSERT INTO story_revisions_v2(revision_id, project_id, object_id, object_kind, content_digest, content_json) VALUES (?1, ?2, ?3, 'draft', 'digest', ?4)",
            params![revision_id, project_id.as_str(), object_id, content.to_string()],
        ).unwrap();
    }
    let before_id = RevisionId::parse("delta-before").unwrap();
    let after_id = RevisionId::parse("delta-after").unwrap();
    let delta = repository
        .draft_revision_delta(&project_id, &before_id, &after_id)
        .unwrap();
    assert_eq!(delta["objectId"], "draft-one");
    assert_eq!(
        delta["bodyDelta"],
        json!({
            "fromRange": {"start": 2, "end": 4},
            "toRange": {"start": 2, "end": 4},
            "fromExcerpt": "烧掉", "toExcerpt": "藏起", "excerptTruncated": false
        })
    );
    assert_eq!(
        delta["sceneChanges"],
        json!([
            {"sceneId": "ending", "status": "moved", "fromOrder": 2, "toOrder": 1},
            {"sceneId": "opening", "status": "movedAndEdited", "fromOrder": 1, "toOrder": 2}
        ])
    );
    assert_eq!(delta["coverage"]["semanticChangeInferred"], false);
    let other_id = RevisionId::parse("delta-other").unwrap();
    assert!(matches!(
        repository.draft_revision_delta(&project_id, &before_id, &other_id),
        Err(RepositoryError::InvalidContent(_))
    ));
    let another_project = ProjectId::parse("another-delta-project").unwrap();
    assert!(matches!(
        repository.draft_revision_delta(&another_project, &before_id, &after_id),
        Err(RepositoryError::CandidateNotFound)
    ));
}

use crate::{ProjectRevisionRepository, RevisionDeltaRepository};
