use pretty_assertions::assert_eq;
use rusqlite::params;
use serde_json::{Value, json};

use crate::{
    BranchId, ProjectId, ProjectRecord, RepositoryError, SnapshotId, SqliteStoryRepository,
};

fn seed(repository: &mut SqliteStoryRepository, suffix: &str) -> ProjectId {
    let project_id = ProjectId::parse(format!("assertion-audit-{suffix}")).unwrap();
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
            SnapshotId::parse(format!("assertion-snapshot-{suffix}")).unwrap(),
        )
        .unwrap();
    let revision = format!("revision-{suffix}");
    let batch = format!("batch-{suffix}");
    let evidence = json!({"quote":"林夕","start":0,"end":2});
    let assertion = json!({"kind":"assertion","subjectId":"linxi","predicate":"appeared","value":true,"evidence":evidence});
    repository.connection.execute(
        "INSERT INTO story_revisions_v2(revision_id, project_id, object_id, object_kind, content_digest, content_json) VALUES (?1, ?2, 'draft-one', 'draft', 'digest', '{\"body\":\"林夕\"}')",
        params![revision, project_id.as_str()],
    ).unwrap();
    repository.connection.execute(
        "INSERT INTO story_modeling_batches_v2(batch_id, project_id, object_id, source_revision_id, input_project_revision, extractor_version, status, assertions_json, entities_json, created_at, published_at) VALUES (?1, ?2, 'draft-one', ?3, 0, 'test', 'stale', ?4, ?5, 1, 2)",
        params![batch, project_id.as_str(), revision, json!([assertion]).to_string(),json!([{"id":"linxi","kind":"character","displayName":"林夕","aliases":[],"evidence":evidence}]).to_string()],
    ).unwrap();
    repository.connection.execute(
        "INSERT INTO story_assertion_claims_v1(project_id, batch_id, assertion_index, subject_id, assertion_json) VALUES (?1, ?2, 0, 'linxi', ?3)",
        params![project_id.as_str(), batch, assertion.to_string()],
    ).unwrap();
    repository.rebuild_entity_index(&project_id).unwrap();
    project_id
}

#[test]
fn assertion_index_audit_and_rebuild_are_project_scoped_and_source_checked() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let project_id = seed(&mut repository, "one");
    let other_id = seed(&mut repository, "two");
    assert!(
        repository
            .audit_assertion_index(&project_id)
            .unwrap()
            .complete()
    );
    repository.connection.execute(
        "UPDATE story_assertion_claims_v1 SET subject_id = 'forged' WHERE batch_id = 'batch-one'",
        [],
    ).unwrap();
    let broken = repository.audit_assertion_index(&project_id).unwrap();
    assert_eq!(
        (
            broken.missing_claims,
            broken.extra_claims,
            broken.changed_claims
        ),
        (0, 0, 1)
    );
    let revision_before = repository.project_revision(&project_id).unwrap();
    assert!(
        repository
            .rebuild_assertion_index(&project_id)
            .unwrap()
            .complete()
    );
    assert!(
        repository
            .audit_assertion_index(&other_id)
            .unwrap()
            .complete()
    );
    assert_eq!(
        repository.project_revision(&project_id).unwrap(),
        revision_before
    );
    repository.connection.execute(
        "UPDATE story_modeling_batches_v2 SET assertions_json = 'invalid' WHERE batch_id = 'batch-one'",
        [],
    ).unwrap();
    assert!(matches!(
        repository.rebuild_assertion_index(&project_id),
        Err(RepositoryError::CorruptState)
    ));
    let retained: String = repository
        .connection
        .query_row(
            "SELECT subject_id FROM story_assertion_claims_v1 WHERE batch_id = 'batch-one'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(retained, "linxi");
}

#[test]
fn state_entity_endpoints_rebuild_from_source_and_reject_malformed_schema_atomically() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let project = seed(&mut repository, "state");
    let claim = json!({"kind":"state","subjectId":"linxi","predicate":"carried",
        "stateSchema":{"cardinality":"multiple","valueType":{"type":"entity","kind":"item"}},
        "value":{"entityId":"key"},"validFrom":1,"evidence":{"quote":"钥匙","start":0,"end":2}});
    repository.connection.execute("UPDATE story_revisions_v2 SET content_json='{\"body\":\"钥匙\"}' WHERE revision_id='revision-state'",[]).unwrap();
    repository
        .connection
        .execute(
            "UPDATE story_modeling_batches_v2 SET entities_json=?1 WHERE batch_id='batch-state'",
            [json!([
                {"id":"linxi","kind":"character","displayName":"林夕","evidence":claim["evidence"]},
                {"id":"key","kind":"item","displayName":"钥匙","evidence":claim["evidence"]}
            ])
            .to_string()],
        )
        .unwrap();
    repository
        .connection
        .execute(
            "UPDATE story_modeling_batches_v2 SET assertions_json=?1 WHERE batch_id='batch-state'",
            [json!([claim.clone()]).to_string()],
        )
        .unwrap();
    assert_eq!(
        repository
            .audit_assertion_index(&project)
            .unwrap()
            .changed_claims,
        1
    );
    let clean = repository.rebuild_assertion_index(&project).unwrap();
    assert!(clean.complete());
    let stored:(String,String)=repository.connection.query_row("SELECT target_id,assertion_json FROM story_assertion_claims_v1 WHERE batch_id='batch-state'",[],|row|Ok((row.get(0)?,row.get(1)?))).unwrap();
    assert_eq!(stored, ("key".into(), claim.to_string()));
    repository
        .connection
        .execute(
            "DELETE FROM story_assertion_claims_v1 WHERE batch_id='batch-state'",
            [],
        )
        .unwrap();
    assert_eq!(
        repository
            .audit_assertion_index(&project)
            .unwrap()
            .missing_claims,
        1
    );
    assert_eq!(repository.rebuild_assertion_index(&project).unwrap(), clean);
    repository.connection.execute("UPDATE story_modeling_batches_v2 SET assertions_json=json_set(assertions_json,'$[0].validFrom','invalid') WHERE batch_id='batch-state'",[]).unwrap();
    assert!(matches!(
        repository.rebuild_assertion_index(&project),
        Err(RepositoryError::CorruptState)
    ));
    let retained: String = repository
        .connection
        .query_row(
            "SELECT assertion_json FROM story_assertion_claims_v1 WHERE batch_id='batch-state'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(retained, claim.to_string());
}

#[test]
fn matching_indexes_cannot_certify_corrupted_original_evidence() {
    for corruption in [
        "UPDATE story_revisions_v2 SET content_json='{\"body\":\"张衡\"}' WHERE revision_id='revision-evidence'",
        "UPDATE story_revisions_v2 SET content_json='{}' WHERE revision_id='revision-evidence'",
        "UPDATE story_modeling_batches_v2 SET assertions_json=json_set(assertions_json,'$[0].evidence.end',999) WHERE batch_id='batch-evidence'",
        "UPDATE story_modeling_batches_v2 SET entities_json=json_set(entities_json,'$[0].evidence.quote','张衡') WHERE batch_id='batch-evidence'",
    ] {
        let mut repository = SqliteStoryRepository::in_memory().unwrap();
        let project = seed(&mut repository, "evidence");
        assert!(
            repository
                .audit_assertion_index(&project)
                .unwrap()
                .complete()
        );
        repository.connection.execute(corruption, []).unwrap();
        // Deliberately make the derived payload agree with damaged source JSON.
        repository.connection.execute("UPDATE story_assertion_claims_v1 SET assertion_json=(SELECT json_extract(assertions_json,'$[0]') FROM story_modeling_batches_v2 WHERE batch_id='batch-evidence') WHERE batch_id='batch-evidence'",[]).unwrap();
        let before: String = repository.connection.query_row("SELECT assertion_json FROM story_assertion_claims_v1 WHERE batch_id='batch-evidence'",[],|row|row.get(0)).unwrap();
        let revision = repository.project_revision(&project).unwrap();
        assert_eq!(
            repository.audit_assertion_index(&project),
            Err(RepositoryError::CorruptState)
        );
        assert_eq!(
            repository.rebuild_assertion_index(&project),
            Err(RepositoryError::CorruptState)
        );
        let retained: String = repository.connection.query_row("SELECT assertion_json FROM story_assertion_claims_v1 WHERE batch_id='batch-evidence'",[],|row|row.get(0)).unwrap();
        assert_eq!(retained, before);
        let job = repository.start_knowledge_audit(&project).unwrap();
        let failed = repository
            .advance_knowledge_audit(&project, &job.id)
            .unwrap();
        assert_eq!(
            (
                failed.status.as_str(),
                failed.verified_consistent,
                failed.next_rowid,
                failed.audit.assertions.published_batches
            ),
            ("failed", None, 0, 0)
        );
        assert_eq!(repository.project_revision(&project).unwrap(), revision);
    }
}

#[test]
fn event_entity_contract_is_shared_by_full_audit_rebuild_and_durable_jobs() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("event-audit.sqlite3");
    let mut repository = SqliteStoryRepository::open(&path).unwrap();
    let project = seed(&mut repository, "event");
    let other = seed(&mut repository, "unrelated");
    let claim = json!({"kind":"event","subjectId":"delivery","predicate":"transfer","storyTime":1,
        "value":{"participants":[{"role":"actor","entityId":"linxi"},{"role":"item","entityId":"letter"}],
            "preconditions":[],"effects":[],"untilExclusive":null},"evidence":{"quote":"林夕交信","start":0,"end":4}});
    let entities = json!([
        {"id":"delivery","kind":"event","displayName":"交信","aliases":[],"evidence":claim["evidence"]},
        {"id":"linxi","kind":"character","displayName":"林夕","aliases":[],"evidence":claim["evidence"]},
        {"id":"letter","kind":"item","displayName":"信","aliases":[],"evidence":claim["evidence"]}
    ]);
    repository
        .connection
        .execute(
            "UPDATE story_revisions_v2 SET content_json=?1 WHERE revision_id='revision-event'",
            [json!({"body":"林夕交信"}).to_string()],
        )
        .unwrap();
    repository.connection.execute("UPDATE story_modeling_batches_v2 SET assertions_json=?1,entities_json=?2 WHERE batch_id='batch-event'",
        params![json!([claim]).to_string(),entities.to_string()]).unwrap();
    let clean = repository.rebuild_assertion_index(&project).unwrap();
    assert!(clean.complete());
    assert_eq!(repository.audit_assertion_index(&project).unwrap(), clean);
    repository.rebuild_entity_index(&project).unwrap();
    let queued = repository.start_knowledge_audit(&project).unwrap();
    let completed = repository
        .advance_knowledge_audit(&project, &queued.id)
        .unwrap();
    assert_eq!(completed.verified_consistent, Some(true));
    let revision = repository.project_revision(&project).unwrap();
    let head = repository
        .branch_head(&project, &BranchId::parse("main").unwrap())
        .unwrap();
    for mut corrupted in [
        json!([{"id":"delivery","kind":"event"},{"id":"linxi","kind":"item"},{"id":"letter","kind":"item"}]),
        json!([{"id":"delivery","kind":"character"},{"id":"linxi","kind":"character"},{"id":"letter","kind":"item"}]),
        json!([{"id":"delivery","kind":"event"},{"id":"letter","kind":"item"}]),
        json!([{"id":"delivery","kind":"event"},{"id":"linxi","kind":"item"},{"id":"linxi","kind":"character"},{"id":"letter","kind":"item"}]),
    ] {
        for entity in corrupted.as_array_mut().unwrap() {
            entity["displayName"] = entity["id"].clone();
            entity["aliases"] = json!([]);
            entity["evidence"] = claim["evidence"].clone();
        }
        repository.connection.execute("UPDATE story_modeling_batches_v2 SET entities_json=?1 WHERE batch_id='batch-event'",
            [corrupted.to_string()]).unwrap();
        let old = repository
            .knowledge_audit_job(&project, &completed.id)
            .unwrap();
        assert_eq!(
            (old.status.as_str(), old.verified_consistent),
            ("stale", None)
        );
        assert_eq!(
            repository.audit_assertion_index(&project),
            Err(RepositoryError::CorruptState)
        );
        assert_eq!(
            repository.rebuild_assertion_index(&project),
            Err(RepositoryError::CorruptState)
        );
        let retained: (String,String) = repository.connection.query_row("SELECT subject_id,assertion_json FROM story_assertion_claims_v1 WHERE batch_id='batch-event'",[],|row|Ok((row.get(0)?,row.get(1)?))).unwrap();
        assert_eq!(retained, ("delivery".into(), claim.to_string()));
        let source: String = repository
            .connection
            .query_row(
                "SELECT entities_json FROM story_modeling_batches_v2 WHERE batch_id='batch-event'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(serde_json::from_str::<Value>(&source).unwrap(), corrupted);
        let job = repository.start_knowledge_audit(&project).unwrap();
        let failed = repository
            .advance_knowledge_audit(&project, &job.id)
            .unwrap();
        assert_eq!(
            (
                failed.status.as_str(),
                failed.verified_consistent,
                failed.audit.assertions.published_batches
            ),
            ("failed", None, 0)
        );
        drop(repository);
        repository = SqliteStoryRepository::open(&path).unwrap();
        assert_eq!(
            repository
                .knowledge_audit_job(&project, &failed.id)
                .unwrap(),
            failed
        );
        assert_eq!(repository.project_revision(&project).unwrap(), revision);
        assert_eq!(
            repository
                .branch_head(&project, &BranchId::parse("main").unwrap())
                .unwrap(),
            head
        );
        assert!(repository.audit_assertion_index(&other).unwrap().complete());
    }
    repository
        .connection
        .execute(
            "UPDATE story_modeling_batches_v2 SET entities_json=?1 WHERE batch_id='batch-event'",
            [entities.to_string()],
        )
        .unwrap();
    assert_eq!(repository.audit_assertion_index(&project).unwrap(), clean);
}

#[test]
fn source_payload_epoch_overflow_rolls_back_payload_and_all_epoch_changes() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let project = seed(&mut repository, "overflow");
    let original: String = repository
        .connection
        .query_row(
            "SELECT entities_json FROM story_modeling_batches_v2 WHERE batch_id='batch-overflow'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    repository
        .connection
        .execute(
            "INSERT INTO story_relation_index_state_v1(project_id,generation) VALUES(?1,?2)",
            params![project.as_str(), i64::MAX],
        )
        .unwrap();
    let before: (i64,i64) = repository.connection.query_row("SELECT (SELECT COUNT(*) FROM story_knowledge_index_state_v1),(SELECT COUNT(*) FROM story_entity_index_state_v1)",[],|row|Ok((row.get(0)?,row.get(1)?))).unwrap();
    assert!(repository.connection.execute("UPDATE story_modeling_batches_v2 SET entities_json='[{\"id\":\"linxi\",\"kind\":\"character\"}]' WHERE batch_id='batch-overflow'",[]).is_err());
    let after: (i64,i64) = repository.connection.query_row("SELECT (SELECT COUNT(*) FROM story_knowledge_index_state_v1),(SELECT COUNT(*) FROM story_entity_index_state_v1)",[],|row|Ok((row.get(0)?,row.get(1)?))).unwrap();
    let payload: String = repository
        .connection
        .query_row(
            "SELECT entities_json FROM story_modeling_batches_v2 WHERE batch_id='batch-overflow'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!((after, payload), (before, original));
}

#[test]
fn published_payload_changes_revoke_all_dependent_reports_but_noops_do_not() {
    assert_source_mutation_revokes_reports(
        "UPDATE story_modeling_batches_v2 SET assertions_json=json_set(assertions_json,'$[0].value',0) WHERE batch_id='batch-fence'",
    );
}

#[test]
fn source_revision_epoch_overflow_rolls_back_revision_and_earlier_epoch_writes() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let project = seed(&mut repository, "revision-overflow");
    repository.connection.execute(
        "INSERT INTO story_snapshot_knowledge_audit_state_v1(project_id,generation) VALUES(?1,?2) ON CONFLICT(project_id) DO UPDATE SET generation=excluded.generation",
        params![project.as_str(),i64::MAX],
    ).unwrap();
    let inspect = || {
        repository.connection.query_row(
        "SELECT content_json,content_digest,(SELECT COALESCE(SUM(generation),0) FROM story_knowledge_index_state_v1),(SELECT COALESCE(SUM(generation),0) FROM story_entity_index_state_v1),(SELECT COALESCE(SUM(generation),0) FROM story_relation_index_state_v1) FROM story_revisions_v2 WHERE revision_id='revision-revision-overflow'",
        [], |row| Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,i64>(2)?,row.get::<_,i64>(3)?,row.get::<_,i64>(4)?)),
    ).unwrap()
    };
    let before = inspect();
    assert!(repository.connection.execute(
        "UPDATE story_revisions_v2 SET content_json='{\"body\":\"changed\"}',content_digest='changed' WHERE revision_id='revision-revision-overflow'",[],
    ).is_err());
    assert_eq!(inspect(), before);
}

#[test]
fn published_draft_changes_revoke_all_dependent_reports_but_noops_do_not() {
    for mutation in [
        "UPDATE story_revisions_v2 SET content_json='{\"body\":\"changed\"}' WHERE revision_id='revision-fence'",
        "UPDATE story_revisions_v2 SET content_digest='changed' WHERE revision_id='revision-fence'",
    ] {
        assert_source_mutation_revokes_reports(mutation);
    }
}

fn assert_source_mutation_revokes_reports(mutation: &str) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("payload-fence.sqlite3");
    let mut repository = SqliteStoryRepository::open(&path).unwrap();
    let project = seed(&mut repository, "fence");
    let other = seed(&mut repository, "fence-other");
    let knowledge = repository.start_knowledge_audit(&project).unwrap();
    let knowledge = repository
        .advance_knowledge_audit(&project, &knowledge.id)
        .unwrap();
    let entity = repository.start_entity_audit(&project).unwrap();
    let entity = repository
        .advance_entity_audit(&project, &entity.id)
        .unwrap();
    let relation = repository.start_relation_audit(&project).unwrap();
    let relation = repository
        .advance_relation_audit(&project, &relation.id)
        .unwrap();
    let history = repository.start_snapshot_knowledge_audit(&project).unwrap();
    let history = repository
        .advance_snapshot_knowledge_audit(&project, &history.id)
        .unwrap();
    assert_eq!(
        [
            knowledge.verified_consistent,
            entity.verified_consistent,
            relation.verified_consistent,
            history.verified_consistent
        ],
        [Some(true); 4]
    );
    let other_job = repository.start_knowledge_audit(&other).unwrap();
    let other_job = repository
        .advance_knowledge_audit(&other, &other_job.id)
        .unwrap();
    repository.connection.execute("UPDATE story_modeling_batches_v2 SET entities_json=entities_json,assertions_json=assertions_json WHERE batch_id='batch-fence'",[]).unwrap();
    repository.connection.execute("UPDATE story_revisions_v2 SET content_json=content_json,content_digest=content_digest WHERE revision_id='revision-fence'",[]).unwrap();
    repository.connection.execute("INSERT INTO story_revisions_v2(revision_id,project_id,object_id,object_kind,content_digest,content_json) VALUES('unpublished-draft',?1,'draft-unpublished','draft','test','{}')",[project.as_str()]).unwrap();
    repository.connection.execute("UPDATE story_revisions_v2 SET content_json='{\"body\":\"changed\"}' WHERE revision_id='unpublished-draft'",[]).unwrap();
    repository.connection.execute("INSERT INTO story_modeling_batches_v2(batch_id,project_id,object_id,source_revision_id,input_project_revision,extractor_version,status,created_at) VALUES('fence-unpublished',?1,'draft-one','revision-fence',0,'unpublished','queued',1)",[project.as_str()]).unwrap();
    repository.connection.execute("UPDATE story_modeling_batches_v2 SET entities_json='[]',assertions_json='[]' WHERE batch_id='fence-unpublished'",[]).unwrap();
    assert_eq!(
        repository
            .knowledge_audit_job(&project, &knowledge.id)
            .unwrap(),
        knowledge
    );
    assert_eq!(
        repository.entity_audit_job(&project, &entity.id).unwrap(),
        entity
    );
    assert_eq!(
        repository
            .relation_audit_job(&project, &relation.id)
            .unwrap(),
        relation
    );
    assert_eq!(
        repository
            .snapshot_knowledge_audit_job(&project, &history.id)
            .unwrap(),
        history
    );
    repository.connection.execute(mutation, []).unwrap();
    drop(repository);
    repository = SqliteStoryRepository::open(&path).unwrap();
    let reports = [
        {
            let job = repository
                .knowledge_audit_job(&project, &knowledge.id)
                .unwrap();
            (job.status, job.verified_consistent)
        },
        {
            let job = repository.entity_audit_job(&project, &entity.id).unwrap();
            (job.status, job.verified_consistent)
        },
        {
            let job = repository
                .relation_audit_job(&project, &relation.id)
                .unwrap();
            (job.status, job.verified_consistent)
        },
        {
            let job = repository
                .snapshot_knowledge_audit_job(&project, &history.id)
                .unwrap();
            (job.status, job.verified_consistent)
        },
    ];
    assert_eq!(reports, std::array::from_fn(|_| ("stale".into(), None)));
    assert_eq!(
        repository
            .knowledge_audit_job(&other, &other_job.id)
            .unwrap(),
        other_job
    );
}

use crate::{AssertionIndexIntegrityRepository, EntityAuditJobsRepository, EntityIndexIntegrityRepository, KnowledgeAuditJobsRepository, ProjectRevisionRepository, RelationAuditJobsRepository, SnapshotKnowledgeAuditJobsRepository};
