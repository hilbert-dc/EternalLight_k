use std::collections::BTreeMap;

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use novel_codex_story_core::{ProjectId, SqliteStoryRepository};
use novel_codex_story_runtime::{RuntimeState, router};
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use tower::ServiceExt;

#[path = "../../story-core/examples/support/long_work_fixture.rs"]
mod fixture;

const AUTHOR: &str = "proposal-selection-author-token-1234567890";
const CANDIDATE: &str = "proposal-selection-candidate-token-123456";

async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    payload: Value,
    token: &str,
) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    (
        status,
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap(),
    )
}

async fn setup(
    app: &Router,
    repo: &SqliteStoryRepository,
    project: &ProjectId,
) -> (String, Value, Value) {
    let base = format!("/api/projects/{project}");
    let (status, brief) = call(app, "POST", &format!("{base}/artifacts/brief"), json!({"payload":{"premise":"港城中的见证"},"expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":"selection-brief"}), AUTHOR).await;
    assert_eq!(status, StatusCode::CREATED, "{brief}");
    let (status, identity) = call(app, "POST", &format!("{base}/artifacts/identity"), json!({"payload":{"decisions":[{"kind":"distinct","leftId":"person-0","rightId":"person-1"}]},"expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":"selection-identity"}), AUTHOR).await;
    assert_eq!(status, StatusCode::CREATED, "{identity}");
    let batch = repo.chapter_knowledge(project, "chapter-1").unwrap();
    let mut reviews = Vec::new();
    let mut draft_review = Value::Null;
    for (operation, kind, revision, modeling_batch) in [
        ("brief", "brief", brief["revisionId"].clone(), Value::Null),
        (
            "draft",
            "draft",
            json!("revision-1"),
            batch["batchId"].clone(),
        ),
        (
            "identity",
            "identity",
            identity["revisionId"].clone(),
            Value::Null,
        ),
    ] {
        let (status, review) = call(app, "POST", &format!("{base}/reviews"), json!({"artifactKind":kind,"revisionId":revision,"modelingBatchId":modeling_batch,"proposals":[],"selections":{},"idempotencyKey":format!("selection-review-{operation}")}), AUTHOR).await;
        assert_eq!(status, StatusCode::CREATED, "{review}");
        if operation == "draft" {
            draft_review = review.clone();
        }
        reviews.push(json!({"operationId":operation,"reviewId":review["id"],"dependsOn":if operation == "draft" {vec!["brief"]} else {vec![]}}));
    }
    let (_, workspace) = call(
        app,
        "GET",
        &format!("{base}/creative-workspace"),
        Value::Null,
        AUTHOR,
    )
    .await;
    let (status, saved) = call(app, "POST", &format!("{base}/proposal-sets"), json!({"baseSnapshotId":workspace["headSnapshotId"],"intent":"同时调整创作方向、正文与身份","operations":reviews,"idempotencyKey":"mixed-proposal"}), AUTHOR).await;
    assert_eq!(status, StatusCode::CREATED, "{saved}");
    (
        saved["id"].as_str().unwrap().to_owned(),
        draft_review,
        workspace,
    )
}

#[tokio::test]
async fn mixed_proposal_preflight_and_atomic_apply_share_explicit_fact_choices() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("selection.sqlite3");
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    let (project, _) = fixture::build(&mut repo, 2);
    let app = router(RuntimeState::new(path, AUTHOR.into(), CANDIDATE.into()).unwrap());
    let (id, draft, workspace) = setup(&app, &repo, &project).await;
    let base = format!("/api/projects/{project}");
    let uri = format!("{base}/proposal-sets/{id}");
    let first = draft["factProposals"][0]["id"].as_str().unwrap();
    let second = draft["factProposals"][1]["id"].as_str().unwrap();
    let choices = json!({"brief":{},"identity":{},"draft":{"sceneGroupIds":["scene-1-a"],"factDecisions":{first:"confirmed",second:"rejected"}}});
    let request = json!({"selectedOperationIds":["brief","draft","identity"],"selectionsByOperationId":choices});
    let (status, report) = call(
        &app,
        "POST",
        &format!("{uri}/preview"),
        request.clone(),
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{report}");
    assert_eq!(report["selection"]["applyReady"], true);
    assert_eq!(report["selection"]["selectionsByOperationId"], choices);
    assert_eq!(
        report["reviewOptions"]["draft"]["factProposals"],
        draft["factProposals"]
    );
    assert_eq!(
        report["reviewOptions"]["draft"]["sceneGroupIds"],
        json!(["scene-1-a", "scene-1-b"])
    );
    assert_eq!(repo.project_revision(&project).unwrap(), 0);
    assert!(repo.facts(&project).unwrap().is_empty());
    assert!(
        repo.object_heads(&project)
            .unwrap()
            .iter()
            .all(|head| head.accepted_revision_id.is_none())
    );
    for bad_choices in [
        json!({"sceneGroupIds":[],"factDecisions":{first:"confirmed"}}),
        json!({"sceneGroupIds":["foreign-scene"],"factDecisions":{}}),
        json!({"sceneGroupIds":["scene-1-a"],"factDecisions":{"unknown-fact":"confirmed"}}),
        json!({"sceneGroupIds":["scene-1-a"],"factDecisions":{first:"trust-model"}}),
        json!({"sceneGroupIds":["scene-1-a"],"grantAuthor":true}),
    ] {
        let mut bad = request.clone();
        bad["selectionsByOperationId"]["draft"] = bad_choices;
        assert_ne!(
            call(&app, "POST", &format!("{uri}/preview"), bad.clone(), AUTHOR)
                .await
                .0,
            StatusCode::OK
        );
        bad["idempotencyKey"] = json!("invalid-selection");
        assert_ne!(
            call(&app, "POST", &format!("{uri}/apply"), bad, AUTHOR)
                .await
                .0,
            StatusCode::OK
        );
        assert_eq!(repo.project_revision(&project).unwrap(), 0);
        assert!(repo.facts(&project).unwrap().is_empty());
    }
    assert_eq!(
        call(
            &app,
            "POST",
            &format!("{uri}/preview"),
            request.clone(),
            CANDIDATE
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    let mut apply = request;
    apply["idempotencyKey"] = json!("selection-apply");
    let (status, receipt) =
        call(&app, "POST", &format!("{uri}/apply"), apply.clone(), AUTHOR).await;
    assert_eq!(status, StatusCode::OK, "{receipt}");
    assert_eq!(receipt["resultRevision"], 1);
    let adopted = repo
        .object_heads(&project)
        .unwrap()
        .into_iter()
        .find(|head| head.object_id.as_str() == "draft-chapter-1")
        .unwrap()
        .accepted_revision_id
        .unwrap();
    let adopted: Value =
        serde_json::from_str(&repo.revision_content(&adopted).unwrap().unwrap()).unwrap();
    assert_eq!(
        adopted["sceneGroups"],
        json!([workspace["chapters"][0]["candidate"]["sceneGroups"][0]])
    );
    assert_eq!(
        adopted["body"],
        workspace["chapters"][0]["candidate"]["sceneGroups"][0]["body"]
    );
    let facts = repo.facts(&project).unwrap();
    assert_eq!(facts.len(), 1);
    assert_eq!(facts[0]["id"], first);
    assert_eq!(
        repo.modeling_entity_catalog(&project, /*byte_budget*/ 60000)
            .unwrap()["identityDecisions"],
        json!([{"kind":"distinct","leftId":"person-0","rightId":"person-1"}])
    );
    assert_eq!(
        call(&app, "POST", &format!("{uri}/apply"), apply, AUTHOR)
            .await
            .1["applicationId"],
        receipt["applicationId"]
    );
    let (status, undo) = call(
        &app,
        "POST",
        &format!(
            "{base}/proposal-applications/{}/undo",
            receipt["applicationId"].as_str().unwrap()
        ),
        json!({"expectedProjectRevision":1,"idempotencyKey":"selection-undo"}),
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{undo}");
    assert_eq!(undo["resultRevision"], 2);
    assert!(repo.facts(&project).unwrap().is_empty());
    assert_eq!(
        repo.modeling_entity_catalog(&project, /*byte_budget*/ 60000)
            .unwrap()["identityDecisions"],
        json!([])
    );
    let (_, after) = call(
        &app,
        "GET",
        &format!("{base}/creative-workspace"),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert_eq!(
        after["chapters"][0]["candidate"]["body"],
        workspace["chapters"][0]["candidate"]["body"]
    );
}

#[tokio::test]
async fn partial_application_receipt_preserves_the_exact_selected_members() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("selection-partial.sqlite3");
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    let (project, _) = fixture::build(&mut repo, 2);
    let app = router(RuntimeState::new(path, AUTHOR.into(), CANDIDATE.into()).unwrap());
    let (id, _, workspace) = setup(&app, &repo, &project).await;
    let base = format!("/api/projects/{project}");
    let uri = format!("{base}/proposal-sets/{id}");
    let selected = json!(["brief", "draft"]);
    let (status, receipt) = call(&app, "POST", &format!("{uri}/apply"), json!({"selectedOperationIds":selected,"selectionsByOperationId":{"brief":{},"draft":{}},"idempotencyKey":"partial-selection"}), AUTHOR).await;
    assert_eq!(status, StatusCode::OK, "{receipt}");
    let (_, report) = call(&app, "GET", &uri, Value::Null, AUTHOR).await;
    assert_eq!(report["application"]["selectedOperationIds"], selected);
    let (_, after) = call(
        &app,
        "GET",
        &format!("{base}/creative-workspace"),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert_eq!(
        after["artifacts"]["identity"],
        workspace["artifacts"]["identity"]
    );
    let (status, undo) = call(
        &app,
        "POST",
        &format!(
            "{base}/proposal-applications/{}/undo",
            receipt["applicationId"].as_str().unwrap()
        ),
        json!({"expectedProjectRevision":1,"idempotencyKey":"partial-undo"}),
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{undo}");
    let (_, undone) = call(&app, "GET", &uri, Value::Null, AUTHOR).await;
    assert_eq!(undone["application"]["selectedOperationIds"], selected);
    let (_, restored) = call(
        &app,
        "GET",
        &format!("{base}/creative-workspace"),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert_eq!(
        restored["artifacts"]["identity"],
        workspace["artifacts"]["identity"]
    );
}

#[tokio::test]
async fn preflight_never_authorizes_apply_after_a_source_changes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("selection-stale.sqlite3");
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    let (project, _) = fixture::build(&mut repo, 2);
    let app = router(RuntimeState::new(path, AUTHOR.into(), CANDIDATE.into()).unwrap());
    let (id, _, _) = setup(&app, &repo, &project).await;
    let selected = vec![
        "brief".to_owned(),
        "draft".to_owned(),
        "identity".to_owned(),
    ];
    let choices = selected
        .iter()
        .map(|id| (id.clone(), json!({})))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(
        repo.preview_proposal_application(&project, &id, &selected, &choices)
            .unwrap()["selection"]["applyReady"],
        true
    );
    let (status, saved) = call(&app, "POST", &format!("/api/projects/{project}/drafts/checkpoints"), json!({"payload":{"chapterId":"chapter-1","chapterNumber":1,"title":"改稿","body":"改为另一条证词。"},"expectedRevision":0,"expectedCandidateRevisionId":"revision-1","idempotencyKey":"change-after-preview"}), AUTHOR).await;
    assert_eq!(status, StatusCode::CREATED, "{saved}");
    let current = repo
        .branch_head(
            &project,
            &novel_codex_story_core::BranchId::parse("main").unwrap(),
        )
        .unwrap();
    let (status, _) = call(&app, "POST", &format!("/api/projects/{project}/proposal-sets/{id}/apply"), json!({"selectedOperationIds":selected,"selectionsByOperationId":choices,"idempotencyKey":"stale-apply"}), AUTHOR).await;
    assert_ne!(status, StatusCode::OK);
    assert_eq!(
        repo.branch_head(
            &project,
            &novel_codex_story_core::BranchId::parse("main").unwrap()
        )
        .unwrap(),
        current
    );
    assert!(
        repo.object_heads(&project)
            .unwrap()
            .iter()
            .all(|head| head.accepted_revision_id.is_none())
    );
    assert!(repo.facts(&project).unwrap().is_empty());
}

use novel_codex_story_core::{EntityIdentityRepository, ModelingRepository, ProjectRevisionRepository, ProposalSelectionRepository, WorkflowRepository};
