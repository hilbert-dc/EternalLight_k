use axum::{
    Json,
    extract::{Path, State},
    response::{IntoResponse, Response},
};
use novel_codex_story_core::{ProjectId, WorkBudgetAuthorization};

use crate::server::{RuntimeState, bad_request, repository_error, unix_now};

pub(crate) async fn pending_usage(
    State(state): State<RuntimeState>,
    Path((project_id, work_id)): Path<(String, String)>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |repo| {
        let project = match ProjectId::parse(project_id) {
            Ok(project) => project,
            Err(error) => return bad_request("invalid_project_id", error),
        };

        match repo.pending_work_provider_usage(&project, &work_id) {
            Ok(pending) => Json(serde_json::json!({"pending":pending})).into_response(),
            Err(error) => repository_error(error),
        }
    })
    .await
}

pub(crate) async fn read_authorization(
    State(state): State<RuntimeState>,
    Path((project_id, work_id)): Path<(String, String)>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |repo| {
        let project = match ProjectId::parse(project_id) {
            Ok(project) => project,
            Err(error) => return bad_request("invalid_project_id", error),
        };

        match repo.work_budget_authorization(&project, &work_id) {
            Ok(authorization) => {
                Json(serde_json::json!({"authorization":authorization})).into_response()
            }
            Err(error) => repository_error(error),
        }
    })
    .await
}

#[derive(serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "WorkBudgetApiUsageRequest", optional_fields = nullable)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct UsageRequest {
    task_id: String,
}

pub(crate) async fn charge_usage(
    State(state): State<RuntimeState>,
    Path((project_id, work_id)): Path<(String, String)>,
    Json(request): Json<UsageRequest>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |mut repo| {
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

        let receipt = match eternallight_engine_core::PersistentBudget::receipt(
            budget_path,
            &request.task_id,
        ) {
            Ok(receipt) => receipt,
            // Exact absence is distinct from a corrupt, active or unreadable
            // ledger. Terminal Native evidence additionally fences any turn/write.
            Err(error) if error == "persistent budget run not found" => {
                match repo.unstarted_work_usage_receipt(&project, &work_id, &request.task_id) {
                    Ok(receipt) => receipt,
                    Err(error) => return repository_error(error),
                }
            }
            Err(_)
                if std::fs::metadata(budget_path)
                    .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound) =>
            {
                match repo.undispatched_message_usage_receipt(&project, &work_id, &request.task_id)
                {
                    Ok(receipt) => receipt,
                    Err(error) => return repository_error(error),
                }
            }
            Err(error) => return bad_request("engine_budget_receipt_unavailable", error),
        };
        match repo.charge_work_provider_usage(&project, &work_id, &request.task_id, &receipt) {
            Ok(budget) => Json(budget).into_response(),
            Err(error) => repository_error(error),
        }
    })
    .await
}
pub(crate) async fn authorize(
    State(state): State<RuntimeState>,
    Path((project_id, work_id)): Path<(String, String)>,
    Json(request): Json<WorkBudgetAuthorization>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |mut repo| {
        let project = match ProjectId::parse(project_id) {
            Ok(project) => project,
            Err(error) => return bad_request("invalid_project_id", error),
        };

        match repo.authorize_work_budget(&project, &work_id, &request, unix_now()) {
            Ok(budget) => Json(budget).into_response(),
            Err(error) => repository_error(error),
        }
    })
    .await
}

/// Author/Host only; authentication is applied by server composition.
pub(crate) fn routes() -> axum::Router<crate::server::RuntimeState> {
    axum::Router::new()
        .route(
            "/api/projects/{project_id}/work-tasks/{work_id}/budget-authorization",
            axum::routing::get(crate::work_budget_api::read_authorization)
                .post(crate::work_budget_api::authorize),
        )
        .route(
            "/api/projects/{project_id}/work-tasks/{work_id}/provider-usage",
            axum::routing::get(crate::work_budget_api::pending_usage)
                .post(crate::work_budget_api::charge_usage),
        )
}

use novel_codex_story_core::{OrdinaryWorkBudgetRepository, WorkBudgetAuthorizationRepository};
