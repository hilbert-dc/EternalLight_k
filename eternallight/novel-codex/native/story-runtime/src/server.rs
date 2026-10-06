use std::collections::{HashMap, HashSet};
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
use novel_codex_story_core::ContextSourceScope;
use novel_codex_story_core::CreateReviewRequest;
use novel_codex_story_core::CreateTaskRequest;
use novel_codex_story_core::DelegationTargetSelection;
use novel_codex_story_core::IdempotencyKey;
use novel_codex_story_core::KnowledgePerspective;
use novel_codex_story_core::KnowledgeStateQuery;
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
    pub(crate) engine_budget_database: Option<PathBuf>,
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
            engine_budget_database: None,
            author_token: author_token.into(),
            candidate_token: candidate_token.into(),
        }
    }

    pub fn with_engine_budget_database(mut self, path: PathBuf) -> Self {
        self.engine_budget_database = Some(path);
        self
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
    modeling_batch_id: Option<String>,
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
struct RejectIdentityReviewHttpRequest {
    expected_revision: i64,
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
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CreateTaskHttpRequest {
    initial_budget: Option<novel_codex_story_core::InitialWorkBudget>,
    operation: String,
    input_revision: i64,
    thread_id: String,
    model: Option<String>,
    #[serde(default)]
    target: Value,
    capability_hash: String,
    capability_expires_at: i64,
    author_intent: String,
    experimental_methods: Option<Vec<String>>,
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
    delivery_review: Option<Value>,
    #[serde(default)]
    summary: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BindTaskTurnRequest {
    thread_id: String,
    turn_id: String,
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
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
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
    query_plan: Option<String>,
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
        .route("/api/projects/{project_id}/conversations", get(crate::conversation_api::list).post(crate::conversation_api::create))
        .route("/api/projects/{project_id}/conversations/{thread_id}", get(crate::conversation_api::read).patch(crate::conversation_api::update))
        .route("/api/projects/{project_id}/quality-notebook", get(crate::quality_api::get).post(crate::quality_api::change))
        .route("/api/projects/{project_id}/quality-packets/{packet_id}", get(crate::quality_api::packet))
        .route("/api/projects/{project_id}/quality-packets/{packet_id}/review", get(crate::quality_api::review))
        .route(
            "/api/projects/{project_id}/revision-outcomes/{writer_task_id}",
            get(crate::revision_outcome_api::get),
        )
        .route(
            "/api/projects/{project_id}/revision-outcomes/{writer_task_id}/decisions",
            axum::routing::post(crate::revision_outcome_api::decide),
        )
        .route(
            "/api/projects/{project_id}/chapters/{chapter_id}/scene-assessments",
            get(crate::scene_assessment_api::get),
        )
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
            "/api/projects/{project_id}/generation-runs",
            axum::routing::get(crate::authoring_api::list).post(crate::authoring_api::start),
        )
        .route(
            "/api/projects/{project_id}/generation-start-position",
            get(crate::authoring_api::start_position),
        )
        .route(
            "/api/projects/{project_id}/generation-runs/{run_id}",
            get(crate::authoring_api::get),
        )
        .route(
            "/api/projects/{project_id}/generation-runs/{run_id}/control",
            axum::routing::post(crate::authoring_api::control),
        )
        .route(
            "/api/projects/{project_id}/generation-runs/{run_id}/replan",
            get(crate::authoring_api::preview_replan).post(crate::authoring_api::replan),
        )
        .route(
            "/api/projects/{project_id}/generation-runs/{run_id}/resolve-feedback",
            axum::routing::post(crate::authoring_api::resolve_feedback),
        )
        .route(
            "/api/projects/{project_id}/generation-runs/{run_id}/next",
            axum::routing::post(crate::authoring_api::next),
        )
        .route(
            "/api/projects/{project_id}/generation-runs/{run_id}/future-intents",
            axum::routing::post(crate::authoring_api::add_future_intent),
        )
        .route(
            "/api/projects/{project_id}/generation-runs/{run_id}/current-chapter-revision",
            axum::routing::post(crate::authoring_api::revise_current_chapter),
        )
        .route(
            "/api/projects/{project_id}/generation-runs/{run_id}/advance-task",
            axum::routing::post(crate::authoring_api::advance_task),
        )
        .route(
            "/api/projects/{project_id}/work-tasks/{work_id}/budget-authorization",
            get(crate::work_budget_api::read_authorization).post(crate::work_budget_api::authorize),
        )
        .route(
            "/api/projects/{project_id}/work-tasks/{work_id}/provider-usage",
            get(crate::work_budget_api::pending_usage).post(crate::work_budget_api::charge_usage),
        )
        .route(
            "/api/projects/{project_id}/tasks/{task_id}/provider-budget",
            get(crate::task_provider_budget_api::read),
        )
        .route(
            "/api/projects/{project_id}/generation-runs/{run_id}/provider-usage",
            axum::routing::post(crate::authoring_api::charge_provider_usage),
        )
        .route(
            "/api/projects/{project_id}/generation-runs/{run_id}/retry-failed-task",
            axum::routing::post(crate::authoring_api::retry_failed_task),
        )
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
            "/api/projects/{project_id}/impact-analysis",
            get(crate::impact_api::analyze_impact),
        )
        .route(
            "/api/projects/{project_id}/hypotheses",
            axum::routing::post(crate::hypothesis_api::create),
        )
        .route(
            "/api/projects/{project_id}/commitments",
            get(crate::commitment_api::list),
        )
        .route(
            "/api/projects/{project_id}/commitments/{id}",
            get(crate::commitment_api::read).post(crate::commitment_api::save),
        )
        .route(
            "/api/projects/{project_id}/commitments/{id}/reviews",
            axum::routing::post(crate::commitment_api::review),
        )
        .route(
            "/api/projects/{project_id}/chapters/{chapter_id}/commitment-extractions",
            get(crate::commitment_extraction_api::get),
        )
        .route(
            "/api/projects/{project_id}/chapters/{chapter_id}/commitment-extractions/establish",
            axum::routing::post(crate::commitment_extraction_api::establish),
        )
        .route(
            "/api/projects/{project_id}/hypotheses/{hypothesis_id}",
            get(crate::hypothesis_api::read),
        )
        .route(
            "/api/projects/{project_id}/proposal-sets",
            get(crate::proposal_set_api::list).post(crate::proposal_set_api::create),
        )
        .route(
            "/api/projects/{project_id}/proposal-sets/{proposal_set_id}",
            get(crate::proposal_set_api::read),
        )
        .route(
            "/api/projects/{project_id}/proposal-sets/{proposal_set_id}/preview",
            axum::routing::post(crate::proposal_set_api::preview),
        )
        .route(
            "/api/projects/{project_id}/proposal-sets/{proposal_set_id}/apply",
            axum::routing::post(crate::proposal_set_api::apply),
        )
        .route(
            "/api/projects/{project_id}/proposal-applications/{application_id}/undo",
            axum::routing::post(crate::proposal_set_api::undo),
        )
        .route(
            "/api/projects/{project_id}/knowledge",
            get(crate::modeling_api::current_knowledge),
        )
        .route(
            "/api/projects/{project_id}/knowledge/snapshots/{snapshot_id}/objects/{object_id}",
            get(crate::modeling_api::snapshot_knowledge),
        )
        .route(
            "/api/projects/{project_id}/knowledge/state",
            get(crate::modeling_api::knowledge_state),
        )
        .route(
            "/api/projects/{project_id}/knowledge/entities",
            get(crate::modeling_api::entity_catalog),
        )
        .route(
            "/api/projects/{project_id}/knowledge/relations",
            get(crate::modeling_api::relation_graph),
        )
        .route(
            "/api/projects/{project_id}/knowledge/evolution",
            get(crate::modeling_api::evolution_trace),
        )
        .route(
            "/api/projects/{project_id}/modeling-batches",
            axum::routing::post(crate::modeling_api::begin_batch),
        )
        .route(
            "/api/projects/{project_id}/modeling-outbox",
            get(crate::modeling_outbox_api::list),
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
            "/api/projects/{project_id}/planning-reviews/{run_id}",
            axum::routing::get(crate::planning_review_api::read),
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
            "/api/projects/{project_id}/reviews/{review_id}/reject-identity",
            axum::routing::post(reject_identity_review),
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
            "/api/projects/{project_id}/chapters/rebind-plan",
            axum::routing::post(crate::chapter_api::rebind_plan),
        )
        .route(
            "/api/projects/{project_id}/chapters/{chapter_id}/delete",
            axum::routing::post(crate::chapter_deletion_api::delete),
        )
        .route(
            "/api/projects/{project_id}/chapter-structure-changes",
            get(crate::chapter_structure_api::list),
        )
        .route(
            "/api/projects/{project_id}/chapter-structure-changes/{change_id}",
            get(crate::chapter_structure_api::get),
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
            "/api/projects/{project_id}/tasks/{task_id}/capability/renew",
            axum::routing::post(crate::task_capability_api::renew),
        )
        .route(
            "/api/projects/{project_id}/threads/{thread_id}/author-messages",
            axum::routing::post(crate::thread_author_api::read),
        )
        .route(
            "/api/projects/{project_id}/task-actions/proposed",
            get(crate::task_action_api::proposed),
        )
        .route(
            "/api/projects/{project_id}/work-tasks",
            get(crate::work_task_api::list),
        )
        .route(
            "/api/projects/{project_id}/search-audits",
            get(crate::search_audit_api::latest).post(crate::search_audit_api::start),
        )
        .route(
            "/api/projects/{project_id}/search-audits/{job_id}",
            get(crate::search_audit_api::get),
        )
        .route(
            "/api/projects/{project_id}/search-audits/{job_id}/control",
            axum::routing::post(crate::search_audit_api::control),
        )
        .route(
            "/api/projects/{project_id}/entity-audits",
            get(crate::entity_audit_api::latest).post(crate::entity_audit_api::start),
        )
        .route(
            "/api/projects/{project_id}/entity-audits/{job_id}",
            get(crate::entity_audit_api::get),
        )
        .route(
            "/api/projects/{project_id}/entity-audits/{job_id}/control",
            axum::routing::post(crate::entity_audit_api::control),
        )
        .route(
            "/api/projects/{project_id}/relation-audits",
            get(crate::relation_audit_api::latest).post(crate::relation_audit_api::start),
        )
        .route(
            "/api/projects/{project_id}/relation-audits/{job_id}",
            get(crate::relation_audit_api::get),
        )
        .route(
            "/api/projects/{project_id}/relation-audits/{job_id}/control",
            axum::routing::post(crate::relation_audit_api::control),
        )
        .route(
            "/api/projects/{project_id}/knowledge-audits",
            get(crate::knowledge_audit_api::latest).post(crate::knowledge_audit_api::start),
        )
        .route(
            "/api/projects/{project_id}/knowledge-audits/{job_id}",
            get(crate::knowledge_audit_api::get),
        )
        .route(
            "/api/projects/{project_id}/knowledge-audits/{job_id}/control",
            axum::routing::post(crate::knowledge_audit_api::control),
        )
        .route(
            "/api/projects/{project_id}/snapshot-knowledge-audits",
            get(crate::snapshot_knowledge_audit_api::latest)
                .post(crate::snapshot_knowledge_audit_api::start),
        )
        .route(
            "/api/projects/{project_id}/snapshot-knowledge-audits/{job_id}",
            get(crate::snapshot_knowledge_audit_api::get),
        )
        .route(
            "/api/projects/{project_id}/snapshot-knowledge-audits/{job_id}/control",
            axum::routing::post(crate::snapshot_knowledge_audit_api::control),
        )
        .route(
            "/api/projects/{project_id}/fact-audits",
            get(crate::fact_audit_api::latest).post(crate::fact_audit_api::start),
        )
        .route(
            "/api/projects/{project_id}/fact-audits/{job_id}",
            get(crate::fact_audit_api::get),
        )
        .route(
            "/api/projects/{project_id}/fact-audits/{job_id}/control",
            axum::routing::post(crate::fact_audit_api::control),
        )
        .route(
            "/api/projects/{project_id}/work-tasks/{id}",
            get(crate::work_task_api::get),
        )
        .route(
            "/api/projects/{project_id}/work-tasks/{id}/control",
            axum::routing::post(crate::work_task_api::control),
        )
        .route(
            "/api/projects/{project_id}/tasks/{task_id}/work-task",
            get(crate::work_task_api::from_attempt),
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
            axum::routing::post(crate::recovery_api::snapshot),
        )
        .route(
            "/api/projects/{project_id}/tasks/recovery-snapshot",
            axum::routing::post(crate::recovery_api::snapshot),
        )
        .route(
            "/api/projects/{project_id}/tasks/{task_id}",
            get(get_task).patch(update_task),
        )
        .route(
            "/api/projects/{project_id}/tasks/{task_id}/cancel",
            axum::routing::post(cancel_task),
        )
        .route(
            "/api/projects/{project_id}/tasks/{task_id}/turn",
            axum::routing::post(bind_task_turn),
        )
        .route(
            "/api/projects/{project_id}/tasks/{task_id}/actions",
            get(crate::task_action_api::list),
        )
        .route(
            "/api/projects/{project_id}/tasks/{task_id}/generation-handoff",
            get(crate::generation_handoff_api::read),
        )
        .route(
            "/api/projects/{project_id}/tasks/{task_id}/intents",
            get(crate::task_intent_api::list).post(crate::task_intent_api::append),
        )
        .route(
            "/api/projects/{project_id}/tasks/{task_id}/final-reply",
            get(crate::task_final_reply_api::read).post(crate::task_final_reply_api::record),
        )
        .route(
            "/api/projects/{project_id}/tasks/{task_id}/intents/{intent_revision}/delivered",
            axum::routing::post(crate::task_intent_api::deliver),
        )
        .route(
            "/api/projects/{project_id}/chapters/{chapter_id}/next-step",
            get(crate::task_action_api::chapter_next_step),
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
            "/api/projects/{project_id}/tasks/{task_id}/actions/{action_id}/permission",
            get(crate::action_permission_api::read),
        )
        .route(
            "/api/projects/{project_id}/work-tasks/{work_id}/decision",
            axum::routing::post(crate::work_decision_api::prepare),
        )
        .route(
            "/api/projects/{project_id}/tasks/{task_id}/execution-scope/preview",
            axum::routing::post(crate::execution_scope_api::preview),
        )
        .route(
            "/api/projects/{project_id}/tasks/{task_id}/execution-scope",
            get(crate::execution_scope_api::read).post(crate::execution_scope_api::confirm),
        )
        .route(
            "/api/projects/{project_id}/tasks/{task_id}/actions/{action_id}/decline",
            axum::routing::post(crate::task_action_api::decline),
        )
        .merge(crate::condition_evaluation_api::routes())
        .route(
            "/api/projects/{project_id}/catalog",
            get(crate::catalog_api::read).post(crate::catalog_api::update),
        )
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            require_author,
        ));
    let writer = Router::new()
        .route(
            "/api/projects/{project_id}/tasks/{task_id}/work-decision",
            axum::routing::get(crate::work_decision_api::read),
        )
        .route(
            "/api/projects/{project_id}/tasks/{task_id}/work-decision/actions",
            axum::routing::post(crate::work_decision_api::select),
        )
        .route(
            "/api/projects/{project_id}/tasks/{task_id}/planning-reviews",
            axum::routing::get(crate::task_planning_review_api::list)
                .post(crate::task_planning_review_api::register),
        )
        .route(
            "/api/projects/{project_id}/tasks/{task_id}/hypothesis",
            axum::routing::post(crate::task_hypothesis_api::run),
        )
        .route(
            "/api/projects/{project_id}/tasks/{task_id}/source-evidence",
            axum::routing::post(crate::task_evidence_api::resolve),
        )
        .route(
            "/api/projects/{project_id}/revision-outcomes/{writer_task_id}",
            axum::routing::post(crate::revision_outcome_api::submit),
        )
        .route(
            "/api/projects/{project_id}/tasks/{task_id}/task-source",
            get(crate::task_source_api::read),
        )
        .route("/api/projects/{project_id}/tasks/{task_id}/quality", axum::routing::post(crate::quality_api::task))
        .route(
            "/api/projects/{project_id}/commitments/{commitment_id}/task-input",
            get(crate::commitment_revision_api::read),
        )
        .route(
            "/api/projects/{project_id}/commitments/{commitment_id}/task-candidate",
            axum::routing::post(crate::commitment_revision_api::submit),
        )
        .route(
            "/api/projects/{project_id}/chapters/{chapter_id}/scene-assessments",
            axum::routing::post(crate::scene_assessment_api::submit),
        )
        .route(
            "/api/projects/{project_id}/chapters/{chapter_id}/commitment-extractions",
            axum::routing::post(crate::commitment_extraction_api::submit),
        )
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
        .route(
            "/api/projects/{project_id}/tasks/{task_id}/chapter-structure-source",
            axum::routing::post(crate::chapter_structure_api::source),
        )
        .route("/api/projects/{project_id}/context", get(read_context))
        .route(
            "/api/projects/{project_id}/tasks/{task_id}/search",
            get(crate::read_api::task_search),
        )
        .route(
            "/api/projects/{project_id}/tasks/{task_id}/query",
            axum::routing::post(crate::query_plan_api::execute),
        )
        .route(
            "/api/projects/{project_id}/tasks/{task_id}/impact-analysis",
            axum::routing::post(crate::task_impact_api::analyze),
        )
        .route(
            "/api/projects/{project_id}/tasks/{task_id}/reading-progress",
            get(crate::reading_checkpoint_api::read).post(crate::reading_checkpoint_api::save),
        )
        .route(
            "/api/projects/{project_id}/tasks/{task_id}/relations",
            get(crate::read_api::task_relations),
        )
        .route(
            "/api/projects/{project_id}/tasks/{task_id}/actions",
            axum::routing::post(crate::task_action_api::request),
        )
        .route(
            "/api/projects/{project_id}/tasks/{task_id}/generation-handoff",
            axum::routing::post(crate::generation_handoff_api::propose),
        )
        .route(
            "/api/projects/{project_id}/modeling-batches/{batch_id}/publish",
            axum::routing::post(crate::modeling_api::publish_batch),
        )
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
    Query(mut query): Query<ContextQuery>,
) -> Response {
    if !matches!(query.purpose.as_str(), "planner" | "writer" | "critic")
        || query.budget == 0
        || query.budget > 100_000
        || !(4_000..=60_000).contains(&query.byte_budget)
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
    if let Some(task) = task.as_ref()
        && let Err(source) = repository.assert_task_inputs_current(task)
    {
        return repository_error(source);
    }
    let writer_baseline = match task
        .as_ref()
        .map(|task| repository.task_writer_baseline(task))
        .transpose()
    {
        Ok(source) => source.flatten(),
        Err(source) => return repository_error(source),
    };
    let writer_scene_limit =
        if let Some(task) = task.as_ref().filter(|task| task.purpose == "writer") {
            let target: Value = match serde_json::from_str(&task.target_json) {
                Ok(value) => value,
                Err(_) => return internal_error("Writer task target is corrupt"),
            };
            query.reading_chapter = query
                .reading_chapter
                .or_else(|| target["readingChapter"].as_i64());
            query.scene_order = query.scene_order.or_else(|| target["sceneOrder"].as_i64());
            query.story_time = query.story_time.or_else(|| target["storyTime"].as_i64());
            query.pov_character_id = query
                .pov_character_id
                .or_else(|| target["povCharacterId"].as_str().map(str::to_owned));
            if target["readingChapter"].as_i64() != query.reading_chapter
                || target["sceneOrder"].as_i64() != query.scene_order
                || target["storyTime"].as_i64() != query.story_time
                || target["povCharacterId"].as_str() != query.pov_character_id.as_deref()
            {
                return error(
                    StatusCode::FORBIDDEN,
                    "writer_scope_mismatch",
                    "Writer context position must match its frozen task target",
                );
            }
            match crate::context_visibility::writer_scene_limit(&target) {
                Ok(limit) => limit,
                Err(source) => return repository_error(source),
            }
        } else {
            None
        };
    let commitment_source_task = task
        .as_ref()
        .is_some_and(|task| task.operation == "extractCommitments");
    let scene_assessment_task = task
        .as_ref()
        .filter(|task| task.operation == "assessSceneOutcome");
    let chapter_structure_input = match task
        .as_ref()
        .filter(|task| task.operation == "assessChapterStructure")
        .map(|task| repository.chapter_structure_context(task, unix_now()))
        .transpose()
    {
        Ok(value) => value,
        Err(source) => return repository_error(source),
    };
    let revision_outcome_task = task
        .as_ref()
        .filter(|task| task.operation == "assessRevisionOutcome");
    let revision_outcome_input = match revision_outcome_task
        .map(|task| repository.revision_outcome_input(task))
        .transpose()
    {
        Ok(value) => value,
        Err(source) => return repository_error(source),
    };
    let revision_repair_input = match task
        .as_ref()
        .map(|task| repository.revision_repair_input(task))
        .transpose()
    {
        Ok(value) => value.flatten(),
        Err(source) => return repository_error(source),
    };
    let mandatory_input_bytes = if task.is_some() {
        0
    } else {
        revision_outcome_input
            .iter()
            .chain(revision_repair_input.iter())
            .chain(chapter_structure_input.iter())
            .map(|input| input.to_string().len())
            .sum::<usize>()
    };
    let assessment_chapter = task
        .as_ref()
        .filter(|task| {
            matches!(
                task.operation.as_str(),
                "assessSceneOutcome" | "extractCommitments" | "assessRevisionOutcome"
            )
        })
        .and_then(|task| serde_json::from_str::<Value>(&task.target_json).ok())
        .and_then(|target| target["chapterNumber"].as_i64());
    let rolling_plan_position = task
        .as_ref()
        .filter(|task| task.operation == "generatePlan")
        .and_then(|task| serde_json::from_str::<Value>(&task.target_json).ok())
        .and_then(|target| target["planFromChapter"].as_i64())
        .filter(|chapter| *chapter >= 1)
        .map(|chapter| chapter - 1)
        .or(assessment_chapter);
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
    let main_branch = BranchId::parse("main").expect("main branch id");
    let source_snapshot = match repository.branch_head(&project_id, &main_branch) {
        Ok(Some(snapshot)) => snapshot.snapshot_id,
        Ok(None) => return internal_error("Main branch is missing"),
        Err(source) => return repository_error(source),
    };
    let source_heads = match repository.project_source_watermarks(&project_id) {
        Ok(watermarks) => watermarks,
        Err(source) => return repository_error(source),
    };
    let query_report = match crate::context_query::retrieve(
        &repository,
        task.as_ref(),
        query.query_plan.as_deref(),
        crate::context_query::ContextQueryBinding {
            snapshot_id: source_snapshot.as_str(),
            byte_limit: query.byte_budget,
            now: unix_now(),
        },
    ) {
        Ok(report) => report,
        Err(RepositoryError::TaskOutputForbidden) => {
            return error(
                StatusCode::FORBIDDEN,
                "context_query_forbidden",
                "Query plan exceeds the bound task scope",
            );
        }
        Err(source) => return repository_error(source),
    };
    let query_scope_policy = match task
        .as_ref()
        .map(|task| repository.task_query_policy(task))
        .transpose()
    {
        Ok(policy) => policy,
        Err(source) => return repository_error(source),
    };
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
    let (heads, working_baselines) = match task
        .as_ref()
        .map(|task| repository.task_context_origins(task))
        .unwrap_or_else(|| {
            repository.object_heads(&project_id).and_then(|heads| {
                repository
                    .working_baselines(&project_id)
                    .map(|baselines| (heads, baselines))
            })
        }) {
        Ok(value) => value,
        Err(source) => return repository_error(source),
    };
    let run_bound_task = task
        .as_ref()
        .and_then(|task| serde_json::from_str::<Value>(&task.target_json).ok())
        .is_some_and(|target| target["runId"].as_str().is_some());
    let source_policy = if run_bound_task || commitment_source_task || assessment_chapter.is_some()
    {
        crate::context_working_source::SourcePolicy::RunOrBoundTask
    } else {
        crate::context_working_source::SourcePolicy::AuthorSelected
    };
    let draft_sources =
        crate::context_working_source::draft_sources(&heads, &working_baselines, source_policy);
    let mut artifacts = serde_json::Map::new();
    if let Some(task) = task
        .as_ref()
        .filter(|task| task.operation == "generatePropertyRules")
    {
        let baseline = match repository.task_property_rule_input(task) {
            Ok(value) => value,
            Err(source) => return repository_error(source),
        };
        artifacts.insert("propertyRules".into(), baseline);
    }
    for kind in ["brief", "outline", "plan"] {
        if query.purpose == "writer" {
            artifacts.insert(kind.into(), Value::Null);
            continue;
        }
        let object_id = format!("artifact-{kind}");
        let head = heads
            .iter()
            .find(|head| head.object_id.as_str() == object_id);
        let selected = head.and_then(|head| {
            crate::context_working_source::context_source(
                head,
                crate::context_working_source::selected_baseline(&working_baselines, kind, kind),
                source_policy,
            )
        });
        let value = if let Some(selected) = selected {
            let content = match repository.revision_content(selected.revision_id) {
                Ok(Some(content)) => content,
                Ok(None) => return repository_error(RepositoryError::CorruptState),
                Err(source) => return repository_error(source),
            };
            let mut value: Value = match serde_json::from_str(&content) {
                Ok(value) => value,
                Err(_) => return repository_error(RepositoryError::CorruptState),
            };
            value = value.get("payload").cloned().unwrap_or(value);
            if let Some(object) = value.as_object_mut() {
                object.insert(
                    "sourceRevisionId".into(),
                    json!(selected.revision_id.as_str()),
                );
                object.insert("sourceAuthority".into(), json!(selected.authority));
                object.insert(
                    "workingSelectionRevision".into(),
                    json!(selected.selection_revision),
                );
            }
            value
        } else {
            Value::Null
        };
        artifacts.insert(kind.into(), value);
    }
    if query.purpose == "writer" {
        let bindings = match repository.chapter_bindings(&project_id) {
            Ok(bindings) => bindings,
            Err(source) => return repository_error(source),
        };
        if let Some(binding) = bindings
            .iter()
            .find(|binding| Some(binding.chapter_number) == query.reading_chapter)
        {
            let chapter = match repository.bound_chapter_plan(&project_id, binding) {
                Ok(chapter) => chapter,
                Err(source) => return repository_error(source),
            };
            let mut plan = match writer_scene_limit {
                Some(end) => crate::context_visibility::visible_scene_plan(&chapter, end),
                None => json!({"chapters": [chapter]}),
            };
            if !plan.is_null() {
                plan["planRevisionId"] = json!(binding.plan_revision_id);
            }
            artifacts.insert("plan".into(), plan);
        }
    }
    if commitment_source_task || revision_outcome_task.is_some() || scene_assessment_task.is_some()
    {
        for value in artifacts.values_mut() {
            *value = Value::Null;
        }
    }
    let chapters = match repository.chapters(&project_id) {
        Ok(chapters) => chapters,
        Err(source) => return repository_error(source),
    };
    let pov_writer = query.purpose == "writer" && query.pov_character_id.is_some();
    let pov_prior_text_withheld = pov_writer
        && chapters.iter().any(|chapter| {
            query
                .reading_chapter
                .is_some_and(|position| chapter.number < position)
        });
    let writer_allowed_objects = if query.purpose == "writer" {
        let mut allowed = chapters
            .iter()
            .filter(|chapter| {
                query
                    .reading_chapter
                    .is_some_and(|position| chapter.number <= position)
            })
            .map(|chapter| format!("draft-{}", chapter.chapter_id))
            .collect::<HashSet<_>>();
        allowed.insert("artifact-plan".to_owned());
        allowed
    } else {
        HashSet::new()
    };
    // Transport budgets cannot change the material registered for a bound source.
    let draft_limit = if task.is_some() {
        80
    } else {
        query.budget.min(80)
    };
    let mut scoped_chapters = chapters
        .iter()
        .filter(|chapter| {
            revision_outcome_task.is_none()
                && (query.purpose != "writer"
                    || query
                        .reading_chapter
                        .is_some_and(|position| chapter.number <= position))
                && (!pov_writer || query.reading_chapter == Some(chapter.number))
                && assessment_chapter.is_none_or(|position| chapter.number == position)
                && rolling_plan_position.is_none_or(|position| chapter.number <= position)
        })
        .collect::<Vec<_>>();
    let drafts_in_scope = scoped_chapters.len();
    if query.purpose == "writer" || rolling_plan_position.is_some() {
        scoped_chapters.reverse();
        scoped_chapters.truncate(draft_limit);
    } else {
        scoped_chapters.truncate(draft_limit);
    }
    let mut drafts_truncated = drafts_in_scope > scoped_chapters.len();
    let mut used_bytes = mandatory_input_bytes;
    let mut draft_context = Vec::new();
    for chapter in scoped_chapters {
        let Some(head) = heads
            .iter()
            .find(|head| head.object_id.as_str() == format!("draft-{}", chapter.chapter_id))
        else {
            continue;
        };
        let Some(source) = crate::context_working_source::context_source(
            head,
            crate::context_working_source::selected_baseline(
                &working_baselines,
                "draft",
                &chapter.chapter_id,
            ),
            source_policy,
        ) else {
            continue;
        };
        let content = match repository.revision_content(source.revision_id) {
            Ok(Some(content)) => content,
            Ok(None) => return repository_error(RepositoryError::CorruptState),
            Err(source) => return repository_error(source),
        };
        let mut value: Value = match serde_json::from_str(&content) {
            Ok(value) => value,
            Err(_) => return internal_error("Draft revision is corrupt"),
        };
        if query.purpose == "writer"
            && query.reading_chapter == Some(chapter.number)
            && let Some(end) = writer_scene_limit
            && let Err(source) = crate::context_visibility::visible_draft_prefix(&mut value, end)
        {
            return repository_error(source);
        }
        let draft = json!({
            "objectId": head.object_id.as_str(),
            "sourceRevisionId": source.revision_id.as_str(),
            "content": value,
            "authority": source.authority,
            "workingSelectionRevision": source.selection_revision
        });
        let draft = writer_baseline
            .as_ref()
            .filter(|baseline| baseline["objectId"] == draft["objectId"])
            .cloned()
            .unwrap_or(draft);
        // Budget the model projection while retaining canonical text for source linking.
        let mut projected_draft = draft.clone();
        if task.is_some() {
            projected_draft = json!({"objectId":draft["objectId"],"sourceRevisionId":draft["sourceRevisionId"],"textDelivered":false});
        } else if scene_assessment_task.is_some()
            || writer_baseline
                .as_ref()
                .is_some_and(|baseline| baseline["objectId"] == draft["objectId"])
        {
            match crate::context_assessment_source::project(&mut projected_draft) {
                Ok(_) => {}
                Err(source) => return repository_error(source),
            }
        } else {
            crate::context_model_item_budget::project_text(&mut projected_draft);
        }
        let visible_bytes = projected_draft.to_string().len();
        if task.is_none() && used_bytes + visible_bytes > query.byte_budget.saturating_sub(2_000) {
            if query.purpose == "writer" && query.reading_chapter == Some(chapter.number) {
                return error(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "context_budget_exceeded",
                    "The target draft exceeds byteBudget; narrow the scene scope or increase the budget",
                );
            }
            drafts_truncated = true;
            break;
        }
        used_bytes += visible_bytes;
        draft_context.push(draft);
    }
    if commitment_source_task
        && !draft_context.iter().any(|draft| {
            query_scope_policy
                .as_ref()
                .is_some_and(|policy| draft["sourceRevisionId"] == policy["sourceRevisionId"])
        })
    {
        return error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "context_budget_exceeded",
            "The bound extraction source must fit the context budget",
        );
    }
    if query.purpose == "writer" || rolling_plan_position.is_some() {
        draft_context.sort_by_key(|draft| {
            draft["content"]["chapterNumber"]
                .as_i64()
                .unwrap_or(i64::MAX)
        });
    }
    let available_facts = match repository.facts(&project_id) {
        Ok(value) => value,
        Err(source) => return repository_error(source),
    };
    let mut facts = Vec::new();
    let mut facts_truncated = false;
    let mut typed_state_facts_withheld = 0;
    let mut typed_event_facts_withheld = 0;
    let mut fact_bytes = 0;
    let fact_byte_budget = query.byte_budget.saturating_sub(used_bytes + 2_000);
    for fact in available_facts {
        if fact["sourceRevisionId"]
            .as_str()
            .is_some_and(|revision| !draft_sources.values().any(|selected| selected == revision))
        {
            continue;
        }
        if query_scope_policy
            .as_ref()
            .and_then(|policy| policy["structuredThroughChapter"].as_i64())
            .is_some_and(|position| {
                fact["readingChapter"]
                    .as_i64()
                    .is_none_or(|chapter| chapter > position)
            })
        {
            continue;
        }
        if query.purpose == "writer" {
            let in_past = fact["readingChapter"].as_i64().is_some_and(|chapter| {
                query
                    .reading_chapter
                    .is_some_and(|current| chapter < current)
            });
            let visible = if pov_writer {
                fact["factKind"] == "character"
                    && fact["epistemicOwnerId"].as_str() == query.pov_character_id.as_deref()
                    && query.story_time.is_some_and(|at| {
                        novel_codex_story_core::epistemic_claim_active_at(
                            &fact["modelingAssertion"],
                            at,
                        )
                    })
            } else {
                match fact["factKind"].as_str() {
                    Some("world" | "reader") => true,
                    Some("character") => {
                        fact["epistemicOwnerId"].as_str() == query.pov_character_id.as_deref()
                    }
                    _ => false,
                }
            };
            if !in_past || !visible {
                continue;
            }
        }
        if fact["modelingAssertion"]["kind"] == "state" {
            // Confirmation does not erase the interval or make this a timeless fact.
            // Consume these through the typed, time-scoped knowledge query instead.
            typed_state_facts_withheld += 1;
            continue;
        }
        if fact["modelingAssertion"]["kind"] == "event" {
            // Event references retain their source-batch identity in typed queries.
            typed_event_facts_withheld += 1;
            continue;
        }
        let size = fact.to_string().len();
        if facts.len() >= query.budget.min(200) || fact_bytes + size > fact_byte_budget {
            facts_truncated = true;
            break;
        }
        fact_bytes += size;
        facts.push(fact);
    }
    used_bytes += fact_bytes;
    let modeling_through = if query.purpose == "writer" {
        query.reading_chapter
    } else {
        query_scope_policy
            .as_ref()
            .and_then(|policy| policy["structuredThroughChapter"].as_i64())
    };
    let modeling_scope = modeling_through.map_or(
        ContextSourceScope::Project,
        ContextSourceScope::ThroughChapter,
    );
    let modeling_sources = match repository.context_sources(&project_id, modeling_scope) {
        Ok(value) => value,
        Err(source) => return repository_error(source),
    };
    let modeling_sources_in_scope = modeling_sources.total_sources;
    let mut modeling = modeling_sources.sources;
    let chapter_numbers = chapters
        .iter()
        .map(|chapter| (chapter.chapter_id.as_str(), chapter.number))
        .collect::<HashMap<_, _>>();
    let mut modeling_source_mismatch = heads.iter().any(|head| {
        head.object_id
            .as_str()
            .strip_prefix("draft-")
            .and_then(|chapter_id| chapter_numbers.get(chapter_id))
            .is_some_and(|chapter| modeling_through.is_none_or(|through| *chapter <= through))
            && draft_sources
                .get(head.object_id.as_str())
                .is_some_and(|selected| {
                    head.candidate_revision_id
                        .as_ref()
                        .or(head.accepted_revision_id.as_ref())
                        .is_none_or(|current| selected != current.as_str())
                })
    });
    for source in &mut modeling {
        if let Some(selected) = source["objectId"]
            .as_str()
            .and_then(|object_id| draft_sources.get(object_id))
            && source["sourceRevisionId"] != *selected
        {
            source["sourceRevisionId"] = json!(selected);
            source["batchId"] = Value::Null;
            source["status"] = json!("model_pending");
            modeling_source_mismatch = true;
        }
    }
    let planning_review = match task.as_ref() {
        Some(task) if task.operation == "reviewPlanningSeat" => {
            match repository.planning_review_context(&project_id, &task.task_id) {
                Ok(value) => value,
                Err(source) => return repository_error(source),
            }
        }
        _ => None,
    };
    let modeling_batch = match task.as_ref() {
        Some(task) if task.operation == "modelKnowledge" => {
            let target = serde_json::from_str::<Value>(&task.target_json).unwrap_or(Value::Null);
            match target["batchId"].as_str() {
                Some(batch_id) => match repository.modeling_batch(&project_id, batch_id) {
                    Ok(batch)
                        if batch.input_project_revision == revision && batch.status == "queued" =>
                    {
                        Some(batch.as_json())
                    }
                    Ok(_) => {
                        return error(
                            StatusCode::CONFLICT,
                            "modeling_batch_stale",
                            "Modeling batch is not current",
                        );
                    }
                    Err(source) => return repository_error(source),
                },
                None => {
                    return bad_request("modeling_batch_missing", "Modeling task has no batchId");
                }
            }
        }
        _ => None,
    };
    if let Some(batch) = modeling_batch.as_ref() {
        let revision_id = match RevisionId::parse(
            batch["sourceRevisionId"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
        ) {
            Ok(value) => value,
            Err(_) => return internal_error("Modeling batch source revision is invalid"),
        };
        let content = match repository.revision_content(&revision_id) {
            Ok(Some(content)) => content,
            Ok(None) => return internal_error("Modeling batch source revision is missing"),
            Err(source) => return repository_error(source),
        };
        let draft = match serde_json::from_str::<Value>(&content) {
            Ok(value) => json!({
                "objectId": batch["objectId"],
                "sourceRevisionId": revision_id.as_str(),
                "content": value,
                "authority": "candidate"
            }),
            Err(_) => return internal_error("Modeling source revision is corrupt"),
        };
        let mut projected_draft = draft.clone();
        crate::context_model_item_budget::project_text(&mut projected_draft);
        let source_bytes = if task.is_some() {
            128
        } else {
            projected_draft.to_string().len()
        };
        if source_bytes + mandatory_input_bytes > query.byte_budget.saturating_sub(2_000) {
            return error(
                StatusCode::UNPROCESSABLE_ENTITY,
                "modeling_source_too_large",
                "Source draft exceeds the modeling context budget",
            );
        }
        draft_context = vec![draft];
        used_bytes = source_bytes + mandatory_input_bytes;
        drafts_truncated = false;
    }
    let entity_catalog = if modeling_batch.is_some() {
        let report_bytes = query_report
            .as_ref()
            .map_or(0, |report| report.to_string().len());
        let available = if task.is_some() {
            64 * 1024 * 1024
        } else {
            query
                .byte_budget
                .saturating_sub(used_bytes + report_bytes + 2_000)
        };
        let catalog = match query_scope_policy
            .as_ref()
            .and_then(|policy| policy["structuredThroughChapter"].as_i64())
        {
            Some(position) => {
                repository.modeling_entity_catalog_through_chapter(&project_id, position, available)
            }
            None => return repository_error(RepositoryError::TaskBindingInvalid),
        };
        match catalog {
            Ok(value) => Some(value),
            Err(source) => return repository_error(source),
        }
    } else {
        None
    };
    let working_knowledge = if query.purpose == "writer" || rolling_plan_position.is_some() {
        let projection_result = if let Some(task) = task.as_ref() {
            repository
                .task_context_knowledge(task)
                .and_then(|projection| projection.ok_or(RepositoryError::TaskBindingInvalid))
        } else {
            repository.knowledge_state(KnowledgeStateQuery {
                project_id: &project_id,
                snapshot_id: None,
                perspective: if pov_writer {
                    KnowledgePerspective::Character
                } else {
                    KnowledgePerspective::Reader
                },
                world_time: None,
                epistemic_time: if pov_writer { query.story_time } else { None },
                reading_chapter: rolling_plan_position.or_else(|| {
                    query
                        .reading_chapter
                        .map(|chapter| chapter.saturating_sub(1))
                }),
                owner_id: if pov_writer {
                    query.pov_character_id.as_deref()
                } else {
                    None
                },
                entity_id: None,
                predicate: None,
            })
        };
        let mut projection = match projection_result {
            Ok(value) => value,
            Err(source) => return repository_error(source),
        };
        if modeling_source_mismatch {
            if let Some(entries) = projection["entries"].as_array_mut() {
                entries.retain(|entry| {
                    entry["sourceObjectId"]
                        .as_str()
                        .and_then(|object_id| draft_sources.get(object_id))
                        .is_some_and(|selected| entry["sourceRevisionId"] == *selected)
                });
            }
            projection["findings"] = json!([]);
            projection["coverage"]["complete"] = json!(false);
            projection["coverage"]["sourceModelingIncomplete"] = json!(true);
        }
        if pov_writer {
            if query.story_time.is_none() {
                projection["entries"] = json!([]);
            }
            projection["findings"] = json!([]);
            projection["coverage"]["complete"] = json!(false);
            projection["coverage"]["povWorldEvidenceWithheld"] = json!(true);
        }
        let available = query.byte_budget.saturating_sub(used_bytes + 2_000);
        let mut retained_bytes = 0;
        if let Some(entries) = projection["entries"].as_array_mut() {
            let before = entries.len();
            entries.retain(|entry| {
                let size = entry.to_string().len();
                if task.is_none() && retained_bytes + size > available {
                    return false;
                }
                retained_bytes += size;
                true
            });
            let truncated = entries.len() < before;
            let confirmed_entries = entries
                .iter()
                .filter(|entry| entry["confirmationState"] == "confirmed")
                .count();
            let candidate_entries = entries.len() - confirmed_entries;
            if truncated {
                projection["coverage"]["complete"] = json!(false);
                projection["coverage"]["truncatedEntries"] = json!(true);
                projection["coverage"]["findingsTruncated"] = json!(true);
                projection["findings"] = json!([]);
            }
            projection["coverage"]["confirmedEntries"] = json!(confirmed_entries);
            projection["coverage"]["candidateEntries"] = json!(candidate_entries);
        }
        Some(projection)
    } else {
        None
    };
    if let Some(task) = scene_assessment_task {
        let target: Value = match serde_json::from_str(&task.target_json) {
            Ok(target) => target,
            Err(_) => return repository_error(RepositoryError::CorruptState),
        };
        let Some(draft) = draft_context
            .iter()
            .find(|draft| draft["sourceRevisionId"] == target["draftRevisionId"])
        else {
            return error(
                StatusCode::UNPROCESSABLE_ENTITY,
                "context_budget_exceeded",
                "The frozen assessment source exceeds byteBudget; no target chapter text was delivered",
            );
        };
        if draft["content"]["chapterId"] != target["chapterId"]
            || draft["content"]["sourcePlanRefs"]["planRevisionId"] != target["planRevisionId"]
        {
            return repository_error(RepositoryError::TaskBindingInvalid);
        }
    }
    let scene_links = if let Some(last_completed_chapter) = rolling_plan_position
        && revision_outcome_task.is_none()
    {
        match crate::scene_plan_context::scene_plan_draft_links(
            &repository,
            &project_id,
            &draft_context,
            last_completed_chapter + 1,
            working_knowledge.as_ref(),
            task.as_ref(),
        ) {
            Ok(links) => links,
            Err(source) => return repository_error(source),
        }
    } else {
        crate::scene_plan_context::ScenePlanLinks {
            links: Vec::new(),
            scene_limit_reached: false,
            event_limit_reached: false,
        }
    };
    let modeling_complete = modeling.len() == modeling_sources_in_scope
        && modeling.iter().all(|item| item["status"] == "ready");
    let working_knowledge_complete = working_knowledge
        .as_ref()
        .is_none_or(|value| value["coverage"]["complete"] == true);
    let watermark_result = if let Some(task) = task.as_ref() {
        repository.task_query_source_watermarks(task)
    } else if query.purpose == "writer" {
        repository.writer_source_watermarks(&project_id, &writer_allowed_objects)
    } else {
        repository.project_source_watermarks(&project_id)
    };
    let mut source_watermarks = match watermark_result {
        Ok(value) => value,
        Err(source) => return repository_error(source),
    };
    source_watermarks["snapshotId"] = json!(source_snapshot.as_str());
    source_watermarks["projectRevision"] = json!(revision);
    let task_target = task
        .as_ref()
        .and_then(|task| serde_json::from_str::<Value>(&task.target_json).ok())
        .unwrap_or(Value::Null);
    let input_manifest = task
        .as_ref()
        .and_then(|task| serde_json::from_str::<Value>(&task.input_manifest_json).ok());
    let mut commitments = match task.as_ref() {
        Some(task) => match repository.task_context_commitments(task) {
            Ok(value) => value,
            Err(source) => return repository_error(source),
        },
        None => {
            json!({"entries":[],"proposalContext":{"entries":[],"coverage":{"requested":false,"complete":true,"includedEntries":0}},"coverage":{"complete":false,"taskBound":false,
            "semanticCompleteness":"unknown","includedEntries":0}})
        }
    };
    if modeling_source_mismatch
        && matches!(
            source_policy,
            crate::context_working_source::SourcePolicy::AuthorSelected
        )
    {
        crate::context_working_source::withhold_mismatched_commitments(
            &mut commitments,
            &draft_sources,
        );
    }
    let delegation_targets = match task
        .as_ref()
        .map(|task| {
            repository.task_delegation_targets(
                task,
                unix_now(),
                query.reading_chapter.map_or(
                    DelegationTargetSelection::Project,
                    DelegationTargetSelection::ChapterNumber,
                ),
            )
        })
        .transpose()
    {
        Ok(value) => value.flatten(),
        Err(source) => return repository_error(source),
    };
    // Scene links consume canonical text before the final transport projection.
    let mut required_source_read = None;
    for draft in &mut draft_context {
        let writer_source = writer_baseline
            .as_ref()
            .is_some_and(|baseline| baseline["objectId"] == draft["objectId"]);
        if task.is_some() {
            crate::context_model_item_budget::project_text(draft);
        } else if scene_assessment_task.is_some() || writer_source {
            match crate::context_assessment_source::project(draft) {
                Ok(mut source) => {
                    if writer_source {
                        source["plan"]["steps"][0]["id"] = json!("writer-baseline");
                        source["plan"]["steps"][0]["operation"]["type"] =
                            json!("writerBaselinePassages");
                    }
                    required_source_read = Some(source);
                }
                Err(source) => return repository_error(source),
            }
        } else {
            crate::context_model_item_budget::project_text(draft);
        }
    }
    let mut context =
        crate::context_pack::build_context_pack(crate::context_pack::ContextPackInput {
            query_report,
            query_scope_policy,
            project_id: project_id.as_str().to_owned(),
            project_revision: revision,
            snapshot_id: source_snapshot.as_str().to_owned(),
            task_id: task.as_ref().map(|task| task.task_id.clone()),
            purpose: query.purpose.clone(),
            actor: if actor == Actor::Author {
                "author"
            } else {
                "candidate"
            }
            .to_owned(),
            position: json!({
                "storyTime": query.story_time,
                "readingChapter": query.reading_chapter,
                "sceneOrder": query.scene_order,
                "povCharacterId": query.pov_character_id
            }),
            task_target,
            delegation_targets,
            input_manifest,
            title: project.title,
            logline: project.logline,
            artifacts,
            drafts: draft_context,
            facts,
            modeling,
            commitments,
            planning_review,
            modeling_batch,
            entity_catalog,
            working_knowledge,
            scene_plan_draft_links: scene_links.links,
            scene_links_truncated: scene_links.scene_limit_reached,
            scene_event_refs_truncated: scene_links.event_limit_reached,
            source_watermarks,
            chapters_in_scope: drafts_in_scope,
            modeling_sources_in_scope,
            drafts_truncated,
            facts_truncated,
            typed_state_facts_withheld,
            typed_event_facts_withheld,
            modeling_complete,
            working_knowledge_complete,
            pov_evidence_withheld: pov_writer,
            pov_prior_text_withheld,
            byte_limit: query.byte_budget,
            entry_limit: query.budget,
        });
    if let Some(task) = task.as_ref().filter(|task| task.operation == "advisory") {
        let intents = match repository.task_intents(&project_id, &task.task_id) {
            Ok(intents) => intents,
            Err(source) => return repository_error(source),
        };
        let latest = intents.last();
        let pending = latest.is_some_and(|intent| intent.status == "pending");
        context["authorIntentState"] = json!({
            "expectedIntentRevision": latest.map_or(0, |intent| intent.intent_revision),
            "canRequestActions": !pending,
            "status": if pending { "pendingDelivery" } else { "current" }
        });
    }
    let has_mandatory_input = revision_outcome_input.is_some()
        || revision_repair_input.is_some()
        || chapter_structure_input.is_some()
        || required_source_read.is_some();
    if let Some(source) = required_source_read {
        context["operationContext"]["requiredSourceReads"] = source;
        context["coverage"]["complete"] = json!(false);
        let missing = context["coverage"]["missingScope"]
            .as_array_mut()
            .expect("missing scope array");
        if !missing.iter().any(|entry| entry == "chapterText") {
            missing.push(json!("chapterText"));
        }
    }
    if let Some(input) = chapter_structure_input {
        context["operationContext"]["chapterStructureChange"] = input;
    }
    if let Some(input) = revision_outcome_input {
        if input["coverage"]["complete"] != true {
            context["coverage"]["complete"] = json!(false);
            context["coverage"]["missingScope"]
                .as_array_mut()
                .expect("missing scope array")
                .push(json!("revisionOutcomeScenes"));
            context["coverage"]["truncationReasons"]
                .as_array_mut()
                .expect("truncation reasons array")
                .push(json!("revisionOutcomeTextBudget"));
        }
        context["operationContext"]["revisionOutcomeInput"] = input;
    }
    if let Some(input) = revision_repair_input {
        context["operationContext"]["revisionRepairInput"] = input;
    }
    if let Some(task) = task.as_ref() {
        match repository.task_generation_scene_feedback(task) {
            Ok(Some(feedback)) => context["operationContext"]["generationSceneFeedback"] = feedback,
            Ok(None) => (),
            Err(source) => return repository_error(source),
        }
    }
    if let Some(task) = task.as_ref()
        && let Err(source) = repository.project_task_sources(task, &mut context)
    {
        return repository_error(source);
    }
    if has_mandatory_input || task.is_some() {
        // Source-bound inputs are mandatory. Fit and hash once after inserting both.
        crate::context_budget::fit_optional_context(&mut context, query.byte_budget);
        loop {
            let size = context.to_string().len();
            if context["budgetsUsed"]["byteSize"] == json!(size) {
                break;
            }
            context["budgetsUsed"]["byteSize"] = json!(size);
        }
        let mut content = context.clone();
        let object = content.as_object_mut().expect("context pack object");
        object.remove("id");
        object.remove("contentHash");
        let hash = format!("{:x}", Sha256::digest(content.to_string().as_bytes()));
        context["id"] = json!(format!("context-{hash}"));
        context["contentHash"] = json!(hash);
    }
    let final_size = context["budgetsUsed"]["byteSize"]
        .as_u64()
        .unwrap_or(u64::MAX);
    if !crate::context_model_item_budget::fits(&context) {
        return error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "context_model_item_budget_exceeded",
            "Required scoped context exceeds the model-item budget; no chapter text was delivered. Reduce optional budget or read authorized passages with query_story in the same frozen scope. Do not infer missing facts or complete reading from this error.",
        );
    }
    if final_size > query.byte_budget as u64 {
        return error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "context_budget_exceeded",
            "The scoped context exceeds byteBudget; narrow the task scope or increase the budget",
        );
    }
    if let Some(task) = task.as_ref()
        && let Err(source) = repository.assert_task_inputs_current(task)
    {
        return repository_error(source);
    }
    let final_snapshot = match repository.branch_head(&project_id, &main_branch) {
        Ok(Some(snapshot)) => snapshot.snapshot_id,
        Ok(None) => return internal_error("Main branch is missing"),
        Err(source) => return repository_error(source),
    };
    if final_snapshot != source_snapshot {
        return error(
            StatusCode::CONFLICT,
            "context_snapshot_changed",
            "Context source snapshot changed during construction",
        );
    }
    let final_heads = match repository.project_source_watermarks(&project_id) {
        Ok(watermarks) => watermarks,
        Err(source) => return repository_error(source),
    };
    if final_heads != source_heads {
        return error(
            StatusCode::CONFLICT,
            "context_sources_changed",
            "Context object or knowledge sources changed during construction",
        );
    }
    Json(context).into_response()
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
    let head_snapshot_id = match repository.branch_head(
        &project_id,
        &BranchId::parse("main").expect("main branch id"),
    ) {
        Ok(Some(head)) => head.snapshot_id,
        Ok(None) => return repository_error(RepositoryError::BranchNotFound),
        Err(error) => return repository_error(error),
    };
    let source_digest = match repository.project_source_watermarks(&project_id) {
        Ok(watermarks) => match watermarks["digest"].as_str() {
            Some(digest) => digest.to_owned(),
            None => return repository_error(RepositoryError::CorruptState),
        },
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
    let mut projected_bindings = Vec::with_capacity(chapter_bindings.len());
    for binding in &chapter_bindings {
        let chapter = match repository.bound_chapter_plan(&project_id, binding) {
            Ok(chapter) => chapter,
            Err(source) => return repository_error(source),
        };
        projected_bindings.push(json!({
            "project_id": project_id.as_str(), "plan_revision_id": binding.plan_revision_id,
            "chapter_plan_id": binding.chapter_plan_id, "chapter_id": binding.chapter_id,
            "lineageStatus": "current", "writerProjection": {
                "chapterPlanId": binding.chapter_plan_id, "chapterNumber": binding.chapter_number,
                "primaryNodeId": chapter["primaryNodeId"], "publicTitle": chapter["publicTitle"].as_str().unwrap_or_default(),
                "writerGuidance": chapter["writerGuidance"].as_str().unwrap_or_default(),
                "publicCommitments": chapter["publicCommitments"].as_array().cloned().unwrap_or_default(),
                "povCharacterId": chapter["povCharacterId"], "storyTime": chapter["storyTime"]
            }
        }));
    }
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
                    "proposalApplicationId": commit.proposal_application_id,
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
    for kind in ["brief", "outline", "plan", "identity", "propertyRules"] {
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
    let modeling = match repository.current_knowledge(&project_id) {
        Ok(value) => value,
        Err(error) => return repository_error(error),
    };
    Json(json!({
        "project": ProjectResponse::from(project),
        "revision": revision,
        "headSnapshotId": head_snapshot_id.as_str(),
        "sourceDigest": source_digest,
        "planWindow": 3,
        "planLineageStatus": "missing",
        "index": {"targetRevision": revision, "indexedRevision": revision, "status": "ready"},
        "artifacts": artifacts,
        "chapters": chapters,
        "factProposals": fact_proposals,
        "reviews": reviews,
        "impacts": impacts.items,
        "openImpactCount": impacts.open,
        "chapterPlanBindings": projected_bindings,
        "commits": commits,
        "tasks": tasks,
        "workingBaselines": baselines.iter().map(working_baseline_json).collect::<Vec<_>>(),
        "continuationIntents": continuations.iter().map(|intent| json!({
            "targetKind": intent.target_kind, "targetId": intent.target_id,
            "workingRevisionId": intent.working_revision_id.as_str(),
            "authorIntent": intent.author_intent, "status": intent.status,
            "taskId": intent.task_id, "updatedAt": intent.updated_at.to_string()
        })).collect::<Vec<_>>(),
        "planningReviews": planning_reviews,
        "modeling": modeling
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
    let mut manifest = json!({
        "operation": request.operation,
        "projectId": project_id.as_str(),
        "projectRevision": request.input_revision,
        "target": target,
        "authorIntent": request.author_intent,
        "expectedCandidateRevisionId": null,
        "branchContext": null
    });
    if let Some(methods) = request.experimental_methods {
        manifest["experimentalMethods"] = json!(methods);
    }
    let mut repository = match SqliteStoryRepository::open(&state.database_path) {
        Ok(value) => value,
        Err(error) => return internal_error(error),
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
                                && target["chapterNumber"].as_i64() == Some(binding.chapter_number)
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
    let task_request = CreateTaskRequest {
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
    };
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
}

async fn get_task(
    State(state): State<RuntimeState>,
    Path((project_id, task_id)): Path<(String, String)>,
) -> Response {
    let (repository, project_id) = match open_project_repository(&state, project_id) {
        Ok(value) => value,
        Err(response) => return *response,
    };
    match repository.task(&project_id, &task_id) {
        Ok(Some(task)) => Json(task_json(&task)).into_response(),
        Ok(None) => repository_error(RepositoryError::TaskNotFound),
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

async fn bind_task_turn(
    State(state): State<RuntimeState>,
    Path((project_id, task_id)): Path<(String, String)>,
    Json(request): Json<BindTaskTurnRequest>,
) -> Response {
    let project_id = match ProjectId::parse(project_id) {
        Ok(value) => value,
        Err(source) => return bad_request("invalid_project_id", source),
    };
    let mut repository = match SqliteStoryRepository::open(&state.database_path) {
        Ok(value) => value,
        Err(source) => return repository_error(source),
    };
    match repository.bind_task_turn(&project_id, &task_id, &request.thread_id, &request.turn_id) {
        Ok(task) => Json(task_json(&task)).into_response(),
        Err(source) => repository_error(source),
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
        request.summary.chars().take(4_000).collect(), request.delivery_review,
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
    let mut source_plan_refs = payload
        .get("sourcePlanRefs")
        .cloned()
        .unwrap_or_else(|| json!({"localOnly": true, "lineageStatus": "historical"}));
    let mut response_scene_limit = None;
    let body = if let Some(task) = task.as_ref() {
        let target: Value = match serde_json::from_str(&task.target_json) {
            Ok(value) => value,
            Err(_) => return internal_error("Task target is corrupt"),
        };
        match crate::context_visibility::writer_scene_limit(&target) {
            Ok(Some(end)) if task.operation == "generateChapter" => {
                response_scene_limit = Some(end);
                let prior = match repository.task_draft_base(&project_id, &task.task_id) {
                    Ok(value) => value,
                    Err(source) => return repository_error(source),
                };
                if let Some(prior) = prior.as_ref() {
                    if prior["title"].as_str() != Some(chapter_title)
                        || prior["chapterNumber"].as_i64() != Some(chapter_number)
                    {
                        return repository_error(RepositoryError::TaskOutputForbidden);
                    }
                    source_plan_refs = prior["sourcePlanRefs"].clone();
                }
                match crate::context_visibility::merge_visible_draft_prefix(
                    &mut scene_groups,
                    prior.as_ref(),
                    end,
                ) {
                    Ok(body) => body,
                    Err(source) => return repository_error(source),
                }
            }
            Ok(_) => body.to_owned(),
            Err(source) => return repository_error(source),
        }
    } else {
        body.to_owned()
    };
    if let Some(task) = task
        .as_ref()
        .filter(|task| task.operation == "generateChapter")
    {
        let target: Value = match serde_json::from_str(&task.target_json) {
            Ok(value) => value,
            Err(_) => return internal_error("Task target is corrupt"),
        };
        if target["runId"].as_str().is_some() {
            let bindings = match repository.chapter_bindings(&project_id) {
                Ok(bindings) => bindings,
                Err(source) => return repository_error(source),
            };
            if let Some(binding) = bindings.iter().find(|binding| binding.chapter_id == chapter_id) {
                if binding.chapter_number != chapter_number {
                    return repository_error(RepositoryError::TaskBindingInvalid);
                }
                // A present binding is authoritative, including its immutable chapter entry.
                // The absence of a plan is an equally valid agent-selected writing strategy.
                if let Err(source) = repository.bound_chapter_plan(&project_id, binding) {
                    return repository_error(source);
                }
                let plan_content = match RevisionId::parse(binding.plan_revision_id.clone()) {
                    Ok(id) => match repository.revision_content(&id) {
                        Ok(Some(content)) => content,
                        Ok(None) => return internal_error("Bound plan revision is missing"),
                        Err(source) => return repository_error(source),
                    },
                    Err(_) => return internal_error("Bound plan revision is invalid"),
                };
                let plan: Value = match serde_json::from_str(&plan_content) {
                    Ok(value) => value,
                    Err(_) => return internal_error("Bound plan revision is corrupt"),
                };
                source_plan_refs = json!({
                    "lineageStatus": "current", "localOnly": false,
                    "planRevisionId": binding.plan_revision_id,
                    "chapterPlanId": binding.chapter_plan_id,
                    "outlineRevisionId": plan.get("payload").unwrap_or(&plan).get("sourceOutlineRevisionId")
                });
            } else {
                // Model-supplied references cannot invent a plan for an unbound chapter.
                source_plan_refs = json!({"localOnly": true, "lineageStatus": "historical"});
            }
        }
    }
    if body.chars().count() > 120_000 {
        return error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "draft_too_large",
            "Draft exceeds 120000 characters",
        );
    }
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
        "sourcePlanRefs": source_plan_refs,
        "completeness": payload.get("completeness").cloned().unwrap_or_else(|| json!("chapterComplete")),
        "source": if actor == Actor::Author { "author" } else { "agent" }
    });
    let request_digest = digest(&json!({
        "kind": "draft",
        "payload": request.payload,
        "expectedRevision": request.expected_revision,
        "expectedCandidateRevisionId": expected_candidate.as_ref().map(RevisionId::as_str)
    }));
    if let Err(source) = crate::recovery_binding::bind(
        &mut repository,
        &headers,
        task.as_ref(),
        "save_draft_checkpoint",
        Some(idempotency_key.as_str()),
    ) {
        return repository_error(source);
    }
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
            Ok(mut stored) => {
                if let Some(end) = response_scene_limit
                    && let Err(source) =
                        crate::context_visibility::visible_draft_prefix(&mut stored, end)
                {
                    return repository_error(source);
                }
                (
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
                    .into_response()
            }
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
    if request.modeling_batch_id.is_some()
        && (actor != Actor::Author
            || request.artifact_kind != "draft"
            || !request.proposals.is_empty())
    {
        return error(
            StatusCode::BAD_REQUEST,
            "invalid_modeling_review",
            "Modeling reviews must be author-initiated and cannot contain caller proposals",
        );
    }
    if actor == Actor::Candidate && !request.proposals.is_empty() {
        return error(
            StatusCode::FORBIDDEN,
            "candidate_fact_proposals_disabled",
            "Candidate facts must be published through a source-bound modeling batch",
        );
    }
    if request.artifact_kind == "identity"
        && (actor != Actor::Author
            || request.modeling_batch_id.is_some()
            || !request.proposals.is_empty())
    {
        return error(
            StatusCode::FORBIDDEN,
            "identity_decision_author_only",
            "Only the author can review identity decisions",
        );
    }
    if request.artifact_kind == "propertyRules"
        && (actor != Actor::Author
            || request.modeling_batch_id.is_some()
            || !request.proposals.is_empty()
            || (!request.selections.is_null()
                && request
                    .selections
                    .as_object()
                    .is_none_or(|fields| !fields.is_empty())))
    {
        return error(
            StatusCode::FORBIDDEN,
            "property_rules_author_only",
            "Only the author can review property rules without facts or selections",
        );
    }
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
        "brief" | "outline" | "plan" | "draft" | "identity" | "propertyRules"
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
        "modelingBatchId": request.modeling_batch_id,
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
        modeling_batch_id: request.modeling_batch_id,
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
                "factProposals": serde_json::from_str::<Value>(&review.proposals_json).unwrap_or(Value::Null),
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
    if !request.selections.is_object() {
        return error(
            StatusCode::BAD_REQUEST,
            "invalid_selections",
            "Review selections must be an object",
        );
    }
    let selections = request.selections;
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

async fn reject_identity_review(
    State(state): State<RuntimeState>,
    Path((project_id, review_id)): Path<(String, String)>,
    Json(request): Json<RejectIdentityReviewHttpRequest>,
) -> Response {
    let project_id = match ProjectId::parse(project_id) {
        Ok(value) => value,
        Err(error) => return bad_request("invalid_project_id", error),
    };
    let mut repository = match SqliteStoryRepository::open(&state.database_path) {
        Ok(value) => value,
        Err(error) => return internal_error(error),
    };
    match repository.reject_identity_review(&project_id, &review_id, request.expected_revision) {
        Ok(()) => Json(json!({ "reviewId": review_id, "status": "rejected" })).into_response(),
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
    if kind == "identity" && actor != Actor::Author {
        return error(
            StatusCode::FORBIDDEN,
            "identity_decision_author_only",
            "Only the author can save identity decisions",
        );
    }
    if !matches!(
        kind.as_str(),
        "brief" | "outline" | "plan" | "identity" | "propertyRules"
    ) || !request.payload.is_object()
    {
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
    if kind == "brief"
        && let Err(message) = crate::brief_payload::validate(&request.payload)
    {
        return error(StatusCode::BAD_REQUEST, "invalid_brief_payload", message);
    }
    if kind == "propertyRules"
        && actor != Actor::Author
        && task
            .as_ref()
            .is_none_or(|task| task.operation != "generatePropertyRules")
    {
        return error(
            StatusCode::FORBIDDEN,
            "property_rules_task_required",
            "Only a bound generatePropertyRules task can save a model proposal",
        );
    }
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
    let version = match expected_candidate.as_ref() {
        None => 1,
        Some(revision_id) => {
            let content = match repository.revision_content(revision_id) {
                Ok(Some(content)) => content,
                Ok(None) => {
                    return error(
                        StatusCode::CONFLICT,
                        "candidate_revision_conflict",
                        "expectedCandidateRevisionId does not identify a saved revision. Copy the frozen manifest CAS or the last successful save receipt exactly. For a first save, use JSON null, not the string \"null\"; do not invent an ID.",
                    );
                }
                Err(source) => return repository_error(source),
            };
            let previous: Value = match serde_json::from_str(&content) {
                Ok(value) => value,
                Err(_) => return repository_error(RepositoryError::CorruptState),
            };
            let Some(version) = previous["version"]
                .as_u64()
                .filter(|version| *version > 0)
                .and_then(|version| version.checked_add(1))
            else {
                return repository_error(RepositoryError::CorruptState);
            };
            version
        }
    };
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
    let tool = match kind.as_str() {
        "brief" => "save_brief",
        "outline" => "save_outline",
        "plan" => "save_chapter_plans",
        "propertyRules" => "save_property_rules",
        _ => "author",
    };
    if let Err(source) = crate::recovery_binding::bind(
        &mut repository,
        &headers,
        task.as_ref(),
        tool,
        Some(&request.idempotency_key),
    ) {
        return repository_error(source);
    }
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
    let title = envelope
        .get("title")
        .and_then(Value::as_str)
        .ok_or(RepositoryError::CorruptState)?;
    Ok(json!({
        "id": revision_id.as_str(),
        "chapterId": envelope["chapterId"],
        "title": title,
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
    let task = candidate_task_with_reader(repository, actor, headers, project_id, task_id, thread_id, expected_revision)?;
    if task.as_ref().is_some_and(|task| task.operation == "reviewQuality") {
        return Err(Box::new(error(StatusCode::FORBIDDEN, "reader_packet_only", "Independent readers may only read their packet, save its report and complete the task")));
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
        | RepositoryError::AuthoringRunNotFound
        | RepositoryError::BranchNotFound
        | RepositoryError::CandidateNotFound
        | RepositoryError::ReviewNotFound
        | RepositoryError::CommitNotFound
        | RepositoryError::ConversationNotFound
        | RepositoryError::TaskNotFound => {
            error(StatusCode::NOT_FOUND, "not_found", &source.to_string())
        }
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
        Json(json!({
            "error": {"code": code, "message": message, "details": {}, "recovery": null}
        })),
    )
        .into_response()
}
