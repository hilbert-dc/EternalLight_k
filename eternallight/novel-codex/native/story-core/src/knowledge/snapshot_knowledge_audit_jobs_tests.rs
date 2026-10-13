use rusqlite::params;

use crate::{BranchId, ProjectId, ProjectRecord, SnapshotId, SqliteStoryRepository};

fn project(repository: &mut SqliteStoryRepository, suffix: &str) -> ProjectId {
    let id = ProjectId::parse(format!("snapshot-job-{suffix}")).unwrap();
    repository
        .create_project(
            ProjectRecord {
                project_id: id.clone(),
                title: "Audit".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            BranchId::parse("main").unwrap(),
            SnapshotId::parse(format!("snapshot-job-start-{suffix}")).unwrap(),
        )
        .unwrap();
    id
}

fn add_member(
    repository: &SqliteStoryRepository,
    project_id: &ProjectId,
    suffix: &str,
    number: usize,
) {
    repository.connection.execute(
        "INSERT INTO story_revisions_v2(revision_id, project_id, object_id, object_kind, content_digest, content_json) VALUES (?1, ?2, ?3, 'draft', 'digest', '{\"body\":\"林夕\"}')",
        params![format!("revision-{suffix}-{number}"), project_id.as_str(), format!("draft-{suffix}-{number}")],
    ).unwrap();
    repository.connection.execute(
        "INSERT INTO story_snapshot_members_v2(snapshot_id, object_id, revision_id) VALUES (?1, ?2, ?3)",
        params![format!("snapshot-job-start-{suffix}"), format!("draft-{suffix}-{number}"), format!("revision-{suffix}-{number}")],
    ).unwrap();
}

#[test]
fn job_checkpoints_and_resumes_then_revokes_completed_report_on_capture_change() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("story.sqlite3");
    let mut repository = SqliteStoryRepository::open(&path).unwrap();
    let id = project(&mut repository, "many");
    for number in 0..205 {
        add_member(&repository, &id, "many", number);
    }
    let queued = repository.start_snapshot_knowledge_audit(&id).unwrap();
    assert_eq!(
        repository.start_snapshot_knowledge_audit(&id).unwrap(),
        queued
    );
    let first = repository
        .advance_snapshot_knowledge_audit(&id, &queued.id)
        .unwrap();
    assert_eq!(
        (
            first.status.as_str(),
            first.audit.members,
            first.verified_consistent
        ),
        ("running", 100, None)
    );
    let paused = repository
        .control_snapshot_knowledge_audit(&id, &queued.id, "pause", first.version)
        .unwrap();
    drop(repository);
    let mut reopened = SqliteStoryRepository::open(&path).unwrap();
    assert_eq!(
        reopened
            .snapshot_knowledge_audit_job(&id, &queued.id)
            .unwrap(),
        paused
    );
    let resumed = reopened
        .control_snapshot_knowledge_audit(&id, &queued.id, "resume", paused.version)
        .unwrap();
    assert_eq!(resumed.status, "queued");
    let done = loop {
        let next = reopened
            .advance_snapshot_knowledge_audit(&id, &queued.id)
            .unwrap();
        if next.status == "completed" {
            break next;
        }
    };
    assert_eq!(done.verified_consistent, Some(true));
    assert_eq!(done.audit, reopened.audit_snapshot_knowledge(&id).unwrap());
    reopened.connection.execute(
        "DELETE FROM story_snapshot_knowledge_v2 WHERE snapshot_id = 'snapshot-job-start-many' AND object_id = 'draft-many-0'",
        [],
    ).unwrap();
    let stale = reopened
        .snapshot_knowledge_audit_job(&id, &done.id)
        .unwrap();
    assert_eq!(
        (stale.status.as_str(), stale.verified_consistent),
        ("stale", None)
    );
    let next = reopened.start_snapshot_knowledge_audit(&id).unwrap();
    let inconsistent = loop {
        let result = reopened
            .advance_snapshot_knowledge_audit(&id, &next.id)
            .unwrap();
        if result.status == "completed" {
            break result;
        }
    };
    assert_eq!(inconsistent.verified_consistent, Some(false));
    assert_eq!(inconsistent.audit.missing_captures, 1);
}

#[test]
fn new_snapshot_member_stales_a_running_job_and_pause_can_be_cancelled() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let id = project(&mut repository, "live");
    add_member(&repository, &id, "live", 0);
    let queued = repository.start_snapshot_knowledge_audit(&id).unwrap();
    add_member(&repository, &id, "live", 1);
    let stale = repository
        .advance_snapshot_knowledge_audit(&id, &queued.id)
        .unwrap();
    assert_eq!(
        (stale.status.as_str(), stale.verified_consistent),
        ("stale", None)
    );
    let replacement = repository.start_snapshot_knowledge_audit(&id).unwrap();
    let paused = repository
        .control_snapshot_knowledge_audit(&id, &replacement.id, "pause", replacement.version)
        .unwrap();
    assert_eq!(paused.status, "paused");
    let cancelled = repository
        .control_snapshot_knowledge_audit(&id, &replacement.id, "cancel", paused.version)
        .unwrap();
    assert_eq!(cancelled.status, "cancelled");
    assert_eq!(
        repository.next_snapshot_knowledge_audit_job().unwrap(),
        None
    );
}

#[test]
fn deleting_a_snapshot_revokes_a_previously_consistent_report() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let id = project(&mut repository, "deleted");
    add_member(&repository, &id, "deleted", 0);
    let queued = repository.start_snapshot_knowledge_audit(&id).unwrap();
    let done = repository
        .advance_snapshot_knowledge_audit(&id, &queued.id)
        .unwrap();
    assert_eq!(done.verified_consistent, Some(true));
    repository
        .connection
        .execute(
            "DELETE FROM story_snapshots_v2 WHERE snapshot_id = 'snapshot-job-start-deleted'",
            [],
        )
        .unwrap();
    let old = repository
        .snapshot_knowledge_audit_job(&id, &done.id)
        .unwrap();
    assert_eq!(
        (old.status.as_str(), old.verified_consistent),
        ("stale", None)
    );
}

use crate::{ProjectRevisionRepository, SnapshotKnowledgeAuditJobsRepository, SnapshotKnowledgeIntegrityRepository};
