use axum::Extension;
use axum::Json;
use axum::extract::Path;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::response::Response;
use novel_codex_story_core::ProjectId;
use novel_codex_story_core::RepositoryError;
use novel_codex_story_core::SqliteStoryRepository;
use novel_codex_story_core::TaskRecord;
use novel_codex_story_core::TaskUpdate;
use novel_codex_story_core::{AuthorTaskSubmission, CreateTaskRequest};
use serde_json::Value;
use serde_json::json;

use crate::contracts::*;
use crate::server::{
    Actor, RuntimeState, bad_request, candidate_task_with_reader, error, repository_error,
    task_json, unix_now,
};
pub(crate) async fn lookup_tasks_by_thread(
    State(state): State<RuntimeState>,
    Path(project_id): Path<String>,
    Json(request): Json<ThreadLookupRequest>,
) -> Response {
    crate::repository_executor::run(state.clone(), move || {
        if !valid_lookup_ids(&request.thread_ids) {
            return error(
                StatusCode::BAD_REQUEST,
                "invalid_thread_ids",
                "Thread IDs are invalid",
            );
        }
        let (repository, project_id) = match open_project_repository(&state, project_id) {
            Ok(value) => value,
            Err(response) => return *response,
        };
        let tasks = match repository.tasks(&project_id) {
            Ok(value) => value,
            Err(source) => return repository_error(source),
        };
        let tasks = request
            .thread_ids
            .iter()
            .filter_map(|thread_id| tasks.iter().find(|task| &task.thread_id == thread_id))
            .map(task_lookup_json)
            .collect::<Vec<_>>();
        Json(json!({"tasks": tasks})).into_response()
    })
    .await
}

pub(crate) async fn lookup_recovery_tasks(
    State(state): State<RuntimeState>,
    Path(project_id): Path<String>,
    Json(request): Json<RecoveryLookupRequest>,
) -> Response {
    crate::repository_executor::run(state.clone(), move || {
        if !valid_lookup_ids(&request.task_ids) {
            return error(
                StatusCode::BAD_REQUEST,
                "invalid_task_ids",
                "Task IDs are invalid",
            );
        }
        let (repository, project_id) = match open_project_repository(&state, project_id) {
            Ok(value) => value,
            Err(response) => return *response,
        };
        let mut tasks = Vec::new();
        for task_id in request.task_ids {
            match repository.task(&project_id, &task_id) {
                Ok(Some(task)) => tasks.push(recovery_task_json(&task)),
                Ok(None) => {}
                Err(source) => return repository_error(source),
            }
        }
        Json(json!({"tasks": tasks})).into_response()
    })
    .await
}

pub(crate) fn valid_lookup_ids(ids: &[String]) -> bool {
    (1..=100).contains(&ids.len())
        && ids
            .iter()
            .all(|value| !value.trim().is_empty() && value.len() <= 160)
}

pub(crate) fn open_project_repository(
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

pub(crate) fn task_lookup_json(task: &TaskRecord) -> Value {
    json!({
        "id": task.task_id, "threadId": task.thread_id, "operation": task.operation,
        "purpose": task.purpose, "status": task.status,
        "updatedAt": null,
        "target": serde_json::from_str::<Value>(&task.target_json).unwrap_or_else(|_| json!({}))
    })
}

pub(crate) fn recovery_task_json(task: &TaskRecord) -> Value {
    let outputs = serde_json::from_str::<Vec<Value>>(&task.output_refs_json).unwrap_or_default();
    let mut kinds = outputs
        .iter()
        .filter_map(|value| value.get("kind").and_then(Value::as_str).map(str::to_owned))
        .collect::<Vec<_>>();
    kinds.sort();
    kinds.dedup();
    json!({
        "id": task.task_id, "threadId": task.thread_id, "operation": task.operation,
        "status": task.status, "deliveryStatus": task.delivery_status,
        "outputKinds": kinds, "updatedAt": null
    })
}

pub(crate) async fn recover_orphaned_tasks(State(state): State<RuntimeState>) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |mut repository| {
        match repository.recover_orphaned_tasks(unix_now()) {
            Ok(task_ids) => {
                Json(json!({"recovered": task_ids.len(), "taskIds": task_ids})).into_response()
            }
            Err(error) => repository_error(error),
        }
    })
    .await
}

pub(crate) async fn create_task(
    State(state): State<RuntimeState>,
    Path(project_id): Path<String>,
    Json(request): Json<CreateTaskHttpRequest>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |mut repository| {
        let project_id = match ProjectId::parse(project_id) {
            Ok(value) => value,
            Err(error) => return bad_request("invalid_project_id", error),
        };
        let now = unix_now();
        if request.thread_id.trim().is_empty()
            || request.author_intent.trim().is_empty()
                && !(request.operation == "advisory"
                    && request.target["type"] == "edition"
                    && request.target["imageAssetRevisionIds"]
                        .as_array()
                        .is_some_and(|ids| !ids.is_empty() && ids.len() <= 5))
            || request.author_intent.len() > 64_000
            || request.capability_hash.len() != 64
            || !request
                .capability_hash
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            || request.capability_expires_at <= now
            || request.capability_expires_at > now + 3600
        {
            return error(
                StatusCode::BAD_REQUEST,
                "invalid_task",
                "Task contract is invalid",
            );
        }
        let target = if request.target.is_object() {
            request.target
        } else {
            json!({})
        };
        if request.operation == "generateChapter" {
            let chapter_id = target.get("chapterId").and_then(Value::as_str);
            let chapter = match repository.chapters(&project_id) {
                Ok(chapters) => chapters
                    .into_iter()
                    .find(|chapter| Some(chapter.chapter_id.as_str()) == chapter_id),
                Err(error) => return repository_error(error),
            };
            if chapter.is_none()
                || chapter.as_ref().is_some_and(|chapter| {
                    target["chapterNumber"]
                        .as_i64()
                        .is_some_and(|number| number != chapter.number)
                        || target["readingChapter"]
                            .as_i64()
                            .is_some_and(|number| number != chapter.number)
                })
                || crate::context_visibility::writer_scene_limit(&target).is_err()
            {
                return error(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "invalid_task_target",
                    "Target chapter does not belong to the project",
                );
            }
        }
        if request.operation == "modelKnowledge" {
            let valid_batch = match target.get("batchId").and_then(Value::as_str) {
                Some(batch_id) => match repository.modeling_batch(&project_id, batch_id) {
                    Ok(batch) => {
                        batch.status == "queued"
                            && batch.input_project_revision == request.input_revision
                    }
                    Err(RepositoryError::CandidateNotFound) => false,
                    Err(source) => return repository_error(source),
                },
                None => false,
            };
            if !valid_batch {
                return error(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "invalid_task_target",
                    "Modeling batch is not current",
                );
            }
        }
        if request.operation == "assessSceneOutcome" {
            let valid = if let Some(chapter_id) = target["chapterId"].as_str() {
                match (
                    repository.chapter_knowledge(&project_id, chapter_id),
                    repository.chapter_bindings(&project_id),
                ) {
                    (Ok(knowledge), Ok(bindings)) => {
                        knowledge["status"] == "ready"
                            && knowledge["sourceRevisionId"] == target["draftRevisionId"]
                            && knowledge["batchId"] == target["knowledgeBatchId"]
                            && bindings.iter().any(|binding| {
                                binding.chapter_id == chapter_id
                                    && target["chapterNumber"].as_i64()
                                        == Some(binding.chapter_number)
                                    && target["planRevisionId"].as_str()
                                        == Some(binding.plan_revision_id.as_str())
                            })
                    }
                    (Err(source), _) | (_, Err(source)) => return repository_error(source),
                }
            } else {
                false
            };
            if !valid {
                return error(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "invalid_task_target",
                    "Scene assessment sources are not current",
                );
            }
        }
        let task_request = CreateTaskRequest::from_author_submission(AuthorTaskSubmission {
            project_id,
            thread_id: request.thread_id,
            operation: request.operation,
            input_revision: request.input_revision,
            model: request.model,
            target,
            author_intent: request.author_intent,
            experimental_methods: request.experimental_methods,
            capability_hash: request.capability_hash,
            capability_expires_at: request.capability_expires_at,
        });
        if state.engine_budget_database.is_none() {
            return bad_request(
                "engine_budget_unavailable",
                "engine budget database is not configured",
            );
        }
        let created = match request.initial_budget {
            Some(initial) => repository.create_budgeted_root_task(task_request, &initial, now),
            None => repository.create_funded_task(task_request, now),
        };
        match created {
            Ok(task) => (StatusCode::CREATED, Json(task_json(&task))).into_response(),
            Err(error) => repository_error(error),
        }
    })
    .await
}

pub(crate) async fn get_task(
    State(state): State<RuntimeState>,
    Path((project_id, task_id)): Path<(String, String)>,
) -> Response {
    crate::repository_executor::run(state.clone(), move || {
        let (repository, project_id) = match open_project_repository(&state, project_id) {
            Ok(value) => value,
            Err(response) => return *response,
        };
        match repository.task(&project_id, &task_id) {
            Ok(Some(task)) => Json(task_json(&task)).into_response(),
            Ok(None) => repository_error(RepositoryError::TaskNotFound),
            Err(error) => repository_error(error),
        }
    })
    .await
}

pub(crate) async fn update_task(
    State(state): State<RuntimeState>,
    Path((project_id, task_id)): Path<(String, String)>,
    Json(request): Json<UpdateTaskHttpRequest>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |mut repository| {
        let project_id = match ProjectId::parse(project_id) {
            Ok(value) => value,
            Err(error) => return bad_request("invalid_project_id", error),
        };

        match repository.update_task(
            &project_id,
            &task_id,
            &request.thread_id,
            TaskUpdate {
                status: request.status,
                error: request.error,
                result_summary: None,
                now: unix_now(),
            },
        ) {
            Ok(task) => Json(task_json(&task)).into_response(),
            Err(error) => repository_error(error),
        }
    })
    .await
}

pub(crate) async fn cancel_task(
    State(state): State<RuntimeState>,
    Path((project_id, task_id)): Path<(String, String)>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |mut repository| {
        let project_id = match ProjectId::parse(project_id) {
            Ok(value) => value,
            Err(error) => return bad_request("invalid_project_id", error),
        };

        let task = match repository.task(&project_id, &task_id) {
            Ok(Some(value)) => value,
            Ok(None) => return error(StatusCode::NOT_FOUND, "task_not_found", "Task not found"),
            Err(error) => return repository_error(error),
        };
        match repository.update_task(
            &project_id,
            &task_id,
            &task.thread_id,
            TaskUpdate {
                status: "cancelled".into(),
                error: None,
                result_summary: None,
                now: unix_now(),
            },
        ) {
            Ok(task) => Json(task_json(&task)).into_response(),
            Err(error) => repository_error(error),
        }
    })
    .await
}

pub(crate) async fn bind_task_turn(
    State(state): State<RuntimeState>,
    Path((project_id, task_id)): Path<(String, String)>,
    Json(request): Json<BindTaskTurnRequest>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |mut repository| {
        let project_id = match ProjectId::parse(project_id) {
            Ok(value) => value,
            Err(source) => return bad_request("invalid_project_id", source),
        };

        match repository.bind_task_turn(&project_id, &task_id, &request.thread_id, &request.turn_id)
        {
            Ok(task) => Json(task_json(&task)).into_response(),
            Err(source) => repository_error(source),
        }
    })
    .await
}

pub(crate) async fn complete_task(
    State(state): State<RuntimeState>,
    Path((project_id, task_id)): Path<(String, String)>,
    Extension(actor): Extension<Actor>,
    headers: HeaderMap,
    Json(request): Json<CompleteTaskHttpRequest>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |mut repository| {
        let project_id = match ProjectId::parse(project_id) {
            Ok(value) => value,
            Err(error) => return bad_request("invalid_project_id", error),
        };

        let task = match candidate_task_with_reader(
            &repository,
            actor,
            &headers,
            &project_id,
            Some(&task_id),
            Some(&request.thread_id),
            None,
        ) {
            Ok(task) => task,
            Err(response) => return *response,
        };
        if let Err(source) = crate::recovery_binding::bind(
            &mut repository,
            &headers,
            task.as_ref(),
            "complete_task",
            None,
        ) {
            return repository_error(source);
        }
        match repository.complete_task(
            &project_id,
            &task_id,
            &request.thread_id,
            request.summary.chars().take(4_000).collect(),
            request.delivery_review,
            unix_now(),
        ) {
            Ok(task) => Json(task_json(&task)).into_response(),
            Err(error) => repository_error(error),
        }
    })
    .await
}

/// Author/Host only; authentication is applied by server composition.
pub(crate) fn routes() -> axum::Router<crate::server::RuntimeState> {
    axum::Router::new()
        .route(
            "/api/tasks/recover-orphans",
            axum::routing::post(crate::task_lifecycle_api::recover_orphaned_tasks),
        )
        .route(
            "/api/projects/{project_id}/tasks",
            axum::routing::post(crate::task_lifecycle_api::create_task),
        )
        .route(
            "/api/projects/{project_id}/tasks/lookup",
            axum::routing::post(crate::task_lifecycle_api::lookup_tasks_by_thread),
        )
        .route(
            "/api/projects/{project_id}/tasks/recovery-lookup",
            axum::routing::post(crate::task_lifecycle_api::lookup_recovery_tasks),
        )
        .route(
            "/api/projects/{project_id}/tasks/{task_id}",
            axum::routing::get(crate::task_lifecycle_api::get_task)
                .patch(crate::task_lifecycle_api::update_task),
        )
        .route(
            "/api/projects/{project_id}/tasks/{task_id}/cancel",
            axum::routing::post(crate::task_lifecycle_api::cancel_task),
        )
        .route(
            "/api/projects/{project_id}/tasks/{task_id}/turn",
            axum::routing::post(crate::task_lifecycle_api::bind_task_turn),
        )
}

/// Author or candidate; task capabilities remain checked by the handler.
pub(crate) fn writer_routes() -> axum::Router<crate::server::RuntimeState> {
    axum::Router::new().route(
        "/api/projects/{project_id}/tasks/{task_id}/complete",
        axum::routing::post(crate::task_lifecycle_api::complete_task),
    )
}

use novel_codex_story_core::{ChaptersRepository, InitialWorkBudgetRepository, ModelingRepository, ProjectRevisionRepository, TaskProviderBudgetRepository, TasksRepository};
