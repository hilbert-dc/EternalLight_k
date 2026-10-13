use crate::server::{Actor, RuntimeState, bad_request, candidate_task, error, repository_error};
use axum::{
    Extension, Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use novel_codex_story_core::{GenerationHandoffRequest, ProjectId};
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "GenerationHandoffApiRequest", optional_fields = nullable)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Request {
    task_id: String,
    thread_id: String,
    expected_revision: i64,
    target_chapters: i64,
    resume_run: Option<serde_json::Value>,
    instruction: String,
    expected_intent_revision: i64,
    idempotency_key: String,
}

pub(crate) async fn propose(
    State(state): State<RuntimeState>,
    Path((project, task)): Path<(String, String)>,
    Extension(actor): Extension<Actor>,
    headers: HeaderMap,
    Json(request): Json<Request>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |mut repository| {
        if task != request.task_id {
            return bad_request("invalid_task", "proposal task does not match route");
        }
        let project = match ProjectId::parse(project) {
            Ok(id) => id,
            Err(source) => return bad_request("invalid_project_id", source),
        };

        let task = match candidate_task(
            &repository,
            actor,
            &headers,
            &project,
            Some(&task),
            Some(&request.thread_id),
            Some(request.expected_revision),
        ) {
            Ok(Some(task)) => task,
            Ok(None) => {
                return error(
                    StatusCode::UNAUTHORIZED,
                    "task_capability_required",
                    "a live advisory task is required",
                );
            }
            Err(response) => return *response,
        };
        if !(1..=novel_codex_story_core::MAX_GENERATION_CHAPTERS).contains(&request.target_chapters)
            || request.instruction.trim().is_empty()
            || request.instruction.chars().count() > 1_000
            || request.idempotency_key.is_empty()
            || request.idempotency_key.len() > 160
            || request.idempotency_key.chars().any(char::is_control)
            || request.expected_intent_revision < 0
        {
            return bad_request("invalid_proposal", "invalid generation proposal");
        }
        if let Err(source) = crate::recovery_binding::bind(
            &mut repository,
            &headers,
            Some(&task),
            "request_generation_handoff",
            Some(&request.idempotency_key),
        ) {
            return repository_error(source);
        }
        match repository.propose_generation_handoff(
            &task,
            &GenerationHandoffRequest {
                target_chapters: request.target_chapters,
                resume_run: request.resume_run,
                instruction: request.instruction,
                expected_intent_revision: request.expected_intent_revision,
                idempotency_key: request.idempotency_key,
            },
        ) {
            Ok(proposal) => Json(json!({"proposal":proposal})).into_response(),
            Err(source) => repository_error(source),
        }
    })
    .await
}

pub(crate) async fn read(
    State(state): State<RuntimeState>,
    Path((project, task)): Path<(String, String)>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |repository| {
        let project = match ProjectId::parse(project) {
            Ok(id) => id,
            Err(source) => return bad_request("invalid_project_id", source),
        };

        match repository.generation_handoff(&project, &task) {
            Ok(proposal) => Json(json!({"proposal":proposal})).into_response(),
            Err(source) => repository_error(source),
        }
    })
    .await
}

pub(crate) async fn from_work(
    State(state): State<RuntimeState>,
    Path((project, work)): Path<(String, String)>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |repository| {
        let project = match ProjectId::parse(project) {
            Ok(id) => id,
            Err(source) => return bad_request("invalid_project_id", source),
        };

        match repository.work_generation_handoff(&project, &work) {
            Ok(handoff) => Json(json!({"handoff":handoff})).into_response(),
            Err(source) => repository_error(source),
        }
    })
    .await
}

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "GenerationHandoffApiExecuteRequest", optional_fields = nullable)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ExecuteRequest {
    proposal_id: String,
}

fn execution_error(source: novel_codex_story_core::RepositoryError) -> Response {
    use novel_codex_story_core::RepositoryError as E;
    let (code, message) = match &source {
        E::AuthoringRunStale | E::WorkTaskStale => (
            "generation_handoff_run_changed",
            "原任务状态或可用额度已变化，需要重新决定接续。",
        ),
        E::TaskInputStale | E::StaleProjectRevision { .. } => (
            "generation_handoff_source_changed",
            "前置资料已变化，需要重新核对后再接续。",
        ),
        E::TaskBindingInvalid | E::TaskOutputForbidden => (
            "generation_handoff_binding_invalid",
            "交接身份已失效，需要重新读取原任务。",
        ),
        E::TaskNotWritable | E::TaskOutputMissing => (
            "generation_handoff_unsettled_results",
            "原任务仍有待核对的保存或结算结果，请先处理后再接续。",
        ),
        E::InvalidContent(_) => (
            "generation_handoff_prerequisites_incomplete",
            "前置任务与执行条件尚未满足，请查看未完成事项。",
        ),
        _ => return repository_error(source),
    };
    (StatusCode::CONFLICT,Json(json!({"error":{"code":code,"message":message,"details":{"diagnostic":source.to_string()},"recovery":null}}))).into_response()
}

/// This route exists only on the trusted author Host router. Candidate tools
/// can propose a continuation, but cannot execute or grant themselves one.
pub(crate) async fn execute(
    State(state): State<RuntimeState>,
    Path((project, source)): Path<(String, String)>,
    Json(request): Json<ExecuteRequest>,
) -> Response {
    crate::repository_executor::run(state.clone(), move || {
        let project = match ProjectId::parse(project) {
            Ok(id) => id,
            Err(source) => return bad_request("invalid_project_id", source),
        };
        let mut repository = match state.database.repository() {
            Ok(repo) => repo,
            Err(source) => return execution_error(source),
        };
        let admission = match repository.generation_handoff_execution(&project, &source) {
            Ok(value) => value,
            Err(source) => return execution_error(source),
        };
        if admission["proposal"]["proposalId"] != request.proposal_id {
            return execution_error(novel_codex_story_core::RepositoryError::TaskBindingInvalid);
        }
        match novel_codex_story_authoring::AuthoringService::new(&mut repository)
            .execute_generation_handoff(&project, &source, crate::server::unix_now())
        {
            Ok(value) => Json(value).into_response(),
            Err(source) => execution_error(source),
        }
    })
    .await
}

/// Author/Host only; authentication is applied by server composition.
pub(crate) fn routes() -> axum::Router<crate::server::RuntimeState> {
    axum::Router::new()
        .route(
            "/api/projects/{project_id}/tasks/{task_id}/generation-handoff",
            axum::routing::get(crate::generation_handoff_api::read),
        )
        .route(
            "/api/projects/{project_id}/work-tasks/{work_task_id}/generation-handoff",
            axum::routing::get(crate::generation_handoff_api::from_work),
        )
        .route(
            "/api/projects/{project_id}/tasks/{task_id}/generation-handoff/execute",
            axum::routing::post(crate::generation_handoff_api::execute),
        )
}

/// Author or candidate; task capabilities remain checked by the handler.
pub(crate) fn writer_routes() -> axum::Router<crate::server::RuntimeState> {
    axum::Router::new().route(
        "/api/projects/{project_id}/tasks/{task_id}/generation-handoff",
        axum::routing::post(crate::generation_handoff_api::propose),
    )
}

use novel_codex_story_core::{GenerationHandoffExecutionRepository, GenerationHandoffRepository};
