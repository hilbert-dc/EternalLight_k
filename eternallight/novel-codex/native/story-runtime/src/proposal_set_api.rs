use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use novel_codex_story_core::{
    ApplyProposalSet, CreateProposalSet, ProjectId, ProposedReview, SnapshotId, UndoProposalSet,
};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;

use crate::RuntimeState;
use crate::server::{bad_request, internal_error, repository_error, unix_now};

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "ProposalSetApiCreateProposalSetRequest", optional_fields = nullable)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CreateProposalSetRequest {
    base_snapshot_id: String,
    intent: String,
    operations: Vec<ProposedReview>,
    idempotency_key: String,
}

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "ProposalSetApiPreviewProposalSetRequest", optional_fields = nullable)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PreviewProposalSetRequest {
    selected_operation_ids: Vec<String>,
    selections_by_operation_id: BTreeMap<String, Value>,
}

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "ProposalSetApiApplyProposalSetRequest", optional_fields = nullable)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ApplyProposalSetRequest {
    selected_operation_ids: Vec<String>,
    selections_by_operation_id: BTreeMap<String, Value>,
    idempotency_key: String,
}

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "ProposalSetApiUndoProposalSetRequest", optional_fields = nullable)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct UndoProposalSetRequest {
    expected_project_revision: i64,
    idempotency_key: String,
}

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "ProposalSetApiProposalSetPageQuery", optional_fields = nullable)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProposalSetPageQuery {
    cursor: Option<i64>,
}

pub(crate) async fn list(
    State(state): State<RuntimeState>,
    Path(project_id): Path<String>,
    Query(query): Query<ProposalSetPageQuery>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |repository| {
        let project_id = match ProjectId::parse(project_id) {
            Ok(value) => value,
            Err(error) => return bad_request("invalid_project_id", error),
        };

        match repository.proposal_sets_page(&project_id, query.cursor) {
            Ok(value) => Json(value).into_response(),
            Err(error) => repository_error(error),
        }
    })
    .await
}

pub(crate) async fn create(
    State(state): State<RuntimeState>,
    Path(project_id): Path<String>,
    Json(request): Json<CreateProposalSetRequest>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |mut repository| {
        let project_id = match ProjectId::parse(project_id) {
            Ok(value) => value,
            Err(error) => return bad_request("invalid_project_id", error),
        };
        let snapshot_id = match SnapshotId::parse(request.base_snapshot_id) {
            Ok(value) => value,
            Err(error) => return bad_request("invalid_snapshot_id", error),
        };

        match repository.create_proposal_set(CreateProposalSet {
            project_id: &project_id,
            base_snapshot_id: &snapshot_id,
            intent: &request.intent,
            operations: &request.operations,
            idempotency_key: &request.idempotency_key,
            now: unix_now(),
        }) {
            Ok(value) => (StatusCode::CREATED, Json(value)).into_response(),
            Err(error) => repository_error(error),
        }
    })
    .await
}

pub(crate) async fn read(
    State(state): State<RuntimeState>,
    Path((project_id, proposal_set_id)): Path<(String, String)>,
) -> Response {
    crate::repository_executor::run(state.clone(), move || {
        report(
            &state,
            project_id,
            proposal_set_id,
            &[],
            /*choices*/ None,
        )
    })
    .await
}

pub(crate) async fn preview(
    State(state): State<RuntimeState>,
    Path((project_id, proposal_set_id)): Path<(String, String)>,
    Json(request): Json<PreviewProposalSetRequest>,
) -> Response {
    crate::repository_executor::run(state.clone(), move || {
        report(
            &state,
            project_id,
            proposal_set_id,
            &request.selected_operation_ids,
            Some(&request.selections_by_operation_id),
        )
    })
    .await
}

pub(crate) async fn apply(
    State(state): State<RuntimeState>,
    Path((project_id, proposal_set_id)): Path<(String, String)>,
    Json(request): Json<ApplyProposalSetRequest>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |mut repository| {
        let project_id = match ProjectId::parse(project_id) {
            Ok(value) => value,
            Err(error) => return bad_request("invalid_project_id", error),
        };

        match repository.apply_proposal_set(ApplyProposalSet {
            project_id: &project_id,
            proposal_set_id: &proposal_set_id,
            selected_operation_ids: &request.selected_operation_ids,
            selections_by_operation_id: &request.selections_by_operation_id,
            idempotency_key: &request.idempotency_key,
            now: unix_now(),
        }) {
            Ok(value) => Json(value).into_response(),
            Err(error) => repository_error(error),
        }
    })
    .await
}

pub(crate) async fn undo(
    State(state): State<RuntimeState>,
    Path((project_id, application_id)): Path<(String, String)>,
    Json(request): Json<UndoProposalSetRequest>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |mut repository| {
        let project_id = match ProjectId::parse(project_id) {
            Ok(value) => value,
            Err(error) => return bad_request("invalid_project_id", error),
        };

        match repository.undo_proposal_set(UndoProposalSet {
            project_id: &project_id,
            application_id: &application_id,
            expected_project_revision: request.expected_project_revision,
            idempotency_key: &request.idempotency_key,
        }) {
            Ok(value) => Json(value).into_response(),
            Err(error) => repository_error(error),
        }
    })
    .await
}

fn report(
    state: &RuntimeState,
    project_id: String,
    proposal_set_id: String,
    selected: &[String],
    choices: Option<&BTreeMap<String, Value>>,
) -> Response {
    let project_id = match ProjectId::parse(project_id) {
        Ok(value) => value,
        Err(error) => return bad_request("invalid_project_id", error),
    };
    let repository = match state.database.repository() {
        Ok(value) => value,
        Err(error) => return internal_error(error),
    };
    let result = match choices {
        Some(choices) => repository.preview_proposal_application(
            &project_id,
            &proposal_set_id,
            selected,
            choices,
        ),
        None => repository.preview_proposal_set(&project_id, &proposal_set_id, selected),
    };
    match result {
        Ok(value) => Json(value).into_response(),
        Err(error) => repository_error(error),
    }
}

/// Author/Host only; authentication is applied by server composition.
pub(crate) fn routes() -> axum::Router<crate::server::RuntimeState> {
    axum::Router::new()
        .route(
            "/api/projects/{project_id}/proposal-sets",
            axum::routing::get(crate::proposal_set_api::list).post(crate::proposal_set_api::create),
        )
        .route(
            "/api/projects/{project_id}/proposal-sets/{proposal_set_id}",
            axum::routing::get(crate::proposal_set_api::read),
        )
        .route(
            "/api/projects/{project_id}/proposal-sets/{proposal_set_id}/preview",
            axum::routing::post(crate::proposal_set_api::preview),
        )
        .route(
            "/api/projects/{project_id}/proposal-sets/{proposal_set_id}/apply",
            axum::routing::post(crate::proposal_set_api::apply),
        )
        .route(
            "/api/projects/{project_id}/proposal-applications/{application_id}/undo",
            axum::routing::post(crate::proposal_set_api::undo),
        )
}

use novel_codex_story_core::{ProposalApplicationRepository, ProposalSelectionRepository, ProposalSetsRepository};
