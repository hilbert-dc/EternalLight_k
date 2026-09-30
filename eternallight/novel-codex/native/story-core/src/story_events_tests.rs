use pretty_assertions::assert_eq;
use serde_json::{Value, json};

use super::validate_batch;
use crate::{ProjectId, RepositoryError, SqliteStoryRepository};

fn fixture() -> (Vec<Value>, Vec<Value>) {
    let evidence = json!({"quote":"甲交信给乙","start":0,"end":5});
    let entities = vec![
        json!({"id":"delivery","kind":"event"}),
        json!({"id":"a","kind":"character"}),
        json!({"id":"b","kind":"character"}),
        json!({"id":"letter","kind":"item"}),
    ];
    let claims = vec![
        json!({"kind":"event","subjectId":"delivery","predicate":"transfer","storyTime":5,
            "value":{"participants":[{"role":"actor","entityId":"a"},{"role":"recipient","entityId":"b"},{"role":"item","entityId":"letter"}],
                "preconditions":[],"effects":[{"sourceBatchId":null,"assertionIndex":1}],"untilExclusive":6},"evidence":evidence}),
        json!({"kind":"state","subjectId":"letter","predicate":"holder","value":{"entityId":"b"},
            "stateSchema":{"cardinality":"single","valueType":{"type":"entity","kind":"character"}},"validFrom":6,"evidence":evidence}),
    ];
    (claims, entities)
}

#[test]
fn event_links_are_typed_source_local_unique_and_disclosure_bounded() {
    let (claims, entities) = fixture();
    assert_eq!(validate_batch(&claims, &entities), Ok(()));
    for (path, value) in [
        ("/value/participants/0/role", json!("item")),
        ("/value/participants/0/entityId", json!("missing")),
        (
            "/value/effects",
            json!([{"sourceBatchId":null,"assertionIndex":0}]),
        ),
        (
            "/value/effects",
            json!([{"sourceBatchId":null,"assertionIndex":2}]),
        ),
        (
            "/value/effects",
            json!([{"sourceBatchId":null,"assertionIndex":1},{"sourceBatchId":null,"assertionIndex":1}]),
        ),
        (
            "/value/preconditions",
            json!([{"sourceBatchId":null,"assertionIndex":1}]),
        ),
        (
            "/value/effects",
            json!([{"sourceBatchId":"prior","assertionIndex":1}]),
        ),
        ("/value/preconditions", json!([{"assertionIndex":1}])),
        (
            "/value/preconditions",
            json!([{"sourceBatchId":"x".repeat(161),"assertionIndex":1}]),
        ),
        ("/value/untilExclusive", json!(5)),
        ("/storyTime", Value::Null),
        ("/subjectId", json!("a")),
    ] {
        let mut changed = claims.clone();
        *changed[0].pointer_mut(path).unwrap() = value;
        assert!(validate_batch(&changed, &entities).is_err(), "{path}");
    }
    let mut outside = claims.clone();
    outside[1]["evidence"] = json!({"quote":"乙藏信","start":6,"end":9});
    assert!(validate_batch(&outside, &entities).is_err());
    let mut unrelated = claims.clone();
    unrelated[1]["subjectId"] = json!("delivery");
    assert!(validate_batch(&unrelated, &entities).is_err());
}

#[test]
fn missing_event_source_and_value_fields_report_the_correctable_shape() {
    let (claims, entities) = fixture();
    let mut missing_evidence = claims.clone();
    missing_evidence[0]
        .as_object_mut()
        .unwrap()
        .remove("evidence");
    assert!(matches!(
        validate_batch(&missing_evidence, &entities),
        Err(RepositoryError::InvalidContent(message)) if message.contains("{quote,start,end}")
    ));

    let mut missing_links = claims;
    missing_links[0]["value"]
        .as_object_mut()
        .unwrap()
        .remove("preconditions");
    assert!(matches!(
        validate_batch(&missing_links, &entities),
        Err(RepositoryError::InvalidContent(message)) if message.contains("participants, preconditions, effects and untilExclusive")
    ));
}

#[test]
fn damaged_persisted_event_cannot_be_returned_as_a_ready_model() {
    let repository = SqliteStoryRepository::in_memory().unwrap();
    let project = ProjectId::parse("event-corruption").unwrap();
    repository.connection.execute("INSERT INTO story_projects_v2(project_id,title,logline,created_at,updated_at) VALUES(?1,'event','',1,1)",[project.as_str()]).unwrap();
    repository.connection.execute("INSERT INTO story_revisions_v2(revision_id,project_id,object_id,object_kind,content_digest,content_json) VALUES('event-source',?1,'draft-event','draft','digest','{}')",[project.as_str()]).unwrap();
    let (mut claims, entities) = fixture();
    claims[0]["value"]["effects"] = json!([{"sourceBatchId":null,"assertionIndex":99}]);
    repository.connection.execute("INSERT INTO story_modeling_batches_v2(batch_id,project_id,object_id,source_revision_id,input_project_revision,extractor_version,status,assertions_json,entities_json,created_at) VALUES('event-batch',?1,'draft-event','event-source',0,'test','ready',?2,?3,1)",
        rusqlite::params![project.as_str(),json!(claims).to_string(),json!(entities).to_string()]).unwrap();
    assert!(matches!(
        repository.modeling_batch(&project, "event-batch"),
        Err(RepositoryError::CorruptState)
    ));
    let before: String = repository
        .connection
        .query_row(
            "SELECT assertions_json FROM story_modeling_batches_v2 WHERE batch_id='event-batch'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&before).unwrap(),
        json!(claims)
    );
}
