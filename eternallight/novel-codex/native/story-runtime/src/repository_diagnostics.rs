//! Stable, bounded diagnostics for domain rejections. Storage failures remain unknown.
use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use novel_codex_story_core::RepositoryError;
use serde_json::{Value, json};

pub(crate) fn response(source: &RepositoryError) -> Option<Response> {
    let (status, category, reason, field, next, facts) = match source {
        RepositoryError::StaleProjectRevision { expected, actual } => (
            StatusCode::CONFLICT,
            "conflict",
            "project_revision_stale",
            "/expectedRevision",
            "read_current_state",
            json!({"expected":expected,"actual":actual}),
        ),
        RepositoryError::StaleCandidate { expected, actual } => (
            StatusCode::CONFLICT,
            "conflict",
            "candidate_revision_stale",
            "/expectedCandidateRevisionId",
            "read_current_state",
            json!({"expected":expected.as_ref().map(|v|v.as_str()),"actual":actual.as_ref().map(|v|v.as_str())}),
        ),
        RepositoryError::TaskInputStale => (
            StatusCode::CONFLICT,
            "conflict",
            "task_input_stale",
            "/expectedRevision",
            "refresh_task_binding",
            Value::Null,
        ),
        RepositoryError::GenerationResumeStateConflict {
            run_id,
            run_status,
            selection_status,
            work_status,
        } => (
            StatusCode::CONFLICT,
            "conflict",
            if *selection_status == "notNeeded" {
                "generation_resume_not_needed"
            } else {
                "generation_resume_unavailable"
            },
            "/resumeRun",
            "read_work_status",
            json!({"runId":run_id,"runStatus":run_status,"workStatus":work_status,"resumeSelection":{"status":selection_status}}),
        ),
        RepositoryError::TaskBindingInvalid => (
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "task_binding_invalid",
            "/taskId",
            "verify_task_binding",
            Value::Null,
        ),
        RepositoryError::TaskOutputForbidden => (
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "task_output_forbidden",
            "/payload",
            "review_authorized_scope",
            Value::Null,
        ),
        RepositoryError::ObjectLocked => (
            StatusCode::CONFLICT,
            "conflict",
            "object_locked",
            "/payload",
            "preserve_locked_content",
            Value::Null,
        ),
        RepositoryError::IdempotencyConflict => (
            StatusCode::CONFLICT,
            "conflict",
            "idempotency_conflict",
            "/idempotencyKey",
            "reconcile_original_request",
            Value::Null,
        ),
        RepositoryError::TaskNotWritable => (
            StatusCode::CONFLICT,
            "conflict",
            "task_not_writable",
            "/taskId",
            "read_task_status",
            Value::Null,
        ),
        _ => return None,
    };
    let mut details = json!({"origin":"storyRuntime","phase":"domainValidation","reason":reason,"instancePath":field,"nextAction":next});
    if let Some(facts) = facts.as_object() {
        details.as_object_mut().unwrap().extend(facts.clone());
    }
    Some((status,Json(json!({"error":{"code":category,"message":source.to_string(),"details":details,"recovery":null}}))).into_response())
}
