use axum::{
    Json,
    extract::{Path, State},
    response::{IntoResponse, Response},
};
use novel_codex_story_core::ProjectId;
use serde::Deserialize;
use serde_json::json;

use crate::server::{RuntimeState, bad_request, repository_error, unix_now};

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "TaskIntentApiAppendIntent", optional_fields = nullable)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AppendIntent {
    expected_intent_revision: i64,
    author_text: String,
}

pub(crate) async fn append(
    State(state): State<RuntimeState>,
    Path((project_id, task_id)): Path<(String, String)>,
    Json(request): Json<AppendIntent>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |mut repository| {
        let project_id = match ProjectId::parse(project_id) {
            Ok(id) => id,
            Err(source) => return bad_request("invalid_project_id", source),
        };

        match repository.task_intent_store().append_task_intent(
            &project_id,
            &task_id,
            request.expected_intent_revision,
            &request.author_text,
            unix_now(),
        ) {
            Ok(intent) => Json(json!(intent)).into_response(),
            Err(source) => repository_error(source),
        }
    })
    .await
}

pub(crate) async fn answer(
    State(state): State<RuntimeState>,
    Path((project_id, task_id)): Path<(String, String)>,
    Json(request): Json<AppendIntent>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |mut repository| {
        let project = match ProjectId::parse(project_id) {
            Ok(id) => id,
            Err(e) => return bad_request("invalid_project_id", e),
        };

        match repository.task_intent_store().append_task_answer(
            &project,
            &task_id,
            request.expected_intent_revision,
            &request.author_text,
            unix_now(),
        ) {
            Ok(answer) => Json(json!(answer)).into_response(),
            Err(e) => repository_error(e),
        }
    })
    .await
}

pub(crate) async fn deliver(
    State(state): State<RuntimeState>,
    Path((project_id, task_id, intent_revision)): Path<(String, String, i64)>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |mut repository| {
        let project_id = match ProjectId::parse(project_id) {
            Ok(id) => id,
            Err(source) => return bad_request("invalid_project_id", source),
        };

        match repository.task_intent_store().deliver_task_intent(
            &project_id,
            &task_id,
            intent_revision,
            unix_now(),
        ) {
            Ok(intent) => Json(json!(intent)).into_response(),
            Err(source) => repository_error(source),
        }
    })
    .await
}

pub(crate) async fn retain_unsent(
    State(state): State<RuntimeState>,
    Path((project_id, task_id, intent_revision)): Path<(String, String, i64)>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |mut repository| {
        let project_id = match ProjectId::parse(project_id) {
            Ok(id) => id,
            Err(source) => return bad_request("invalid_project_id", source),
        };

        match repository.task_intent_store().retain_unsent_task_intent(
            &project_id,
            &task_id,
            intent_revision,
            unix_now(),
        ) {
            Ok(intent) => Json(json!(intent)).into_response(),
            Err(source) => repository_error(source),
        }
    })
    .await
}

pub(crate) async fn list(
    State(state): State<RuntimeState>,
    Path((project_id, task_id)): Path<(String, String)>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |mut repository| {
        let project_id = match ProjectId::parse(project_id) {
            Ok(id) => id,
            Err(source) => return bad_request("invalid_project_id", source),
        };

        match repository
            .task_intent_store()
            .task_intents(&project_id, &task_id)
        {
            Ok(intents) => Json(json!({"intents": intents})).into_response(),
            Err(source) => repository_error(source),
        }
    })
    .await
}

/// Author/Host only; authentication is applied by server composition.
pub(crate) fn routes() -> axum::Router<crate::server::RuntimeState> {
    axum::Router::new()
        .route(
            "/api/projects/{project_id}/tasks/{task_id}/answers",
            axum::routing::post(crate::task_intent_api::answer),
        )
        .route(
            "/api/projects/{project_id}/tasks/{task_id}/intents",
            axum::routing::get(crate::task_intent_api::list).post(crate::task_intent_api::append),
        )
        .route(
            "/api/projects/{project_id}/tasks/{task_id}/intents/{intent_revision}/not-sent",
            axum::routing::post(crate::task_intent_api::retain_unsent),
        )
        .route(
            "/api/projects/{project_id}/tasks/{task_id}/intents/{intent_revision}/delivered",
            axum::routing::post(crate::task_intent_api::deliver),
        )
}
