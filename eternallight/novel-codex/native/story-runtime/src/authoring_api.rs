use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use eternallight_engine_core::PersistentBudget;
use novel_codex_story_authoring::{
    AuthoringService, RebindRun, ReplanRun, ReplanStage, ResolveFeedback, StartRun,
};
use novel_codex_story_core::{ProjectId, RevisionPredecessor, SqliteStoryRepository};
use serde::Deserialize;
use serde_json::json;

use crate::server::{RuntimeState, bad_request, repository_error, unix_now};

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "AuthoringApiStartRequest", optional_fields = nullable)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StartRequest {
    expected_project_revision: i64,
    expected_base_snapshot_id: String,
    expected_source_digest: String,
    author_intent: String,
    revision_predecessor: Option<RevisionPredecessor>,
    generation_proposal_id: Option<String>,
    target_chapters: i64,
    #[serde(deserialize_with = "novel_codex_story_core::deserialize_execution_limit")]
    #[ts(optional = false)]
    max_task_attempts: Option<i64>,
    #[serde(deserialize_with = "novel_codex_story_core::deserialize_execution_limit")]
    #[ts(optional = false)]
    max_provider_requests: Option<u64>,
    #[serde(deserialize_with = "novel_codex_story_core::deserialize_execution_limit")]
    #[ts(optional = false)]
    max_provider_tokens: Option<u64>,
    #[serde(deserialize_with = "novel_codex_story_core::deserialize_execution_limit")]
    #[ts(optional = false)]
    deadline_at: Option<i64>,
}

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "AuthoringApiChargeProviderUsageRequest", optional_fields = nullable)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ChargeProviderUsageRequest {
    task_id: String,
}

pub(crate) async fn charge_provider_usage(
    State(state): State<RuntimeState>,
    Path((project_id, run_id)): Path<(String, String)>,
    Json(request): Json<ChargeProviderUsageRequest>,
) -> Response {
    crate::repository_executor::run(state.clone(), move || {
        let Some(budget_path) = state.engine_budget_database.as_deref() else {
            return bad_request(
                "engine_budget_unavailable",
                "engine budget database is not configured",
            );
        };
        let (mut repository, project_id) = match open(&state, project_id) {
            Ok(value) => value,
            Err(response) => return *response,
        };
        let receipt = match PersistentBudget::receipt(budget_path, &request.task_id) {
            Ok(receipt) => receipt,
            Err(error) if error == "persistent budget run not found" => {
                match repository.unstarted_work_usage_receipt(
                    &project_id,
                    &format!("work-{run_id}"),
                    &request.task_id,
                ) {
                    Ok(receipt) => receipt,
                    Err(error) => return repository_error(error),
                }
            }
            Err(error) => return bad_request("engine_budget_receipt_unavailable", error),
        };
        match AuthoringService::new(&mut repository).charge_provider_usage(
            &project_id,
            &run_id,
            &request.task_id,
            &receipt,
        ) {
            Ok(run) => Json(json!(run)).into_response(),
            Err(source) => repository_error(source),
        }
    })
    .await
}

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "AuthoringApiAdvanceTaskRequest", optional_fields = nullable)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AdvanceTaskRequest {
    expected_version: i64,
    task_id: String,
}

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "AuthoringApiStepRequest", optional_fields = nullable)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StepRequest {
    expected_version: i64,
}

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "AuthoringApiReplanRequest", optional_fields = nullable)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ReplanRequest {
    expected_version: i64,
    expected_project_revision: i64,
    expected_source_digest: String,
    restart_from: ReplanStage,
    instruction: String,
}

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "AuthoringApiRebindRequest", optional_fields = nullable)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RebindRequest {
    expected_version: i64,
    expected_project_revision: i64,
    expected_source_digest: String,
    restart_from: ReplanStage,
}

pub(crate) async fn rebind(
    State(state): State<RuntimeState>,
    Path((project_id, run_id)): Path<(String, String)>,
    Json(request): Json<RebindRequest>,
) -> Response {
    crate::repository_executor::run(state.clone(), move || {
        let (mut repository, project_id) = match open(&state, project_id) {
            Ok(value) => value,
            Err(response) => return *response,
        };
        match AuthoringService::new(&mut repository).rebind(RebindRun {
            project_id: &project_id,
            run_id: &run_id,
            expected_version: request.expected_version,
            expected_project_revision: request.expected_project_revision,
            expected_source_digest: &request.expected_source_digest,
            restart_from: request.restart_from,
            now: unix_now(),
        }) {
            Ok(run) => Json(json!(run)).into_response(),
            Err(source) => repository_error(source),
        }
    })
    .await
}

pub(crate) async fn preview_replan(
    State(state): State<RuntimeState>,
    Path((project_id, run_id)): Path<(String, String)>,
) -> Response {
    crate::repository_executor::run(state.clone(), move || {
        let (mut repository, project_id) = match open(&state, project_id) {
            Ok(value) => value,
            Err(response) => return *response,
        };
        match AuthoringService::new(&mut repository).preview_replan(&project_id, &run_id) {
            Ok(preview) => Json(json!(preview)).into_response(),
            Err(source) => repository_error(source),
        }
    })
    .await
}

pub(crate) async fn replan(
    State(state): State<RuntimeState>,
    Path((project_id, run_id)): Path<(String, String)>,
    Json(request): Json<ReplanRequest>,
) -> Response {
    crate::repository_executor::run(state.clone(), move || {
        let (mut repository, project_id) = match open(&state, project_id) {
            Ok(value) => value,
            Err(response) => return *response,
        };
        match AuthoringService::new(&mut repository).replan(ReplanRun {
            project_id: &project_id,
            run_id: &run_id,
            expected_version: request.expected_version,
            expected_project_revision: request.expected_project_revision,
            expected_source_digest: &request.expected_source_digest,
            restart_from: request.restart_from,
            instruction: &request.instruction,
            now: unix_now(),
        }) {
            Ok(run) => Json(json!(run)).into_response(),
            Err(source) => repository_error(source),
        }
    })
    .await
}

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "AuthoringApiFutureIntentRequest", optional_fields = nullable)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct FutureIntentRequest {
    expected_version: i64,
    instruction: String,
}

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "AuthoringApiControlRunRequest", optional_fields = nullable)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ControlRunRequest {
    expected_version: i64,
    command: String,
}

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "AuthoringApiResolveFeedbackRequest", optional_fields = nullable)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ResolveFeedbackRequest {
    expected_version: i64,
    assessment_revision_id: String,
    choice: String,
    instruction: String,
}

pub(crate) async fn resolve_feedback(
    State(state): State<RuntimeState>,
    Path((project_id, run_id)): Path<(String, String)>,
    Json(request): Json<ResolveFeedbackRequest>,
) -> Response {
    crate::repository_executor::run(state.clone(), move || {
        let (mut repository, project_id) = match open(&state, project_id) {
            Ok(value) => value,
            Err(response) => return *response,
        };
        match AuthoringService::new(&mut repository).resolve_feedback(ResolveFeedback {
            project_id: &project_id,
            run_id: &run_id,
            expected_version: request.expected_version,
            assessment_revision_id: &request.assessment_revision_id,
            choice: &request.choice,
            instruction: &request.instruction,
            now: unix_now(),
        }) {
            Ok(run) => Json(json!(run)).into_response(),
            Err(source) => repository_error(source),
        }
    })
    .await
}

pub(crate) async fn control(
    State(state): State<RuntimeState>,
    Path((project_id, run_id)): Path<(String, String)>,
    Json(request): Json<ControlRunRequest>,
) -> Response {
    crate::repository_executor::run(state.clone(), move || {
        if !matches!(request.command.as_str(), "pause" | "resume" | "cancel") {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "invalid_command"})),
            )
                .into_response();
        }
        let (mut repository, project_id) = match open(&state, project_id) {
            Ok(value) => value,
            Err(response) => return *response,
        };
        match AuthoringService::new(&mut repository).control(
            &project_id,
            &run_id,
            request.expected_version,
            &request.command,
            unix_now(),
        ) {
            Ok(run) => Json(json!(run)).into_response(),
            Err(source) => repository_error(source),
        }
    })
    .await
}

pub(crate) async fn start(
    State(state): State<RuntimeState>,
    Path(project_id): Path<String>,
    Json(request): Json<StartRequest>,
) -> Response {
    crate::repository_executor::run(state.clone(), move || {
        if state.engine_budget_database.is_none() {
            return bad_request(
                "engine_budget_unavailable",
                "engine budget database is not configured",
            );
        }
        let (mut repository, project_id) = match open(&state, project_id) {
            Ok(value) => value,
            Err(response) => return *response,
        };
        match AuthoringService::new(&mut repository).start(StartRun {
            project_id: &project_id,
            expected_project_revision: request.expected_project_revision,
            expected_base_snapshot_id: &request.expected_base_snapshot_id,
            expected_source_digest: &request.expected_source_digest,
            author_intent: &request.author_intent,
            revision_predecessor: request.revision_predecessor,
            generation_proposal_id: request.generation_proposal_id.as_deref(),
            target_chapters: request.target_chapters,
            max_task_attempts: request.max_task_attempts,
            max_provider_requests: request.max_provider_requests,
            max_provider_tokens: request.max_provider_tokens,
            deadline_at: request.deadline_at,
            now: unix_now(),
        }) {
            Ok(run) => (StatusCode::CREATED, Json(json!(run))).into_response(),
            Err(source) => repository_error(source),
        }
    })
    .await
}

pub(crate) async fn start_position(
    State(state): State<RuntimeState>,
    Path(project_id): Path<String>,
) -> Response {
    crate::repository_executor::run(state.clone(), move || {
        let (repository, project_id) = match open(&state, project_id) {
            Ok(value) => value,
            Err(response) => return *response,
        };
        match repository.generation_start_position(&project_id) {
            Ok(position) => Json(json!(position)).into_response(),
            Err(source) => repository_error(source),
        }
    })
    .await
}

pub(crate) async fn get(
    State(state): State<RuntimeState>,
    Path((project_id, run_id)): Path<(String, String)>,
) -> Response {
    crate::repository_executor::run(state.clone(), move || {
        let (mut repository, project_id) = match open(&state, project_id) {
            Ok(value) => value,
            Err(response) => return *response,
        };
        match AuthoringService::new(&mut repository).get(&project_id, &run_id) {
            Ok(run) => Json(json!(run)).into_response(),
            Err(source) => repository_error(source),
        }
    })
    .await
}

pub(crate) async fn list(
    State(state): State<RuntimeState>,
    Path(project_id): Path<String>,
) -> Response {
    crate::repository_executor::run(state.clone(), move || {
        let (mut repository, project_id) = match open(&state, project_id) {
            Ok(value) => value,
            Err(response) => return *response,
        };
        match AuthoringService::new(&mut repository).list(&project_id) {
            Ok(runs) => Json(json!({"runs": runs})).into_response(),
            Err(source) => repository_error(source),
        }
    })
    .await
}

pub(crate) async fn add_future_intent(
    State(state): State<RuntimeState>,
    Path((project_id, run_id)): Path<(String, String)>,
    Json(request): Json<FutureIntentRequest>,
) -> Response {
    crate::repository_executor::run(state.clone(), move || {
        let (mut repository, project_id) = match open(&state, project_id) {
            Ok(value) => value,
            Err(response) => return *response,
        };
        match AuthoringService::new(&mut repository).add_future_intent(
            &project_id,
            &run_id,
            request.expected_version,
            &request.instruction,
            unix_now(),
        ) {
            Ok(run) => Json(json!(run)).into_response(),
            Err(source) => repository_error(source),
        }
    })
    .await
}

pub(crate) async fn revise_current_chapter(
    State(state): State<RuntimeState>,
    Path((project_id, run_id)): Path<(String, String)>,
    Json(request): Json<FutureIntentRequest>,
) -> Response {
    crate::repository_executor::run(state.clone(), move || {
        let (mut repository, project_id) = match open(&state, project_id) {
            Ok(value) => value,
            Err(response) => return *response,
        };
        match AuthoringService::new(&mut repository).revise_current_chapter(
            &project_id,
            &run_id,
            request.expected_version,
            &request.instruction,
            unix_now(),
        ) {
            Ok(run) => Json(json!(run)).into_response(),
            Err(source) => repository_error(source),
        }
    })
    .await
}

pub(crate) async fn next(
    State(state): State<RuntimeState>,
    Path((project_id, run_id)): Path<(String, String)>,
) -> Response {
    crate::repository_executor::run(state.clone(), move || {
        let (mut repository, project_id) = match open(&state, project_id) {
            Ok(value) => value,
            Err(response) => return *response,
        };
        match AuthoringService::new(&mut repository).next_action(&project_id, &run_id, unix_now()) {
            Ok(action) => Json(action).into_response(),
            Err(source) => repository_error(source),
        }
    })
    .await
}

pub(crate) async fn advance_task(
    State(state): State<RuntimeState>,
    Path((project_id, run_id)): Path<(String, String)>,
    Json(request): Json<AdvanceTaskRequest>,
) -> Response {
    crate::repository_executor::run(state.clone(), move || {
        let (mut repository, project_id) = match open(&state, project_id) {
            Ok(value) => value,
            Err(response) => return *response,
        };
        match AuthoringService::new(&mut repository).advance_task(
            &project_id,
            &run_id,
            request.expected_version,
            &request.task_id,
            unix_now(),
        ) {
            Ok(run) => Json(json!(run)).into_response(),
            Err(source) => repository_error(source),
        }
    })
    .await
}

pub(crate) async fn retry_failed_task(
    State(state): State<RuntimeState>,
    Path((project_id, run_id)): Path<(String, String)>,
    Json(request): Json<StepRequest>,
) -> Response {
    crate::repository_executor::run(state.clone(), move || {
        let (mut repository, project_id) = match open(&state, project_id) {
            Ok(value) => value,
            Err(response) => return *response,
        };
        match AuthoringService::new(&mut repository).retry_failed_task(
            &project_id,
            &run_id,
            request.expected_version,
            unix_now(),
        ) {
            Ok(run) => Json(json!(run)).into_response(),
            Err(source) => repository_error(source),
        }
    })
    .await
}

fn open(
    state: &RuntimeState,
    project_id: String,
) -> Result<(SqliteStoryRepository, ProjectId), Box<Response>> {
    let project_id = ProjectId::parse(project_id)
        .map_err(|source| Box::new(bad_request("invalid_project_id", source)))?;
    let repository = state
        .database
        .repository()
        .map_err(|source| Box::new(repository_error(source)))?;
    Ok((repository, project_id))
}

/// Author/Host only; authentication is applied by server composition.
pub(crate) fn routes() -> axum::Router<crate::server::RuntimeState> {
    axum::Router::new()
        .route(
            "/api/projects/{project_id}/generation-runs",
            axum::routing::get(crate::authoring_api::list).post(crate::authoring_api::start),
        )
        .route(
            "/api/projects/{project_id}/generation-start-position",
            axum::routing::get(crate::authoring_api::start_position),
        )
        .route(
            "/api/projects/{project_id}/generation-runs/{run_id}",
            axum::routing::get(crate::authoring_api::get),
        )
        .route(
            "/api/projects/{project_id}/generation-runs/{run_id}/control",
            axum::routing::post(crate::authoring_api::control),
        )
        .route(
            "/api/projects/{project_id}/generation-runs/{run_id}/replan",
            axum::routing::get(crate::authoring_api::preview_replan)
                .post(crate::authoring_api::replan),
        )
        .route(
            "/api/projects/{project_id}/generation-runs/{run_id}/rebind",
            axum::routing::post(crate::authoring_api::rebind),
        )
        .route(
            "/api/projects/{project_id}/generation-runs/{run_id}/resolve-feedback",
            axum::routing::post(crate::authoring_api::resolve_feedback),
        )
        .route(
            "/api/projects/{project_id}/generation-runs/{run_id}/next",
            axum::routing::post(crate::authoring_api::next),
        )
        .route(
            "/api/projects/{project_id}/generation-runs/{run_id}/future-intents",
            axum::routing::post(crate::authoring_api::add_future_intent),
        )
        .route(
            "/api/projects/{project_id}/generation-runs/{run_id}/current-chapter-revision",
            axum::routing::post(crate::authoring_api::revise_current_chapter),
        )
        .route(
            "/api/projects/{project_id}/generation-runs/{run_id}/advance-task",
            axum::routing::post(crate::authoring_api::advance_task),
        )
        .route(
            "/api/projects/{project_id}/generation-runs/{run_id}/provider-usage",
            axum::routing::post(crate::authoring_api::charge_provider_usage),
        )
        .route(
            "/api/projects/{project_id}/generation-runs/{run_id}/retry-failed-task",
            axum::routing::post(crate::authoring_api::retry_failed_task),
        )
}

use novel_codex_story_core::{AuthoringRunsRepository, OrdinaryWorkBudgetRepository};
