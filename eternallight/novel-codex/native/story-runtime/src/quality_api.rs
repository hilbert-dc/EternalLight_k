use crate::server::{
    Actor, RuntimeState, bad_request, candidate_task_with_reader, error, repository_error,
};
use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use novel_codex_story_core::{ProjectId, QualityMutation};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "QualityApiPage", optional_fields = nullable)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Page {
    before: Option<i64>,
}
pub(crate) async fn get(
    State(state): State<RuntimeState>,
    Path(project): Path<String>,
    Query(page): Query<Page>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |mut repo| {
        let project = match ProjectId::parse(project) {
            Ok(p) => p,
            Err(e) => return bad_request("invalid_project", e),
        };

        match repo.quality().quality_notebook(&project, page.before, None) {
            Ok(v) => Json(v).into_response(),
            Err(e) => repository_error(e),
        }
    })
    .await
}
pub(crate) async fn change(
    State(state): State<RuntimeState>,
    Path(project): Path<String>,
    Json(request): Json<QualityMutation>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |mut repo| {
        let project = match ProjectId::parse(project) {
            Ok(p) => p,
            Err(e) => return bad_request("invalid_project", e),
        };

        match repo
            .quality()
            .change_quality_notebook(&project, request, None)
        {
            Ok(v) => Json(v).into_response(),
            Err(e) => repository_error(e),
        }
    })
    .await
}
pub(crate) async fn packet(
    State(state): State<RuntimeState>,
    Path((project, id)): Path<(String, String)>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |mut repo| {
        let project = match ProjectId::parse(project) {
            Ok(p) => p,
            Err(e) => return bad_request("invalid_project", e),
        };

        match repo.quality().quality_packet(&project, &id) {
            Ok(v) => Json(v).into_response(),
            Err(e) => repository_error(e),
        }
    })
    .await
}
pub(crate) async fn review(
    State(state): State<RuntimeState>,
    Path((project, id)): Path<(String, String)>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |mut repo| {
        let project = match ProjectId::parse(project) {
            Ok(p) => p,
            Err(e) => return bad_request("invalid_project", e),
        };

        match repo.quality().quality_review(&project, &id) {
            Ok(v) => Json(v).into_response(),
            Err(e) => repository_error(e),
        }
    })
    .await
}
#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "QualityApiBoundRequest", optional_fields = nullable)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct BoundRequest {
    thread_id: String,
    expected_revision: i64,
    action: String,
    before: Option<i64>,
    mutation: Option<QualityMutation>,
    page: Option<usize>,
    report: Option<Value>,
    packet_id: Option<String>,
}
pub(crate) async fn task(
    State(state): State<RuntimeState>,
    Path((project, id)): Path<(String, String)>,
    Extension(actor): Extension<Actor>,
    headers: HeaderMap,
    Json(request): Json<BoundRequest>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |mut repo| {
        let project = match ProjectId::parse(project) {
            Ok(p) => p,
            Err(e) => return bad_request("invalid_project", e),
        };

        let task = match candidate_task_with_reader(
            &repo,
            actor,
            &headers,
            &project,
            Some(&id),
            Some(&request.thread_id),
            Some(request.expected_revision),
        ) {
            Ok(Some(t)) => t,
            Ok(None) => {
                return error(
                    StatusCode::FORBIDDEN,
                    "task_required",
                    "Quality tools require an active task",
                );
            }
            Err(r) => return *r,
        };
        if task.operation == "advisory" {
            if let Err(e) = repo.assert_task_inputs_current(&task) {
                return repository_error(e);
            }
        }
        if request.action != "review" && request.packet_id.is_some() {
            return bad_request(
                "invalid_quality_fields",
                "packetId is only valid for review reads",
            );
        }
        let tool = match request.action.as_str() {
            "read" => "read_quality_notebook",
            "review" => "read_quality_review",
            "change" => "record_quality_note",
            "page" => "read_quality_packet",
            "report" => "submit_quality_reading",
            _ => return bad_request("invalid_quality_action", "Unknown quality action"),
        };
        if let Err(e) = crate::recovery_binding::require_root(&headers, &task, tool) {
            return repository_error(e);
        }
        if matches!(request.action.as_str(), "change" | "report") {
            let key = request
                .mutation
                .as_ref()
                .map(|m| m.idempotency_key.as_str());
            if let Err(e) =
                crate::recovery_binding::bind(&mut repo, &headers, Some(&task), tool, key)
            {
                return repository_error(e);
            }
        }
        let value = match (task.operation.as_str(), request.action.as_str()) {
            ("advisory", "review")
                if request.mutation.is_none()
                    && request.page.is_none()
                    && request.report.is_none()
                    && request.before.is_none() =>
            {
                match request.packet_id {
                    Some(id) => repo
                        .quality()
                        .quality_review(&project, &id)
                        .map(|mut review| {
                            if let Some(feedback) = review["latestAuthorFeedback"].as_array_mut() {
                                feedback.truncate(1);
                            }
                            review["feedbackTruncated"] =
                                (review["feedbackCount"].as_u64().unwrap_or(0) > 1).into();
                            review
                        }),
                    None => return bad_request("packet_required", "Packet ID required"),
                }
            }
            (
                "advisory" | "generateChapter" | "generateBrief" | "generateOutline"
                | "generatePlan",
                "read",
            ) if request.mutation.is_none()
                && request.page.is_none()
                && request.report.is_none() =>
            {
                repo.quality()
                    .quality_notebook(&project, request.before, Some(&task))
            }
            ("advisory", "change")
                if request.before.is_none()
                    && request.page.is_none()
                    && request.report.is_none() =>
            {
                match request.mutation {
                    Some(m) => repo
                        .quality()
                        .change_quality_notebook(&project, m, Some(&task)),
                    None => {
                        return bad_request(
                            "mutation_required",
                            "Quality change requires a mutation",
                        );
                    }
                }
            }
            ("reviewQuality", "page")
                if request.mutation.is_none()
                    && request.before.is_none()
                    && request.report.is_none() =>
            {
                match request.page {
                    Some(p) => repo.quality().read_quality_packet_page(&task, p),
                    None => return bad_request("page_required", "Reader page is required"),
                }
            }
            ("reviewQuality", "report")
                if request.mutation.is_none()
                    && request.before.is_none()
                    && request.page.is_none() =>
            {
                match request.report {
                    Some(r) => repo.quality().submit_quality_reader(&task, &r),
                    None => return bad_request("report_required", "Reader report is required"),
                }
            }
            _ => {
                return error(
                    StatusCode::FORBIDDEN,
                    "quality_scope_forbidden",
                    "Quality action is outside this task scope",
                );
            }
        };
        match value {
            Ok(v)
                if crate::context_model_item_budget::model_item_bytes(&v)
                    <= crate::context_model_item_budget::MODEL_ITEM_BYTE_LIMIT =>
            {
                Json(v).into_response()
            }
            Ok(_) => error(
                StatusCode::UNPROCESSABLE_ENTITY,
                "quality_model_item_budget_exceeded",
                "Quality result exceeds the model-item budget; no content was delivered",
            ),
            Err(e) => repository_error(e),
        }
    })
    .await
}

/// Author/Host only; authentication is applied by server composition.
pub(crate) fn routes() -> axum::Router<crate::server::RuntimeState> {
    axum::Router::new()
        .route(
            "/api/projects/{project_id}/quality-notebook",
            axum::routing::get(crate::quality_api::get).post(crate::quality_api::change),
        )
        .route(
            "/api/projects/{project_id}/quality-packets/{packet_id}",
            axum::routing::get(crate::quality_api::packet),
        )
        .route(
            "/api/projects/{project_id}/quality-packets/{packet_id}/review",
            axum::routing::get(crate::quality_api::review),
        )
}

/// Author or candidate; task capabilities remain checked by the handler.
pub(crate) fn writer_routes() -> axum::Router<crate::server::RuntimeState> {
    axum::Router::new().route(
        "/api/projects/{project_id}/tasks/{task_id}/quality",
        axum::routing::post(crate::quality_api::task),
    )
}

use novel_codex_story_core::{TaskInputRepository};
