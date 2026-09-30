use std::path::Path;

use axum::http::StatusCode;
use novel_codex_story_core::{
    ConditionEvaluationStatus, ConditionEvaluationTarget, ProjectId, SqliteStoryRepository,
};
use pretty_assertions::assert_eq;
use serde_json::{Value, json};

use super::{Role, call};

pub(super) async fn verify(path: &Path, project: &str, base: &str, batch: &Value, draft: &Value) {
    let project = ProjectId::parse(project).unwrap();
    let mut repository = SqliteStoryRepository::open(path).unwrap();
    let target = ConditionEvaluationTarget {
        snapshot_id: None,
        source_batch_id: batch["id"].as_str().unwrap().into(),
        assertion_index: 0,
    };
    assert!(
        repository
            .start_condition_evaluation(&project, target.clone())
            .is_err()
    );
    repository.rebuild_entity_index(&project).unwrap();
    repository.rebuild_assertion_index(&project).unwrap();
    let audit = repository.start_knowledge_audit(&project).unwrap();
    loop {
        let state = repository
            .advance_knowledge_audit(&project, &audit.id)
            .unwrap();
        if state.status != "running" {
            assert_eq!(state.verified_consistent, Some(true));
            break;
        }
    }
    let mut job = repository
        .start_condition_evaluation(&project, target)
        .unwrap();
    while job.status != ConditionEvaluationStatus::Completed {
        job = repository
            .advance_condition_evaluation(&project, &job.id, job.version)
            .unwrap();
    }
    let expected = job.result.unwrap();
    let (status, state) = call(
        path,
        "GET",
        &format!("{base}/knowledge/state?perspective=world&predicate=transfer"),
        Value::Null,
        Role::Author,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{state}");
    assert_eq!(state["entries"][0]["eventConditionAssessment"], expected);
    let (status, trace) = call(
        path,
        "GET",
        &format!("{base}/knowledge/evolution?dimension=world&projectRevision=1&limit=100"),
        Value::Null,
        Role::Author,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{trace}");
    let event = trace["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["change"] == "event")
        .unwrap();
    assert_eq!(event["eventConditionAssessment"], expected);
    let (status,review)=call(path,"POST",&format!("{base}/reviews"),json!({"artifactKind":"draft","revisionId":draft["revisionId"],"modelingBatchId":batch["id"],"proposals":[],"selections":{},"idempotencyKey":"condition-cached-review"}),Role::Author).await;
    assert_eq!(status, StatusCode::CREATED, "{review}");
    assert_eq!(
        review["factProposals"][0]["storyEvent"]["evaluationTarget"],
        json!({"snapshotId":null,"sourceBatchId":batch["id"],"assertionIndex":0})
    );
    assert_eq!(
        review["factProposals"][0]["storyEvent"]["conditionAssessment"],
        expected
    );
    let (status, limited) = call(
        path,
        "GET",
        &format!("{base}/knowledge/state?perspective=reader&readingChapter=3&predicate=transfer"),
        Value::Null,
        Role::Author,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(limited["entries"][0]["eventConditionAssessment"].is_null());
}
