use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use axum::Json;
use axum::Router;
use axum::extract::Request;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::middleware;
use axum::middleware::Next;
use axum::response::IntoResponse;
use axum::response::Response;
use axum::routing::get;
use novel_codex_story_core::ProjectId;
use novel_codex_story_core::ProjectRecord;
use novel_codex_story_core::RepositoryError;
use novel_codex_story_core::SqliteStoryRepository;
use novel_codex_story_core::TaskRecord;
use serde_json::Value;
use serde_json::json;
use sha2::Digest;
use sha2::Sha256;

#[derive(Clone)]
pub struct RuntimeState {
    pub(crate) database: novel_codex_story_core::StoryDatabase,
    pub(crate) database_slots: Arc<tokio::sync::Semaphore>,
    pub(crate) engine_budget_database: Option<PathBuf>,
    pub(crate) media_host_callback: Option<(String, Arc<str>)>,
    author_token: Arc<str>,
    candidate_token: Arc<str>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Actor {
    Author,
    Candidate,
}

impl RuntimeState {
    pub fn new(
        database_path: PathBuf,
        author_token: String,
        candidate_token: String,
    ) -> Result<Self, RepositoryError> {
        let database = novel_codex_story_core::StoryDatabase::open(database_path, 8)?;
        Ok(Self {
            database_slots: Arc::new(tokio::sync::Semaphore::new(database.capacity())),
            database,
            engine_budget_database: None,
            media_host_callback: None,
            author_token: author_token.into(),
            candidate_token: candidate_token.into(),
        })
    }

    pub fn database(&self) -> &novel_codex_story_core::StoryDatabase {
        &self.database
    }

    pub fn with_media_host_callback(mut self, url: String, token: String) -> Result<Self, String> {
        let parsed = reqwest::Url::parse(&url).map_err(|_| "invalid media Host URL")?;
        if parsed.scheme() != "http"
            || parsed.host_str() != Some("127.0.0.1")
            || parsed.port().is_none()
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.query().is_some()
            || parsed.fragment().is_some()
            || parsed.path() != "/media/execute"
            || token.len() < 24
        {
            return Err("invalid media Host callback".into());
        }
        self.media_host_callback = Some((url, token.into()));
        Ok(self)
    }

    pub fn with_engine_budget_database(mut self, path: PathBuf) -> Self {
        self.engine_budget_database = Some(path);
        self
    }
}

use crate::contracts::*;

pub fn router(state: RuntimeState) -> Router {
    let author = Router::new()
        .merge(crate::conversation_message_api::routes())
        .merge(crate::conversation_api::routes())
        .merge(crate::quality_api::routes())
        .merge(crate::revision_outcome_api::routes())
        .merge(crate::scene_assessment_api::routes())
        .merge(crate::read_api::routes())
        .merge(crate::task_lifecycle_api::routes())
        .merge(crate::project_api::routes())
        .merge(crate::authoring_api::routes())
        .merge(crate::work_issue_api::routes())
        .merge(crate::work_budget_api::routes())
        .merge(crate::task_provider_budget_api::routes())
        .merge(crate::candidate_api::routes())
        .merge(crate::impact_api::routes())
        .merge(crate::hypothesis_api::routes())
        .merge(crate::commitment_api::routes())
        .merge(crate::commitment_extraction_api::routes())
        .merge(crate::proposal_set_api::routes())
        .merge(crate::modeling_api::routes())
        .merge(crate::modeling_outbox_api::routes())
        .merge(crate::planning_review_api::routes())
        .merge(crate::review_api::routes())
        .merge(crate::selection_api::routes())
        .merge(crate::chapter_api::routes())
        .merge(crate::chapter_deletion_api::routes())
        .merge(crate::chapter_structure_api::routes())
        .merge(crate::task_capability_api::routes())
        .merge(crate::thread_author_api::routes())
        .merge(crate::task_action_api::routes())
        .merge(crate::work_task_api::routes())
        .merge(crate::work_execution_event_api::routes())
        .merge(crate::search_audit_api::routes())
        .merge(crate::entity_audit_api::routes())
        .merge(crate::relation_audit_api::routes())
        .merge(crate::knowledge_audit_api::routes())
        .merge(crate::snapshot_knowledge_audit_api::routes())
        .merge(crate::fact_audit_api::routes())
        .merge(crate::work_interlude_api::routes())
        .merge(crate::recovery_api::routes())
        .merge(crate::generation_handoff_api::routes())
        .merge(crate::task_intent_api::routes())
        .merge(crate::task_final_reply_api::routes())
        .merge(crate::action_permission_api::routes())
        .merge(crate::work_decision_api::routes())
        .merge(crate::execution_scope_api::routes())
        .merge(crate::catalog_api::routes())
        .merge(crate::multimedia_api::routes())
        .merge(crate::condition_evaluation_api::routes())
        .route_layer(middleware::from_fn_with_state(state.clone(), crate::response_contract::enforce))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            require_author,
        ));
    let writer = Router::new()
        .merge(crate::tool_binding_api::writer_routes())
        .merge(crate::recovery_api::writer_routes())
        .merge(crate::delegated_draft_api::writer_routes())
        .merge(crate::revision_outcome_api::writer_routes())
        .merge(crate::work_issue_api::writer_routes())
        .merge(crate::work_decision_api::writer_routes())
        .merge(crate::task_planning_review_api::writer_routes())
        .merge(crate::task_hypothesis_api::writer_routes())
        .merge(crate::task_evidence_api::writer_routes())
        .merge(crate::task_source_api::writer_routes())
        .merge(crate::quality_api::writer_routes())
        .merge(crate::commitment_revision_api::writer_routes())
        .merge(crate::scene_assessment_api::writer_routes())
        .merge(crate::commitment_extraction_api::writer_routes())
        .merge(crate::candidate_api::writer_routes())
        .merge(crate::review_api::writer_routes())
        .merge(crate::planning_review_api::writer_routes())
        .merge(crate::chapter_structure_api::writer_routes())
        .merge(crate::context_read_api::writer_routes())
        .merge(crate::read_api::writer_routes())
        .merge(crate::query_plan_api::writer_routes())
        .merge(crate::task_impact_api::writer_routes())
        .merge(crate::reading_checkpoint_api::writer_routes())
        .merge(crate::task_action_api::writer_routes())
        .merge(crate::generation_model_control_api::writer_routes())
        .merge(crate::generation_handoff_api::writer_routes())
        .merge(crate::modeling_api::writer_routes())
        .merge(crate::task_lifecycle_api::writer_routes())
        .merge(crate::multimedia_api::writer_routes())
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

pub(crate) fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs() as i64)
}

pub(crate) fn task_json(task: &TaskRecord) -> Value {
    json!({
        "id": task.task_id,
        "projectId": task.project_id.as_str(),
        "threadId": task.thread_id,
        "operation": task.operation,
        "taskKind": task.task_kind,
        "purpose": task.purpose,
        "requiredOutputKind": task.required_output_kind,
        "target": serde_json::from_str::<Value>(&task.target_json).unwrap_or_else(|_| json!({})),
        "inputManifest": serde_json::from_str::<Value>(&task.input_manifest_json).unwrap_or_else(|_| json!({})),
        "inputRevision": task.input_revision,
        "model": task.model,
        "turnId": task.turn_id,
        "status": task.status,
        "stage": task.stage,
        "deliveryStatus": task.delivery_status,
        "outputRefs": serde_json::from_str::<Value>(&task.output_refs_json).unwrap_or_else(|_| json!([])),
        "resultSummary": task.result_summary,
        "error": task.error
    })
}

async fn health() -> Json<Value> {
    Json(
        json!({"ok": true, "service": "creative-runtime", "creativeMethodDigests": novel_codex_story_core::creative_method_digests()}),
    )
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
    let guard_state = state.clone();
    let route = request.uri().path().to_owned();
    match crate::repository_executor::execute(state.clone(), move || {
        crate::multimedia_api::guard_novel_route(&guard_state, &route)
    })
    .await
    {
        Ok(Ok(())) => {}
        Ok(Err(response)) | Err(response) => return response,
    }
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
    let task = candidate_task_with_reader(
        repository,
        actor,
        headers,
        project_id,
        task_id,
        thread_id,
        expected_revision,
    )?;
    if task
        .as_ref()
        .is_some_and(|task| task.operation == "reviewQuality")
    {
        return Err(Box::new(error(
            StatusCode::FORBIDDEN,
            "reader_packet_only",
            "Independent readers may only read their packet, save its report and complete the task",
        )));
    }
    Ok(task)
}

// Only the quality endpoint and task completion may accept packet-only readers.
pub(crate) fn candidate_task_with_reader(
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
            "Candidate requests require a task binding",
        )));
    };
    let Some(thread_id) = thread_id else {
        return Err(Box::new(error(
            StatusCode::UNAUTHORIZED,
            "task_capability_required",
            "Candidate requests require a thread binding",
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
            "Candidate requests require a task capability",
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
    mut request: Request,
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
    request.extensions_mut().insert(Actor::Author);
    let guard_state = state.clone();
    let route = request.uri().path().to_owned();
    match crate::repository_executor::execute(state.clone(), move || {
        crate::multimedia_api::guard_novel_route(&guard_state, &route)
    })
    .await
    {
        Ok(Ok(())) => {}
        Ok(Err(response)) | Err(response) => return response,
    }
    next.run(request).await
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
    if let Some(response) = crate::repository_diagnostics::response(&source) {
        return response;
    }
    match source {
        RepositoryError::WorkBudgetExhausted => error(StatusCode::CONFLICT, "work_budget_exhausted", "本次明确设置的执行额度已用完；已保存内容仍保留。"),
        RepositoryError::WorkUsageUnsettled => error(StatusCode::CONFLICT, "work_usage_unsettled", "原请求的费用登记尚未结束，正在等待核对。"),
        RepositoryError::TaskBudgetAlreadySettled => error(StatusCode::CONFLICT, "task_budget_already_settled", "原尝试已结算，不能再次派发；请核对原任务的后续行动。"),
        RepositoryError::TaskBudgetExpired => error(StatusCode::CONFLICT, "task_budget_expired", "本次明确设置的执行有效期已结束；已保存内容仍保留。"),
        RepositoryError::WorkUsageUnknown => error(
            StatusCode::CONFLICT,
            "work_usage_unknown",
            "本次模型用量仍有未核对记录，后续模型执行已停止；已保存内容可以查看。",
        ),
        RepositoryError::PayloadValidation {
            code,
            instance_path,
            mut details,
        } => {
            let Some(fields) = details.as_object_mut() else {
                return internal_error(RepositoryError::CorruptState);
            };
            fields.insert("reason".into(), json!(code));
            fields.insert("origin".into(), json!("storyRuntime"));
            fields.insert("phase".into(), json!("domainValidation"));
            fields.insert("instancePath".into(), json!(instance_path));
            (
                StatusCode::BAD_REQUEST,
                Json(json!({"error":{
                    "code":"invalid_output_payload",
                    "message":"The submitted payload does not match its frozen task contract",
                    "details":details,"recovery":null
                }})),
            )
                .into_response()
        }
        RepositoryError::ProjectNotFound
        | RepositoryError::AuthoringRunNotFound
        | RepositoryError::BranchNotFound
        | RepositoryError::CandidateNotFound
        | RepositoryError::ReviewNotFound
        | RepositoryError::CommitNotFound
        | RepositoryError::ConversationNotFound
        | RepositoryError::TaskNotFound => {
            error(StatusCode::NOT_FOUND, "not_found", &source.to_string())
        }
        RepositoryError::TaskOutputIncomplete => (
            StatusCode::CONFLICT,
            Json(json!({"error":{"code":"task_output_incomplete",
                "message":"The draft checkpoint is saved, but its completeness is not chapterComplete. Complete the author-requested work within the authorized scope and save its actual completeness before completing this task. Do not expand the request or relabel unfinished prose merely to pass this check.",
                "details":{"requiredCompleteness":"chapterComplete","savedOutputPreserved":true},"recovery":null}})),
        ).into_response(),
        RepositoryError::StaleProjectRevision { .. }
        | RepositoryError::ConversationStale { .. }
        | RepositoryError::AuthoringRunStale
        | RepositoryError::StaleKnowledgeManifest
        | RepositoryError::StaleCandidate { .. }
        | RepositoryError::StaleSnapshot { .. }
        | RepositoryError::StaleSearchCursor
        | RepositoryError::IdempotencyConflict
        | RepositoryError::ObjectLocked
        | RepositoryError::ReviewChecksFailed
        | RepositoryError::UndoRequiresLatestCommit
        | RepositoryError::TaskNotWritable
        | RepositoryError::WorkTaskStale
        | RepositoryError::TaskInputStale
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
        Json(crate::contracts::RuntimeErrorEnvelope {
            error: crate::contracts::RuntimeError {
                code: code.into(),
                message: message.into(),
                details: json!({}),
                recovery: None,
            },
        }),
    )
        .into_response()
}

use novel_codex_story_core::{TasksRepository};
