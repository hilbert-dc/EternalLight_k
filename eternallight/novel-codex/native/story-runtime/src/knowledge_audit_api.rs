use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use novel_codex_story_core::{ProjectId, SqliteStoryRepository};
use serde::Deserialize;
use serde_json::json;

use crate::server::{RuntimeState, bad_request, repository_error};

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "KnowledgeAuditApiControlRequest", optional_fields = nullable)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ControlRequest {
    command: String,
    expected_version: i64,
}

pub(crate) async fn start(
    State(state): State<RuntimeState>,
    Path(project_id): Path<String>,
) -> Response {
    crate::repository_executor::run(state.clone(), move || {
        let (mut repository, project_id) = match open(&state, project_id) {
            Ok(value) => value,
            Err(response) => return *response,
        };
        match repository.start_knowledge_audit(&project_id) {
            Ok(job) => (StatusCode::CREATED, Json(json!(job))).into_response(),
            Err(error) => repository_error(error),
        }
    })
    .await
}

pub(crate) async fn latest(
    State(state): State<RuntimeState>,
    Path(project_id): Path<String>,
) -> Response {
    crate::repository_executor::run(state.clone(), move || {
        let (repository, project_id) = match open(&state, project_id) {
            Ok(value) => value,
            Err(response) => return *response,
        };
        match repository.latest_knowledge_audit_job(&project_id) {
            Ok(job) => Json(json!({"job": job})).into_response(),
            Err(error) => repository_error(error),
        }
    })
    .await
}

pub(crate) async fn get(
    State(state): State<RuntimeState>,
    Path((project_id, job_id)): Path<(String, String)>,
) -> Response {
    crate::repository_executor::run(state.clone(), move || {
        let (repository, project_id) = match open(&state, project_id) {
            Ok(value) => value,
            Err(response) => return *response,
        };
        match repository.knowledge_audit_job(&project_id, &job_id) {
            Ok(job) => Json(json!(job)).into_response(),
            Err(error) => repository_error(error),
        }
    })
    .await
}

pub(crate) async fn control(
    State(state): State<RuntimeState>,
    Path((project_id, job_id)): Path<(String, String)>,
    Json(request): Json<ControlRequest>,
) -> Response {
    crate::repository_executor::run(state.clone(), move || {
        if !matches!(request.command.as_str(), "pause" | "resume" | "cancel") {
            return bad_request("invalid_command", "Unknown knowledge audit control");
        }
        let (mut repository, project_id) = match open(&state, project_id) {
            Ok(value) => value,
            Err(response) => return *response,
        };
        match repository.control_knowledge_audit(
            &project_id,
            &job_id,
            &request.command,
            request.expected_version,
        ) {
            Ok(job) => Json(json!(job)).into_response(),
            Err(error) => repository_error(error),
        }
    })
    .await
}

fn open(
    state: &RuntimeState,
    project_id: String,
) -> Result<(SqliteStoryRepository, ProjectId), Box<Response>> {
    let project_id = ProjectId::parse(project_id)
        .map_err(|error| Box::new(bad_request("invalid_project_id", error)))?;
    let repository = state
        .database
        .repository()
        .map_err(|error| Box::new(repository_error(error)))?;
    Ok((repository, project_id))
}

/// Author/Host only; authentication is applied by server composition.
pub(crate) fn routes() -> axum::Router<crate::server::RuntimeState> {
    axum::Router::new()
        .route(
            "/api/projects/{project_id}/knowledge-audits",
            axum::routing::get(crate::knowledge_audit_api::latest)
                .post(crate::knowledge_audit_api::start),
        )
        .route(
            "/api/projects/{project_id}/knowledge-audits/{job_id}",
            axum::routing::get(crate::knowledge_audit_api::get),
        )
        .route(
            "/api/projects/{project_id}/knowledge-audits/{job_id}/control",
            axum::routing::post(crate::knowledge_audit_api::control),
        )
}

use novel_codex_story_core::{KnowledgeAuditJobsRepository};
