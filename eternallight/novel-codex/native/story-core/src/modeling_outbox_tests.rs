use crate::{
    BranchId, ModelingOutboxQuery, ProjectId, ProjectRecord, SnapshotId, SqliteStoryRepository,
};
use pretty_assertions::assert_eq;
use rusqlite::params;
use serde_json::json;

fn seed() -> (SqliteStoryRepository, ProjectId) {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    let project = ProjectId::parse("outbox-clock").unwrap();
    repo.create_project(
        ProjectRecord {
            project_id: project.clone(),
            title: "clock".into(),
            logline: String::new(),
            created_at: 1,
            updated_at: 1,
        },
        BranchId::parse("main").unwrap(),
        SnapshotId::parse("initial").unwrap(),
    )
    .unwrap();
    for object in ["draft-first", "draft-second"] {
        repo.connection.execute("INSERT INTO story_object_heads_v2(project_id,object_id,object_kind,candidate_revision_id) VALUES(?1,?2,'draft','source')",params![project.as_str(),object]).unwrap();
    }
    (repo, project)
}

#[test]
fn continuous_edit_window_caps_due_time_without_overwriting_authority() {
    let (repo, project) = seed();
    repo.connection.execute("UPDATE story_modeling_outbox_v1 SET first_dirty_ms=100,last_dirty_ms=5000 WHERE project_id=?1",[project.as_str()]).unwrap();
    let before = repo
        .branch_head(&project, &BranchId::parse("main").unwrap())
        .unwrap();
    let page = repo
        .modeling_outbox_page(ModelingOutboxQuery {
            project_id: &project,
            cursor: None,
            limit: 20,
            now_ms: 5099,
        })
        .unwrap();
    assert_eq!(
        page["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| (item["dueAtMs"].clone(), item["due"].clone()))
            .collect::<Vec<_>>(),
        vec![(json!(5100), json!(false)); 2]
    );
    let page = repo
        .modeling_outbox_page(ModelingOutboxQuery {
            project_id: &project,
            cursor: None,
            limit: 20,
            now_ms: 5100,
        })
        .unwrap();
    assert!(
        page["data"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["due"] == true)
    );
    assert_eq!(
        repo.branch_head(&project, &BranchId::parse("main").unwrap())
            .unwrap(),
        before
    );
}

#[test]
fn deletion_and_recreation_cannot_restore_an_old_cursor_watermark() {
    let (repo, project) = seed();
    let page = repo
        .modeling_outbox_page(ModelingOutboxQuery {
            project_id: &project,
            cursor: None,
            limit: 1,
            now_ms: i64::MAX,
        })
        .unwrap();
    let cursor = page["nextCursor"].as_str().unwrap();
    repo.connection.execute("UPDATE story_object_heads_v2 SET candidate_revision_id=NULL WHERE project_id=?1 AND object_id='draft-second'",[project.as_str()]).unwrap();
    repo.connection.execute("UPDATE story_object_heads_v2 SET candidate_revision_id='source' WHERE project_id=?1 AND object_id='draft-second'",[project.as_str()]).unwrap();
    assert!(
        repo.modeling_outbox_page(ModelingOutboxQuery {
            project_id: &project,
            cursor: Some(cursor),
            limit: 1,
            now_ms: i64::MAX
        })
        .is_err()
    );
    repo.connection
        .execute(
            "DELETE FROM story_projects_v2 WHERE project_id=?1",
            [project.as_str()],
        )
        .unwrap();
    assert!(
        repo.modeling_outbox_page(ModelingOutboxQuery {
            project_id: &project,
            cursor: None,
            limit: 1,
            now_ms: i64::MAX
        })
        .is_err()
    );
}
