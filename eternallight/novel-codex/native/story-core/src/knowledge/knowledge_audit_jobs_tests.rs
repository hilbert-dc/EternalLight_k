use pretty_assertions::assert_eq;
use rusqlite::params;
use serde_json::json;

use crate::{BranchId, ProjectId, ProjectRecord, SnapshotId, SqliteStoryRepository};

fn project(repository: &mut SqliteStoryRepository, suffix: &str) -> ProjectId {
    let project_id = ProjectId::parse(format!("knowledge-audit-project-{suffix}")).unwrap();
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
            SnapshotId::parse(format!("knowledge-audit-snapshot-{suffix}")).unwrap(),
        )
        .unwrap();
    project_id
}

fn batch(
    repository: &SqliteStoryRepository,
    project_id: &ProjectId,
    number: usize,
    assertions: &str,
) {
    repository.connection.execute(
        "INSERT INTO story_revisions_v2(revision_id, project_id, object_id, object_kind, content_digest, content_json) VALUES (?1, ?2, ?3, 'draft', 'digest', '{\"body\":\"林夕在港口\"}')",
        params![format!("knowledge-audit-revision-{number}"), project_id.as_str(), format!("draft-knowledge-audit-{number}")],
    ).unwrap();
    repository.connection.execute(
        "INSERT INTO story_modeling_batches_v2(batch_id, project_id, object_id, source_revision_id, input_project_revision, extractor_version, status, assertions_json, entities_json, created_at, published_at) VALUES (?1, ?2, ?3, ?4, 0, 'test', 'stale', ?5, ?6, 1, 2)",
        params![format!("knowledge-audit-batch-{number}"), project_id.as_str(),
            format!("draft-knowledge-audit-{number}"), format!("knowledge-audit-revision-{number}"), assertions,
            json!([
                {"id":"linxi","kind":"character","displayName":"林夕","evidence":{"quote":"林夕在港口","start":0,"end":5}},
                {"id":"linxi-at-port","kind":"proposition","displayName":"在港口","evidence":{"quote":"林夕在港口","start":0,"end":5}}
            ]).to_string()],
    ).unwrap();
}

#[test]
fn knowledge_audit_resumes_from_checkpoint_and_rebuild_invalidates_old_report() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("story.sqlite3");
    let mut repository = SqliteStoryRepository::open(&path).unwrap();
    let project_id = project(&mut repository, "long");
    for number in 0..56 {
        batch(&repository, &project_id, number, "[]");
    }
    repository.rebuild_entity_index(&project_id).unwrap();
    let queued = repository.start_knowledge_audit(&project_id).unwrap();
    assert_eq!(
        repository.start_knowledge_audit(&project_id).unwrap(),
        queued
    );
    let first = repository
        .advance_knowledge_audit(&project_id, &queued.id)
        .unwrap();
    assert_eq!(first.status, "running");
    assert_eq!(first.audit.assertions.published_batches, 25);
    assert_eq!(first.audit.epistemic.published_batches, 25);
    assert_eq!(first.verified_consistent, None);
    let paused = repository
        .control_knowledge_audit(&project_id, &first.id, "pause", first.version)
        .unwrap();
    drop(repository);
    let mut reopened = SqliteStoryRepository::open(&path).unwrap();
    assert_eq!(
        reopened
            .knowledge_audit_job(&project_id, &queued.id)
            .unwrap(),
        paused
    );
    let resumed = reopened
        .control_knowledge_audit(&project_id, &paused.id, "resume", paused.version)
        .unwrap();
    assert_eq!(resumed.status, "queued");
    let done = loop {
        let next = reopened
            .advance_knowledge_audit(&project_id, &queued.id)
            .unwrap();
        if next.status == "completed" {
            break next;
        }
    };
    assert_eq!(done.verified_consistent, Some(true));
    assert_eq!(
        done.audit.assertions,
        reopened.audit_assertion_index(&project_id).unwrap()
    );
    assert_eq!(
        done.audit.epistemic,
        reopened.audit_epistemic_index(&project_id).unwrap()
    );
    reopened.rebuild_assertion_index(&project_id).unwrap();
    let stale = reopened.knowledge_audit_job(&project_id, &done.id).unwrap();
    assert_eq!(stale.status, "stale");
    assert_eq!(stale.verified_consistent, None);
    let replacement = reopened.start_knowledge_audit(&project_id).unwrap();
    reopened.rebuild_epistemic_index(&project_id).unwrap();
    assert_eq!(
        reopened
            .advance_knowledge_audit(&project_id, &replacement.id)
            .unwrap()
            .status,
        "stale"
    );
}

#[test]
fn knowledge_audit_detects_both_missing_indexes_and_unpublished_orphans() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let project_id = project(&mut repository, "missing");
    let assertion = json!({
        "kind":"epistemic", "subjectId":"linxi-at-port", "propositionId":"linxi-at-port",
        "epistemicOwnerId":"linxi", "epistemicState":"known",
        "predicate":"known","value":true,"evidence":{"quote":"林夕在港口","start":0,"end":5},
    });
    batch(&repository, &project_id, 0, &json!([assertion]).to_string());
    repository.connection.execute(
        "INSERT INTO story_modeling_batches_v2(batch_id, project_id, object_id, source_revision_id, input_project_revision, extractor_version, status, created_at) VALUES ('knowledge-unpublished', ?1, 'draft-knowledge-audit-0', 'knowledge-audit-revision-0', 0, 'unpublished', 'queued', 1)",
        [project_id.as_str()],
    ).unwrap();
    repository.connection.execute(
        "INSERT INTO story_assertion_claims_v1(project_id, batch_id, assertion_index, assertion_json) VALUES (?1, 'knowledge-unpublished', 0, '{}')",
        [project_id.as_str()],
    ).unwrap();
    repository.connection.execute(
        "INSERT INTO story_epistemic_claims_v1(project_id, batch_id, assertion_index, owner_id, proposition_id, proposition_source_batch_id, epistemic_state, assertion_json) VALUES (?1, 'knowledge-unpublished', 0, 'ghost', 'ghost', 'knowledge-unpublished', 'known', '{}')",
        [project_id.as_str()],
    ).unwrap();
    let queued = repository.start_knowledge_audit(&project_id).unwrap();
    let done = repository
        .advance_knowledge_audit(&project_id, &queued.id)
        .unwrap();
    assert_eq!(done.status, "completed");
    assert_eq!(done.verified_consistent, Some(false));
    assert_eq!(done.audit.assertions.missing_claims, 1);
    assert_eq!(done.audit.assertions.extra_claims, 1);
    assert_eq!(done.audit.epistemic.missing_claims, 1);
    assert_eq!(done.audit.epistemic.extra_claims, 1);
    assert_eq!(
        done.audit.assertions,
        repository.audit_assertion_index(&project_id).unwrap()
    );
    assert_eq!(
        done.audit.epistemic,
        repository.audit_epistemic_index(&project_id).unwrap()
    );
}

#[test]
fn malformed_knowledge_source_fails_without_changing_formal_state() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let project_id = project(&mut repository, "bad");
    batch(&repository, &project_id, 0, "{");
    let revision = repository.project_revision(&project_id).unwrap();
    let queued = repository.start_knowledge_audit(&project_id).unwrap();
    let failed = repository
        .advance_knowledge_audit(&project_id, &queued.id)
        .unwrap();
    assert_eq!(failed.status, "failed");
    assert_eq!(failed.verified_consistent, None);
    assert!(failed.error.unwrap().contains("knowledge-audit-batch-0"));
    assert_eq!(repository.project_revision(&project_id).unwrap(), revision);
}

#[test]
fn knowledge_audit_finds_changed_rows_and_never_completes_after_source_change() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let project_id = project(&mut repository, "changed");
    let assertion = json!({
        "kind":"epistemic", "subjectId":"linxi-at-port", "propositionId":"linxi-at-port",
        "epistemicOwnerId":"linxi", "epistemicState":"known",
        "predicate":"known","value":true,"evidence":{"quote":"林夕在港口","start":0,"end":5},
    });
    batch(&repository, &project_id, 0, &json!([assertion]).to_string());
    repository.rebuild_assertion_index(&project_id).unwrap();
    repository.rebuild_epistemic_index(&project_id).unwrap();
    repository.connection.execute(
        "UPDATE story_assertion_claims_v1 SET assertion_json = '{\"corrupt\":true}' WHERE project_id = ?1",
        [project_id.as_str()],
    ).unwrap();
    repository.connection.execute(
        "UPDATE story_epistemic_claims_v1 SET epistemic_state = 'suspected' WHERE project_id = ?1",
        [project_id.as_str()],
    ).unwrap();
    let queued = repository.start_knowledge_audit(&project_id).unwrap();
    let done = repository
        .advance_knowledge_audit(&project_id, &queued.id)
        .unwrap();
    assert_eq!(done.status, "completed");
    assert_eq!(done.verified_consistent, Some(false));
    assert_eq!(done.audit.assertions.changed_claims, 1);
    assert_eq!(done.audit.epistemic.changed_claims, 1);
    assert_eq!(
        done.audit.assertions,
        repository.audit_assertion_index(&project_id).unwrap()
    );
    assert_eq!(
        done.audit.epistemic,
        repository.audit_epistemic_index(&project_id).unwrap()
    );
    let pending = repository.start_knowledge_audit(&project_id).unwrap();
    batch(&repository, &project_id, 1, "[]");
    let stale = repository
        .advance_knowledge_audit(&project_id, &pending.id)
        .unwrap();
    assert_eq!(stale.status, "stale");
    assert_eq!(stale.verified_consistent, None);
}

#[test]
fn knowledge_audit_queue_is_bounded() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    for index in 0..16 {
        let id = project(&mut repository, &format!("queue-{index}"));
        repository.start_knowledge_audit(&id).unwrap();
    }
    let overflow = project(&mut repository, "queue-overflow");
    assert!(repository.start_knowledge_audit(&overflow).is_err());
}

#[test]
fn automatic_long_source_audits_respect_author_controls_and_do_not_spin() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("automatic-audit.sqlite3");
    let mut repository = SqliteStoryRepository::open(&path).unwrap();
    let id = project(&mut repository, "automatic");
    for number in 0..200 {
        batch(&repository, &id, number, "[]");
    }
    assert_eq!(repository.schedule_knowledge_audit().unwrap(), None);
    batch(&repository, &id, 200, "[]");
    repository.rebuild_entity_index(&id).unwrap();
    let queued = repository.schedule_knowledge_audit().unwrap().unwrap();
    assert_eq!(
        (queued.project_id.as_str(), queued.status.as_str()),
        (id.as_str(), "queued")
    );
    let mut reopened = SqliteStoryRepository::open(&path).unwrap();
    assert_eq!(reopened.schedule_knowledge_audit().unwrap(), None);
    let paused = repository
        .control_knowledge_audit(&id, &queued.id, "pause", queued.version)
        .unwrap();
    batch(&repository, &id, 201, "[]");
    assert_eq!(reopened.schedule_knowledge_audit().unwrap(), None);
    repository
        .control_knowledge_audit(&id, &paused.id, "cancel", paused.version)
        .unwrap();
    assert_eq!(reopened.schedule_knowledge_audit().unwrap(), None);
    repository.rebuild_entity_index(&id).unwrap();
    let explicit = repository.start_knowledge_audit(&id).unwrap();
    let completed = loop {
        let job = repository
            .advance_knowledge_audit(&id, &explicit.id)
            .unwrap();
        if job.status != "running" {
            break job;
        }
    };
    assert_eq!(completed.verified_consistent, Some(true));
    assert_eq!(reopened.schedule_knowledge_audit().unwrap(), None);
    repository.connection.execute("UPDATE story_entity_names_v2 SET name='forged' WHERE batch_id='knowledge-audit-batch-0' AND entity_id='linxi'",[]).unwrap();
    let changed = reopened.schedule_knowledge_audit().unwrap().unwrap();
    let inconsistent = loop {
        let job = repository
            .advance_knowledge_audit(&id, &changed.id)
            .unwrap();
        if job.status != "running" {
            break job;
        }
    };
    assert_eq!(inconsistent.verified_consistent, Some(false));
    assert_eq!(reopened.schedule_knowledge_audit().unwrap(), None);
    repository.connection.execute("UPDATE story_modeling_batches_v2 SET entities_json='invalid' WHERE batch_id='knowledge-audit-batch-0'",[]).unwrap();
    let changed = reopened.schedule_knowledge_audit().unwrap().unwrap();
    let failed = repository
        .advance_knowledge_audit(&id, &changed.id)
        .unwrap();
    assert_eq!(failed.status, "failed");
    batch(&repository, &id, 202, "[]");
    assert_eq!(reopened.schedule_knowledge_audit().unwrap(), None);
}

#[test]
fn automatic_audit_obeys_queue_capacity_and_does_not_duplicate_across_connections() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("automatic-queue.sqlite3");
    let mut repository = SqliteStoryRepository::open(&path).unwrap();
    let id = project(&mut repository, "auto-queue");
    for number in 0..201 {
        batch(&repository, &id, number, "[]");
    }
    let mut queued = Vec::new();
    for index in 0..16 {
        let other = project(&mut repository, &format!("queue-{index}"));
        queued.push((
            other.clone(),
            repository.start_knowledge_audit(&other).unwrap(),
        ));
    }
    assert_eq!(repository.schedule_knowledge_audit().unwrap(), None);
    let (other, job) = &queued[0];
    repository
        .control_knowledge_audit(other, &job.id, "cancel", job.version)
        .unwrap();
    let barrier = std::sync::Barrier::new(2);
    let results = std::thread::scope(|scope| {
        let handles = (0..2)
            .map(|_| {
                scope.spawn(|| {
                    let mut connection = SqliteStoryRepository::open(&path).unwrap();
                    barrier.wait();
                    connection.schedule_knowledge_audit().unwrap()
                })
            })
            .collect::<Vec<_>>();
        handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(results.iter().filter(|result| result.is_some()).count(), 1);
    assert_eq!(
        results.into_iter().flatten().next().unwrap().project_id,
        id.to_string()
    );
}

#[test]
fn unified_source_audit_detects_entity_gaps_and_revokes_on_entity_mutation() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("unified.sqlite3");
    let mut repository = SqliteStoryRepository::open(&path).unwrap();
    let id = project(&mut repository, "unified");
    batch(&repository, &id, 0, "[]");
    let job = repository.start_knowledge_audit(&id).unwrap();
    let missing = repository.advance_knowledge_audit(&id, &job.id).unwrap();
    assert_eq!(
        (
            missing.verified_consistent,
            missing.audit.entities.missing_claims,
            missing.audit.entities.missing_names
        ),
        (Some(false), 2, 2)
    );
    repository.rebuild_entity_index(&id).unwrap();
    assert_eq!(
        repository
            .knowledge_audit_job(&id, &missing.id)
            .unwrap()
            .status,
        "stale"
    );
    let job = repository.start_knowledge_audit(&id).unwrap();
    let clean = repository.advance_knowledge_audit(&id, &job.id).unwrap();
    assert_eq!(clean.verified_consistent, Some(true));
    repository
        .connection
        .execute("UPDATE story_entity_claims_v2 SET kind=kind", [])
        .unwrap();
    repository
        .connection
        .execute("UPDATE story_entity_names_v2 SET name=name", [])
        .unwrap();
    assert_eq!(
        repository.knowledge_audit_job(&id, &clean.id).unwrap(),
        clean
    );
    let other = project(&mut repository, "other");
    batch(&repository, &other, 7, "[]");
    repository.rebuild_entity_index(&other).unwrap();
    assert_eq!(
        repository.knowledge_audit_job(&id, &clean.id).unwrap(),
        clean
    );
    repository
        .connection
        .execute(
            "DELETE FROM story_entity_names_v2 WHERE project_id=?1",
            [id.as_str()],
        )
        .unwrap();
    drop(repository);
    let mut repository = SqliteStoryRepository::open(&path).unwrap();
    let stale = repository.knowledge_audit_job(&id, &clean.id).unwrap();
    assert_eq!(
        (stale.status.as_str(), stale.verified_consistent),
        ("stale", None)
    );
    let job = repository.start_knowledge_audit(&id).unwrap();
    let missing = repository.advance_knowledge_audit(&id, &job.id).unwrap();
    assert_eq!(
        (
            missing.verified_consistent,
            missing.audit.entities.missing_names
        ),
        (Some(false), 2)
    );
    let pending = repository.start_knowledge_audit(&id).unwrap();
    repository
        .connection
        .execute(
            "UPDATE story_entity_claims_v2 SET kind='item' WHERE project_id=?1",
            [id.as_str()],
        )
        .unwrap();
    assert_eq!(
        repository
            .advance_knowledge_audit(&id, &pending.id)
            .unwrap()
            .status,
        "stale"
    );
}

#[test]
fn entity_index_epoch_overflow_rejects_the_index_write_atomically() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let id = project(&mut repository, "entity-overflow");
    batch(&repository, &id, 0, "[]");
    repository.rebuild_entity_index(&id).unwrap();
    repository
        .connection
        .execute(
            "UPDATE story_entity_index_state_v1 SET generation=?1 WHERE project_id=?2",
            params![i64::MAX, id.as_str()],
        )
        .unwrap();
    assert!(
        repository
            .connection
            .execute(
                "DELETE FROM story_entity_names_v2 WHERE project_id=?1",
                [id.as_str()]
            )
            .is_err()
    );
    assert!(repository.audit_entity_index(&id).unwrap().complete());
}

#[test]
fn knowledge_index_writes_revoke_completed_and_inflight_proofs() {
    for (mutation, changes_other) in [
        (
            "UPDATE story_modeling_batches_v2 SET status='ready' WHERE batch_id='knowledge-audit-batch-0'",
            false,
        ),
        (
            "UPDATE story_assertion_claims_v1 SET assertion_json='{}' WHERE batch_id='knowledge-audit-batch-0'",
            false,
        ),
        (
            "DELETE FROM story_assertion_claims_v1 WHERE batch_id='knowledge-audit-batch-0'",
            false,
        ),
        (
            "UPDATE story_epistemic_claims_v1 SET epistemic_state='suspected' WHERE batch_id='knowledge-audit-batch-0'",
            false,
        ),
        (
            "DELETE FROM story_epistemic_claims_v1 WHERE batch_id='knowledge-audit-batch-0'",
            false,
        ),
        (
            "INSERT INTO story_knowledge_heads_v2(project_id,object_id,batch_id) VALUES('knowledge-audit-project-fences','draft-knowledge-audit-0','knowledge-audit-batch-0')",
            false,
        ),
        (
            "INSERT INTO story_model_dependencies_v1(project_id,batch_id,source_batch_id) VALUES('knowledge-audit-project-fences','knowledge-audit-batch-0','knowledge-audit-batch-0')",
            false,
        ),
        (
            "INSERT INTO story_object_heads_v2(project_id,object_id,object_kind,candidate_revision_id) VALUES('knowledge-audit-project-fences','draft-knowledge-audit-0','draft','knowledge-audit-revision-0')",
            false,
        ),
        (
            "UPDATE story_assertion_claims_v1 SET project_id='knowledge-audit-project-other-fence' WHERE batch_id='knowledge-audit-batch-0'",
            true,
        ),
        (
            "UPDATE story_epistemic_claims_v1 SET project_id='knowledge-audit-project-other-fence' WHERE batch_id='knowledge-audit-batch-0'",
            true,
        ),
    ] {
        let mut repository = SqliteStoryRepository::in_memory().unwrap();
        let id = project(&mut repository, "fences");
        let other = project(&mut repository, "other-fence");
        let assertion = json!({"kind":"epistemic","subjectId":"linxi-at-port","propositionId":"linxi-at-port","epistemicOwnerId":"linxi","epistemicState":"known","predicate":"known","value":true,"evidence":{"quote":"林夕在港口","start":0,"end":5}});
        batch(&repository, &id, 0, &json!([assertion]).to_string());
        repository.rebuild_entity_index(&id).unwrap();
        repository.rebuild_assertion_index(&id).unwrap();
        repository.rebuild_epistemic_index(&id).unwrap();
        let started = repository.start_knowledge_audit(&id).unwrap();
        let clean = repository
            .advance_knowledge_audit(&id, &started.id)
            .unwrap();
        assert_eq!(clean.verified_consistent, Some(true));
        let other_started = repository.start_knowledge_audit(&other).unwrap();
        let other_clean = repository
            .advance_knowledge_audit(&other, &other_started.id)
            .unwrap();
        repository
            .connection
            .execute(
                "UPDATE story_assertion_claims_v1 SET assertion_json=assertion_json",
                [],
            )
            .unwrap();
        repository
            .connection
            .execute(
                "UPDATE story_epistemic_claims_v1 SET epistemic_state=epistemic_state",
                [],
            )
            .unwrap();
        assert_eq!(
            repository.knowledge_audit_job(&id, &clean.id).unwrap(),
            clean
        );
        let pending = repository.start_knowledge_audit(&id).unwrap();
        repository.connection.execute(mutation, []).unwrap();
        let old = repository.knowledge_audit_job(&id, &clean.id).unwrap();
        assert_eq!(
            (old.status.as_str(), old.verified_consistent),
            ("stale", None),
            "{mutation}"
        );
        assert_eq!(
            repository
                .advance_knowledge_audit(&id, &pending.id)
                .unwrap()
                .status,
            "stale"
        );
        let other_after = repository
            .knowledge_audit_job(&other, &other_clean.id)
            .unwrap();
        if changes_other {
            assert_eq!(
                (other_after.status.as_str(), other_after.verified_consistent),
                ("stale", None)
            );
        } else {
            assert_eq!(other_after, other_clean);
        }
    }
}

#[test]
fn bound_draft_bytes_split_audit_ticks_without_starving_one_large_source() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let project_id = project(&mut repository, "draft-budget");
    for (number, size) in [300_000, 160_000, 160_000].into_iter().enumerate() {
        batch(&repository, &project_id, number, "[]");
        repository
            .connection
            .execute(
                "UPDATE story_revisions_v2 SET content_json=?1 WHERE revision_id=?2",
                params![
                    json!({"body":format!("林夕在港口{}", "文".repeat(size / 3))}).to_string(),
                    format!("knowledge-audit-revision-{number}")
                ],
            )
            .unwrap();
    }
    let queued = repository.start_knowledge_audit(&project_id).unwrap();
    repository.rebuild_entity_index(&project_id).unwrap();
    // Rebuilding after creation revokes the queued proof; bind a fresh job.
    assert_eq!(
        repository
            .knowledge_audit_job(&project_id, &queued.id)
            .unwrap()
            .status,
        "stale"
    );
    let queued = repository.start_knowledge_audit(&project_id).unwrap();
    for (count, status, certified) in [
        (1, "running", None),
        (2, "running", None),
        (3, "completed", Some(true)),
    ] {
        let progress = repository
            .advance_knowledge_audit(&project_id, &queued.id)
            .unwrap();
        assert_eq!(
            (
                progress.audit.assertions.published_batches,
                progress.status.as_str(),
                progress.verified_consistent
            ),
            (count, status, certified)
        );
    }
}

use crate::{AssertionIndexIntegrityRepository, EntityIndexIntegrityRepository, EpistemicIndexIntegrityRepository, KnowledgeAuditJobsRepository, KnowledgeAuditSchedulingRepository, ProjectRevisionRepository};
