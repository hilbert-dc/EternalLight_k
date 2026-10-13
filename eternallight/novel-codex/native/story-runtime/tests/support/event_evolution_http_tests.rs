use super::{Role, call, prepare_chapter};
use axum::http::StatusCode;
use novel_codex_story_core::{BranchId, ProjectId, SqliteStoryRepository};
use pretty_assertions::assert_eq;
use serde_json::{Value, json};

// Exercise the public router, reopening the actual SQLite repository on each request.
#[tokio::test]
async fn event_evolution_separates_world_claims_confirmation_history_and_effects() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("event-evolution.sqlite3");
    let (status, created) = call(
        &path,
        "POST",
        "/api/projects",
        json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"交信演进"}),
        Role::Author,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let project = created["project"]["id"].as_str().unwrap();
    let base = format!("/api/projects/{project}");
    let body = "甲把信交出，港口门开着。";
    let evidence = json!({"quote":body,"start":0,"end":body.chars().count()});
    let (draft, batch, task) = prepare_chapter(&path, &base, /*number*/ 1, body).await;
    let batch_id = batch["id"].as_str().unwrap();
    let entities = [
        ("a", "character", "甲"), ("letter", "item", "信"),
        ("port", "location", "港口"), ("opened", "proposition", "门开着"),
        ("delivery", "event", "交信"),
    ].map(|(id, kind, display)| json!({"id":id,"kind":kind,"displayName":display,"aliases":[],"evidence":evidence}));
    let assertions = json!([
        {"kind":"event","subjectId":"delivery","predicate":"transfer","storyTime":5,
         "value":{"participants":[{"role":"actor","entityId":"a"},{"role":"item","entityId":"letter"}],
            "preconditions":[{"sourceBatchId":null,"assertionIndex":1}],
            "effects":[{"sourceBatchId":null,"assertionIndex":2}],"untilExclusive":6},"evidence":evidence},
        {"kind":"state","subjectId":"letter","predicate":"held","value":true,
         "stateSchema":{"cardinality":"single","valueType":{"type":"boolean"}},
         "validFrom":1,"validUntilExclusive":5,"validUntilEvidence":evidence,"evidence":evidence},
        {"kind":"state","subjectId":"letter","predicate":"held","value":false,
         "stateSchema":{"cardinality":"single","valueType":{"type":"boolean"}},
         "validFrom":5,"validUntilExclusive":8,"validUntilEvidence":evidence,"evidence":evidence},
        {"kind":"relation","subjectId":"a","predicate":"locatedAt","relationType":"locatedAt","targetId":"port","propositionId":"opened",
         "value":true,"validFrom":1,"validUntilExclusive":7,"validUntilEvidence":evidence,"evidence":evidence},
        {"kind":"epistemic","subjectId":"opened","propositionId":"opened","predicate":"knows",
         "epistemicOwnerId":"a","epistemicState":"known","epistemicTime":2,"value":true,"evidence":evidence},
        {"kind":"relation","subjectId":"opened","predicate":"supports","relationType":"supports","targetId":"opened","propositionId":"opened",
         "value":true,"storyTime":2,"evidence":evidence}
    ]);
    let (status, published) = call(&path,"POST",&format!("{base}/modeling-batches/{batch_id}/publish"),
        json!({"taskId":task["id"],"threadId":"cross-event","expectedRevision":0,"entities":entities,"assertions":assertions}),Role::Candidate).await;
    assert_eq!(status, StatusCode::OK, "{published}");
    let world_uri =
        format!("{base}/knowledge/evolution?dimension=world&projectRevision=0&limit=100");
    let (status, initial) = call(&path, "GET", &world_uri, Value::Null, Role::Author).await;
    assert_eq!(status, StatusCode::OK, "{initial}");
    let entries = initial["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 7);
    assert!(
        entries
            .iter()
            .all(|entry| entry["predicate"] != "knows" && entry["predicate"] != "supports")
    );
    let end = entries
        .iter()
        .find(|entry| entry["change"] == "relationEnded")
        .unwrap();
    assert_eq!(
        (
            &end["worldTime"],
            &end["before"],
            &end["after"],
            &end["interpretation"]
        ),
        (
            &json!(7),
            &json!(true),
            &Value::Null,
            &json!("claimNoLongerApplies")
        )
    );
    let event = entries
        .iter()
        .find(|entry| entry["change"] == "event")
        .unwrap();
    let expected_effect = json!({"referenceIndex":0,"interpretation":"coincidentClaim","conditionCompatibility":"unruled","confirmationState":"candidate","confirmedFactId":null});
    assert_eq!(
        event["eventEvolutionAssessment"],
        json!({"basis":"declaredSourceEffectLinks","effects":[expected_effect],"stateChangesExecuted":false,"causalityCertified":false,"worldTruthCertified":false})
    );
    assert_eq!(
        (&event["modelingBatchId"], &event["assertionIndex"]),
        (&json!(batch_id), &json!(0))
    );
    assert_eq!(initial["coverage"]["unresolvedEffectsInPage"], 1);
    assert_eq!(initial["coverage"]["semanticCompleteness"], "unknown");
    let (status, narrative) = call(
        &path,
        "GET",
        &world_uri.replace("dimension=world", "dimension=narrative"),
        Value::Null,
        Role::Author,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{narrative}");
    assert_eq!(narrative["entries"].as_array().unwrap().len(), 6);
    let query = format!("{base}/knowledge/state?perspective=world&predicate=transfer");
    let (status, projection) = call(&path, "GET", &query, Value::Null, Role::Author).await;
    assert_eq!(status, StatusCode::OK, "{projection}");
    assert_eq!(
        &projection["entries"][0]["eventEvolutionAssessment"],
        &event["eventEvolutionAssessment"]
    );
    let (status, effect) = call(
        &path,
        "GET",
        &format!(
            "{base}/knowledge/state?perspective=world&entityId=letter&predicate=held&worldTime=7"
        ),
        Value::Null,
        Role::Author,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{effect}");
    assert_eq!(effect["entries"].as_array().unwrap().len(), 1);
    assert_eq!(effect["entries"][0]["assertion"]["value"], false);
    // Confirm only the event. Its declared effect must retain independent candidate authority.
    let (status,review) = call(&path,"POST",&format!("{base}/reviews"),json!({"artifactKind":"draft","revisionId":draft["revisionId"],"modelingBatchId":batch["id"],"proposals":[],"selections":{},"idempotencyKey":"evolution-review"}),Role::Author).await;
    assert_eq!(status, StatusCode::CREATED, "{review}");
    let fact = review["factProposals"][0]["id"].as_str().unwrap();
    let (status,receipt) = call(&path,"POST",&format!("{base}/reviews/{}/accept",review["id"].as_str().unwrap()),
        json!({"expectedRevision":0,"idempotencyKey":"evolution-accept","selections":{"sceneGroupIds":[review["factProposals"][0]["sceneGroupId"]],"factDecisions":{fact:"confirmed"}}}),Role::Author).await;
    assert_eq!(status, StatusCode::OK, "{receipt}");
    let snapshot = SqliteStoryRepository::open(&path)
        .unwrap()
        .branch_head(
            &ProjectId::parse(project).unwrap(),
            &BranchId::parse("main").unwrap(),
        )
        .unwrap()
        .unwrap()
        .snapshot_id;
    let historical_uri =
        format!("{base}/knowledge/evolution?dimension=world&snapshotId={snapshot}&limit=100");
    let (status, historical) = call(&path, "GET", &historical_uri, Value::Null, Role::Author).await;
    assert_eq!(status, StatusCode::OK, "{historical}");
    let confirmed_event = historical["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["change"] == "event")
        .unwrap();
    assert_eq!(confirmed_event["confirmationState"], "confirmed");
    assert_eq!(
        confirmed_event["eventEvolutionAssessment"]["effects"],
        json!([expected_effect])
    );
    let (status, limited) = call(
        &path,
        "GET",
        &format!("{base}/knowledge/state?perspective=reader&readingChapter=1&predicate=transfer"),
        Value::Null,
        Role::Author,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{limited}");
    assert_eq!(limited["entries"].as_array().unwrap().len(), 1);
    assert!(
        limited["entries"][0]
            .get("eventEvolutionAssessment")
            .is_none()
    );
    assert!(
        limited["entries"][0]
            .get("eventConditionAssessment")
            .is_none()
    );
    let (status, undone) = call(
        &path,
        "POST",
        &format!(
            "{base}/commits/{}/undo",
            receipt["commitId"].as_str().unwrap()
        ),
        json!({"expectedRevision":1,"idempotencyKey":"evolution-undo"}),
        Role::Author,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{undone}");
    let current_uri = world_uri.replace("projectRevision=0", "projectRevision=2");
    let (status, current) = call(&path, "GET", &current_uri, Value::Null, Role::Author).await;
    assert_eq!(status, StatusCode::OK, "{current}");
    let current_event = current["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["change"] == "event")
        .unwrap();
    assert_eq!(current_event["confirmationState"], "candidate");
    let (status, replayed) = call(&path, "GET", &historical_uri, Value::Null, Role::Author).await;
    assert_eq!(status, StatusCode::OK, "{replayed}");
    assert_eq!(replayed, historical);
    // A syntactically valid source mutation must still invalidate a cursor, even without changed times or IDs.
    let page_uri = current_uri.replace("limit=100", "limit=1");
    let (status, page) = call(&path, "GET", &page_uri, Value::Null, Role::Author).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    let cursor = page["nextCursor"].as_str().unwrap();
    let connection = rusqlite::Connection::open(&path).unwrap();
    let mut changed = assertions.clone();
    changed[2]["value"] = json!(true);
    connection
        .execute(
            "UPDATE story_modeling_batches_v2 SET assertions_json=?1 WHERE batch_id=?2",
            rusqlite::params![changed.to_string(), batch_id],
        )
        .unwrap();
    let (status, rejected) = call(
        &path,
        "GET",
        &format!("{page_uri}&cursor={cursor}"),
        Value::Null,
        Role::Author,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{rejected}");
    connection
        .execute(
            "UPDATE story_modeling_batches_v2 SET assertions_json=?1 WHERE batch_id=?2",
            rusqlite::params![assertions.to_string(), batch_id],
        )
        .unwrap();
    // Invalid original evidence is a hard integrity failure for both dimensions, not an omitted row.
    changed = assertions.clone();
    changed[2]["evidence"]["quote"] = json!("损坏原文");
    connection
        .execute(
            "UPDATE story_modeling_batches_v2 SET assertions_json=?1 WHERE batch_id=?2",
            rusqlite::params![changed.to_string(), batch_id],
        )
        .unwrap();
    for dimension in ["world", "narrative"] {
        let (status, error) = call(
            &path,
            "GET",
            &current_uri.replace("dimension=world", &format!("dimension={dimension}")),
            Value::Null,
            Role::Author,
        )
        .await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{error}");
    }
    connection
        .execute(
            "UPDATE story_modeling_batches_v2 SET assertions_json=?1 WHERE batch_id=?2",
            rusqlite::params![assertions.to_string(), batch_id],
        )
        .unwrap();
    connection
        .execute(
            "DELETE FROM story_snapshot_knowledge_v2 WHERE snapshot_id=?1",
            [snapshot.as_str()],
        )
        .unwrap();
    for dimension in ["world", "narrative"] {
        let (status, error) = call(
            &path,
            "GET",
            &historical_uri.replace("dimension=world", &format!("dimension={dimension}")),
            Value::Null,
            Role::Author,
        )
        .await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{error}");
    }
}

use novel_codex_story_core::{ProjectRevisionRepository};
