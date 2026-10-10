use crate::{
    BranchId, ModelingOutboxQuery, ProjectId, ProjectRecord, RepositoryError, SnapshotId,
    SqliteStoryRepository,
};
use pretty_assertions::assert_eq;
use rusqlite::params;

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
fn source_freshness_is_passive_and_does_not_grant_execution() {
    let (repo, project) = seed();
    let before = repo
        .branch_head(&project, &BranchId::parse("main").unwrap())
        .unwrap();
    let page = repo
        .modeling_outbox_page(ModelingOutboxQuery {
            project_id: &project,
            cursor: None,
            limit: 20,
        })
        .unwrap();
    assert_eq!(page["pendingCount"], 2);
    assert_eq!(page["authority"], "sourceFreshnessOnly");
    for source in page["data"].as_array().unwrap() {
        assert_eq!(source["sourceRevisionId"], "source");
        assert_eq!(source["status"], "pending");
        assert!(source.get("due").is_none());
        assert!(source.get("dueAtMs").is_none());
    }
    assert!(repo.tasks(&project).unwrap().is_empty());
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
        })
        .is_err()
    );
    {
        // Current projects own edition and canon scopes. Clean their default
        // references before deleting this fixture project; keep FK enforcement.
        let tx = repo.connection.unchecked_transaction().unwrap();
        tx.execute(
            "DELETE FROM story_scope_branches_v1 WHERE project_id=?1",
            [project.as_str()],
        )
        .unwrap();
        tx.execute(
            "DELETE FROM story_editions_v1 WHERE project_id=?1",
            [project.as_str()],
        )
        .unwrap();
        tx.execute(
            "DELETE FROM story_projects_v2 WHERE project_id=?1",
            [project.as_str()],
        )
        .unwrap();
        tx.commit().unwrap();
    }
    assert!(matches!(
        repo.modeling_outbox_page(ModelingOutboxQuery {
            project_id: &project,
            cursor: None,
            limit: 1,
        }),
        Err(RepositoryError::ProjectNotFound)
    ));
}
