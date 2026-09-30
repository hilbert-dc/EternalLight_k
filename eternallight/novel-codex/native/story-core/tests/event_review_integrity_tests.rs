use novel_codex_story_core::{
    AcceptReviewRequest, BranchId, CommitId, CreateReviewRequest, IdempotencyKey, ObjectId,
    ProjectId, ProjectRecord, RepositoryError, RevisionId, SaveCandidateRequest, SnapshotId,
    SqliteStoryRepository,
};
use pretty_assertions::assert_eq;
use rusqlite::{Connection, params};
use serde_json::{Value, json};

#[test]
fn corrupted_event_source_cannot_create_or_accept_review_and_failures_are_atomic() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("event-review.sqlite3");
    let mut repository = SqliteStoryRepository::open(&path).unwrap();
    let project = ProjectId::parse("event-review").unwrap();
    let branch = BranchId::parse("main").unwrap();
    let object = ObjectId::parse("draft-event").unwrap();
    let revision = RevisionId::parse("event-source").unwrap();
    repository
        .create_project(
            ProjectRecord {
                project_id: project.clone(),
                title: "交信".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            branch.clone(),
            SnapshotId::parse("event-start").unwrap(),
        )
        .unwrap();
    repository
        .save_candidate(SaveCandidateRequest {
            project_id: project.clone(),
            branch_id: branch.clone(),
            object_id: object.clone(),
            object_kind: "draft".into(),
            revision_id: revision.clone(),
            commit_id: CommitId::parse("event-save").unwrap(),
            idempotency_key: IdempotencyKey::parse("event-save").unwrap(),
            request_digest: "event-save".into(),
            expected_project_revision: 0,
            expected_snapshot_id: SnapshotId::parse("event-start").unwrap(),
            next_snapshot_id: SnapshotId::parse("event-candidate").unwrap(),
            expected_candidate_revision_id: None,
            content_digest: "event-body".into(),
            content_json: json!({"body":"林夕交信","chapterNumber":1,
            "sceneGroups":[{"id":"group-event","sceneId":"scene-event","body":"林夕交信"}]})
            .to_string(),
            chapter_projection: None,
            task_output: None,
            linked_impact_ids: vec![],
        })
        .unwrap();
    let evidence = json!({"quote":"林夕交信","start":0,"end":4});
    let entities = [
        ("delivery", "event"),
        ("linxi", "character"),
        ("letter", "item"),
    ]
    .map(
        |(id, kind)| json!({"id":id,"kind":kind,"displayName":id,"aliases":[],"evidence":evidence}),
    );
    let claim = json!({"kind":"event","subjectId":"delivery","predicate":"transfer","storyTime":1,
        "value":{"participants":[{"role":"actor","entityId":"linxi"},{"role":"item","entityId":"letter"}],
            "preconditions":[{"sourceBatchId":null,"assertionIndex":1}],"effects":[],"untilExclusive":null},"evidence":evidence});
    let prerequisite = json!({"kind":"state","subjectId":"linxi","predicate":"heldLetter","value":true,
        "stateSchema":{"cardinality":"single","valueType":{"type":"boolean"}},"validFrom":0,
        "validUntilExclusive":1,"validUntilEvidence":evidence,"evidence":evidence});
    let connection = Connection::open(&path).unwrap();
    connection.execute("INSERT INTO story_modeling_batches_v2(batch_id,project_id,object_id,source_revision_id,input_project_revision,extractor_version,status,assertions_json,entities_json,created_at,published_at) VALUES('event-batch',?1,?2,?3,0,'test','ready',?4,?5,1,2)",
        params![project.as_str(),object.as_str(),revision.as_str(),json!([claim,prerequisite]).to_string(),json!(entities).to_string()]).unwrap();
    connection.execute("INSERT INTO story_knowledge_heads_v2(project_id,object_id,batch_id) VALUES(?1,?2,'event-batch')",
        params![project.as_str(),object.as_str()]).unwrap();
    let request = CreateReviewRequest {
        review_id: "event-review-ready".into(),
        project_id: project.clone(),
        object_id: object,
        object_kind: "draft".into(),
        revision_id: revision,
        idempotency_key: IdempotencyKey::parse("event-review-ready").unwrap(),
        request_digest: "event-review-ready".into(),
        checks_json: json!([{"blocking":true,"passed":true}]).to_string(),
        proposals_json: "[]".into(),
        modeling_batch_id: Some("event-batch".into()),
        selections_json: "{}".into(),
    };
    let review = repository.create_review(request.clone()).unwrap();
    let proposals: Value = serde_json::from_str(&review.proposals_json).unwrap();
    assert_eq!(
        proposals[0]["storyEvent"]["participants"],
        claim["value"]["participants"]
    );
    let selections = json!({"sceneGroupIds":["group-event"],
        "factDecisions":{proposals[0]["id"].as_str().unwrap():"confirmed"}})
    .to_string();
    let accept = AcceptReviewRequest {
        project_id: project.clone(),
        branch_id: branch.clone(),
        review_id: review.review_id.clone(),
        commit_id: CommitId::parse("event-accept").unwrap(),
        idempotency_key: IdempotencyKey::parse("event-accept").unwrap(),
        request_digest: "event-accept".into(),
        expected_project_revision: 0,
        next_snapshot_id: SnapshotId::parse("event-accepted").unwrap(),
        selections_json: selections,
    };
    let before = (
        repository.project_revision(&project).unwrap(),
        repository.branch_head(&project, &branch).unwrap(),
        repository.object_heads(&project).unwrap(),
        repository.acceptances(&project).unwrap(),
    );
    for (index, mutated) in [
        {
            let mut value = json!(entities);
            value[1]["kind"] = json!("item");
            value
        },
        {
            let mut value = json!(entities);
            value[0]["kind"] = json!("character");
            value
        },
        json!([entities[0], entities[2]]),
        json!([entities[0], entities[1], entities[2], entities[1]]),
        {
            let mut value = json!(entities);
            value[1]["evidence"]["quote"] = json!("伪造证据");
            value
        },
    ]
    .into_iter()
    .enumerate()
    {
        connection.execute("UPDATE story_modeling_batches_v2 SET entities_json=?1 WHERE batch_id='event-batch'",[mutated.to_string()]).unwrap();
        let mut next = request.clone();
        next.review_id = format!("invalid-review-{index}");
        next.idempotency_key = IdempotencyKey::parse(next.review_id.clone()).unwrap();
        next.request_digest = next.review_id.clone();
        assert_eq!(
            repository.create_review(next.clone()),
            Err(RepositoryError::CorruptState)
        );
        assert_eq!(repository.review(&project, &next.review_id).unwrap(), None);
        assert_eq!(
            repository.accept_review(accept.clone()),
            Err(RepositoryError::CorruptState)
        );
        assert_eq!(
            (
                repository.project_revision(&project).unwrap(),
                repository.branch_head(&project, &branch).unwrap(),
                repository.object_heads(&project).unwrap(),
                repository.acceptances(&project).unwrap()
            ),
            before
        );
        assert_eq!(
            repository.review(&project, &review.review_id).unwrap(),
            Some(review.clone())
        );
        let stored: String = connection
            .query_row(
                "SELECT entities_json FROM story_modeling_batches_v2 WHERE batch_id='event-batch'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(serde_json::from_str::<Value>(&stored).unwrap(), mutated);
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM story_review_receipts_v2 WHERE idempotency_key=?1",
                    [next.idempotency_key.as_str()],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            0
        );
        drop(repository);
        repository = SqliteStoryRepository::open(&path).unwrap();
        assert_eq!(
            repository.accept_review(accept.clone()),
            Err(RepositoryError::CorruptState)
        );
    }
    // The same requests can succeed after explicitly restoring the test-injected source;
    // a failed transaction must not have left an acceptance receipt or partial commit.
    connection
        .execute(
            "UPDATE story_modeling_batches_v2 SET entities_json=?1 WHERE batch_id='event-batch'",
            [json!(entities).to_string()],
        )
        .unwrap();
    connection.execute("UPDATE story_modeling_batches_v2 SET assertions_json=json_set(assertions_json,'$[1].value','corrupt boolean') WHERE batch_id='event-batch'",[]).unwrap();
    assert_eq!(
        repository.accept_review(accept.clone()),
        Err(RepositoryError::CorruptState)
    );
    let mut invalid_review = request;
    invalid_review.review_id = "invalid-prerequisite-review".into();
    invalid_review.idempotency_key = IdempotencyKey::parse("invalid-prerequisite-review").unwrap();
    invalid_review.request_digest = "invalid-prerequisite-review".into();
    assert_eq!(
        repository.create_review(invalid_review),
        Err(RepositoryError::CorruptState)
    );
    assert_eq!(repository.project_revision(&project).unwrap(), before.0);
    connection
        .execute(
            "UPDATE story_modeling_batches_v2 SET assertions_json=?1 WHERE batch_id='event-batch'",
            [json!([claim, prerequisite]).to_string()],
        )
        .unwrap();
    let accepted = repository.accept_review(accept.clone()).unwrap();
    assert_eq!(accepted.result_revision, 1);
    assert_eq!(repository.accept_review(accept).unwrap(), accepted);
}
