use axum::{
    Json,
    extract::{Path, State},
    response::{IntoResponse, Response},
};
use novel_codex_story_core::{ProjectId, SqliteStoryRepository, WorkBudgetAuthorization};

use crate::server::{RuntimeState, bad_request, repository_error, unix_now};

pub(crate) async fn pending_usage(
    State(state): State<RuntimeState>,
    Path((project_id, work_id)): Path<(String, String)>,
) -> Response {
    let project = match ProjectId::parse(project_id) {
        Ok(project) => project,
        Err(error) => return bad_request("invalid_project_id", error),
    };
    let repo = match SqliteStoryRepository::open(&state.database_path) {
        Ok(repo) => repo,
        Err(error) => return repository_error(error),
    };
    match repo.pending_work_provider_usage(&project, &work_id) {
        Ok(pending) => Json(serde_json::json!({"pending":pending})).into_response(),
        Err(error) => repository_error(error),
    }
}

pub(crate) async fn read_authorization(
    State(state): State<RuntimeState>,
    Path((project_id, work_id)): Path<(String, String)>,
) -> Response {
    let project = match ProjectId::parse(project_id) {
        Ok(project) => project,
        Err(error) => return bad_request("invalid_project_id", error),
    };
    let repo = match SqliteStoryRepository::open(&state.database_path) {
        Ok(repo) => repo,
        Err(error) => return repository_error(error),
    };
    match repo.work_budget_authorization(&project, &work_id) {
        Ok(authorization) => {
            Json(serde_json::json!({"authorization":authorization})).into_response()
        }
        Err(error) => repository_error(error),
    }
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct UsageRequest {
    task_id: String,
}

pub(crate) async fn charge_usage(
    State(state): State<RuntimeState>,
    Path((project_id, work_id)): Path<(String, String)>,
    Json(request): Json<UsageRequest>,
) -> Response {
    let project = match ProjectId::parse(project_id) {
        Ok(project) => project,
        Err(error) => return bad_request("invalid_project_id", error),
    };
    let Some(budget_path) = state.engine_budget_database.as_deref() else {
        return bad_request(
            "engine_budget_unavailable",
            "engine budget database is not configured",
        );
    };
    let receipt =
        match eternallight_engine_core::PersistentBudget::receipt(budget_path, &request.task_id) {
            Ok(receipt) => receipt,
            Err(error) => return bad_request("engine_budget_receipt_unavailable", error),
        };
    let mut repo = match SqliteStoryRepository::open(&state.database_path) {
        Ok(repo) => repo,
        Err(error) => return repository_error(error),
    };
    match repo.charge_work_provider_usage(&project, &work_id, &request.task_id, &receipt) {
        Ok(budget) => Json(budget).into_response(),
        Err(error) => repository_error(error),
    }
}
pub(crate) async fn authorize(
    State(state): State<RuntimeState>,
    Path((project_id, work_id)): Path<(String, String)>,
    Json(request): Json<WorkBudgetAuthorization>,
) -> Response {
    let project = match ProjectId::parse(project_id) {
        Ok(project) => project,
        Err(error) => return bad_request("invalid_project_id", error),
    };
    let mut repo = match SqliteStoryRepository::open(&state.database_path) {
        Ok(repo) => repo,
        Err(error) => return repository_error(error),
    };
    match repo.authorize_work_budget(&project, &work_id, &request, unix_now()) {
        Ok(budget) => Json(budget).into_response(),
        Err(error) => repository_error(error),
    }
}
