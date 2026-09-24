use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use axum::Extension;
use axum::Json;
use axum::Router;
use axum::extract::Path;
use axum::extract::Query;
use axum::extract::Request;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::middleware;
use axum::middleware::Next;
use axum::response::IntoResponse;
use axum::response::Response;
use axum::routing::get;
use novel_codex_story_core::AcceptReviewRequest;
use novel_codex_story_core::BranchId;
use novel_codex_story_core::ChapterProjection;
use novel_codex_story_core::CommitId;
use novel_codex_story_core::CreateReviewRequest;
use novel_codex_story_core::CreateTaskRequest;
use novel_codex_story_core::IdempotencyKey;
use novel_codex_story_core::ObjectId;
use novel_codex_story_core::ProjectId;
use novel_codex_story_core::ProjectRecord;
use novel_codex_story_core::RepositoryError;
use novel_codex_story_core::RevisionId;
use novel_codex_story_core::SaveCandidateRequest;
use novel_codex_story_core::SnapshotId;
use novel_codex_story_core::SqliteStoryRepository;
use novel_codex_story_core::TaskOutputBinding;
use novel_codex_story_core::TaskRecord;
use novel_codex_story_core::TaskUpdate;
use novel_codex_story_core::UndoAcceptanceRequest;
use novel_codex_story_core::WorkingBaseline;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use serde_json::json;
use sha2::Digest;
use sha2::Sha256;
use uuid::Uuid;

#[derive(Clone)]
pub struct RuntimeState {
    pub(crate) database_path: PathBuf,
    author_token: Arc<str>,
    candidate_token: Arc<str>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Actor {
    Author,
    Candidate,
}

impl RuntimeState {
    pub fn new(database_path: PathBuf, author_token: String, candidate_token: String) -> Self {
        Self {
            database_path,
            author_token: author_token.into(),
            candidate_token: candidate_token.into(),
        }
    }
}

#[derive(Deserialize)]
struct CreateProjectRequest {
    title: String,
    #[serde(default)]
    logline: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveArtifactRequest {
    payload: Value,
    expected_revision: i64,
    expected_candidate_revision_id: Option<String>,
    idempotency_key: String,
    #[serde(default)]
    linked_impact_ids: Vec<String>,
    task_id: Option<String>,
    thread_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateReviewHttpRequest {
    artifact_kind: String,
    revision_id: String,
    #[serde(default)]
    proposals: Vec<Value>,
    #[serde(default)]
    selections: Value,
    idempotency_key: String,
    task_id: Option<String>,
    thread_id: Option<String>,
    expected_revision: Option<i64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AcceptReviewHttpRequest {
    expected_revision: i64,
    idempotency_key: String,
    #[serde(default)]
    selections: Value,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UndoHttpRequest {
    expected_revision: i64,
    idempotency_key: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LockArtifactRequest {
    locked: bool,
    expected_revision: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateTaskHttpRequest {
    operation: String,
    input_revision: i64,
    thread_id: String,
    model: Option<String>,
    #[serde(default)]
    target: Value,
    capability_hash: String,
    capability_expires_at: i64,
    author_intent: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateTaskHttpRequest {
    thread_id: String,
    status: String,
    error: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CompleteTaskHttpRequest {
    thread_id: String,
    #[serde(default)]
    summary: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThreadLookupRequest {
    thread_ids: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RecoveryLookupRequest {
    task_ids: Vec<String>,
    fence: Option<String>,
    #[serde(default)]
    cursor: usize,
    #[serde(default = "default_recovery_limit")]
    limit: usize,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ContextQuery {
    task_id: Option<String>,
    thread_id: Option<String>,
    project_revision: i64,
    purpose: String,
    story_time: Option<i64>,
    reading_chapter: Option<i64>,
    scene_order: Option<i64>,
    pov_character_id: Option<String>,
    #[serde(default = "default_context_budget")]
    budget: usize,
    #[serde(default = "default_context_byte_budget")]
    byte_budget: usize,
}

#[derive(Serialize)]
struct ProjectResponse {
    id: String,
    title: String,
    logline: String,
    created_at: i64,
    updated_at: i64,
}

pub fn router(state: RuntimeState) -> Router {
    let author = Router::new()
        .route("/api/metrics", get(crate::read_api::metrics))
        .route(
            "/api/metrics/abandon",
            axum::routing::post(crate::read_api::record_abandonment),
        )
        .route(
            "/api/tasks/recover-orphans",
            axum::routing::post(recover_orphaned_tasks),
        )
        .route("/api/projects", get(list_projects).post(create_project))
        .route(
            "/api/projects/{project_id}/creative-workspace",
            get(workspace_state),
        )
        .route(
            "/api/projects/{project_id}/state",
            get(crate::read_api::project_state),
        )
        .route(
            "/api/projects/{project_id}/search",
            get(crate::read_api::search),
        )
        .route(
            "/api/projects/{project_id}/story-tests",
            axum::routing::post(crate::read_api::story_tests),
        )
        .route(
            "/api/projects/{project_id}/chapters/{chapter_id}/diffs",
            get(crate::read_api::chapter_diffs),
        )
        .route(
            "/api/projects/{project_id}/export",
            axum::routing::post(crate::read_api::export_project),
        )
        .route(
            "/api/projects/{project_id}/impacts",
            get(crate::impact_api::list_impacts),
        )
        .route(
            "/api/projects/{project_id}/impacts/{impact_id}/resolve",
            axum::routing::post(crate::impact_api::resolve_impact),
        )
        .route(
            "/api/projects/{project_id}/planning-reviews",
            axum::routing::post(crate::planning_review_api::start),
        )
        .route(
            "/api/projects/{project_id}/planning-reviews/{run_id}/seats/{role}/tasks",
            axum::routing::post(crate::planning_review_api::create_seat_task),
        )
        .route(
            "/api/projects/{project_id}/planning-reviews/{run_id}/finalize",
            axum::routing::post(crate::planning_review_api::finalize),
        )
        .route(
            "/api/projects/{project_id}/planning-reviews/{run_id}/cancel",
            axum::routing::post(crate::planning_review_api::cancel),
        )
        .route(
            "/api/projects/{project_id}/reviews/{review_id}/accept",
            axum::routing::post(accept_review),
        )
        .route(
            "/api/projects/{project_id}/artifacts/{kind}/lock",
            axum::routing::patch(set_artifact_lock),
        )
        .route(
            "/api/projects/{project_id}/working-baselines/{kind}",
            axum::routing::put(crate::selection_api::set_baseline)
                .delete(crate::selection_api::clear_baseline),
        )
        .route(
            "/api/projects/{project_id}/continuations/{kind}",
            axum::routing::put(crate::selection_api::save_continuation),
        )
        .route(
            "/api/projects/{project_id}/chapters/materialize-from-plan",
            axum::routing::post(crate::chapter_api::materialize_from_plan),
        )
        .route(
            "/api/projects/{project_id}/chapters/materialize-standalone",
            axum::routing::post(crate::chapter_api::materialize_standalone),
        )
        .route(
            "/api/projects/{project_id}/chapters/associate-plan",
            axum::routing::post(crate::chapter_api::associate_plan),
        )
        .route(
            "/api/projects/{project_id}/commits/{commit_id}/undo",
            axum::routing::post(undo_commit),
        )
        .route(
            "/api/projects/{project_id}/tasks",
            axum::routing::post(create_task),
        )
        .route(
            "/api/projects/{project_id}/tasks/lookup",
            axum::routing::post(lookup_tasks_by_thread),
        )
        .route(
            "/api/projects/{project_id}/tasks/recovery-lookup",
            axum::routing::post(lookup_recovery_tasks),
        )
        .route(
            "/api/projects/{project_id}/tasks/recovery-operations",
            axum::routing::post(recovery_operations),
        )
        .route(
            "/api/projects/{project_id}/tasks/recovery-snapshot",
            axum::routing::post(recovery_snapshot),
        )
        .route(
            "/api/projects/{project_id}/tasks/{task_id}",
            axum::routing::patch(update_task),
        )
        .route(
            "/api/projects/{project_id}/tasks/{task_id}/cancel",
            axum::routing::post(cancel_task),
        )
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            require_author,
        ));
    let writer = Router::new()
        .route(
            "/api/projects/{project_id}/artifacts/{kind}",
            axum::routing::post(save_artifact),
        )
        .route(
            "/api/projects/{project_id}/drafts/checkpoints",
            axum::routing::post(save_draft_checkpoint),
        )
        .route(
            "/api/projects/{project_id}/reviews",
            axum::routing::post(create_review),
        )
        .route(
            "/api/projects/{project_id}/planning-reviews/{run_id}/reports",
            axum::routing::post(crate::planning_review_api::submit_report),
        )
        .route("/api/projects/{project_id}/context", get(read_context))
        .route(
            "/api/projects/{project_id}/tasks/{task_id}/complete",
            axum::routing::post(complete_task),
        )
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            require_writer,
        ));
    Router::new()
        .route("/health", get(health))
        .merge(author)
        .merge(writer)
        .with_state(state)
}

async fn set_artifact_lock(
    State(state): State<RuntimeState>,
    Path((project_id, kind)): Path<(String, String)>,
    Json(request): Json<LockArtifactRequest>,
) -> Response {
    if !matches!(kind.as_str(), "brief" | "outline" | "plan") {
        return error(
            StatusCode::BAD_REQUEST,
            "invalid_artifact_kind",
            "Artifact kind is invalid",
        );
    }
    let project_id = match ProjectId::parse(project_id) {
        Ok(value) => value,
        Err(source) => return bad_request("invalid_project_id", source),
    };
    let mut repository = match SqliteStoryRepository::open(&state.database_path) {
        Ok(value) => value,
        Err(source) => return internal_error(source),
    };
    match repository.set_object_lock(
        &project_id,
        &ObjectId::parse(format!("artifact-{kind}")).expect("artifact object id"),
        request.locked,
        request.expected_revision,
    ) {
        Ok(result_revision) => Json(json!({
            "artifactKind": kind, "locked": request.locked, "resultRevision": result_revision
        }))
        .into_response(),
        Err(source) => repository_error(source),
    }
}

fn default_context_budget() -> usize {
    24
}

fn default_context_byte_budget() -> usize {
    24_000
}

fn default_recovery_limit() -> usize {
    500
}

async fn lookup_tasks_by_thread(
    State(state): State<RuntimeState>,
    Path(project_id): Path<String>,
    Json(request): Json<ThreadLookupRequest>,
) -> Response {
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
}

async fn lookup_recovery_tasks(
    State(state): State<RuntimeState>,
    Path(project_id): Path<String>,
    Json(request): Json<RecoveryLookupRequest>,
) -> Response {
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
}

async fn recovery_operations(
    State(state): State<RuntimeState>,
    Path(project_id): Path<String>,
    Json(request): Json<RecoveryLookupRequest>,
) -> Response {
    if !valid_lookup_ids(&request.task_ids) {
        return error(
            StatusCode::BAD_REQUEST,
            "invalid_task_ids",
            "Task IDs are invalid",
        );
    }
    if let Err(response) = open_project_repository(&state, project_id) {
        return *response;
    }
    Json(json!({"operations": []})).into_response()
}

async fn recovery_snapshot(
    State(state): State<RuntimeState>,
    Path(project_id): Path<String>,
    Json(request): Json<RecoveryLookupRequest>,
) -> Response {
    if !valid_lookup_ids(&request.task_ids) || request.limit == 0 || request.limit > 500 {
        return error(
            StatusCode::BAD_REQUEST,
            "invalid_recovery_snapshot",
            "Recovery snapshot is invalid",
        );
    }
    let (repository, project_id) = match open_project_repository(&state, project_id) {
        Ok(value) => value,
        Err(response) => return *response,
    };
    let mut tasks = Vec::new();
    for task_id in &request.task_ids {
        match repository.task(&project_id, task_id) {
            Ok(Some(task)) => tasks.push(recovery_task_json(&task)),
            Ok(None) => {}
            Err(source) => return repository_error(source),
        }
    }
    let fence = request
        .fence
        .unwrap_or_else(|| format!("recovery-{}", Uuid::new_v4().simple()));
    Json(json!({
        "fence": fence, "tasks": tasks, "operations": [],
        "cursor": request.cursor, "nextCursor": null, "complete": true
    }))
    .into_response()
}

fn valid_lookup_ids(ids: &[String]) -> bool {
    (1..=100).contains(&ids.len())
        && ids
            .iter()
            .all(|value| !value.trim().is_empty() && value.len() <= 160)
}

fn open_project_repository(
    state: &RuntimeState,
    project_id: String,
) -> Result<(SqliteStoryRepository, ProjectId), Box<Response>> {
    let project_id = ProjectId::parse(project_id)
        .map_err(|source| Box::new(bad_request("invalid_project_id", source)))?;
    let repository = SqliteStoryRepository::open(&state.database_path)
        .map_err(|source| Box::new(repository_error(source)))?;
    Ok((repository, project_id))
}

fn task_lookup_json(task: &TaskRecord) -> Value {
    json!({
        "id": task.task_id, "threadId": task.thread_id, "operation": task.operation,
        "purpose": task.purpose, "status": task.status,
        "updatedAt": null,
        "target": serde_json::from_str::<Value>(&task.target_json).unwrap_or_else(|_| json!({}))
    })
}

fn recovery_task_json(task: &TaskRecord) -> Value {
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

async fn read_context(
    State(state): State<RuntimeState>,
    Path(project_id): Path<String>,
    Extension(actor): Extension<Actor>,
    headers: HeaderMap,
    Query(query): Query<ContextQuery>,
) -> Response {
    if !matches!(query.purpose.as_str(), "planner" | "writer" | "critic")
        || query.budget == 0
        || query.budget > 100_000
        || !(4_000..=60_000).contains(&query.byte_budget)
        || (matches!(query.purpose.as_str(), "writer" | "critic")
            && (query.reading_chapter.is_none() || query.scene_order.is_none()))
    {
        return error(
            StatusCode::BAD_REQUEST,
            "invalid_context_request",
            "Context scope or budget is invalid",
        );
    }
    let project_id = match ProjectId::parse(project_id) {
        Ok(value) => value,
        Err(source) => return bad_request("invalid_project_id", source),
    };
    let repository = match SqliteStoryRepository::open(&state.database_path) {
        Ok(value) => value,
        Err(source) => return internal_error(source),
    };
    let task = match candidate_task(
        &repository,
        actor,
        &headers,
        &project_id,
        query.task_id.as_deref(),
        query.thread_id.as_deref(),
        Some(query.project_revision),
    ) {
        Ok(task) => task,
        Err(response) => return *response,
    };
    if task
        .as_ref()
        .is_some_and(|task| task.purpose != query.purpose)
    {
        return error(
            StatusCode::UNAUTHORIZED,
            "task_scope_invalid",
            "Task does not permit this context purpose",
        );
    }
    let revision = match repository.project_revision(&project_id) {
        Ok(value) => value,
        Err(source) => return repository_error(source),
    };
    if revision != query.project_revision {
        return error(
            StatusCode::CONFLICT,
            "context_revision_mismatch",
            "Context must be read from the current project revision",
        );
    }
    let project = match repository.project(&project_id) {
        Ok(Some(value)) => value,
        Ok(None) => {
            return error(
                StatusCode::NOT_FOUND,
                "project_not_found",
                "Project not found",
            );
        }
        Err(source) => return repository_error(source),
    };
    let heads = match repository.object_heads(&project_id) {
        Ok(value) => value,
        Err(source) => return repository_error(source),
    };
    let mut artifacts = serde_json::Map::new();
    for kind in ["brief", "outline", "plan"] {
        let object_id = format!("artifact-{kind}");
        let head = heads
            .iter()
            .find(|head| head.object_id.as_str() == object_id);
        let selected = head.and_then(|head| {
            head.accepted_revision_id
                .as_ref()
                .or(head.candidate_revision_id.as_ref())
        });
        let value = selected
            .and_then(|revision_id| repository.revision_content(revision_id).ok().flatten())
            .and_then(|content| serde_json::from_str::<Value>(&content).ok())
            .map(|value| value.get("payload").cloned().unwrap_or(value))
            .unwrap_or(Value::Null);
        artifacts.insert(kind.into(), value);
    }
    let chapters = match repository.chapters(&project_id) {
        Ok(chapters) => chapters,
        Err(source) => return repository_error(source),
    };
    let mut used_bytes = 0;
    let mut draft_context = Vec::new();
    for chapter in chapters.into_iter().take(query.budget.min(80)) {
        let Some(head) = heads
            .iter()
            .find(|head| head.object_id.as_str() == format!("draft-{}", chapter.chapter_id))
        else {
            continue;
        };
        let Some(revision_id) = head
            .accepted_revision_id
            .as_ref()
            .or(head.candidate_revision_id.as_ref())
        else {
            continue;
        };
        let Some(content) = repository.revision_content(revision_id).ok().flatten() else {
            continue;
        };
        if used_bytes + content.len() > query.byte_budget.saturating_sub(2_000) {
            break;
        }
        used_bytes += content.len();
        if let Ok(value) = serde_json::from_str::<Value>(&content) {
            draft_context.push(value);
        }
    }
    let facts = match repository.facts(&project_id) {
        Ok(value) => value,
        Err(source) => return repository_error(source),
    };
    let planning_review = match task.as_ref() {
        Some(task) if task.operation == "reviewPlanningSeat" => {
            match repository.planning_review_context(&project_id, &task.task_id) {
                Ok(value) => value,
                Err(source) => return repository_error(source),
            }
        }
        _ => None,
    };
    Json(json!({
        "projectId": project_id.as_str(),
        "projectRevision": revision,
        "purpose": query.purpose,
        "position": {
            "storyTime": query.story_time,
            "readingChapter": query.reading_chapter,
            "sceneOrder": query.scene_order,
            "povCharacterId": query.pov_character_id
        },
        "project": {"title": project.title, "logline": project.logline},
        "artifacts": artifacts,
        "drafts": draft_context,
        "facts": facts,
        "planningReview": planning_review,
        "coverage": {"complete": false, "reason": "Structured fact modeling is not yet available"},
        "inputSnapshot": {"replayed": false, "byteSize": used_bytes}
    }))
    .into_response()
}

async fn workspace_state(
    State(state): State<RuntimeState>,
    Path(project_id): Path<String>,
) -> Response {
    let project_id = match ProjectId::parse(project_id) {
        Ok(project_id) => project_id,
        Err(error) => return bad_request("invalid_project_id", error),
    };
    let repository = match SqliteStoryRepository::open(&state.database_path) {
        Ok(repository) => repository,
        Err(error) => return internal_error(error),
    };
    let Some(project) = (match repository.project(&project_id) {
        Ok(project) => project,
        Err(error) => return repository_error(error),
    }) else {
        return error(
            StatusCode::NOT_FOUND,
            "project_not_found",
            "Project not found",
        );
    };
    let revision = match repository.project_revision(&project_id) {
        Ok(revision) => revision,
        Err(error) => return repository_error(error),
    };
    let baselines = match repository.working_baselines(&project_id) {
        Ok(value) => value,
        Err(error) => return repository_error(error),
    };
    let continuations = match repository.continuation_intents(&project_id) {
        Ok(value) => value,
        Err(error) => return repository_error(error),
    };
    let chapter_bindings = match repository.chapter_bindings(&project_id) {
        Ok(value) => value,
        Err(error) => return repository_error(error),
    };
    let heads = match repository.object_heads(&project_id) {
        Ok(heads) => heads,
        Err(error) => return repository_error(error),
    };
    let chapters = match repository.chapters(&project_id) {
        Ok(chapters) => {
            let mut projected = Vec::with_capacity(chapters.len());
            for chapter in chapters {
                let object_id = format!("draft-{}", chapter.chapter_id);
                let head = heads
                    .iter()
                    .find(|head| head.object_id.as_str() == object_id);
                let candidate = match head.and_then(|head| head.candidate_revision_id.as_ref()) {
                    Some(revision_id) => match draft_revision(&repository, revision_id) {
                        Ok(revision) => revision,
                        Err(error) => return repository_error(error),
                    },
                    None => Value::Null,
                };
                let accepted = match head.and_then(|head| head.accepted_revision_id.as_ref()) {
                    Some(revision_id) => match draft_revision(&repository, revision_id) {
                        Ok(revision) => revision,
                        Err(error) => return repository_error(error),
                    },
                    None => Value::Null,
                };
                let selection = baselines.iter().find(|selection| {
                    selection.object_kind == "draft" && selection.object_id == chapter.chapter_id
                });
                let working = match selection.and_then(|selection| selection.revision_id.as_ref()) {
                    Some(revision_id) => match draft_revision(&repository, revision_id) {
                        Ok(revision) => revision,
                        Err(error) => return repository_error(error),
                    },
                    None => Value::Null,
                };
                projected.push(json!({
                    "id": chapter.chapter_id,
                    "number": chapter.number,
                    "title": chapter.title,
                    "status": chapter.status,
                    "summary": chapter.summary,
                    "updated_at": null,
                    "candidate": candidate,
                    "accepted": accepted,
                    "working": working,
                    "workingSelection": selection.map(working_baseline_json)
                }));
            }
            projected
        }
        Err(error) => return repository_error(error),
    };
    let review_records = match repository.reviews(&project_id) {
        Ok(value) => value,
        Err(error) => return repository_error(error),
    };
    let fact_proposals = review_records
        .iter()
        .filter(|review| review.status == "ready")
        .flat_map(|review| {
            serde_json::from_str::<Vec<Value>>(&review.proposals_json)
                .unwrap_or_default()
                .into_iter()
                .map(|mut proposal| {
                    proposal["draft_revision_id"] = json!(review.revision_id.as_str());
                    proposal
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let reviews = review_records
            .into_iter()
            .map(|review| {
                json!({
                    "id": review.review_id,
                    "artifactKind": review.object_kind,
                    "artifactRevisionId": review.revision_id.as_str(),
                    "projectRevision": review.project_revision,
                    "checks": serde_json::from_str::<Value>(&review.checks_json).unwrap_or_else(|_| json!([])),
                    "selections": serde_json::from_str::<Value>(&review.selections_json).unwrap_or_else(|_| json!({})),
                    "status": review.status
                })
            })
            .collect::<Vec<_>>();
    let commits = match repository.acceptances(&project_id) {
        Ok(commits) => commits
            .into_iter()
            .map(|commit| {
                json!({
                    "id": commit.commit_id.as_str(),
                    "operation_kind": commit.operation,
                    "base_revision": commit.base_revision,
                    "result_revision": commit.result_revision,
                    "receipt": {
                        "commitId": commit.commit_id.as_str(),
                        "artifactKind": commit.object_kind,
                        "artifactRevisionId": commit.accepted_revision_id.as_str(),
                        "previousAcceptedRevisionId": commit.previous_accepted_revision_id.as_ref().map(RevisionId::as_str),
                        "baseRevision": commit.base_revision,
                        "resultRevision": commit.result_revision
                    }
                })
            })
            .collect::<Vec<_>>(),
        Err(error) => return repository_error(error),
    };
    let tasks = match repository.tasks(&project_id) {
        Ok(tasks) => tasks.iter().map(task_json).collect::<Vec<_>>(),
        Err(error) => return repository_error(error),
    };
    let mut artifacts = serde_json::Map::new();
    for kind in ["brief", "outline", "plan"] {
        let object_id = format!("artifact-{kind}");
        let head = heads
            .iter()
            .find(|head| head.object_id.as_str() == object_id);
        let candidate = match head.and_then(|head| head.candidate_revision_id.as_ref()) {
            Some(revision_id) => match artifact_revision(&repository, revision_id) {
                Ok(revision) => revision,
                Err(error) => return repository_error(error),
            },
            None => Value::Null,
        };
        let accepted = match head.and_then(|head| head.accepted_revision_id.as_ref()) {
            Some(revision_id) => match artifact_revision(&repository, revision_id) {
                Ok(revision) => revision,
                Err(error) => return repository_error(error),
            },
            None => Value::Null,
        };
        let selection = baselines
            .iter()
            .find(|selection| selection.object_kind == kind && selection.object_id == kind);
        let working = match selection.and_then(|selection| selection.revision_id.as_ref()) {
            Some(revision_id) => match artifact_revision(&repository, revision_id) {
                Ok(revision) => revision,
                Err(error) => return repository_error(error),
            },
            None => Value::Null,
        };
        artifacts.insert(
            kind.into(),
            json!({
                "candidate": candidate,
                "accepted": accepted,
                "working": working,
                "locked": head.is_some_and(|head| head.locked),
                "workingSelection": selection.map(working_baseline_json)
            }),
        );
    }
    let impacts = match repository.planning_impacts(&project_id, 0, 50) {
        Ok(value) => value,
        Err(error) => return repository_error(error),
    };
    let planning_reviews = match repository.planning_reviews(&project_id) {
        Ok(value) => value,
        Err(error) => return repository_error(error),
    };
    Json(json!({
        "project": ProjectResponse::from(project),
        "revision": revision,
        "planWindow": 3,
        "planLineageStatus": "missing",
        "index": {"targetRevision": revision, "indexedRevision": revision, "status": "ready"},
        "artifacts": artifacts,
        "chapters": chapters,
        "factProposals": fact_proposals,
        "reviews": reviews,
        "impacts": impacts.items,
        "openImpactCount": impacts.open,
        "chapterPlanBindings": chapter_bindings.iter().map(|binding| json!({
            "project_id": project_id.as_str(), "plan_revision_id": binding.plan_revision_id,
            "chapter_plan_id": binding.chapter_plan_id, "chapter_id": binding.chapter_id,
            "lineageStatus": "current", "writerProjection": null
        })).collect::<Vec<_>>(),
        "commits": commits,
        "tasks": tasks,
        "workingBaselines": baselines.iter().map(working_baseline_json).collect::<Vec<_>>(),
        "continuationIntents": continuations.iter().map(|intent| json!({
            "targetKind": intent.target_kind, "targetId": intent.target_id,
            "workingRevisionId": intent.working_revision_id.as_str(),
            "authorIntent": intent.author_intent, "status": intent.status,
            "taskId": intent.task_id, "updatedAt": intent.updated_at.to_string()
        })).collect::<Vec<_>>(),
        "planningReviews": planning_reviews
    }))
    .into_response()
}

async fn recover_orphaned_tasks(State(state): State<RuntimeState>) -> Response {
    let mut repository = match SqliteStoryRepository::open(&state.database_path) {
        Ok(value) => value,
        Err(error) => return internal_error(error),
    };
    match repository.recover_orphaned_tasks(unix_now()) {
        Ok(task_ids) => {
            Json(json!({"recovered": task_ids.len(), "taskIds": task_ids})).into_response()
        }
        Err(error) => repository_error(error),
    }
}

async fn create_task(
    State(state): State<RuntimeState>,
    Path(project_id): Path<String>,
    Json(request): Json<CreateTaskHttpRequest>,
) -> Response {
    let project_id = match ProjectId::parse(project_id) {
        Ok(value) => value,
        Err(error) => return bad_request("invalid_project_id", error),
    };
    let now = unix_now();
    if request.thread_id.trim().is_empty()
        || request.author_intent.trim().is_empty()
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
    let task_id = format!("task-{}", Uuid::new_v4().simple());
    let manifest = json!({
        "operation": request.operation,
        "projectId": project_id.as_str(),
        "projectRevision": request.input_revision,
        "target": target,
        "authorIntent": request.author_intent,
        "expectedCandidateRevisionId": null,
        "branchContext": null
    });
    let mut repository = match SqliteStoryRepository::open(&state.database_path) {
        Ok(value) => value,
        Err(error) => return internal_error(error),
    };
    if request.operation == "generateChapter" {
        let chapter_id = target.get("chapterId").and_then(Value::as_str);
        let chapter_exists = match repository.chapters(&project_id) {
            Ok(chapters) => chapters
                .iter()
                .any(|chapter| Some(chapter.chapter_id.as_str()) == chapter_id),
            Err(error) => return repository_error(error),
        };
        if !chapter_exists {
            return error(
                StatusCode::UNPROCESSABLE_ENTITY,
                "invalid_task_target",
                "Target chapter does not belong to the project",
            );
        }
    }
    match repository.create_task(CreateTaskRequest {
        task_id,
        project_id,
        thread_id: request.thread_id,
        operation: request.operation,
        input_revision: request.input_revision,
        model: request.model,
        target_json: target.to_string(),
        input_manifest_json: manifest.to_string(),
        capability_hash: request.capability_hash,
        capability_expires_at: request.capability_expires_at,
    }) {
        Ok(task) => (StatusCode::CREATED, Json(task_json(&task))).into_response(),
        Err(error) => repository_error(error),
    }
}

async fn update_task(
    State(state): State<RuntimeState>,
    Path((project_id, task_id)): Path<(String, String)>,
    Json(request): Json<UpdateTaskHttpRequest>,
) -> Response {
    let project_id = match ProjectId::parse(project_id) {
        Ok(value) => value,
        Err(error) => return bad_request("invalid_project_id", error),
    };
    let mut repository = match SqliteStoryRepository::open(&state.database_path) {
        Ok(value) => value,
        Err(error) => return internal_error(error),
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
}

async fn cancel_task(
    State(state): State<RuntimeState>,
    Path((project_id, task_id)): Path<(String, String)>,
) -> Response {
    let project_id = match ProjectId::parse(project_id) {
        Ok(value) => value,
        Err(error) => return bad_request("invalid_project_id", error),
    };
    let mut repository = match SqliteStoryRepository::open(&state.database_path) {
        Ok(value) => value,
        Err(error) => return internal_error(error),
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
}

async fn complete_task(
    State(state): State<RuntimeState>,
    Path((project_id, task_id)): Path<(String, String)>,
    Extension(actor): Extension<Actor>,
    headers: HeaderMap,
    Json(request): Json<CompleteTaskHttpRequest>,
) -> Response {
    let project_id = match ProjectId::parse(project_id) {
        Ok(value) => value,
        Err(error) => return bad_request("invalid_project_id", error),
    };
    let mut repository = match SqliteStoryRepository::open(&state.database_path) {
        Ok(value) => value,
        Err(error) => return internal_error(error),
    };
    if let Err(response) = candidate_task(
        &repository,
        actor,
        &headers,
        &project_id,
        Some(&task_id),
        Some(&request.thread_id),
        None,
    ) {
        return *response;
    }
    match repository.complete_task(
        &project_id,
        &task_id,
        &request.thread_id,
        request.summary.chars().take(4_000).collect(),
        unix_now(),
    ) {
        Ok(task) => Json(task_json(&task)).into_response(),
        Err(error) => repository_error(error),
    }
}

async fn save_draft_checkpoint(
    State(state): State<RuntimeState>,
    Path(project_id): Path<String>,
    Extension(actor): Extension<Actor>,
    headers: HeaderMap,
    Json(request): Json<SaveArtifactRequest>,
) -> Response {
    let Some(payload) = request.payload.as_object() else {
        return error(
            StatusCode::BAD_REQUEST,
            "invalid_draft",
            "Draft payload must be an object",
        );
    };
    let body = payload
        .get("body")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if body.chars().count() > 120_000 {
        return error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "draft_too_large",
            "Draft exceeds 120000 characters",
        );
    }
    let chapter_number = payload
        .get("chapterNumber")
        .and_then(Value::as_i64)
        .unwrap_or(1);
    let chapter_title = payload
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("未命名章节")
        .trim();
    let chapter_id = payload
        .get("chapterId")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| format!("chapter-{}", Uuid::new_v4().simple()));
    if chapter_number < 1 || chapter_title.chars().count() > 160 {
        return error(
            StatusCode::BAD_REQUEST,
            "invalid_chapter",
            "Chapter metadata is invalid",
        );
    }
    let mut scene_groups = payload
        .get("sceneGroups")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if scene_groups.len() > 100 {
        return error(
            StatusCode::BAD_REQUEST,
            "invalid_scene_groups",
            "Too many scene groups",
        );
    }
    if scene_groups.is_empty() {
        scene_groups.push(json!({
            "id": format!("scene-group-{chapter_id}"),
            "sceneId": null,
            "body": body,
            "locked": false
        }));
    }
    if scene_groups
        .iter()
        .any(|group| group.get("body").and_then(Value::as_str).is_none())
    {
        return error(
            StatusCode::BAD_REQUEST,
            "invalid_scene_group",
            "Scene groups are invalid",
        );
    }
    let grouped_body = scene_groups
        .iter()
        .filter_map(|group| group.get("body").and_then(Value::as_str))
        .map(str::trim)
        .collect::<Vec<_>>()
        .join("\n\n---\n\n");
    if grouped_body != body.trim() {
        return error(
            StatusCode::BAD_REQUEST,
            "draft_scene_groups_mismatch",
            "Draft body and scene groups differ",
        );
    }
    let project_id = match ProjectId::parse(project_id) {
        Ok(value) => value,
        Err(error) => return bad_request("invalid_project_id", error),
    };
    let expected_candidate = match request.expected_candidate_revision_id {
        Some(value) => match RevisionId::parse(value) {
            Ok(value) => Some(value),
            Err(error) => return bad_request("invalid_revision_id", error),
        },
        None => None,
    };
    let idempotency_key = match IdempotencyKey::parse(request.idempotency_key) {
        Ok(value) => value,
        Err(error) => return bad_request("invalid_idempotency_key", error),
    };
    let mut repository = match SqliteStoryRepository::open(&state.database_path) {
        Ok(value) => value,
        Err(error) => return internal_error(error),
    };
    let task = match candidate_task(
        &repository,
        actor,
        &headers,
        &project_id,
        request.task_id.as_deref(),
        request.thread_id.as_deref(),
        Some(request.expected_revision),
    ) {
        Ok(task) => task,
        Err(response) => return *response,
    };
    let branch_id = BranchId::parse("main").expect("static branch id");
    let head = match repository.branch_head(&project_id, &branch_id) {
        Ok(Some(value)) => value,
        Ok(None) => {
            return error(
                StatusCode::NOT_FOUND,
                "project_not_found",
                "Project not found",
            );
        }
        Err(error) => return repository_error(error),
    };
    let version = expected_candidate
        .as_ref()
        .and_then(|revision_id| repository.revision_content(revision_id).ok().flatten())
        .and_then(|content| serde_json::from_str::<Value>(&content).ok())
        .and_then(|content| content.get("version").and_then(Value::as_u64))
        .unwrap_or(0)
        + 1;
    let envelope = json!({
        "chapterId": chapter_id,
        "chapterNumber": chapter_number,
        "title": chapter_title,
        "version": version,
        "baseProjectRevision": request.expected_revision,
        "body": body,
        "sceneGroups": scene_groups,
        "sourcePlanRefs": payload.get("sourcePlanRefs").cloned().unwrap_or_else(|| json!({"localOnly": true, "lineageStatus": "historical"})),
        "completeness": payload.get("completeness").cloned().unwrap_or_else(|| json!("chapterComplete")),
        "source": if actor == Actor::Author { "author" } else { "agent" }
    });
    let request_digest = digest(&json!({
        "kind": "draft",
        "payload": request.payload,
        "expectedRevision": request.expected_revision,
        "expectedCandidateRevisionId": expected_candidate.as_ref().map(RevisionId::as_str)
    }));
    let suffix = Uuid::new_v4().simple().to_string();
    let result = repository.save_candidate(SaveCandidateRequest {
        project_id: project_id.clone(),
        branch_id,
        object_id: ObjectId::parse(format!("draft-{chapter_id}")).expect("draft object id"),
        object_kind: "draft".into(),
        revision_id: RevisionId::parse(format!("draft-{suffix}")).expect("generated revision id"),
        commit_id: CommitId::parse(format!("candidate-{suffix}")).expect("generated commit id"),
        idempotency_key,
        request_digest,
        expected_project_revision: request.expected_revision,
        expected_snapshot_id: head.snapshot_id,
        next_snapshot_id: SnapshotId::parse(format!("snapshot-{suffix}"))
            .expect("generated snapshot id"),
        expected_candidate_revision_id: expected_candidate,
        content_digest: digest(&envelope),
        content_json: envelope.to_string(),
        chapter_projection: Some(ChapterProjection {
            chapter_id: chapter_id.clone(),
            number: chapter_number,
            title: chapter_title.to_owned(),
        }),
        task_output: task.map(|task| TaskOutputBinding {
            task_id: task.task_id,
            thread_id: task.thread_id,
            output_kind: "draft".into(),
            completeness: envelope
                .get("completeness")
                .and_then(Value::as_str)
                .map(str::to_owned),
        }),
        linked_impact_ids: Vec::new(),
    });
    match result {
        Ok(receipt) => match draft_revision(&repository, &receipt.revision_id) {
            Ok(stored) => (
                StatusCode::CREATED,
                Json(json!({
                    "chapterId": stored["chapterId"],
                    "revisionId": receipt.revision_id.as_str(),
                    "version": stored["version"],
                    "body": stored["body"],
                    "sceneGroups": stored["sceneGroups"],
                    "checkpointId": format!("checkpoint-{}", receipt.revision_id.as_str())
                })),
            )
                .into_response(),
            Err(error) => repository_error(error),
        },
        Err(error) => repository_error(error),
    }
}

async fn create_review(
    State(state): State<RuntimeState>,
    Path(project_id): Path<String>,
    Extension(actor): Extension<Actor>,
    headers: HeaderMap,
    Json(request): Json<CreateReviewHttpRequest>,
) -> Response {
    if request.proposals.len() > 200
        || request
            .proposals
            .iter()
            .any(|proposal| !proposal.is_object())
    {
        return error(
            StatusCode::BAD_REQUEST,
            "invalid_fact_proposals",
            "Fact proposals are invalid",
        );
    }
    if !matches!(
        request.artifact_kind.as_str(),
        "brief" | "outline" | "plan" | "draft"
    ) {
        return error(
            StatusCode::BAD_REQUEST,
            "invalid_artifact_kind",
            "Artifact kind is invalid",
        );
    }
    let project_id = match ProjectId::parse(project_id) {
        Ok(value) => value,
        Err(error) => return bad_request("invalid_project_id", error),
    };
    let revision_id = match RevisionId::parse(request.revision_id) {
        Ok(value) => value,
        Err(error) => return bad_request("invalid_revision_id", error),
    };
    let idempotency_key = match IdempotencyKey::parse(request.idempotency_key) {
        Ok(value) => value,
        Err(error) => return bad_request("invalid_idempotency_key", error),
    };
    let review_id = format!("review-{}", Uuid::new_v4().simple());
    let proposals = request
        .proposals
        .into_iter()
        .map(|mut proposal| {
            proposal["id"] = json!(format!("fact-{}", Uuid::new_v4().simple()));
            proposal["source"] = json!(if actor == Actor::Author {
                "author"
            } else {
                "agent"
            });
            proposal["reviewDecision"] = json!("candidate");
            proposal
        })
        .collect::<Vec<_>>();
    let checks = json!([{
        "id": "current-candidate",
        "passed": true,
        "blocking": true,
        "message": "候选版本是当前待审版本"
    }]);
    let selections = if request.selections.is_object() {
        request.selections
    } else {
        json!({})
    };
    let request_digest = digest(&json!({
        "operation": "createReview",
        "artifactKind": request.artifact_kind,
        "revisionId": revision_id.as_str(),
        "proposals": &proposals,
        "selections": selections
    }));
    let mut repository = match SqliteStoryRepository::open(&state.database_path) {
        Ok(value) => value,
        Err(error) => return internal_error(error),
    };
    if let Err(response) = candidate_task(
        &repository,
        actor,
        &headers,
        &project_id,
        request.task_id.as_deref(),
        request.thread_id.as_deref(),
        request.expected_revision,
    ) {
        return *response;
    }
    let object_id = if request.artifact_kind == "draft" {
        match repository.object_heads(&project_id) {
            Ok(heads) => heads
                .into_iter()
                .find(|head| {
                    head.object_kind == "draft"
                        && head.candidate_revision_id.as_ref() == Some(&revision_id)
                })
                .map(|head| head.object_id),
            Err(error) => return repository_error(error),
        }
        .ok_or(RepositoryError::CandidateNotFound)
    } else {
        Ok(
            ObjectId::parse(format!("artifact-{}", request.artifact_kind))
                .expect("artifact object id"),
        )
    };
    let object_id = match object_id {
        Ok(value) => value,
        Err(error) => return repository_error(error),
    };
    match repository.create_review(CreateReviewRequest {
        review_id,
        project_id: project_id.clone(),
        object_id,
        object_kind: request.artifact_kind,
        revision_id,
        idempotency_key,
        request_digest,
        checks_json: checks.to_string(),
        proposals_json: Value::Array(proposals.clone()).to_string(),
        selections_json: selections.to_string(),
    }) {
        Ok(review) => (
            StatusCode::CREATED,
            Json(json!({
                "id": review.review_id,
                "artifactKind": review.object_kind,
                "artifactRevisionId": review.revision_id.as_str(),
                "projectRevision": review.project_revision,
                "status": review.status,
                "checks": checks,
                "factProposals": proposals,
                "provisionalImpacts": []
            })),
        )
            .into_response(),
        Err(error) => repository_error(error),
    }
}

async fn accept_review(
    State(state): State<RuntimeState>,
    Path((project_id, review_id)): Path<(String, String)>,
    Json(request): Json<AcceptReviewHttpRequest>,
) -> Response {
    let project_id = match ProjectId::parse(project_id) {
        Ok(value) => value,
        Err(error) => return bad_request("invalid_project_id", error),
    };
    let key = match IdempotencyKey::parse(request.idempotency_key) {
        Ok(value) => value,
        Err(error) => return bad_request("invalid_idempotency_key", error),
    };
    let selections = if request.selections.is_object() {
        request.selections
    } else {
        json!({})
    };
    let request_digest = digest(&json!({
        "reviewId": review_id,
        "expectedRevision": request.expected_revision,
        "selections": selections
    }));
    let suffix = Uuid::new_v4().simple().to_string();
    let mut repository = match SqliteStoryRepository::open(&state.database_path) {
        Ok(value) => value,
        Err(error) => return internal_error(error),
    };
    match repository.accept_review(AcceptReviewRequest {
        project_id: project_id.clone(),
        branch_id: BranchId::parse("main").expect("static branch id"),
        review_id,
        commit_id: CommitId::parse(format!("commit-{suffix}")).expect("generated commit id"),
        idempotency_key: key,
        request_digest,
        expected_project_revision: request.expected_revision,
        next_snapshot_id: novel_codex_story_core::SnapshotId::parse(format!("snapshot-{suffix}"))
            .expect("generated snapshot id"),
        selections_json: selections.to_string(),
    }) {
        Ok(receipt) => Json(json!({
            "commitId": receipt.commit_id.as_str(),
            "projectId": project_id.as_str(),
            "operation": "accept",
            "baseRevision": request.expected_revision,
            "resultRevision": receipt.result_revision,
            "confirmedFacts": [],
            "impacts": [],
            "resolvedImpactIds": [],
            "migratedImpactIds": [],
            "indexTargetRevision": receipt.result_revision
        }))
        .into_response(),
        Err(error) => repository_error(error),
    }
}

async fn undo_commit(
    State(state): State<RuntimeState>,
    Path((project_id, commit_id)): Path<(String, String)>,
    Json(request): Json<UndoHttpRequest>,
) -> Response {
    let project_id = match ProjectId::parse(project_id) {
        Ok(value) => value,
        Err(error) => return bad_request("invalid_project_id", error),
    };
    let target_commit_id = match CommitId::parse(commit_id) {
        Ok(value) => value,
        Err(error) => return bad_request("invalid_commit_id", error),
    };
    let key = match IdempotencyKey::parse(request.idempotency_key) {
        Ok(value) => value,
        Err(error) => return bad_request("invalid_idempotency_key", error),
    };
    let request_digest = digest(&json!({
        "commitId": target_commit_id.as_str(),
        "expectedRevision": request.expected_revision
    }));
    let suffix = Uuid::new_v4().simple().to_string();
    let mut repository = match SqliteStoryRepository::open(&state.database_path) {
        Ok(value) => value,
        Err(error) => return internal_error(error),
    };
    match repository.undo_acceptance(UndoAcceptanceRequest {
        project_id: project_id.clone(),
        branch_id: BranchId::parse("main").expect("static branch id"),
        target_commit_id,
        commit_id: CommitId::parse(format!("undo-{suffix}")).expect("generated commit id"),
        idempotency_key: key,
        request_digest,
        expected_project_revision: request.expected_revision,
        next_snapshot_id: novel_codex_story_core::SnapshotId::parse(format!("snapshot-{suffix}"))
            .expect("generated snapshot id"),
    }) {
        Ok(receipt) => Json(json!({
            "commitId": receipt.commit_id.as_str(),
            "projectId": project_id.as_str(),
            "operation": "undo",
            "baseRevision": request.expected_revision,
            "resultRevision": receipt.result_revision,
            "indexTargetRevision": receipt.result_revision
        }))
        .into_response(),
        Err(error) => repository_error(error),
    }
}

async fn save_artifact(
    State(state): State<RuntimeState>,
    Path((project_id, kind)): Path<(String, String)>,
    Extension(actor): Extension<Actor>,
    headers: HeaderMap,
    Json(request): Json<SaveArtifactRequest>,
) -> Response {
    if !matches!(kind.as_str(), "brief" | "outline" | "plan") || !request.payload.is_object() {
        return error(
            StatusCode::BAD_REQUEST,
            "invalid_artifact",
            "Artifact is invalid",
        );
    }
    if request.idempotency_key.is_empty() || request.idempotency_key.len() > 160 {
        return error(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "idempotencyKey is invalid",
        );
    }
    let project_id = match ProjectId::parse(project_id) {
        Ok(project_id) => project_id,
        Err(error) => return bad_request("invalid_project_id", error),
    };
    let object_id = ObjectId::parse(format!("artifact-{kind}")).expect("artifact object id");
    let expected_candidate = match request.expected_candidate_revision_id {
        Some(revision_id) => match RevisionId::parse(revision_id) {
            Ok(revision_id) => Some(revision_id),
            Err(error) => return bad_request("invalid_revision_id", error),
        },
        None => None,
    };
    let mut repository = match SqliteStoryRepository::open(&state.database_path) {
        Ok(repository) => repository,
        Err(error) => return internal_error(error),
    };
    let task = match candidate_task(
        &repository,
        actor,
        &headers,
        &project_id,
        request.task_id.as_deref(),
        request.thread_id.as_deref(),
        Some(request.expected_revision),
    ) {
        Ok(task) => task,
        Err(response) => return *response,
    };
    let head = match repository.branch_head(
        &project_id,
        &BranchId::parse("main").expect("static branch id"),
    ) {
        Ok(Some(head)) => head,
        Ok(None) => {
            return error(
                StatusCode::NOT_FOUND,
                "project_not_found",
                "Project not found",
            );
        }
        Err(error) => return repository_error(error),
    };
    let version = expected_candidate
        .as_ref()
        .and_then(|revision_id| repository.revision_content(revision_id).ok().flatten())
        .and_then(|content| serde_json::from_str::<Value>(&content).ok())
        .and_then(|content| content.get("version").and_then(Value::as_u64))
        .unwrap_or(0)
        + 1;
    let envelope = json!({
        "version": version,
        "baseProjectRevision": request.expected_revision,
        "payload": request.payload,
        "source": if actor == Actor::Author { "author" } else { "agent" }
    });
    let content_json = match serde_json::to_string(&envelope) {
        Ok(content) if content.len() <= 48 * 1024 => content,
        Ok(_) => {
            return error(
                StatusCode::PAYLOAD_TOO_LARGE,
                "artifact_too_large",
                "Artifact exceeds 48 KiB",
            );
        }
        Err(error) => return bad_request("invalid_artifact", error),
    };
    let request_digest = digest(&json!({
        "kind": kind,
        "payload": envelope["payload"],
        "expectedRevision": request.expected_revision,
        "expectedCandidateRevisionId": expected_candidate.as_ref().map(RevisionId::as_str),
        "linkedImpactIds": request.linked_impact_ids
    }));
    let suffix = Uuid::new_v4().simple().to_string();
    let revision_id = RevisionId::parse(format!("{kind}-{suffix}")).expect("generated revision id");
    let result = repository.save_candidate(SaveCandidateRequest {
        project_id: project_id.clone(),
        branch_id: BranchId::parse("main").expect("static branch id"),
        object_id,
        object_kind: kind.clone(),
        revision_id: revision_id.clone(),
        commit_id: CommitId::parse(format!("candidate-{suffix}")).expect("generated commit id"),
        idempotency_key: match IdempotencyKey::parse(request.idempotency_key) {
            Ok(key) => key,
            Err(error) => return bad_request("invalid_idempotency_key", error),
        },
        request_digest,
        expected_project_revision: request.expected_revision,
        expected_snapshot_id: head.snapshot_id,
        next_snapshot_id: novel_codex_story_core::SnapshotId::parse(format!("snapshot-{suffix}"))
            .expect("generated snapshot id"),
        expected_candidate_revision_id: expected_candidate,
        content_digest: digest(&envelope),
        content_json,
        chapter_projection: None,
        task_output: task.map(|task| TaskOutputBinding {
            task_id: task.task_id,
            thread_id: task.thread_id,
            output_kind: kind.clone(),
            completeness: None,
        }),
        linked_impact_ids: request.linked_impact_ids.clone(),
    });
    match result {
        Ok(receipt) => match artifact_revision(&repository, &receipt.revision_id) {
            Ok(stored) => (
                StatusCode::CREATED,
                Json(json!({
                    "artifactKind": kind,
                    "revisionId": receipt.revision_id.as_str(),
                    "version": stored["version"],
                    "payload": stored["payload"],
                    "linkedImpactIds": request.linked_impact_ids
                })),
            )
                .into_response(),
            Err(error) => repository_error(error),
        },
        Err(error) => repository_error(error),
    }
}

fn artifact_revision(
    repository: &SqliteStoryRepository,
    revision_id: &RevisionId,
) -> Result<Value, RepositoryError> {
    let content = repository
        .revision_content(revision_id)?
        .ok_or(RepositoryError::CorruptState)?;
    let envelope: Value = serde_json::from_str(&content)
        .map_err(|error| RepositoryError::InvalidContent(error.to_string()))?;
    Ok(json!({
        "id": revision_id.as_str(),
        "version": envelope["version"],
        "parentRevisionId": null,
        "baseProjectRevision": envelope["baseProjectRevision"],
        "payload": envelope["payload"],
        "source": envelope["source"],
        "createdAt": null
    }))
}

fn draft_revision(
    repository: &SqliteStoryRepository,
    revision_id: &RevisionId,
) -> Result<Value, RepositoryError> {
    let content = repository
        .revision_content(revision_id)?
        .ok_or(RepositoryError::CorruptState)?;
    let envelope: Value = serde_json::from_str(&content)
        .map_err(|error| RepositoryError::InvalidContent(error.to_string()))?;
    Ok(json!({
        "id": revision_id.as_str(),
        "chapterId": envelope["chapterId"],
        "version": envelope["version"],
        "parentRevisionId": null,
        "sourcePlanRefs": envelope["sourcePlanRefs"],
        "baseProjectRevision": envelope["baseProjectRevision"],
        "body": envelope["body"],
        "sceneGroups": envelope["sceneGroups"],
        "source": envelope["source"],
        "createdAt": null
    }))
}

fn digest(value: &Value) -> String {
    format!("{:x}", Sha256::digest(value.to_string().as_bytes()))
}

pub(crate) fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs() as i64)
}

pub(crate) fn task_json(task: &TaskRecord) -> Value {
    json!({
        "id": task.task_id,
        "project_id": task.project_id.as_str(),
        "thread_id": task.thread_id,
        "operation": task.operation,
        "task_kind": task.task_kind,
        "purpose": task.purpose,
        "required_output_kind": task.required_output_kind,
        "target": serde_json::from_str::<Value>(&task.target_json).unwrap_or_else(|_| json!({})),
        "inputManifest": serde_json::from_str::<Value>(&task.input_manifest_json).unwrap_or_else(|_| json!({})),
        "input_revision": task.input_revision,
        "model": task.model,
        "status": task.status,
        "stage": task.stage,
        "delivery_status": task.delivery_status,
        "deliveryStatus": task.delivery_status,
        "outputRefs": serde_json::from_str::<Value>(&task.output_refs_json).unwrap_or_else(|_| json!([])),
        "result_summary": task.result_summary,
        "error": task.error
    })
}

fn working_baseline_json(selection: &WorkingBaseline) -> Value {
    json!({
        "objectKind": selection.object_kind,
        "objectId": selection.object_id,
        "revisionId": selection.revision_id.as_ref().map(RevisionId::as_str),
        "selectionRevision": selection.selection_revision,
        "selectedAt": selection.selected_at.to_string()
    })
}

async fn health() -> Json<Value> {
    Json(json!({"ok": true, "service": "creative-runtime"}))
}

async fn require_writer(
    State(state): State<RuntimeState>,
    headers: HeaderMap,
    mut request: Request,
    next: Next,
) -> Response {
    let token = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok());
    let author = format!("Bearer {}", state.author_token);
    let candidate = format!("Bearer {}", state.candidate_token);
    let actor = if token == Some(author.as_str()) {
        Actor::Author
    } else if token == Some(candidate.as_str()) {
        Actor::Candidate
    } else {
        return error(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "Author or candidate authorization is required",
        );
    };
    request.extensions_mut().insert(actor);
    next.run(request).await
}

pub(crate) fn candidate_task(
    repository: &SqliteStoryRepository,
    actor: Actor,
    headers: &HeaderMap,
    project_id: &ProjectId,
    task_id: Option<&str>,
    thread_id: Option<&str>,
    expected_revision: Option<i64>,
) -> Result<Option<TaskRecord>, Box<Response>> {
    if actor == Actor::Author {
        return Ok(None);
    }
    let Some(task_id) = task_id else {
        return Err(Box::new(error(
            StatusCode::UNAUTHORIZED,
            "task_capability_required",
            "Candidate writes require a task binding",
        )));
    };
    let Some(thread_id) = thread_id else {
        return Err(Box::new(error(
            StatusCode::UNAUTHORIZED,
            "task_capability_required",
            "Candidate writes require a thread binding",
        )));
    };
    let Some(capability) = headers
        .get("x-novel-task-capability")
        .and_then(|value| value.to_str().ok())
        .filter(|value| value.len() >= 24)
    else {
        return Err(Box::new(error(
            StatusCode::UNAUTHORIZED,
            "task_capability_required",
            "Candidate writes require a task capability",
        )));
    };
    let revision = match expected_revision {
        Some(revision) => revision,
        None => match repository.task(project_id, task_id) {
            Ok(Some(task)) => task.input_revision,
            Ok(None) => {
                return Err(Box::new(error(
                    StatusCode::UNAUTHORIZED,
                    "task_capability_invalid",
                    "Task capability is invalid",
                )));
            }
            Err(source) => return Err(Box::new(repository_error(source))),
        },
    };
    let capability_hash = format!("{:x}", Sha256::digest(capability.as_bytes()));
    repository
        .authorize_task_capability(
            project_id,
            task_id,
            thread_id,
            revision,
            &capability_hash,
            unix_now(),
        )
        .map(Some)
        .map_err(|source| match source {
            RepositoryError::TaskNotFound
            | RepositoryError::TaskBindingInvalid
            | RepositoryError::TaskCapabilityInvalid
            | RepositoryError::TaskNotWritable => Box::new(error(
                StatusCode::UNAUTHORIZED,
                "task_capability_invalid",
                "Task capability is invalid",
            )),
            _ => Box::new(repository_error(source)),
        })
}

async fn require_author(
    State(state): State<RuntimeState>,
    headers: HeaderMap,
    request: Request,
    next: Next,
) -> Response {
    let expected = format!("Bearer {}", state.author_token);
    if headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        != Some(expected.as_str())
    {
        return error(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "Author authorization is required",
        );
    }
    next.run(request).await
}

async fn list_projects(State(state): State<RuntimeState>) -> Response {
    let repository = match SqliteStoryRepository::open(&state.database_path) {
        Ok(repository) => repository,
        Err(error) => return internal_error(error),
    };
    match repository.list_projects() {
        Ok(projects) => Json(json!({
            "projects": projects.into_iter().map(ProjectResponse::from).collect::<Vec<_>>()
        }))
        .into_response(),
        Err(error) => internal_error(error),
    }
}

async fn create_project(
    State(state): State<RuntimeState>,
    Json(request): Json<CreateProjectRequest>,
) -> Response {
    let title = request.title.trim();
    let logline = request.logline.trim();
    if title.is_empty() || title.chars().count() > 120 || logline.chars().count() > 500 {
        return error(
            StatusCode::BAD_REQUEST,
            "invalid_project",
            "Project title or logline is invalid",
        );
    }
    let timestamp = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_secs() as i64,
        Err(error) => return internal_error(error),
    };
    let suffix = Uuid::new_v4().simple().to_string();
    let project = ProjectRecord {
        project_id: ProjectId::parse(format!("project-{suffix}")).expect("generated project id"),
        title: title.to_owned(),
        logline: logline.to_owned(),
        created_at: timestamp,
        updated_at: timestamp,
    };
    let mut repository = match SqliteStoryRepository::open(&state.database_path) {
        Ok(repository) => repository,
        Err(error) => return internal_error(error),
    };
    let branch_id = BranchId::parse("main").expect("static branch id");
    let snapshot_id =
        SnapshotId::parse(format!("snapshot-{suffix}")).expect("generated snapshot id");
    match repository.create_project(project, branch_id, snapshot_id) {
        Ok(project) => (
            StatusCode::CREATED,
            Json(json!({
                "project": ProjectResponse::from(project),
                "canon": {"characters": [], "relationships": [], "events": [], "locations": [], "worldRules": [], "plotThreads": [], "notes": []},
                "narrativeIr": {"version": "2.0", "scenes": [], "anchors": []},
                "novelEdition": {"chapters": []}
            })),
        )
            .into_response(),
        Err(error) => internal_error(error),
    }
}

impl From<ProjectRecord> for ProjectResponse {
    fn from(project: ProjectRecord) -> Self {
        Self {
            id: project.project_id.to_string(),
            title: project.title,
            logline: project.logline,
            created_at: project.created_at,
            updated_at: project.updated_at,
        }
    }
}

pub(crate) fn internal_error(source: impl std::fmt::Display) -> Response {
    error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "storage_error",
        &source.to_string(),
    )
}

pub(crate) fn bad_request(code: &str, source: impl std::fmt::Display) -> Response {
    error(StatusCode::BAD_REQUEST, code, &source.to_string())
}

pub(crate) fn repository_error(source: RepositoryError) -> Response {
    match source {
        RepositoryError::ProjectNotFound
        | RepositoryError::BranchNotFound
        | RepositoryError::CandidateNotFound
        | RepositoryError::ReviewNotFound
        | RepositoryError::CommitNotFound
        | RepositoryError::TaskNotFound => {
            error(StatusCode::NOT_FOUND, "not_found", &source.to_string())
        }
        RepositoryError::StaleProjectRevision { .. }
        | RepositoryError::StaleCandidate { .. }
        | RepositoryError::StaleSnapshot { .. }
        | RepositoryError::IdempotencyConflict
        | RepositoryError::ObjectLocked
        | RepositoryError::ReviewChecksFailed
        | RepositoryError::TaskNotWritable
        | RepositoryError::TaskOutputMissing => {
            error(StatusCode::CONFLICT, "conflict", &source.to_string())
        }
        RepositoryError::InvalidContent(_)
        | RepositoryError::InvalidObjectKind
        | RepositoryError::InvalidChapter
        | RepositoryError::TaskBindingInvalid
        | RepositoryError::TaskOutputForbidden => error(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            &source.to_string(),
        ),
        _ => internal_error(source),
    }
}

pub(crate) fn error(status: StatusCode, code: &str, message: &str) -> Response {
    (
        status,
        Json(json!({
            "error": {"code": code, "message": message, "details": {}, "recovery": null}
        })),
    )
        .into_response()
}
