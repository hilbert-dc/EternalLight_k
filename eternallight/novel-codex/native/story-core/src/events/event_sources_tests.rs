use pretty_assertions::assert_eq;
use rusqlite::params;
use serde_json::{Value, json};

use crate::model_sources::ModelSourceScope;
use crate::{
    BranchId, ObjectId, ProjectId, ProjectRecord, RepositoryError, SnapshotId,
    SqliteStoryRepository,
};

enum Source<'a> {
    Independent,
    Prerequisite(&'a str),
}

fn resolve(
    connection: &rusqlite::Connection,
    project: &ProjectId,
    scope: ModelSourceScope<'_>,
    batch: &str,
    event: &Value,
) -> Result<std::collections::HashMap<crate::story_events::EventClaimRef, Value>, RepositoryError> {
    let payload: String = connection.query_row(
        "SELECT entities_json FROM story_modeling_batches_v2 WHERE project_id=?1 AND batch_id=?2",
        params![project.as_str(),batch], |row|row.get(0),
    ).unwrap();
    let entities: Vec<Value> = serde_json::from_str(&payload).unwrap();
    super::resolve(connection, project, scope, batch, event, &entities)
}

fn seed(
    repository: &SqliteStoryRepository,
    project: &ProjectId,
    id: &str,
    number: i64,
    source: Source<'_>,
) -> Vec<Value> {
    let evidence = json!({"quote":"甲交信","start":0,"end":3});
    let mut entities = vec![
        json!({"id":"a","kind":"character","displayName":"甲","aliases":[],"evidence":evidence}),
        json!({"id":"letter","kind":"item","displayName":"信","aliases":[],"evidence":evidence}),
    ];
    let state = json!({"kind":"state","subjectId":"letter","predicate":"held","value":true,
        "stateSchema":{"cardinality":"single","valueType":{"type":"boolean"}},"validFrom":number,"evidence":evidence});
    let assertions = if let Source::Prerequisite(source) = source {
        entities.push(
            json!({"id":id,"kind":"event","displayName":"交信","aliases":[],"evidence":evidence}),
        );
        vec![
            json!({"kind":"event","subjectId":id,"predicate":"transfer","storyTime":5,
            "value":{"participants":[{"role":"actor","entityId":"a"},{"role":"item","entityId":"letter"}],
                "preconditions":[{"sourceBatchId":source,"assertionIndex":if source=="prior" {0} else {1}}],
                "effects":[{"sourceBatchId":null,"assertionIndex":1}],"untilExclusive":null},"evidence":evidence}),
            state,
        ]
    } else {
        vec![state]
    };
    repository.connection.execute("INSERT INTO story_chapters_v2(chapter_id,project_id,number,title,status) VALUES(?1,?2,?3,'交信','draft')",params![id,project.as_str(),number]).unwrap();
    repository.connection.execute("INSERT INTO story_revisions_v2(revision_id,project_id,object_id,object_kind,content_digest,content_json) VALUES(?1,?2,?3,'draft','test',?4)",params![format!("rev-{id}"),project.as_str(),format!("draft-{id}"),json!({"body":"甲交信","chapterNumber":number}).to_string()]).unwrap();
    repository.connection.execute("INSERT INTO story_object_heads_v2(project_id,object_id,object_kind,candidate_revision_id) VALUES(?1,?2,'draft',?3)",params![project.as_str(),format!("draft-{id}"),format!("rev-{id}")]).unwrap();
    repository.connection.execute("INSERT INTO story_modeling_batches_v2(batch_id,project_id,object_id,source_revision_id,input_project_revision,extractor_version,status,assertions_json,entities_json,created_at,published_at) VALUES(?1,?2,?3,?4,0,'test','ready',?5,?6,1,2)",params![id,project.as_str(),format!("draft-{id}"),format!("rev-{id}"),json!(assertions).to_string(),json!(entities).to_string()]).unwrap();
    repository
        .connection
        .execute(
            "INSERT INTO story_knowledge_heads_v2(project_id,object_id,batch_id) VALUES(?1,?2,?3)",
            params![project.as_str(), format!("draft-{id}"), id],
        )
        .unwrap();
    repository.connection.execute("UPDATE story_modeling_outbox_v1 SET status='satisfied',completed_batch_id=?1 WHERE project_id=?2 AND object_id=?3",params![id,project.as_str(),format!("draft-{id}")]).unwrap();
    assertions
}

fn fixture() -> (SqliteStoryRepository, ProjectId, SnapshotId, Vec<Value>) {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let project = ProjectId::parse("cross-events").unwrap();
    repository
        .create_project(
            ProjectRecord {
                project_id: project.clone(),
                title: "事件".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            BranchId::parse("main").unwrap(),
            SnapshotId::parse("start").unwrap(),
        )
        .unwrap();
    seed(
        &repository,
        &project,
        "prior",
        /*number*/ 1,
        Source::Independent,
    );
    let assertions = seed(
        &repository,
        &project,
        "delivery",
        /*number*/ 2,
        Source::Prerequisite("prior"),
    );
    seed(
        &repository,
        &project,
        "later",
        /*number*/ 3,
        Source::Prerequisite("delivery"),
    );
    let snapshot = SnapshotId::parse("captured").unwrap();
    repository.connection.execute("INSERT INTO story_snapshots_v2(snapshot_id,project_id,branch_id,parent_snapshot_id) VALUES(?1,?2,'main','start')",params![snapshot.as_str(),project.as_str()]).unwrap();
    for id in ["prior", "delivery", "later"] {
        repository.connection.execute("INSERT INTO story_snapshot_members_v2(snapshot_id,object_id,revision_id) VALUES(?1,?2,?3)",params![snapshot.as_str(),format!("draft-{id}"),format!("rev-{id}")]).unwrap();
    }
    (repository, project, snapshot, assertions)
}

#[test]
fn certified_condition_sources_reach_late_relevant_batches_and_revoke_stale_proofs() {
    let (mut repository, project, snapshot, claims) = fixture();
    for index in 0..210 {
        let id = format!("000-noise-{index:03}");
        seed(&repository, &project, &id, index + 4, Source::Independent);
        repository.connection.execute("UPDATE story_modeling_batches_v2 SET assertions_json=json_set(assertions_json,'$[0].predicate','unrelated') WHERE batch_id=?1",[&id]).unwrap();
        for entity in 0..5 {
            let noise = json!({"id":format!("000-noise-entity-{index}-{entity}"),"kind":"character","displayName":"甲","aliases":[],"evidence":{"quote":"甲交信","start":0,"end":3}});
            repository.connection.execute("UPDATE story_modeling_batches_v2 SET entities_json=json_insert(entities_json,'$[#]',json(?2)) WHERE batch_id=?1",params![id,noise.to_string()]).unwrap();
        }
    }
    let rules = crate::property_rules::PropertyRules {
        revision_id:Some("approved-test-rules".into()),
        definitions:vec![serde_json::from_value(json!({"predicate":"held","subjectKind":"item","schema":{"cardinality":"single","valueType":{"type":"boolean"}},"description":"持有","exceptions":[]})).unwrap()],
        event_rules:vec![serde_json::from_value(json!({"eventType":"transfer","description":"持有","exceptions":[],"condition":{"type":"state","role":"item","subjectKind":"item","predicate":"held","quantifier":"all","operator":"eq","operand":{"type":"boolean","value":true}}})).unwrap()],
    };
    let identity = crate::identity_decisions::IdentityResolution::unresolved();
    let read = repository.connection.unchecked_transaction().unwrap();
    let unverified = crate::event_rule_sources::collect(
        &read,
        &project,
        ModelSourceScope::Current,
        &identity,
        &rules,
    )
    .unwrap();
    assert!(!unverified.complete);
    assert_eq!(unverified.coverage["omittedSources"], 213);
    read.commit().unwrap();
    repository.rebuild_entity_index(&project).unwrap();
    repository.rebuild_assertion_index(&project).unwrap();
    let job = repository.start_knowledge_audit(&project).unwrap();
    let completed = loop {
        let progress = repository
            .advance_knowledge_audit(&project, &job.id)
            .unwrap();
        if progress.status != "running" {
            break progress;
        }
    };
    assert_eq!(completed.verified_consistent, Some(true));
    for scope in [
        ModelSourceScope::Current,
        ModelSourceScope::Snapshot(&snapshot),
    ] {
        let read = repository.connection.unchecked_transaction().unwrap();
        let sources =
            crate::event_rule_sources::collect(&read, &project, scope, &identity, &rules).unwrap();
        assert!(sources.complete);
        assert_eq!(
            sources.kinds,
            std::collections::HashMap::from([("letter".into(), "item".into())])
        );
        assert_eq!(
            (
                &sources.coverage["sourceProofId"],
                &sources.coverage["relevantSources"],
                &sources.coverage["scannedSources"]
            ),
            (&json!(job.id), &json!(3), &json!(3))
        );
        let assessment = crate::event_rule_evaluation::assess(
            &claims[0],
            &rules,
            &identity,
            Some(&sources),
            &mut 100_000,
        )
        .unwrap();
        assert_eq!(assessment["result"], "supported");
        read.commit().unwrap();
    }
    // A different validator must not inherit this proof or continue mixed checkpoints.
    repository.connection.execute("UPDATE story_knowledge_audit_jobs_v1 SET validation_version='different-validator' WHERE job_id=?1",[&job.id]).unwrap();
    assert_eq!(
        repository
            .knowledge_audit_job(&project, &job.id)
            .unwrap()
            .status,
        "stale"
    );
    let read = repository.connection.unchecked_transaction().unwrap();
    let unverified = crate::event_rule_sources::collect(
        &read,
        &project,
        ModelSourceScope::Current,
        &identity,
        &rules,
    )
    .unwrap();
    assert!(!unverified.complete);
    assert_eq!(unverified.coverage["sourceProofId"], Value::Null);
    read.commit().unwrap();
    seed(
        &repository,
        &project,
        "zz-counter",
        /*number*/ 214,
        Source::Independent,
    );
    repository.connection.execute("UPDATE story_modeling_batches_v2 SET assertions_json=json_set(assertions_json,'$[0].value',json('false'),'$[0].validFrom',1) WHERE batch_id='zz-counter'",[]).unwrap();
    repository.rebuild_entity_index(&project).unwrap();
    repository.rebuild_assertion_index(&project).unwrap();
    let refreshed = repository.start_knowledge_audit(&project).unwrap();
    loop {
        let progress = repository
            .advance_knowledge_audit(&project, &refreshed.id)
            .unwrap();
        if progress.status != "running" {
            assert_eq!(progress.verified_consistent, Some(true));
            break;
        }
    }
    for (scope, expected) in [
        (ModelSourceScope::Current, "both"),
        (ModelSourceScope::Snapshot(&snapshot), "supported"),
    ] {
        let read = repository.connection.unchecked_transaction().unwrap();
        let sources =
            crate::event_rule_sources::collect(&read, &project, scope, &identity, &rules).unwrap();
        assert!(sources.complete);
        let report = crate::event_rule_evaluation::assess(
            &claims[0],
            &rules,
            &identity,
            Some(&sources),
            &mut 100_000,
        )
        .unwrap();
        assert_eq!(report["result"], expected);
        read.commit().unwrap();
    }
    // Even an otherwise irrelevant batch can contradict the type of an entity
    // needed by the condition. The targeted lookup must not miss it.
    repository.connection.execute("UPDATE story_modeling_batches_v2 SET entities_json=json_set(entities_json,'$[1].kind','character') WHERE batch_id='000-noise-209'",[]).unwrap();
    repository.rebuild_entity_index(&project).unwrap();
    let job = repository.start_knowledge_audit(&project).unwrap();
    loop {
        let progress = repository
            .advance_knowledge_audit(&project, &job.id)
            .unwrap();
        if progress.status != "running" {
            assert_eq!(progress.verified_consistent, Some(true));
            break;
        }
    }
    let read = repository.connection.unchecked_transaction().unwrap();
    assert!(matches!(
        crate::event_rule_sources::collect(
            &read,
            &project,
            ModelSourceScope::Current,
            &identity,
            &rules
        ),
        Err(RepositoryError::CorruptState)
    ));
    assert!(
        crate::event_rule_sources::collect(
            &read,
            &project,
            ModelSourceScope::Snapshot(&snapshot),
            &identity,
            &rules
        )
        .unwrap()
        .complete
    );
    read.commit().unwrap();
}

#[test]
fn exact_prior_sources_reject_foreign_future_missing_and_unrelated_claims() {
    let (mut repository, project, snapshot, claims) = fixture();
    for scope in [
        ModelSourceScope::Current,
        ModelSourceScope::Snapshot(&snapshot),
        ModelSourceScope::Published,
    ] {
        let source = resolve(
            &repository.connection,
            &project,
            scope,
            "delivery",
            &claims[0],
        )
        .unwrap();
        let assessment = crate::event_conditions::assess(&claims[0], &claims, &source).unwrap();
        assert_eq!(
            assessment["preconditions"],
            json!([{"referenceIndex":0,"timing":"applicableBeforeEvent"}])
        );
    }
    for reference in [
        json!({"sourceBatchId":"absent","assertionIndex":0}),
        json!({"sourceBatchId":"delivery","assertionIndex":1}),
        json!({"sourceBatchId":"later","assertionIndex":1}),
        json!({"sourceBatchId":"prior","assertionIndex":99}),
    ] {
        let mut event = claims[0].clone();
        event["value"]["preconditions"] = json!([reference]);
        assert!(
            resolve(
                &repository.connection,
                &project,
                ModelSourceScope::Current,
                "delivery",
                &event
            )
            .is_err()
        );
    }
    let foreign = ProjectId::parse("foreign").unwrap();
    repository
        .create_project(
            ProjectRecord {
                project_id: foreign.clone(),
                title: "foreign".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            BranchId::parse("main").unwrap(),
            SnapshotId::parse("foreign-start").unwrap(),
        )
        .unwrap();
    seed(
        &repository,
        &foreign,
        "foreign-prior",
        /*number*/ 1,
        Source::Independent,
    );
    let mut event = claims[0].clone();
    event["value"]["preconditions"][0]["sourceBatchId"] = json!("foreign-prior");
    assert!(
        resolve(
            &repository.connection,
            &project,
            ModelSourceScope::Current,
            "delivery",
            &event
        )
        .is_err()
    );
    for (path, value) in [
        ("$[0].subjectId", json!("a")),
        ("$[0].evidence.quote", json!("伪造")),
        ("$[0].value", json!("not boolean")),
    ] {
        let original: String = repository
            .connection
            .query_row(
                "SELECT assertions_json FROM story_modeling_batches_v2 WHERE batch_id='prior'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        repository.connection.execute("UPDATE story_modeling_batches_v2 SET assertions_json=json_set(assertions_json,?1,json(?2)) WHERE batch_id='prior'",params![path,value.to_string()]).unwrap();
        let mut unrelated = claims[0].clone();
        unrelated["value"]["participants"] = json!([{"role":"item","entityId":"letter"}]);
        assert!(
            resolve(
                &repository.connection,
                &project,
                ModelSourceScope::Current,
                "delivery",
                &unrelated
            )
            .is_err()
        );
        repository
            .connection
            .execute(
                "UPDATE story_modeling_batches_v2 SET assertions_json=?1 WHERE batch_id='prior'",
                [original],
            )
            .unwrap();
    }
    repository
        .connection
        .execute(
            "UPDATE story_chapters_v2 SET number=8 WHERE chapter_id='prior'",
            [],
        )
        .unwrap();
    assert!(
        resolve(
            &repository.connection,
            &project,
            ModelSourceScope::Current,
            "delivery",
            &claims[0]
        )
        .is_err()
    );
    assert!(
        resolve(
            &repository.connection,
            &project,
            ModelSourceScope::Snapshot(&snapshot),
            "delivery",
            &claims[0]
        )
        .is_ok()
    );
}

#[test]
fn oversized_authorized_sources_fail_before_payload_decoding_in_every_scope() {
    let (repository, project, snapshot, claims) = fixture();
    let expected = resolve(
        &repository.connection,
        &project,
        ModelSourceScope::Current,
        "delivery",
        &claims[0],
    )
    .unwrap();
    let tx = repository.connection.unchecked_transaction().unwrap();
    let original_bytes: usize = tx.query_row("SELECT length(CAST(b.entities_json AS BLOB))+length(CAST(b.assertions_json AS BLOB))+length(CAST(r.content_json AS BLOB)) FROM story_modeling_batches_v2 b JOIN story_revisions_v2 r ON r.revision_id=b.source_revision_id WHERE b.batch_id='prior'",[],|row| row.get(0)).unwrap();
    tx.execute("UPDATE story_modeling_batches_v2 SET assertions_json=assertions_json||?1 WHERE batch_id='prior'",[" ".repeat(8 * 1024 * 1024 - original_bytes)]).unwrap();
    assert_eq!(
        resolve(
            &tx,
            &project,
            ModelSourceScope::Current,
            "delivery",
            &claims[0]
        )
        .unwrap(),
        expected
    );
    tx.execute("UPDATE story_modeling_batches_v2 SET assertions_json=assertions_json||' ' WHERE batch_id='prior'",[]).unwrap();
    assert!(
        matches!(crate::model_sources::validated_source(&tx, &project, ModelSourceScope::Current, "prior"), Err(RepositoryError::InvalidContent(message)) if message == "model source exceeds 8 MiB read budget")
    );
    tx.rollback().unwrap();
    for sql in [
        "UPDATE story_modeling_batches_v2 SET entities_json=entities_json||?1 WHERE batch_id='prior'",
        "UPDATE story_modeling_batches_v2 SET assertions_json=assertions_json||?1 WHERE batch_id='prior'",
        "UPDATE story_revisions_v2 SET content_json=content_json||?1 WHERE revision_id='rev-prior'",
    ] {
        let tx = repository.connection.unchecked_transaction().unwrap();
        // Legal JSON whitespace: failures must be the byte budget, not parsing.
        tx.execute(sql, [" ".repeat(8 * 1024 * 1024)]).unwrap();
        for scope in [
            ModelSourceScope::Current,
            ModelSourceScope::Snapshot(&snapshot),
            ModelSourceScope::Published,
        ] {
            let result = crate::model_sources::validated_source(&tx, &project, scope, "prior");
            assert!(
                matches!(result, Err(RepositoryError::InvalidContent(message)) if message == "model source exceeds 8 MiB read budget")
            );
            assert!(resolve(&tx, &project, scope, "delivery", &claims[0]).is_err());
            assert!(
                crate::model_sources::validated_source(
                    &tx,
                    &ProjectId::parse("other").unwrap(),
                    scope,
                    "prior"
                )
                .unwrap()
                .is_none()
            );
        }
        tx.rollback().unwrap();
    }
    let tx = repository.connection.unchecked_transaction().unwrap();
    let padding = " ".repeat(3 * 1024 * 1024);
    tx.execute("UPDATE story_modeling_batches_v2 SET entities_json=entities_json||?1,assertions_json=assertions_json||?1 WHERE batch_id='prior'",[&padding]).unwrap();
    tx.execute(
        "UPDATE story_revisions_v2 SET content_json=content_json||?1 WHERE revision_id='rev-prior'",
        [&padding],
    )
    .unwrap();
    assert!(
        matches!(crate::model_sources::validated_source(&tx, &project, ModelSourceScope::Current, "prior"), Err(RepositoryError::InvalidContent(message)) if message == "model source exceeds 8 MiB read budget")
    );
    tx.rollback().unwrap();
    let tx = repository.connection.unchecked_transaction().unwrap();
    // Fewer than 8 Mi characters still exceed the UTF-8 byte limit.
    tx.execute("UPDATE story_revisions_v2 SET content_json=json_set(content_json,'$.body',?1) WHERE revision_id='rev-prior'",[format!("甲交信{}", "潮".repeat(3 * 1024 * 1024))]).unwrap();
    assert!(
        matches!(crate::model_sources::validated_source(&tx, &project, ModelSourceScope::Current, "prior"), Err(RepositoryError::InvalidContent(message)) if message == "model source exceeds 8 MiB read budget")
    );
    tx.rollback().unwrap();
    for scope in [
        ModelSourceScope::Current,
        ModelSourceScope::Snapshot(&snapshot),
        ModelSourceScope::Published,
    ] {
        assert_eq!(
            resolve(
                &repository.connection,
                &project,
                scope,
                "delivery",
                &claims[0]
            )
            .unwrap(),
            expected
        );
    }
}

#[test]
fn shared_identity_kinds_come_from_sources_not_the_entity_index() {
    let (repository, project, snapshot, claims) = fixture();
    let original: String = repository
        .connection
        .query_row(
            "SELECT entities_json FROM story_modeling_batches_v2 WHERE batch_id='prior'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    repository.connection.execute(
        "UPDATE story_modeling_batches_v2 SET entities_json=json_set(entities_json,'$[1].kind','character') WHERE batch_id='prior'",
        [],
    ).unwrap();
    // Both individual payloads are structurally valid; the exact shared identity disagrees.
    for scope in [
        ModelSourceScope::Current,
        ModelSourceScope::Snapshot(&snapshot),
        ModelSourceScope::Published,
    ] {
        assert!(
            crate::model_sources::validated_source(
                &repository.connection,
                &project,
                scope,
                "prior"
            )
            .unwrap()
            .is_some()
        );
        assert!(
            resolve(
                &repository.connection,
                &project,
                scope,
                "delivery",
                &claims[0]
            )
            .is_err()
        );
    }
    repository
        .connection
        .execute(
            "DELETE FROM story_entity_claims_v2 WHERE entity_id='letter'",
            [],
        )
        .unwrap();
    assert!(
        resolve(
            &repository.connection,
            &project,
            ModelSourceScope::Current,
            "delivery",
            &claims[0]
        )
        .is_err()
    );
    // Restoring authority, not rebuilding an index, makes the source usable again.
    repository
        .connection
        .execute(
            "UPDATE story_modeling_batches_v2 SET entities_json=?1 WHERE batch_id='prior'",
            [original],
        )
        .unwrap();
    repository
        .connection
        .execute(
            "UPDATE story_entity_claims_v2 SET kind='organization' WHERE entity_id='a'",
            [],
        )
        .unwrap();
    assert!(
        resolve(
            &repository.connection,
            &project,
            ModelSourceScope::Current,
            "delivery",
            &claims[0]
        )
        .is_ok()
    );
    // A conflicting shared target/actor is rejected even when it is not the referenced subject.
    repository.connection.execute("UPDATE story_modeling_batches_v2 SET entities_json=json_set(entities_json,'$[0].kind','organization') WHERE batch_id='prior'",[]).unwrap();
    assert!(
        resolve(
            &repository.connection,
            &project,
            ModelSourceScope::Current,
            "delivery",
            &claims[0]
        )
        .is_err()
    );
}

#[test]
fn dependency_repair_and_transitive_invalidation_preserve_frozen_sources() {
    let (mut repository, project, snapshot, claims) = fixture();
    assert!(
        repository
            .rebuild_assertion_index(&project)
            .unwrap()
            .complete()
    );
    let before_revision = repository.project_revision(&project).unwrap();
    let before: String = repository
        .connection
        .query_row(
            "SELECT assertions_json FROM story_modeling_batches_v2 WHERE batch_id='delivery'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let job = repository.start_knowledge_audit(&project).unwrap();
    repository.connection.execute("UPDATE story_modeling_batches_v2 SET assertions_json=assertions_json,published_at=published_at WHERE batch_id='delivery'",[]).unwrap();
    assert_eq!(
        repository
            .knowledge_audit_job(&project, &job.id)
            .unwrap()
            .status,
        "queued"
    );
    repository
        .connection
        .execute(
            "DELETE FROM story_model_dependencies_v1 WHERE batch_id='delivery'",
            [],
        )
        .unwrap();
    assert_eq!(
        repository
            .knowledge_audit_job(&project, &job.id)
            .unwrap()
            .status,
        "stale"
    );
    let audit = repository.audit_assertion_index(&project).unwrap();
    assert_eq!(
        (
            audit.missing_dependencies,
            audit.extra_dependencies,
            audit.complete()
        ),
        (1, 0, false)
    );
    repository
        .connection
        .execute(
            "INSERT INTO story_model_dependencies_v1 VALUES(?1,'delivery','forged')",
            [project.as_str()],
        )
        .unwrap();
    assert_eq!(
        repository
            .audit_assertion_index(&project)
            .unwrap()
            .extra_dependencies,
        1
    );
    assert!(
        repository
            .rebuild_assertion_index(&project)
            .unwrap()
            .complete()
    );
    let tx = repository.connection.unchecked_transaction().unwrap();
    crate::modeling::invalidate_source_modeling(
        &tx,
        &project,
        &ObjectId::parse("draft-prior").unwrap(),
    )
    .unwrap();
    tx.commit().unwrap();
    let statuses: Vec<(String,String)> = repository.connection.prepare("SELECT batch_id,status FROM story_modeling_batches_v2 WHERE project_id=?1 ORDER BY batch_id").unwrap().query_map([project.as_str()],|row|Ok((row.get(0)?,row.get(1)?))).unwrap().map(Result::unwrap).collect();
    assert_eq!(
        statuses,
        vec![
            ("delivery".into(), "stale".into()),
            ("later".into(), "stale".into()),
            ("prior".into(), "stale".into())
        ]
    );
    assert!(
        resolve(
            &repository.connection,
            &project,
            ModelSourceScope::Current,
            "delivery",
            &claims[0]
        )
        .is_err()
    );
    assert!(
        resolve(
            &repository.connection,
            &project,
            ModelSourceScope::Snapshot(&snapshot),
            "delivery",
            &claims[0]
        )
        .is_ok()
    );
    let old = repository
        .knowledge_at_snapshot(
            &project,
            &snapshot,
            &ObjectId::parse("draft-delivery").unwrap(),
        )
        .unwrap();
    assert_eq!(old["assertions"], json!(claims));
    assert_eq!(
        repository.project_revision(&project).unwrap(),
        before_revision
    );
    let after: String = repository
        .connection
        .query_row(
            "SELECT assertions_json FROM story_modeling_batches_v2 WHERE batch_id='delivery'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(before, after);
    for _ in 0..2 {
        crate::modeling::invalidate_source_modeling(
            &repository.connection,
            &project,
            &ObjectId::parse("draft-prior").unwrap(),
        )
        .unwrap();
    }
    let queued: Vec<String> = repository.connection.prepare("SELECT object_id FROM story_modeling_outbox_v1 WHERE status='pending' ORDER BY object_id").unwrap().query_map([],|row|row.get(0)).unwrap().map(Result::unwrap).collect();
    assert_eq!(
        queued,
        vec!["draft-delivery".to_owned(), "draft-later".to_owned()]
    );
}

#[test]
fn dependency_rebuild_rejects_corrupt_source_without_partial_repair() {
    let (mut repository, project, _, _) = fixture();
    repository
        .connection
        .execute(
            "DELETE FROM story_model_dependencies_v1 WHERE batch_id='delivery'",
            [],
        )
        .unwrap();
    repository
        .connection
        .execute(
            "UPDATE story_modeling_batches_v2 SET assertions_json='invalid' WHERE batch_id='prior'",
            [],
        )
        .unwrap();
    assert_eq!(
        repository.rebuild_assertion_index(&project),
        Err(RepositoryError::CorruptState)
    );
    let count: i64 = repository
        .connection
        .query_row(
            "SELECT COUNT(*) FROM story_model_dependencies_v1 WHERE batch_id='delivery'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
}

use crate::{AssertionIndexIntegrityRepository, EntityIndexIntegrityRepository, KnowledgeAuditJobsRepository, ProjectRevisionRepository, SnapshotKnowledgeRepository};
