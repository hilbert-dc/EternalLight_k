use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use eternallight_engine_core::PersistentBudget;
use novel_codex_story_authoring::{
    AuthoringService, ReplanRun, ReplanStage, ResolveFeedback, StartRun,
};
use novel_codex_story_core::{ProjectId, RevisionPredecessor, SqliteStoryRepository};
use serde::Deserialize;
use serde_json::json;

use crate::server::{RuntimeState, bad_request, repository_error, unix_now};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StartRequest {
    expected_project_revision: i64,
    expected_base_snapshot_id: String,
    expected_source_digest: String,
    author_intent: String,
    revision_predecessor: Option<RevisionPredecessor>,
    target_chapters: i64,
    max_task_attempts: i64,
    max_provider_requests: u64,
    max_provider_tokens: u64,
    deadline_at: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ChargeProviderUsageRequest {
    task_id: String,
}

pub(crate) async fn charge_provider_usage(
    State(state): State<RuntimeState>,
    Path((project_id, run_id)): Path<(String, String)>,
    Json(request): Json<ChargeProviderUsageRequest>,
) -> Response {
    let Some(budget_path) = state.engine_budget_database.as_deref() else {
        return bad_request(
            "engine_budget_unavailable",
            "engine budget database is not configured",
        );
    };
    let receipt = match PersistentBudget::receipt(budget_path, &request.task_id) {
        Ok(receipt) => receipt,
        Err(error) => return bad_request("engine_budget_receipt_unavailable", error),
    };
    let (mut repository, project_id) = match open(&state, project_id) {
        Ok(value) => value,
        Err(response) => return *response,
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
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AdvanceTaskRequest {
    expected_version: i64,
    task_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StepRequest {
    expected_version: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ReplanRequest {
    expected_version: i64,
    expected_project_revision: i64,
    expected_source_digest: String,
    restart_from: ReplanStage,
    instruction: String,
}

pub(crate) async fn preview_replan(
    State(state): State<RuntimeState>,
    Path((project_id, run_id)): Path<(String, String)>,
) -> Response {
    let (mut repository, project_id) = match open(&state, project_id) {
        Ok(value) => value,
        Err(response) => return *response,
    };
    match AuthoringService::new(&mut repository).preview_replan(&project_id, &run_id) {
        Ok(preview) => Json(json!(preview)).into_response(),
        Err(source) => repository_error(source),
    }
}

pub(crate) async fn replan(
    State(state): State<RuntimeState>,
    Path((project_id, run_id)): Path<(String, String)>,
    Json(request): Json<ReplanRequest>,
) -> Response {
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
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct FutureIntentRequest {
    expected_version: i64,
    instruction: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ControlRunRequest {
    expected_version: i64,
    command: String,
}

#[derive(Deserialize)]
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
}

pub(crate) async fn control(
    State(state): State<RuntimeState>,
    Path((project_id, run_id)): Path<(String, String)>,
    Json(request): Json<ControlRunRequest>,
) -> Response {
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
}

pub(crate) async fn start(
    State(state): State<RuntimeState>,
    Path(project_id): Path<String>,
    Json(request): Json<StartRequest>,
) -> Response {
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
}

pub(crate) async fn start_position(
    State(state): State<RuntimeState>,
    Path(project_id): Path<String>,
) -> Response {
    let (repository, project_id) = match open(&state, project_id) {
        Ok(value) => value,
        Err(response) => return *response,
    };
    match repository.generation_start_position(&project_id) {
        Ok(position) => Json(json!(position)).into_response(),
        Err(source) => repository_error(source),
    }
}

pub(crate) async fn get(
    State(state): State<RuntimeState>,
    Path((project_id, run_id)): Path<(String, String)>,
) -> Response {
    let (mut repository, project_id) = match open(&state, project_id) {
        Ok(value) => value,
        Err(response) => return *response,
    };
    match AuthoringService::new(&mut repository).get(&project_id, &run_id) {
        Ok(run) => Json(json!(run)).into_response(),
        Err(source) => repository_error(source),
    }
}

pub(crate) async fn list(
    State(state): State<RuntimeState>,
    Path(project_id): Path<String>,
) -> Response {
    let (mut repository, project_id) = match open(&state, project_id) {
        Ok(value) => value,
        Err(response) => return *response,
    };
    match AuthoringService::new(&mut repository).list(&project_id) {
        Ok(runs) => Json(json!({"runs": runs})).into_response(),
        Err(source) => repository_error(source),
    }
}

pub(crate) async fn add_future_intent(
    State(state): State<RuntimeState>,
    Path((project_id, run_id)): Path<(String, String)>,
    Json(request): Json<FutureIntentRequest>,
) -> Response {
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
}

pub(crate) async fn revise_current_chapter(
    State(state): State<RuntimeState>,
    Path((project_id, run_id)): Path<(String, String)>,
    Json(request): Json<FutureIntentRequest>,
) -> Response {
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
}

pub(crate) async fn next(
    State(state): State<RuntimeState>,
    Path((project_id, run_id)): Path<(String, String)>,
) -> Response {
    let (mut repository, project_id) = match open(&state, project_id) {
        Ok(value) => value,
        Err(response) => return *response,
    };
    match AuthoringService::new(&mut repository).next_action(&project_id, &run_id, unix_now()) {
        Ok(action) => Json(action).into_response(),
        Err(source) => repository_error(source),
    }
}

pub(crate) async fn advance_task(
    State(state): State<RuntimeState>,
    Path((project_id, run_id)): Path<(String, String)>,
    Json(request): Json<AdvanceTaskRequest>,
) -> Response {
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
}

pub(crate) async fn materialize(
    State(state): State<RuntimeState>,
    Path((project_id, run_id)): Path<(String, String)>,
    Json(request): Json<StepRequest>,
) -> Response {
    let (mut repository, project_id) = match open(&state, project_id) {
        Ok(value) => value,
        Err(response) => return *response,
    };
    match AuthoringService::new(&mut repository).materialize_chapter(
        &project_id,
        &run_id,
        request.expected_version,
        unix_now(),
    ) {
        Ok(run) => Json(json!(run)).into_response(),
        Err(source) => repository_error(source),
    }
}

pub(crate) async fn check(
    State(state): State<RuntimeState>,
    Path((project_id, run_id)): Path<(String, String)>,
    Json(request): Json<StepRequest>,
) -> Response {
    let (mut repository, project_id) = match open(&state, project_id) {
        Ok(value) => value,
        Err(response) => return *response,
    };
    match AuthoringService::new(&mut repository).check_knowledge(
        &project_id,
        &run_id,
        request.expected_version,
        unix_now(),
    ) {
        Ok(run) => Json(json!(run)).into_response(),
        Err(source) => repository_error(source),
    }
}

pub(crate) async fn skip_ready_extract(
    State(state): State<RuntimeState>,
    Path((project_id, run_id)): Path<(String, String)>,
    Json(request): Json<StepRequest>,
) -> Response {
    let (mut repository, project_id) = match open(&state, project_id) {
        Ok(value) => value,
        Err(response) => return *response,
    };
    match AuthoringService::new(&mut repository).skip_ready_extract(
        &project_id,
        &run_id,
        request.expected_version,
        unix_now(),
    ) {
        Ok(run) => Json(json!(run)).into_response(),
        Err(source) => repository_error(source),
    }
}

pub(crate) async fn retry_failed_task(
    State(state): State<RuntimeState>,
    Path((project_id, run_id)): Path<(String, String)>,
    Json(request): Json<StepRequest>,
) -> Response {
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
}

fn open(
    state: &RuntimeState,
    project_id: String,
) -> Result<(SqliteStoryRepository, ProjectId), Box<Response>> {
    let project_id = ProjectId::parse(project_id)
        .map_err(|source| Box::new(bad_request("invalid_project_id", source)))?;
    let repository = SqliteStoryRepository::open(&state.database_path)
        .map_err(|source| Box::new(repository_error(source)))?;
    Ok((repository, project_id))
}
