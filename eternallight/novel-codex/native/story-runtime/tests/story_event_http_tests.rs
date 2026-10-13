use std::path::Path;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use novel_codex_story_core::{BranchId, ProjectId, SqliteStoryRepository};
use novel_codex_story_runtime::router;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const AUTHOR: &str = "event-author-1234567890123456789012345";
const CANDIDATE: &str = "event-candidate-12345678901234567890";
const CAPABILITY: &str = "event-capability-1234567890123456789";

enum Role {
    Author,
    Candidate,
}

#[path = "support/event_rule_http_tests.rs"]
mod condition_tests;

#[path = "support/event_evolution_http_tests.rs"]
mod evolution_tests;

async fn prepare_chapter(
    path: &Path,
    base: &str,
    number: i64,
    body: &str,
) -> (Value, Value, Value) {
    let (status,draft) = call(path,"POST",&format!("{base}/drafts/checkpoints"),json!({"payload":{"chapterNumber":number,"title":"交信","body":body},"expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":format!("cross-draft-{number}")}),Role::Author).await;
    assert_eq!(status, StatusCode::CREATED, "{draft}");
    let (status,batch) = call(path,"POST",&format!("{base}/modeling-batches"),json!({"objectId":format!("draft-{}",draft["chapterId"].as_str().unwrap()),"sourceRevisionId":draft["revisionId"],"expectedRevision":0,"extractorVersion":"cross-event-test"}),Role::Author).await;
    assert_eq!(status, StatusCode::CREATED, "{batch}");
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        + 600;
    let (status,task) = call(path,"POST",&format!("{base}/tasks"),json!({"operation":"modelKnowledge","inputRevision":0,"threadId":"cross-event","target":{"batchId":batch["id"]},"authorIntent":"提取原文","capabilityExpiresAt":expires,"capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes()))}),Role::Author).await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    (draft, batch, task)
}

#[tokio::test]
async fn cross_chapter_prerequisites_publish_atomically_and_replay_after_source_edits() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("cross-events.sqlite3");
    let (status, created) = call(
        &path,
        "POST",
        "/api/projects",
        json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"跨章交信"}),
        Role::Author,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let project = created["project"]["id"].as_str().unwrap();
    let base = format!("/api/projects/{project}");
    let body = "甲持有信。";
    let evidence = json!({"quote":body,"start":0,"end":body.chars().count()});
    let entities = vec![
        json!({"id":"a","kind":"character","displayName":"甲","aliases":[],"evidence":evidence}),
        json!({"id":"letter","kind":"item","displayName":"信","aliases":[],"evidence":evidence}),
    ];
    let prerequisite = json!({"kind":"state","subjectId":"letter","predicate":"held","value":true,"stateSchema":{"cardinality":"single","valueType":{"type":"boolean"}},"validFrom":1,"evidence":evidence});
    let mut earlier = Vec::new();
    for number in [1, 3] {
        let (draft, batch, task) = prepare_chapter(&path, &base, number, body).await;
        let (status,result) = call(&path,"POST",&format!("{base}/modeling-batches/{}/publish",batch["id"].as_str().unwrap()),json!({"taskId":task["id"],"threadId":"cross-event","expectedRevision":0,"entities":entities,"assertions":[prerequisite]}),Role::Candidate).await;
        assert_eq!(status, StatusCode::OK, "{result}");
        earlier.push((draft, batch));
    }
    let (draft, batch, task) = prepare_chapter(&path, &base, /*number*/ 2, "甲交出信。").await;
    let event_evidence = json!({"quote":"甲交出信。","start":0,"end":5});
    let mut event_entities = entities.clone();
    for entity in &mut event_entities {
        entity["evidence"] = event_evidence.clone();
    }
    event_entities.push(json!({"id":"delivery","kind":"event","displayName":"交信","aliases":[],"evidence":event_evidence}));
    let claims = vec![
        json!({"kind":"event","subjectId":"delivery","predicate":"transfer","storyTime":5,"value":{"participants":[{"role":"actor","entityId":"a"},{"role":"item","entityId":"letter"}],"preconditions":[{"sourceBatchId":earlier[0].1["id"],"assertionIndex":0}],"effects":[{"sourceBatchId":null,"assertionIndex":1}],"untilExclusive":null},"evidence":event_evidence}),
        json!({"kind":"state","subjectId":"letter","predicate":"held","value":false,"stateSchema":{"cardinality":"single","valueType":{"type":"boolean"}},"validFrom":5,"evidence":event_evidence}),
    ];
    let publish = format!(
        "{base}/modeling-batches/{}/publish",
        batch["id"].as_str().unwrap()
    );
    for reference in [
        json!({"sourceBatchId":batch["id"],"assertionIndex":1}),
        json!({"sourceBatchId":earlier[1].1["id"],"assertionIndex":0}),
        json!({"sourceBatchId":"absent","assertionIndex":0}),
        json!({"sourceBatchId":earlier[0].1["id"],"assertionIndex":99}),
    ] {
        let mut invalid = claims.clone();
        invalid[0]["value"]["preconditions"] = json!([reference]);
        let (status,error) = call(&path,"POST",&publish,json!({"taskId":task["id"],"threadId":"cross-event","expectedRevision":0,"entities":event_entities,"assertions":invalid}),Role::Candidate).await;
        assert!(!status.is_success(), "{error}");
        let queued = SqliteStoryRepository::open(&path)
            .unwrap()
            .modeling_batch(
                &ProjectId::parse(project).unwrap(),
                batch["id"].as_str().unwrap(),
            )
            .unwrap();
        assert_eq!(
            (queued.status, queued.assertions, queued.entities),
            ("queued".into(), vec![], vec![])
        );
    }
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection.execute("UPDATE story_modeling_batches_v2 SET entities_json=json_set(entities_json,'$[1].kind','character') WHERE batch_id=?1",[earlier[0].1["id"].as_str().unwrap()]).unwrap();
    let (status,error) = call(&path,"POST",&publish,json!({"taskId":task["id"],"threadId":"cross-event","expectedRevision":0,"entities":event_entities,"assertions":claims}),Role::Candidate).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{error}");
    assert!(
        error.to_string().contains("conflicting entity kind"),
        "{error}"
    );
    let queued = SqliteStoryRepository::open(&path)
        .unwrap()
        .modeling_batch(
            &ProjectId::parse(project).unwrap(),
            batch["id"].as_str().unwrap(),
        )
        .unwrap();
    assert_eq!(
        (queued.status, queued.entities, queued.assertions),
        ("queued".into(), vec![], vec![])
    );
    connection
        .execute(
            "UPDATE story_modeling_batches_v2 SET entities_json=?1 WHERE batch_id=?2",
            rusqlite::params![
                json!(entities).to_string(),
                earlier[0].1["id"].as_str().unwrap()
            ],
        )
        .unwrap();
    drop(connection);
    let (status,result) = call(&path,"POST",&publish,json!({"taskId":task["id"],"threadId":"cross-event","expectedRevision":0,"entities":event_entities,"assertions":claims}),Role::Candidate).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    let (status, state) = call(
        &path,
        "GET",
        &format!("{base}/knowledge/state?perspective=world&entityId=letter&predicate=transfer"),
        Value::Null,
        Role::Author,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{state}");
    assert_eq!(
        state["entries"][0]["eventTemporalAssessment"]["preconditions"],
        json!([{"referenceIndex":0,"timing":"applicableBeforeEvent"}])
    );
    let (status,review) = call(&path,"POST",&format!("{base}/reviews"),json!({"artifactKind":"draft","revisionId":draft["revisionId"],"modelingBatchId":batch["id"],"proposals":[],"selections":{},"idempotencyKey":"cross-review"}),Role::Author).await;
    assert_eq!(status, StatusCode::CREATED, "{review}");
    assert_eq!(
        review["factProposals"][0]["storyEvent"]["preconditions"],
        claims[0]["value"]["preconditions"]
    );
    let fact = review["factProposals"][0]["id"].as_str().unwrap();
    let accept_uri = format!("{base}/reviews/{}/accept", review["id"].as_str().unwrap());
    let accept_payload = json!({"expectedRevision":0,"idempotencyKey":"cross-accept","selections":{"sceneGroupIds":[review["factProposals"][0]["sceneGroupId"]],"factDecisions":{fact:"confirmed"}}});
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection.execute("UPDATE story_modeling_batches_v2 SET assertions_json=json_set(assertions_json,'$[0].value','invalid boolean') WHERE batch_id=?1",[earlier[0].1["id"].as_str().unwrap()]).unwrap();
    let (status, error) = call(
        &path,
        "POST",
        &accept_uri,
        accept_payload.clone(),
        Role::Author,
    )
    .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{error}");
    let (status,error) = call(&path,"POST",&format!("{base}/reviews"),json!({"artifactKind":"draft","revisionId":draft["revisionId"],"modelingBatchId":batch["id"],"proposals":[],"selections":{},"idempotencyKey":"damaged-cross-review"}),Role::Author).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{error}");
    assert_eq!(
        SqliteStoryRepository::open(&path)
            .unwrap()
            .project_revision(&ProjectId::parse(project).unwrap())
            .unwrap(),
        0
    );
    // Restore only injected authority: neither failure may consume the acceptance request.
    connection
        .execute(
            "UPDATE story_modeling_batches_v2 SET assertions_json=?1 WHERE batch_id=?2",
            rusqlite::params![
                json!([prerequisite]).to_string(),
                earlier[0].1["id"].as_str().unwrap()
            ],
        )
        .unwrap();
    for (target, originals, damaged, key) in [
        (
            earlier[0].1["id"].as_str().unwrap(),
            json!(entities),
            {
                let mut value = json!(entities);
                value[1]["kind"] = json!("character");
                value
            },
            "cross-kind-review",
        ),
        (
            batch["id"].as_str().unwrap(),
            json!(event_entities),
            {
                let mut value = json!(event_entities);
                value[0]["evidence"]["quote"] = json!("伪造的正文");
                value
            },
            "cross-local-evidence-review",
        ),
    ] {
        connection
            .execute(
                "UPDATE story_modeling_batches_v2 SET entities_json=?1 WHERE batch_id=?2",
                rusqlite::params![damaged.to_string(), target],
            )
            .unwrap();
        let (status, error) = call(
            &path,
            "POST",
            &accept_uri,
            accept_payload.clone(),
            Role::Author,
        )
        .await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{error}");
        let (status,error) = call(&path,"POST",&format!("{base}/reviews"),json!({"artifactKind":"draft","revisionId":draft["revisionId"],"modelingBatchId":batch["id"],"proposals":[],"selections":{},"idempotencyKey":key}),Role::Author).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{error}");
        assert_eq!(
            SqliteStoryRepository::open(&path)
                .unwrap()
                .project_revision(&ProjectId::parse(project).unwrap())
                .unwrap(),
            0
        );
        connection
            .execute(
                "UPDATE story_modeling_batches_v2 SET entities_json=?1 WHERE batch_id=?2",
                rusqlite::params![originals.to_string(), target],
            )
            .unwrap();
    }
    drop(connection);
    let (status, accepted) = call(&path, "POST", &accept_uri, accept_payload, Role::Author).await;
    assert_eq!(status, StatusCode::OK, "{accepted}");
    let repository = SqliteStoryRepository::open(&path).unwrap();
    let project_id = ProjectId::parse(project).unwrap();
    let snapshot = repository
        .branch_head(&project_id, &BranchId::parse("main").unwrap())
        .unwrap()
        .unwrap()
        .snapshot_id;
    drop(repository);
    let (status, current) = call(
        &path,
        "GET",
        &format!("{base}/knowledge/state?perspective=world"),
        Value::Null,
        Role::Author,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{current}");
    assert_eq!(
        current["entries"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|entry| entry["confirmationState"] == "confirmed")
            .count(),
        1
    );
    let (status,edited) = call(&path,"POST",&format!("{base}/drafts/checkpoints"),json!({"payload":{"chapterId":earlier[0].0["chapterId"],"chapterNumber":1,"title":"改稿","body":"甲没有信。"},"expectedRevision":1,"expectedCandidateRevisionId":earlier[0].0["revisionId"],"idempotencyKey":"cross-edit"}),Role::Author).await;
    assert_eq!(status, StatusCode::CREATED, "{edited}");
    let repository = SqliteStoryRepository::open(&path).unwrap();
    assert_eq!(
        repository
            .modeling_batch(&project_id, batch["id"].as_str().unwrap())
            .unwrap()
            .status,
        "stale"
    );
    let (status, now) = call(
        &path,
        "GET",
        &format!("{base}/knowledge/state?perspective=world&entityId=letter&predicate=transfer"),
        Value::Null,
        Role::Author,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{now}");
    assert_eq!(now["entries"], json!([]));
    let (status,old) = call(&path,"GET",&format!("{base}/knowledge/state?perspective=world&entityId=letter&predicate=transfer&snapshotId={snapshot}"),Value::Null,Role::Author).await;
    assert_eq!(status, StatusCode::OK, "{old}");
    assert_eq!(old["entries"][0]["assertion"], claims[0]);
    assert_eq!(
        old["entries"][0]["eventTemporalAssessment"],
        state["entries"][0]["eventTemporalAssessment"]
    );
}

async fn call(
    path: &Path,
    method: &str,
    uri: &str,
    payload: Value,
    role: Role,
) -> (StatusCode, Value) {
    let token = match role {
        Role::Author => AUTHOR,
        Role::Candidate => CANDIDATE,
    };
    let response = router(task_fixture::state(
        path.to_owned(),
        AUTHOR.into(),
        CANDIDATE.into(),
    ))
    .author_fixture_request(
        Request::builder()
            .method(method)
            .uri(uri)
            .header("authorization", format!("Bearer {token}"))
            .header("x-novel-task-capability", CAPABILITY)
            .header("content-type", "application/json")
            .body(if payload.is_null() {
                Body::empty()
            } else {
                Body::from(payload.to_string())
            })
            .unwrap(),
    )
    .await
    .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or_else(|error| {
            panic!(
                "{uri}: {status}: {error}: {}",
                String::from_utf8_lossy(&bytes)
            )
        }),
    )
}

#[tokio::test]
async fn typed_event_lifecycle_preserves_sources_participant_recall_and_independent_confirmation() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("events.sqlite3");
    let (status, created) = call(
        &path,
        "POST",
        "/api/projects",
        json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"交信"}),
        Role::Author,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let project = created["project"]["id"].as_str().unwrap();
    let base = format!("/api/projects/{project}");
    let body = "林夕把密函交给张衡，密函在张衡手中。🙂";
    let evidence = json!({"quote":body,"start":0,"end":body.chars().count()});
    let (status, draft) = call(
        &path,
        "POST",
        &format!("{base}/drafts/checkpoints"),
        json!({
            "payload":{"chapterNumber":1,"title":"交信","body":body},"expectedRevision":0,
            "expectedCandidateRevisionId":null,"idempotencyKey":"event-draft"
        }),
        Role::Author,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{draft}");
    let (status,batch) = call(&path,"POST",&format!("{base}/modeling-batches"),json!({
        "objectId":format!("draft-{}",draft["chapterId"].as_str().unwrap()),"sourceRevisionId":draft["revisionId"],
        "expectedRevision":0,"extractorVersion":"typed-event-test"
    }),Role::Author).await;
    assert_eq!(status, StatusCode::CREATED, "{batch}");
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        + 600;
    let (status,task) = call(&path,"POST",&format!("{base}/tasks"),json!({
        "operation":"modelKnowledge","inputRevision":0,"threadId":"event-model","target":{"batchId":batch["id"]},
        "authorIntent":"按原文解释交信事件","capabilityExpiresAt":expires,
        "capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes()))
    }),Role::Author).await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    let entities = [("delivery","event","交信"),("linxi","character","林夕"),("zhangheng","character","张衡"),("letter","item","密函")]
        .map(|(id,kind,name)|json!({"id":id,"kind":kind,"displayName":name,"aliases":[],"evidence":evidence}));
    let claims = vec![
        json!({"kind":"event","subjectId":"delivery","predicate":"transfer","storyTime":5,
            "value":{"participants":[{"role":"actor","entityId":"linxi"},{"role":"recipient","entityId":"zhangheng"},{"role":"item","entityId":"letter"}],
                "preconditions":[{"sourceBatchId":null,"assertionIndex":2}],"effects":[{"sourceBatchId":null,"assertionIndex":1}],"untilExclusive":6},"evidence":evidence}),
        json!({"kind":"state","subjectId":"letter","predicate":"holder","value":{"entityId":"zhangheng"},
            "stateSchema":{"cardinality":"single","valueType":{"type":"entity","kind":"character"}},"validFrom":5,"evidence":evidence}),
        json!({"kind":"state","subjectId":"letter","predicate":"holder","value":{"entityId":"linxi"},
            "stateSchema":{"cardinality":"single","valueType":{"type":"entity","kind":"character"}},"validFrom":1,"validUntilExclusive":5,
            "validUntilEvidence":evidence,"evidence":evidence}),
    ];
    let publish_uri = format!(
        "{base}/modeling-batches/{}/publish",
        batch["id"].as_str().unwrap()
    );
    for (pointer, value) in [
        ("/value/participants/0/role", json!("item")),
        (
            "/value/effects",
            json!([{"sourceBatchId":null,"assertionIndex":99}]),
        ),
        (
            "/value/effects",
            json!([{"sourceBatchId":null,"assertionIndex":0}]),
        ),
        (
            "/value/effects",
            json!([{"sourceBatchId":null,"assertionIndex":1},{"sourceBatchId":null,"assertionIndex":1}]),
        ),
    ] {
        let mut invalid = claims.clone();
        *invalid[0].pointer_mut(pointer).unwrap() = value;
        let (status, rejected) = call(
            &path,
            "POST",
            &publish_uri,
            json!({"taskId":task["id"],"threadId":"event-model",
            "expectedRevision":0,"entities":entities,"assertions":invalid}),
            Role::Candidate,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{rejected}");
        let persisted = SqliteStoryRepository::open(&path)
            .unwrap()
            .modeling_batch(
                &ProjectId::parse(project).unwrap(),
                batch["id"].as_str().unwrap(),
            )
            .unwrap();
        assert!(persisted.assertions.is_empty());
    }
    let (status, published) = call(
        &path,
        "POST",
        &publish_uri,
        json!({"taskId":task["id"],"threadId":"event-model",
        "expectedRevision":0,"entities":entities,"assertions":claims}),
        Role::Candidate,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{published}");
    for (time, expected) in [
        (4, vec![claims[2].clone()]),
        (5, vec![claims[0].clone(), claims[1].clone()]),
        (7, vec![claims[0].clone(), claims[1].clone()]),
    ] {
        let (status, state) = call(
            &path,
            "GET",
            &format!("{base}/knowledge/state?perspective=world&entityId=letter&worldTime={time}"),
            Value::Null,
            Role::Author,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{state}");
        let actual = state["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["assertion"].clone())
            .collect::<Vec<_>>();
        assert_eq!(actual, expected);
    }
    for id in ["linxi", "zhangheng"] {
        let (status,state) = call(&path,"GET",&format!("{base}/knowledge/state?perspective=world&entityId={id}&predicate=transfer&worldTime=7"),Value::Null,Role::Author).await;
        assert_eq!(status, StatusCode::OK, "{state}");
        assert_eq!(state["entries"][0]["assertion"], claims[0]);
        assert_eq!(
            state["entries"][0]["eventTemporalAssessment"]["preconditions"],
            json!([{"referenceIndex":0,"timing":"applicableBeforeEvent"}])
        );
        assert_eq!(
            state["entries"][0]["eventTemporalAssessment"]["effects"],
            json!([{"referenceIndex":0,"timing":"atEvent"}])
        );
        assert_eq!(state["entries"].as_array().unwrap().len(), 1);
    }
    for query in [
        "perspective=character&ownerId=linxi",
        "perspective=reader&readingChapter=0",
    ] {
        let (status, state) = call(
            &path,
            "GET",
            &format!("{base}/knowledge/state?{query}"),
            Value::Null,
            Role::Author,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{state}");
        assert_eq!(state["entries"], json!([]));
    }
    let (status, _) = call(
        &path,
        "GET",
        &format!("{base}/knowledge/state?perspective=author"),
        Value::Null,
        Role::Candidate,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, review) = call(
        &path,
        "POST",
        &format!("{base}/reviews"),
        json!({
            "artifactKind":"draft","revisionId":draft["revisionId"],"modelingBatchId":batch["id"],
            "proposals":[],"selections":{},"idempotencyKey":"event-review"
        }),
        Role::Author,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{review}");
    assert_eq!(
        review["factProposals"][0]["storyEvent"]["effects"],
        json!([{"sourceBatchId":null,"assertionIndex":1}])
    );
    assert_eq!(
        review["factProposals"][0]["storyEvent"]["temporalAssessment"]["preconditions"],
        json!([{"referenceIndex":0,"timing":"applicableBeforeEvent"}])
    );
    let fact = review["factProposals"][0]["id"].as_str().unwrap();
    let (status,accepted) = call(&path,"POST",&format!("{base}/reviews/{}/accept",review["id"].as_str().unwrap()),json!({
        "expectedRevision":0,"idempotencyKey":"event-accept","selections":{
            "sceneGroupIds":[review["factProposals"][0]["sceneGroupId"]],"factDecisions":{fact:"confirmed"}}
    }),Role::Author).await;
    assert_eq!(status, StatusCode::OK, "{accepted}");
    let (_, state) = call(
        &path,
        "GET",
        &format!("{base}/knowledge/state?perspective=world"),
        Value::Null,
        Role::Author,
    )
    .await;
    assert_eq!(
        state["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["confirmationState"].clone())
            .collect::<Vec<_>>(),
        vec![json!("confirmed"), json!("candidate"), json!("candidate")]
    );
    let snapshot = SqliteStoryRepository::open(&path)
        .unwrap()
        .branch_head(
            &ProjectId::parse(project).unwrap(),
            &BranchId::parse("main").unwrap(),
        )
        .unwrap()
        .unwrap()
        .snapshot_id;
    let (status, context) = call(
        &path,
        "GET",
        &format!("{base}/context?projectRevision=1&purpose=planner&budget=24&byteBudget=60000"),
        Value::Null,
        Role::Author,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{context}");
    assert_eq!(context["stateFacts"], json!([]));
    assert_eq!(
        context["coverage"]["scannedScope"]["typedEventFactsWithheld"],
        1
    );
    assert!(
        context["coverage"]["missingScope"]
            .as_array()
            .unwrap()
            .contains(&json!("typedEventFacts"))
    );
    let (status, _) = call(
        &path,
        "POST",
        &format!(
            "{base}/commits/{}/undo",
            accepted["commitId"].as_str().unwrap()
        ),
        json!({"expectedRevision":1,"idempotencyKey":"event-undo"}),
        Role::Author,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, current) = call(
        &path,
        "GET",
        &format!("{base}/knowledge/state?perspective=world&predicate=transfer"),
        Value::Null,
        Role::Author,
    )
    .await;
    assert_eq!(current["entries"][0]["confirmationState"], "candidate");
    let (_, historical) = call(
        &path,
        "GET",
        &format!("{base}/knowledge/state?perspective=world&snapshotId={snapshot}"),
        Value::Null,
        Role::Author,
    )
    .await;
    assert_eq!(historical["entries"][0]["assertion"], claims[0]);
    assert_eq!(historical["entries"][0]["confirmationState"], "confirmed");
    assert_eq!(
        historical["entries"][0]["eventTemporalAssessment"],
        current["entries"][0]["eventTemporalAssessment"]
    );
    let (status,edited) = call(&path,"POST",&format!("{base}/drafts/checkpoints"),json!({
        "payload":{"chapterId":draft["chapterId"],"chapterNumber":1,"title":"交信","body":"林夕没有交信。"},
        "expectedRevision":2,"expectedCandidateRevisionId":draft["revisionId"],"idempotencyKey":"event-edit"
    }),Role::Author).await;
    assert_eq!(status, StatusCode::CREATED, "{edited}");
    let (_, current) = call(
        &path,
        "GET",
        &format!("{base}/knowledge/state?perspective=world"),
        Value::Null,
        Role::Author,
    )
    .await;
    assert_eq!(current["entries"], json!([]));
    let (status, late) = call(
        &path,
        "POST",
        &publish_uri,
        json!({"taskId":task["id"],"threadId":"event-model",
        "expectedRevision":0,"entities":entities,"assertions":claims}),
        Role::Candidate,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{late}");
    let (_, historical) = call(
        &path,
        "GET",
        &format!("{base}/knowledge/state?perspective=world&snapshotId={snapshot}"),
        Value::Null,
        Role::Author,
    )
    .await;
    assert_eq!(historical["entries"][0]["assertion"], claims[0]);
}
#[path = "support/author_task_fixture.rs"]
mod task_fixture;
use task_fixture::AuthorTaskFixture;

use novel_codex_story_core::{ModelingRepository, ProjectRevisionRepository};
