use axum::{
    Extension, Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use novel_codex_story_core::{ProjectId, TaskActionRequest};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::server::{Actor, RuntimeState, bad_request, candidate_task, error, repository_error};

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "TaskActionApiRequestAction", optional_fields = nullable)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RequestAction {
    task_id: String,
    thread_id: String,
    expected_revision: i64,
    expected_intent_revision: i64,
    idempotency_key: String,
    operation: String,
    target: Value,
    instruction: String,
}

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "TaskActionApiFinishAction", optional_fields = nullable)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FinishAction {
    result_task_id: Option<String>,
    error: Option<String>,
}

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "TaskActionApiDecideAction", optional_fields = nullable)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DecideAction {
    expected_intent_revision: i64,
}

pub(crate) async fn request(
    State(state): State<RuntimeState>,
    Path((project_id, task_id)): Path<(String, String)>,
    Extension(actor): Extension<Actor>,
    headers: HeaderMap,
    Json(request): Json<RequestAction>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |mut repository| {
    if request.task_id != task_id
        || request.idempotency_key.is_empty()
        || request.idempotency_key.len() > 160
        || request.instruction.trim().is_empty()
        || request.instruction.chars().count() > 1_000
        || request.expected_intent_revision < 0
    {
        return error(
            StatusCode::BAD_REQUEST,
            "invalid_action",
            "Candidate request is invalid",
        );
    }
    let mut target = if request.operation == "generateChapter" {
        let Some(chapter_id) = request
            .target
            .get("chapterId")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty() && id.len() <= 160)
        else {
            return error(
                StatusCode::BAD_REQUEST,
                "invalid_action",
                "Chapter target is required",
            );
        };
        let Some(chapter_number) = request
            .target
            .get("chapterNumber")
            .and_then(Value::as_i64)
            .filter(|number| *number > 0)
        else {
            return error(
                StatusCode::BAD_REQUEST,
                "invalid_action",
                "Chapter position is required",
            );
        };
        let scope = request
            .target
            .get("scope")
            .cloned()
            .unwrap_or_else(|| json!({"type": "wholeChapter"}));
        if !matches!(
            scope["type"].as_str(),
            Some("wholeChapter" | "selectedScenes")
        ) {
            return error(
                StatusCode::BAD_REQUEST,
                "invalid_action_scope",
                "Unsupported chapter revision scope",
            );
        }
        json!({"type": "chapter", "chapterId": chapter_id, "chapterNumber": chapter_number,
            "readingChapter": chapter_number, "sceneOrder": 1, "scope": scope})
    } else if matches!(
        request.operation.as_str(),
        "extractCommitments" | "modelKnowledge"
    ) {
        json!({"type":"chapter","chapterId":request.target["chapterId"],
            "chapterNumber":request.target["chapterNumber"],"sourceRevisionId":request.target["sourceRevisionId"]})
    } else if request.operation == "reviseCommitment" {
        json!({"type":"commitment","commitmentId":request.target["commitmentId"],"revisionId":request.target["revisionId"]})
    } else if matches!(
        request.operation.as_str(),
        "generateBrief" | "generateOutline" | "generatePlan" | "generatePropertyRules"
    ) {
        json!({"type": "artifact"})
    } else {
        return error(
            StatusCode::BAD_REQUEST,
            "invalid_action",
            "Candidate operation is unsupported",
        );
    };
    if request.operation == "generateChapter" {
        let Some(acceptance) = request.target.get("chapterAcceptance") else {
            return error(
                StatusCode::BAD_REQUEST,
                "invalid_action",
                "generateChapter requires chapterAcceptance before drafting",
            );
        };
        target["chapterAcceptance"] = acceptance.clone();
        if let Some(outcomes) = request.target.get("desiredOutcomes") {
            target["desiredOutcomes"] = outcomes.clone();
        }
        if let Some(pov) = request.target.get("povCharacterId") {
            if !pov.is_null()
                && !pov
                    .as_str()
                    .is_some_and(|id| !id.trim().is_empty() && id.len() <= 160)
            {
                return error(
                    StatusCode::BAD_REQUEST,
                    "invalid_action_scope",
                    "Writer position is invalid",
                );
            }
            target["povCharacterId"] = pov.clone();
        }
        if let Some(story_time) = request.target.get("storyTime") {
            if !story_time.is_null() && story_time.as_i64().is_none() {
                return error(
                    StatusCode::BAD_REQUEST,
                    "invalid_action_scope",
                    "Writer position is invalid",
                );
            }
            target["storyTime"] = story_time.clone();
        }
    }
    let project_id = match ProjectId::parse(project_id) {
        Ok(id) => id,
        Err(source) => return bad_request("invalid_project_id", source),
    };

    let source = match candidate_task(
        &repository,
        actor,
        &headers,
        &project_id,
        Some(&task_id),
        Some(&request.thread_id),
        Some(request.expected_revision),
    ) {
        Ok(Some(task)) => task,
        Ok(None) => {
            return error(
                StatusCode::UNAUTHORIZED,
                "task_capability_required",
                "A live Agent task is required",
            );
        }
        Err(response) => return *response,
    };
    if source.operation != "advisory" {
        return error(
            StatusCode::FORBIDDEN,
            "invalid_action",
            "Only conversation tasks may request candidate work",
        );
    }
    if request.operation == "generateChapter" {
        let chapter_id = target["chapterId"].as_str().unwrap_or_default();
        let chapter_number = target["chapterNumber"].as_i64().unwrap_or_default();
        let chapters = match repository.chapters(&project_id) {
            Ok(chapters) => chapters,
            Err(source) => return repository_error(source),
        };
        if !chapters
            .iter()
            .any(|chapter| chapter.chapter_id == chapter_id && chapter.number == chapter_number)
        {
            return error(
                StatusCode::UNPROCESSABLE_ENTITY,
                "invalid_action_target",
                "Target chapter does not belong to this project",
            );
        }
    }
    if let Err(source) = crate::recovery_binding::bind(
        &mut repository,
        &headers,
        Some(&source),
        "request_candidate_task",
        Some(&request.idempotency_key),
    ) {
        return repository_error(source);
    }
    let action = TaskActionRequest {
        id: format!("action-{}", Uuid::new_v4().simple()),
        source_task_id: task_id.clone(),
        operation: request.operation,
        target,
        instruction: request.instruction.trim().to_owned(),
        input_revision: request.expected_revision,
        intent_revision: request.expected_intent_revision,
        status: "proposed".into(),
        result_task_id: None,
        error: None,
    };
    match repository.record_task_action(&project_id, &task_id, &request.idempotency_key, action) {
        Ok(action) => (StatusCode::CREATED, Json(json!(action))).into_response(),
        Err(source) => repository_error(source),
    }

    }).await
}

pub(crate) async fn list(
    State(state): State<RuntimeState>,
    Path((project_id, task_id)): Path<(String, String)>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |repository| {
        let project_id = match ProjectId::parse(project_id) {
            Ok(id) => id,
            Err(source) => return bad_request("invalid_project_id", source),
        };

        match repository.task_actions(&project_id, &task_id) {
            Ok(actions) => Json(json!({"actions": actions})).into_response(),
            Err(source) => repository_error(source),
        }
    })
    .await
}

pub(crate) async fn proposed(
    State(state): State<RuntimeState>,
    Path(project_id): Path<String>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |repository| {
        let project_id = match ProjectId::parse(project_id) {
            Ok(id) => id,
            Err(source) => return bad_request("invalid_project_id", source),
        };

        match repository.proposed_task_actions(&project_id) {
            Ok(actions) => Json(json!({"actions":actions})).into_response(),
            Err(source) => repository_error(source),
        }
    })
    .await
}

pub(crate) async fn approve(
    State(state): State<RuntimeState>,
    Path((project_id, task_id, action_id)): Path<(String, String, String)>,
    Json(request): Json<DecideAction>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |mut repository| {
        let project_id = match ProjectId::parse(project_id) {
            Ok(id) => id,
            Err(source) => return bad_request("invalid_project_id", source),
        };

        match repository.approve_task_action(
            &project_id,
            &task_id,
            &action_id,
            request.expected_intent_revision,
            crate::server::unix_now(),
        ) {
            Ok(action) => Json(json!(action)).into_response(),
            Err(source) => repository_error(source),
        }
    })
    .await
}

pub(crate) async fn decline(
    State(state): State<RuntimeState>,
    Path((project_id, task_id, action_id)): Path<(String, String, String)>,
    Json(request): Json<DecideAction>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |mut repository| {
        let project_id = match ProjectId::parse(project_id) {
            Ok(id) => id,
            Err(source) => return bad_request("invalid_project_id", source),
        };

        match repository.decline_task_action(
            &project_id,
            &task_id,
            &action_id,
            request.expected_intent_revision,
        ) {
            Ok(action) => Json(json!(action)).into_response(),
            Err(source) => repository_error(source),
        }
    })
    .await
}

pub(crate) async fn chapter_next_step(
    State(state): State<RuntimeState>,
    Path((project_id, chapter_id)): Path<(String, String)>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |repository| {
        let project_id = match ProjectId::parse(project_id) {
            Ok(id) => id,
            Err(source) => return bad_request("invalid_project_id", source),
        };

        match repository.chapter_followup(&project_id, &chapter_id) {
            Ok(action) => Json(json!({"action": action})).into_response(),
            Err(source) => repository_error(source),
        }
    })
    .await
}

pub(crate) async fn finish(
    State(state): State<RuntimeState>,
    Path((project_id, task_id, action_id)): Path<(String, String, String)>,
    Json(request): Json<FinishAction>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |mut repository| {
        if request.result_task_id.is_some() == request.error.is_some()
            || request
                .error
                .as_deref()
                .is_some_and(|error| error.len() > 400)
        {
            return error(
                StatusCode::BAD_REQUEST,
                "invalid_action_result",
                "Exactly one result or error is required",
            );
        }
        let project_id = match ProjectId::parse(project_id) {
            Ok(id) => id,
            Err(source) => return bad_request("invalid_project_id", source),
        };

        match repository.finish_task_action(
            &project_id,
            &task_id,
            &action_id,
            request.result_task_id.as_deref(),
            request.error.as_deref(),
        ) {
            Ok(action) => Json(json!(action)).into_response(),
            Err(novel_codex_story_core::RepositoryError::TaskBindingInvalid) => error(
                StatusCode::CONFLICT,
                "invalid_action_result",
                "Result does not match the persisted candidate request",
            ),
            Err(source) => repository_error(source),
        }
    })
    .await
}

/// Author/Host only; authentication is applied by server composition.
pub(crate) fn routes() -> axum::Router<crate::server::RuntimeState> {
    axum::Router::new()
        .route(
            "/api/projects/{project_id}/task-actions/proposed",
            axum::routing::get(crate::task_action_api::proposed),
        )
        .route(
            "/api/projects/{project_id}/tasks/{task_id}/actions",
            axum::routing::get(crate::task_action_api::list),
        )
        .route(
            "/api/projects/{project_id}/chapters/{chapter_id}/next-step",
            axum::routing::get(crate::task_action_api::chapter_next_step),
        )
        .route(
            "/api/projects/{project_id}/tasks/{task_id}/actions/{action_id}/finish",
            axum::routing::post(crate::task_action_api::finish),
        )
        .route(
            "/api/projects/{project_id}/tasks/{task_id}/actions/{action_id}/approve",
            axum::routing::post(crate::task_action_api::approve),
        )
        .route(
            "/api/projects/{project_id}/tasks/{task_id}/actions/{action_id}/decline",
            axum::routing::post(crate::task_action_api::decline),
        )
}

/// Author or candidate; task capabilities remain checked by the handler.
pub(crate) fn writer_routes() -> axum::Router<crate::server::RuntimeState> {
    axum::Router::new().route(
        "/api/projects/{project_id}/tasks/{task_id}/actions",
        axum::routing::post(crate::task_action_api::request),
    )
}

use novel_codex_story_core::{ProjectRevisionRepository, TaskActionsRepository, TaskFollowupRepository};
