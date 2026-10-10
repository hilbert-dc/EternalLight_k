use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use novel_codex_story_core::{
    ProjectId, RepositoryError, SqliteStoryRepository, WorkControlCommand,
};
use serde::Deserialize;
use serde_json::json;

use crate::server::{RuntimeState, bad_request, error, repository_error, unix_now};

#[derive(Deserialize)]
pub(crate) struct ListRequest {
    cursor: Option<String>,
}

pub(crate) async fn list(
    State(state): State<RuntimeState>,
    Path(project_id): Path<String>,
    Query(request): Query<ListRequest>,
) -> Response {
    let (repo, project) = match open(&state, project_id) {
        Ok(value) => value,
        Err(response) => return *response,
    };
    match repo.work_tasks_page(&project, request.cursor.as_deref()) {
        Ok(page) => Json(json!(page)).into_response(),
        Err(source) => repository_error(source),
    }
}

pub(crate) async fn get(
    State(state): State<RuntimeState>,
    Path((project_id, id)): Path<(String, String)>,
) -> Response {
    let (repo, project) = match open(&state, project_id) {
        Ok(value) => value,
        Err(response) => return *response,
    };
    match repo.work_task(&project, &id) {
        Ok(task) => Json(json!(task)).into_response(),
        Err(source) => repository_error(source),
    }
}

pub(crate) async fn from_attempt(
    State(state): State<RuntimeState>,
    Path((project_id, id)): Path<(String, String)>,
) -> Response {
    let (repo, project) = match open(&state, project_id) {
        Ok(value) => value,
        Err(response) => return *response,
    };
    match repo.work_task_for_attempt(&project, &id) {
        Ok(task) => Json(json!(task)).into_response(),
        Err(source) => repository_error(source),
    }
}

pub(crate) async fn control(
    State(state): State<RuntimeState>,
    Path((project_id, id)): Path<(String, String)>,
    Json(request): Json<WorkControlCommand>,
) -> Response {
    if !matches!(
        request.command.as_str(),
        "pause" | "pauseAfterStep" | "resume" | "cancel" | "recoverSavedResult"
    ) {
        return error(
            StatusCode::BAD_REQUEST,
            "invalid_command",
            "Unknown task control",
        );
    }
    let (mut repo, project) = match open(&state, project_id) {
        Ok(value) => value,
        Err(response) => return *response,
    };
    let selected = match repo.work_task(&project, &id) {
        Ok(w) => w,
        Err(e) => return repository_error(e),
    };
    let result = repo.apply_work_control_command(&project, &id, &request, unix_now(), |repo| {
        if request.command == "pauseAfterStep" {
            return repo.pause_work_after_step(&project, &id, request.expected_control_revision).map(|_| ());
        }
        if selected.kind == "generation" {
            if request.command == "recoverSavedResult" {
                return Err(RepositoryError::TaskBindingInvalid);
            }
            let run_id = id
                .strip_prefix("work-")
                .ok_or(RepositoryError::TaskBindingInvalid)?;
            repo.with_generation_work_control(&id, request.expected_control_revision, |repo| {
                let run = repo.generation_run(&project, run_id)?;
                novel_codex_story_authoring::AuthoringService::new(repo)
                    .control(&project, run_id, run.version, &request.command, unix_now())
                    .map(|_| ())
            })
        } else {
            repo.control_work_task(
                &project,
                &id,
                &request.command,
                request.expected_control_revision,
                unix_now(),
            )
            .map(|_| ())
        }
    });
    match result {
        Ok(outcome) if outcome.receipt.status == "rejected" => (StatusCode::CONFLICT, Json(json!({
            "error": {"code":"work_control_rejected", "message":outcome.receipt.reason}, "outcome":outcome
        }))).into_response(),
        Ok(outcome) => Json(json!(outcome)).into_response(),
        Err(source) => repository_error(source),
    }
}

pub(crate) async fn receipt(
    State(state): State<RuntimeState>,
    Path((project_id, id, command_id)): Path<(String, String, String)>,
) -> Response {
    let (repo, project) = match open(&state, project_id) {
        Ok(value) => value,
        Err(response) => return *response,
    };
    match repo.work_control_receipt(&project, &id, &command_id) {
        Ok(receipt) => Json(json!({"receipt":receipt})).into_response(),
        Err(source) => repository_error(source),
    }
}

fn open(
    state: &RuntimeState,
    project_id: String,
) -> Result<(SqliteStoryRepository, ProjectId), Box<Response>> {
    let project = ProjectId::parse(project_id)
        .map_err(|source| Box::new(bad_request("invalid_project_id", source)))?;
    let repo = SqliteStoryRepository::open(&state.database_path)
        .map_err(|source| Box::new(repository_error(source)))?;
    Ok((repo, project))
}
