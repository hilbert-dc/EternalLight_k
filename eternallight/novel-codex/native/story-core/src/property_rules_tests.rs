use pretty_assertions::assert_eq;
use serde_json::{Value, json};

use super::{PropertyRules, load, parse_content};
use crate::{
    AcceptReviewRequest, BranchId, CommitId, CreateReviewRequest, CreateTaskRequest,
    IdempotencyKey, ObjectId, ProjectId, ProjectRecord, RepositoryError, RevisionId,
    SaveCandidateRequest, SnapshotId, SqliteStoryRepository, TaskOutputBinding,
    UndoAcceptanceRequest,
};

fn content() -> Value {
    json!({"source":"author","version":1,"baseProjectRevision":0,"payload":{"definitions":[{
        "predicate":"mood","subjectKind":"character","schema":{"cardinality":"single","valueType":{"type":"text"}},
        "description":"独立属性约束","exceptions":[]
    }],"eventRules":[]}})
}

#[test]
fn malformed_or_unbounded_rule_definitions_cannot_be_published() {
    let valid = content();
    let mut invalid = Vec::new();
    let mut unknown = valid.clone();
    unknown["payload"]["definitions"][0]["execute"] = json!("code");
    invalid.push(unknown);
    let mut too_many = valid.clone();
    too_many["payload"]["definitions"] = json!(
        (0..65)
            .map(|index| {
                let mut rule = valid["payload"]["definitions"][0].clone();
                rule["predicate"] = json!(format!("property-{index}"));
                rule
            })
            .collect::<Vec<_>>()
    );
    invalid.push(too_many);
    for (field, value) in [
        ("description", json!("字".repeat(171))),
        ("exceptions", json!(["x".repeat(257)])),
        ("exceptions", json!(vec!["条件"; 9])),
    ] {
        let mut changed = valid.clone();
        changed["payload"]["definitions"][0][field] = value;
        invalid.push(changed);
    }
    let mut oversized = valid.clone();
    oversized["payload"]["definitions"] = json!(
        (0..64)
            .map(|index| {
                let mut rule = valid["payload"]["definitions"][0].clone();
                rule["predicate"] = json!(format!("property-{index}"));
                rule["description"] = json!("x".repeat(512));
                rule
            })
            .collect::<Vec<_>>()
    );
    invalid.push(oversized);
    assert!(parse_content(&valid.to_string()).is_ok());
    for value in invalid {
        assert!(matches!(
            parse_content(&value.to_string()),
            Err(RepositoryError::InvalidContent(_))
        ));
    }
}

#[test]
fn rule_source_and_task_binding_must_agree_before_a_candidate_can_be_saved() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    for (source, output_kind) in [
        ("agent", None),
        ("author", Some("propertyRules")),
        ("agent", Some("brief")),
    ] {
        let mut envelope = content();
        envelope["source"] = json!(source);
        let request = SaveCandidateRequest {
            project_id: ProjectId::parse("unbound-rule-project").unwrap(),
            branch_id: BranchId::parse("main").unwrap(),
            object_id: ObjectId::parse("artifact-propertyRules").unwrap(),
            object_kind: "propertyRules".into(),
            revision_id: RevisionId::parse("unbound-rules").unwrap(),
            commit_id: CommitId::parse("unbound-save").unwrap(),
            idempotency_key: IdempotencyKey::parse("unbound-save").unwrap(),
            request_digest: "request".into(),
            content_digest: "content".into(),
            content_json: envelope.to_string(),
            expected_project_revision: 0,
            expected_snapshot_id: SnapshotId::parse("initial").unwrap(),
            next_snapshot_id: SnapshotId::parse("next").unwrap(),
            expected_candidate_revision_id: None,
            chapter_projection: None,
            linked_impact_ids: Vec::new(),
            task_output: output_kind.map(|kind| TaskOutputBinding {
                task_id: "task".into(),
                thread_id: "root".into(),
                output_kind: kind.into(),
                completeness: None,
            }),
        };
        assert_eq!(
            repository.save_candidate(request),
            Err(RepositoryError::TaskOutputForbidden)
        );
    }
}

#[test]
fn approved_entity_kinds_and_subject_scopes_are_applied_without_rewriting_claims() {
    let mut rule = content();
    rule["payload"]["definitions"][0]["schema"] =
        json!({"cardinality":"multiple","valueType":{"type":"entity","kind":"item"}});
    let rules = PropertyRules {
        revision_id: Some("approved-item-rule".into()),
        definitions: parse_content(&rule.to_string()).unwrap().definitions,
        event_rules: Vec::new(),
    };
    let entities = json!([{"id":"linxi","kind":"character"},{"id":"tower","kind":"location"},{"id":"key","kind":"item"}]);
    let raw = json!({"kind":"state","subjectId":"linxi","predicate":"mood","value":{"entityId":"key"},
        "stateSchema":{"cardinality":"single","valueType":{"type":"entity","kind":"item"}}});
    let mut entry = json!({"assertion":raw,"confirmationState":"candidate","stateSchemaAuthority":"modelDeclared","effectiveStateSchema":raw["stateSchema"]});
    assert!(!rules.project(&mut entry, &entities).unwrap());
    assert_eq!(entry["assertion"], raw);
    assert_eq!(
        entry["effectiveStateSchema"],
        rule["payload"]["definitions"][0]["schema"]
    );
    assert_eq!(entry["confirmationState"], "candidate");
    entry["assertion"]["value"] = json!({"entityId":"tower"});
    assert!(rules.project(&mut entry, &entities).unwrap());
    assert_eq!(entry["propertyRuleAssessment"]["status"], "typeMismatch");
    assert_eq!(entry["effectiveStateSchema"], Value::Null);
    assert_eq!(entry["stateSchemaAuthority"], "modelDeclared");
    let mut other_subject = json!({"assertion":{"subjectId":"tower","predicate":"mood","value":"test"},"stateSchemaAuthority":"modelDeclared"});
    let before = other_subject.clone();
    assert!(!rules.project(&mut other_subject, &entities).unwrap());
    assert_eq!(other_subject, before);
}

#[test]
fn rule_authority_requires_a_formal_receipt_and_uncorrupted_accepted_content() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let project = ProjectId::parse("rule-project").unwrap();
    let branch = BranchId::parse("main").unwrap();
    let initial = SnapshotId::parse("rule-initial").unwrap();
    let object = ObjectId::parse("artifact-propertyRules").unwrap();
    let revision = RevisionId::parse("rule-revision").unwrap();
    repository
        .create_project(
            ProjectRecord {
                project_id: project.clone(),
                title: "规则".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            branch.clone(),
            initial.clone(),
        )
        .unwrap();
    repository
        .save_candidate(SaveCandidateRequest {
            project_id: project.clone(),
            branch_id: branch.clone(),
            object_id: object.clone(),
            object_kind: "propertyRules".into(),
            revision_id: revision.clone(),
            commit_id: CommitId::parse("rule-save").unwrap(),
            idempotency_key: IdempotencyKey::parse("rule-save").unwrap(),
            request_digest: "save".into(),
            expected_project_revision: 0,
            expected_snapshot_id: initial.clone(),
            next_snapshot_id: SnapshotId::parse("rule-candidate").unwrap(),
            expected_candidate_revision_id: None,
            content_digest: "rule-content".into(),
            content_json: content().to_string(),
            chapter_projection: None,
            task_output: None,
            linked_impact_ids: Vec::new(),
        })
        .unwrap();
    assert_eq!(
        load(&repository.connection, &project, /*snapshot*/ None)
            .unwrap()
            .revision_id,
        None
    );
    repository
        .connection
        .execute(
            "UPDATE story_object_heads_v2 SET accepted_revision_id=?1 WHERE project_id=?2",
            rusqlite::params![revision.as_str(), project.as_str()],
        )
        .unwrap();
    assert!(matches!(
        load(&repository.connection, &project, /*snapshot*/ None),
        Err(RepositoryError::CorruptState)
    ));
    repository
        .connection
        .execute(
            "UPDATE story_object_heads_v2 SET accepted_revision_id=NULL WHERE project_id=?1",
            [project.as_str()],
        )
        .unwrap();
    repository
        .create_review(CreateReviewRequest {
            review_id: "rule-review".into(),
            project_id: project.clone(),
            object_id: object,
            object_kind: "propertyRules".into(),
            revision_id: revision.clone(),
            idempotency_key: IdempotencyKey::parse("rule-review").unwrap(),
            request_digest: "review".into(),
            checks_json: "[]".into(),
            proposals_json: "[]".into(),
            modeling_batch_id: None,
            selections_json: "{}".into(),
        })
        .unwrap();
    repository
        .accept_review(AcceptReviewRequest {
            project_id: project.clone(),
            branch_id: branch,
            review_id: "rule-review".into(),
            commit_id: CommitId::parse("rule-accept").unwrap(),
            idempotency_key: IdempotencyKey::parse("rule-accept").unwrap(),
            request_digest: "accept".into(),
            expected_project_revision: 0,
            next_snapshot_id: SnapshotId::parse("rule-approved").unwrap(),
            selections_json: "{}".into(),
        })
        .unwrap();
    assert_eq!(
        load(&repository.connection, &project, /*snapshot*/ None)
            .unwrap()
            .revision_id,
        Some(revision.as_str().to_owned())
    );
    assert_eq!(
        load(&repository.connection, &project, Some(&initial))
            .unwrap()
            .revision_id,
        None
    );
    let input_task = repository
        .create_task(CreateTaskRequest {
            task_id: "rule-input-corruption".into(),
            project_id: project.clone(),
            thread_id: "rule-root".into(),
            operation: "generatePropertyRules".into(),
            input_revision: 1,
            model: None,
            target_json: json!({"type":"artifact"}).to_string(),
            input_manifest_json: "{}".into(),
            capability_hash: "a".repeat(64),
            capability_expires_at: i64::MAX,
        })
        .unwrap();
    assert_eq!(
        repository.task_property_rule_input(&input_task).unwrap()["id"],
        json!(revision.as_str())
    );
    repository
        .connection
        .execute(
            "UPDATE story_revisions_v2 SET content_json='{}' WHERE revision_id=?1",
            [revision.as_str()],
        )
        .unwrap();
    assert!(matches!(
        load(&repository.connection, &project, /*snapshot*/ None),
        Err(RepositoryError::CorruptState)
    ));
    assert_eq!(
        repository.task_property_rule_input(&input_task),
        Err(RepositoryError::CorruptState)
    );
    assert_eq!(
        repository.task_property_rule_page(&input_task, /*cursor*/ None),
        Err(RepositoryError::CorruptState)
    );
    repository
        .connection
        .execute(
            "UPDATE story_revisions_v2 SET content_json=?1 WHERE revision_id=?2",
            rusqlite::params![content().to_string(), revision.as_str()],
        )
        .unwrap();
    repository
        .undo_acceptance(UndoAcceptanceRequest {
            project_id: project.clone(),
            branch_id: BranchId::parse("main").unwrap(),
            target_commit_id: CommitId::parse("rule-accept").unwrap(),
            commit_id: CommitId::parse("rule-undo").unwrap(),
            idempotency_key: IdempotencyKey::parse("rule-undo").unwrap(),
            request_digest: "undo".into(),
            expected_project_revision: 1,
            next_snapshot_id: SnapshotId::parse("rule-undone").unwrap(),
        })
        .unwrap();
    assert_eq!(
        load(&repository.connection, &project, /*snapshot*/ None)
            .unwrap()
            .revision_id,
        None
    );
    repository
        .connection
        .execute(
            "DELETE FROM story_acceptances_v2 WHERE commit_id='rule-accept'",
            [],
        )
        .unwrap();
    assert!(matches!(
        load(&repository.connection, &project, /*snapshot*/ None),
        Err(RepositoryError::CorruptState)
    ));
}
