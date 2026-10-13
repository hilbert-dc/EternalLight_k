use rusqlite::params;
use serde_json::json;

use crate::{
    BranchId, ProjectId, ProjectRecord, RepositoryError, SnapshotId, SqliteStoryRepository,
};

fn project(repository: &mut SqliteStoryRepository, suffix: &str) -> ProjectId {
    let id = ProjectId::parse(format!("fact-audit-project-{suffix}")).unwrap();
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
            SnapshotId::parse(format!("fact-audit-snapshot-{suffix}")).unwrap(),
        )
        .unwrap();
    id
}

fn add_fact(
    repository: &SqliteStoryRepository,
    project_id: &ProjectId,
    suffix: &str,
    number: usize,
) {
    let commit = format!("accept-{suffix}-{number}");
    let fact = format!("fact-{suffix}-{number}");
    let snapshot = format!("fact-audit-snapshot-{suffix}");
    repository.connection.execute(
        "INSERT INTO story_acceptances_v2(commit_id, project_id, branch_id, object_id, object_kind, accepted_revision_id, base_revision, result_revision, base_snapshot_id, result_snapshot_id, operation) VALUES (?1, ?2, 'main', 'artifact-brief', 'brief', 'revision-1', 0, 1, ?3, ?3, 'accept')",
        params![commit, project_id.as_str(), snapshot],
    ).unwrap();
    repository.connection.execute(
        "INSERT INTO story_facts_v2(fact_id, project_id, payload_json, accepted_by_commit_id, project_revision) VALUES (?1, ?2, ?3, ?4, 1)",
        params![fact, project_id.as_str(), json!({"id":fact}).to_string(), commit],
    ).unwrap();
}

#[test]
fn fact_audit_persists_chunks_and_revokes_completed_evidence_after_authority_change() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("facts.sqlite3");
    let mut repository = SqliteStoryRepository::open(&path).unwrap();
    let id = project(&mut repository, "many");
    for number in 0..205 {
        add_fact(&repository, &id, "many", number);
    }
    let queued = repository.start_fact_audit(&id).unwrap();
    assert_eq!(repository.start_fact_audit(&id).unwrap(), queued);
    let first = repository.advance_fact_audit(&id, &queued.id).unwrap();
    assert_eq!(
        (
            first.status.as_str(),
            first.audit.facts,
            first.verified_consistent
        ),
        ("running", 100, None)
    );
    let paused = repository
        .control_fact_audit(&id, &queued.id, "pause", first.version)
        .unwrap();
    drop(repository);
    let mut reopened = SqliteStoryRepository::open(&path).unwrap();
    assert_eq!(reopened.fact_audit_job(&id, &queued.id).unwrap(), paused);
    let resumed = reopened
        .control_fact_audit(&id, &queued.id, "resume", paused.version)
        .unwrap();
    assert_eq!(resumed.status, "queued");
    let done = loop {
        let next = reopened.advance_fact_audit(&id, &queued.id).unwrap();
        if next.status == "completed" {
            break next;
        }
    };
    assert_eq!(done.verified_consistent, Some(true));
    assert_eq!(done.audit, reopened.audit_fact_ledger(&id).unwrap());

    reopened.connection.execute(
        "UPDATE story_acceptances_v2 SET project_id = 'another-project' WHERE commit_id = 'accept-many-0'",
        [],
    ).unwrap();
    let stale = reopened.fact_audit_job(&id, &queued.id).unwrap();
    assert_eq!(
        (stale.status.as_str(), stale.verified_consistent),
        ("stale", None)
    );
    let next = reopened.start_fact_audit(&id).unwrap();
    let inconsistent = loop {
        let result = reopened.advance_fact_audit(&id, &next.id).unwrap();
        if result.status == "completed" {
            break result;
        }
    };
    assert_eq!(inconsistent.verified_consistent, Some(false));
    assert_eq!(inconsistent.audit.invalid_links, 1);
    assert_eq!(inconsistent.audit.affected_facts, vec!["fact-many-0"]);
    assert_eq!(reopened.facts(&id), Err(RepositoryError::CorruptState));
}

#[test]
fn fact_audit_stales_on_fact_change_and_keeps_empty_projects_verifiable() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let id = project(&mut repository, "live");
    let empty = repository.start_fact_audit(&id).unwrap();
    let done = repository.advance_fact_audit(&id, &empty.id).unwrap();
    assert_eq!(
        (done.audit.facts, done.verified_consistent),
        (0, Some(true))
    );
    add_fact(&repository, &id, "live", 0);
    assert_eq!(
        repository.fact_audit_job(&id, &empty.id).unwrap().status,
        "stale"
    );

    let queued = repository.start_fact_audit(&id).unwrap();
    repository.connection.execute(
        "UPDATE story_facts_v2 SET payload_json = '{\"id\":\"wrong\"}' WHERE fact_id = 'fact-live-0'",
        [],
    ).unwrap();
    assert_eq!(
        repository
            .advance_fact_audit(&id, &queued.id)
            .unwrap()
            .status,
        "stale"
    );
    let new_job = repository.start_fact_audit(&id).unwrap();
    let paused = repository
        .control_fact_audit(&id, &new_job.id, "pause", new_job.version)
        .unwrap();
    let cancelled = repository
        .control_fact_audit(&id, &new_job.id, "cancel", paused.version)
        .unwrap();
    assert_eq!(cancelled.status, "cancelled");
    assert_eq!(repository.next_fact_audit_job().unwrap(), None);
}

#[test]
fn inserting_a_previously_missing_acceptance_snapshot_revokes_the_old_audit() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let id = project(&mut repository, "missing");
    add_fact(&repository, &id, "missing", 0);
    repository.connection.execute(
        "UPDATE story_acceptances_v2 SET result_snapshot_id = 'fact-audit-repaired-snapshot' WHERE commit_id = 'accept-missing-0'",
        [],
    ).unwrap();
    let queued = repository.start_fact_audit(&id).unwrap();
    let inconsistent = repository.advance_fact_audit(&id, &queued.id).unwrap();
    assert_eq!(inconsistent.verified_consistent, Some(false));
    repository.connection.execute(
        "INSERT INTO story_snapshots_v2(snapshot_id, project_id, branch_id) VALUES ('fact-audit-repaired-snapshot', ?1, 'main')",
        [id.as_str()],
    ).unwrap();
    assert_eq!(
        repository.fact_audit_job(&id, &queued.id).unwrap().status,
        "stale"
    );
    let restarted = repository.start_fact_audit(&id).unwrap();
    assert_eq!(
        repository
            .advance_fact_audit(&id, &restarted.id)
            .unwrap()
            .verified_consistent,
        Some(true)
    );
}

#[test]
fn certified_fact_reads_keep_certificate_and_rows_in_one_sqlite_snapshot() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("fact-snapshot.sqlite3");
    let mut repository = SqliteStoryRepository::open(&path).unwrap();
    let id = project(&mut repository, "concurrent");
    add_fact(&repository, &id, "concurrent", 0);
    let job = repository.start_fact_audit(&id).unwrap();
    assert_eq!(
        repository
            .advance_fact_audit(&id, &job.id)
            .unwrap()
            .verified_consistent,
        Some(true)
    );
    drop(repository);

    let reader = SqliteStoryRepository::open(&path).unwrap();
    let writer = SqliteStoryRepository::open(&path).unwrap();
    reader
        .with_read_snapshot(|| {
            let before = reader.facts(&id)?;
            writer.connection.execute(
                "UPDATE story_facts_v2 SET payload_json = '{\"id\":\"wrong\"}' WHERE fact_id = 'fact-concurrent-0'",
                [],
            ).unwrap();
            assert_eq!(reader.facts(&id)?, before);
            Ok::<(), RepositoryError>(())
        })
        .unwrap()
        .unwrap();
    assert_eq!(reader.facts(&id), Err(RepositoryError::CorruptState));
    assert_eq!(reader.fact_audit_job(&id, &job.id).unwrap().status, "stale");
}

use crate::{FactAuditJobsRepository, FactIntegrityRepository, ProjectRevisionRepository, ReadSnapshotRepository, WorkflowRepository};
