use crate::{BranchId, ProjectId, ProjectRecord, SnapshotId, SqliteStoryRepository};

#[test]
fn related_reads_do_not_mix_concurrent_workspace_revisions() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("read-snapshot.sqlite3");
    let mut writer = SqliteStoryRepository::open(&path).unwrap();
    let project_id = ProjectId::parse("snapshot-consistency").unwrap();
    writer
        .create_project(
            ProjectRecord {
                project_id: project_id.clone(),
                title: "Snapshot".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            BranchId::parse("main").unwrap(),
            SnapshotId::parse("initial-snapshot").unwrap(),
        )
        .unwrap();
    let reader = SqliteStoryRepository::open(&path).unwrap();
    let (before, after) = reader
        .with_read_snapshot(|| {
            let before = reader.project_revision(&project_id).unwrap();
            writer
                .connection
                .execute(
                    "UPDATE story_workspaces_v2 SET revision = 1 WHERE project_id = ?1",
                    [project_id.as_str()],
                )
                .unwrap();
            let after = reader.project_revision(&project_id).unwrap();
            (before, after)
        })
        .unwrap();
    assert_eq!((before, after), (0, 0));
    assert_eq!(reader.project_revision(&project_id).unwrap(), 1);
}

use crate::{ProjectRevisionRepository, ReadSnapshotRepository};
