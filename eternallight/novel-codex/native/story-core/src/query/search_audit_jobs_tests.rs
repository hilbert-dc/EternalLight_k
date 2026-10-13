use crate::{
    BranchId, CommitId, CommitRequest, IdempotencyKey, ObjectId, ProjectId, ProjectRecord,
    RepositoryError, RevisionId, RevisionRecord, SnapshotId, SqliteStoryRepository,
};

fn project_with_revisions(repository: &mut SqliteStoryRepository, count: usize) -> ProjectId {
    let project_id = ProjectId::parse("project-audit").unwrap();
    repository
        .create_project(
            ProjectRecord {
                project_id: project_id.clone(),
                title: "Audit".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            BranchId::parse("main").unwrap(),
            SnapshotId::parse("snapshot-0").unwrap(),
        )
        .unwrap();
    let revisions = (0..count)
        .map(|index| RevisionRecord {
            project_id: project_id.clone(),
            object_id: ObjectId::parse(format!("object-{index}")).unwrap(),
            object_kind: "brief".into(),
            revision_id: RevisionId::parse(format!("revision-{index}")).unwrap(),
            parent_revision_id: None,
            content_digest: format!("digest-{index}"),
            content_json: serde_json::json!({"text": format!("线索{index}")}).to_string(),
        })
        .collect();
    repository
        .apply_commit(CommitRequest {
            project_id: project_id.clone(),
            branch_id: BranchId::parse("main").unwrap(),
            commit_id: CommitId::parse("commit-1").unwrap(),
            idempotency_key: IdempotencyKey::parse("request-1").unwrap(),
            request_digest: "request-digest".into(),
            base_snapshot_id: SnapshotId::parse("snapshot-0").unwrap(),
            next_snapshot_id: SnapshotId::parse("snapshot-1").unwrap(),
            revisions,
        })
        .unwrap();
    project_id
}

#[test]
fn audit_job_resumes_from_durable_checkpoint_and_matches_full_audit() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("story.sqlite3");
    let mut repository = SqliteStoryRepository::open(&path).unwrap();
    let project_id = project_with_revisions(&mut repository, 205);
    let queued = repository.start_search_audit(&project_id).unwrap();
    assert_eq!(repository.start_search_audit(&project_id).unwrap(), queued);
    let first = repository
        .advance_search_audit(&project_id, &queued.id)
        .unwrap();
    assert_eq!(first.status, "running");
    assert_eq!(first.verified_consistent, None);
    assert_eq!(first.audit.revision_count, 100);
    let paused = repository
        .control_search_audit(&project_id, &queued.id, "pause", first.version)
        .unwrap();
    assert_eq!(paused.status, "paused");
    assert!(matches!(
        repository.advance_search_audit(&project_id, &queued.id),
        Err(RepositoryError::InvalidContent(_))
    ));
    drop(repository);
    let mut reopened = SqliteStoryRepository::open(&path).unwrap();
    assert_eq!(
        reopened.search_audit_job(&project_id, &queued.id).unwrap(),
        paused
    );
    let resumed = reopened
        .control_search_audit(&project_id, &queued.id, "resume", paused.version)
        .unwrap();
    assert_eq!(resumed.status, "queued");
    let second = reopened
        .advance_search_audit(&project_id, &queued.id)
        .unwrap();
    assert_eq!(second.audit.revision_count, 200);
    let done = reopened
        .advance_search_audit(&project_id, &queued.id)
        .unwrap();
    assert_eq!(done.status, "completed");
    assert_eq!(done.verified_consistent, Some(true));
    assert_eq!(done.audit.revision_count, 205);
    assert_eq!(
        done.audit,
        reopened.audit_search_index(&project_id).unwrap()
    );
    assert!(done.audit.complete());
    assert_eq!(reopened.next_search_audit_job().unwrap(), None);
}

#[test]
fn audit_job_never_reports_complete_after_source_or_index_changes() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let project_id = project_with_revisions(&mut repository, 101);
    let started = repository.start_search_audit(&project_id).unwrap();
    repository
        .advance_search_audit(&project_id, &started.id)
        .unwrap();
    repository.rebuild_search_index(&project_id, 0).unwrap();
    let stale = repository
        .advance_search_audit(&project_id, &started.id)
        .unwrap();
    assert_eq!(stale.status, "stale");
    assert_eq!(stale.audit.revision_count, 100);
    let next = repository.start_search_audit(&project_id).unwrap();
    assert_ne!(next.id, stale.id);
    repository.connection.execute(
        "INSERT INTO story_revisions_v2(revision_id, project_id, object_id, object_kind, content_digest, content_json) VALUES ('late', ?1, 'late', 'brief', 'digest', '{\"text\":\"late\"}')",
        [project_id.as_str()],
    ).unwrap();
    let replacement = repository.start_search_audit(&project_id).unwrap();
    assert_ne!(replacement.id, next.id);
    assert_eq!(
        repository
            .search_audit_job(&project_id, &next.id)
            .unwrap()
            .status,
        "stale"
    );
    assert_eq!(replacement.status, "queued");
}

#[test]
fn audit_job_can_be_cancelled_without_leaving_an_active_slot() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let project_id = project_with_revisions(&mut repository, 1);
    let started = repository.start_search_audit(&project_id).unwrap();
    let cancelled = repository
        .control_search_audit(&project_id, &started.id, "cancel", started.version)
        .unwrap();
    assert_eq!(cancelled.status, "cancelled");
    assert!(matches!(
        repository.control_search_audit(&project_id, &started.id, "resume", cancelled.version),
        Err(RepositoryError::InvalidContent(_))
    ));
    assert_ne!(
        repository.start_search_audit(&project_id).unwrap().id,
        cancelled.id
    );
}

#[test]
fn audit_queue_is_bounded_and_serves_each_project() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let mut jobs = Vec::new();
    for index in 0..16 {
        let project_id = ProjectId::parse(format!("audit-project-{index}")).unwrap();
        repository
            .create_project(
                ProjectRecord {
                    project_id: project_id.clone(),
                    title: "Audit".into(),
                    logline: String::new(),
                    created_at: 1,
                    updated_at: 1,
                },
                BranchId::parse("main").unwrap(),
                SnapshotId::parse(format!("audit-snapshot-{index}")).unwrap(),
            )
            .unwrap();
        let job = repository.start_search_audit(&project_id).unwrap();
        jobs.push((project_id, job));
    }
    let extra = ProjectId::parse("audit-project-extra").unwrap();
    repository
        .create_project(
            ProjectRecord {
                project_id: extra.clone(),
                title: "Extra".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            BranchId::parse("main").unwrap(),
            SnapshotId::parse("audit-snapshot-extra").unwrap(),
        )
        .unwrap();
    assert!(matches!(
        repository.start_search_audit(&extra),
        Err(RepositoryError::InvalidContent(_))
    ));
    let first = repository.next_search_audit_job().unwrap().unwrap();
    assert_eq!(first, (jobs[0].0.clone(), jobs[0].1.id.clone()));
    repository.advance_search_audit(&first.0, &first.1).unwrap();
    let second = repository.next_search_audit_job().unwrap().unwrap();
    assert_eq!(second, (jobs[1].0.clone(), jobs[1].1.id.clone()));
    assert!(repository.start_search_audit(&extra).is_ok());
}

#[test]
fn completed_audit_reports_index_mismatch_without_rewriting_authority() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let project_id = project_with_revisions(&mut repository, 1);
    repository
        .connection
        .execute(
            "DELETE FROM story_search_grams_v2 WHERE project_id = ?1",
            [project_id.as_str()],
        )
        .unwrap();
    let before = repository
        .revision_content(&RevisionId::parse("revision-0").unwrap())
        .unwrap();
    let started = repository.start_search_audit(&project_id).unwrap();
    let done = repository
        .advance_search_audit(&project_id, &started.id)
        .unwrap();
    assert_eq!(done.status, "completed");
    assert_eq!(done.verified_consistent, Some(false));
    assert!(!done.audit.complete());
    assert_eq!(done.audit.mismatched_revision_ids, vec!["revision-0"]);
    assert_eq!(
        repository
            .revision_content(&RevisionId::parse("revision-0").unwrap())
            .unwrap(),
        before
    );
}

#[test]
fn completed_audit_is_not_presented_as_current_after_index_rebuild() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let project_id = project_with_revisions(&mut repository, 1);
    let started = repository.start_search_audit(&project_id).unwrap();
    let done = repository
        .advance_search_audit(&project_id, &started.id)
        .unwrap();
    assert_eq!(done.verified_consistent, Some(true));

    repository.rebuild_search_index(&project_id, 0).unwrap();
    let outdated = repository.search_audit_job(&project_id, &done.id).unwrap();
    assert_eq!(outdated.status, "stale");
    assert_eq!(outdated.verified_consistent, None);
    assert_eq!(
        repository.latest_search_audit_job(&project_id).unwrap(),
        Some(outdated)
    );
    assert_ne!(
        repository.start_search_audit(&project_id).unwrap().id,
        done.id
    );
}

#[test]
fn audit_chunk_byte_budget_keeps_large_revisions_in_separate_transactions() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let project_id = ProjectId::parse("large-audit-project").unwrap();
    repository
        .create_project(
            ProjectRecord {
                project_id: project_id.clone(),
                title: "Large".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            BranchId::parse("main").unwrap(),
            SnapshotId::parse("large-snapshot-0").unwrap(),
        )
        .unwrap();
    let revisions = (0..2)
        .map(|index| RevisionRecord {
            project_id: project_id.clone(),
            object_id: ObjectId::parse(format!("large-object-{index}")).unwrap(),
            object_kind: "draft".into(),
            revision_id: RevisionId::parse(format!("large-revision-{index}")).unwrap(),
            parent_revision_id: None,
            content_digest: format!("large-digest-{index}"),
            content_json: serde_json::json!({"body": "文".repeat(200_000)}).to_string(),
        })
        .collect();
    repository
        .apply_commit(CommitRequest {
            project_id: project_id.clone(),
            branch_id: BranchId::parse("main").unwrap(),
            commit_id: CommitId::parse("large-commit").unwrap(),
            idempotency_key: IdempotencyKey::parse("large-request").unwrap(),
            request_digest: "large-request-digest".into(),
            base_snapshot_id: SnapshotId::parse("large-snapshot-0").unwrap(),
            next_snapshot_id: SnapshotId::parse("large-snapshot-1").unwrap(),
            revisions,
        })
        .unwrap();
    let started = repository.start_search_audit(&project_id).unwrap();
    let first = repository
        .advance_search_audit(&project_id, &started.id)
        .unwrap();
    assert_eq!(first.status, "running");
    assert_eq!(first.audit.revision_count, 1);
    let done = repository
        .advance_search_audit(&project_id, &started.id)
        .unwrap();
    assert_eq!(done.status, "completed");
    assert_eq!(done.audit.revision_count, 2);
}

use crate::{ProjectRevisionRepository, SearchAuditJobsRepository, SearchRebuildRepository};
