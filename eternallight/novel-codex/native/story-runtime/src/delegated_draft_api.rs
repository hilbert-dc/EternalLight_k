use crate::server::{Actor, RuntimeState, bad_request, candidate_task, error, repository_error};
use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use novel_codex_story_core::{
    DelegatedDraftCaller, DelegatedDraftWrite, ProjectId, RepositoryError, SqliteStoryRepository,
    TaskRecord,
};
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Read {
    thread_id: String,
    expected_revision: i64,
    candidate_id: Option<String>,
    cursor: Option<usize>,
    view: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Write {
    thread_id: String,
    expected_revision: i64,
    #[serde(flatten)]
    request: DelegatedDraftWrite,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Grant {
    thread_id: String,
    expected_revision: i64,
    child_thread_id: String,
}

fn lineage(headers: &HeaderMap, task: &TaskRecord, tool: &str) -> Result<String, RepositoryError> {
    let get = |key: &str| {
        headers
            .get(key)
            .and_then(|v| v.to_str().ok())
            .filter(|v| !v.is_empty() && v.len() <= 160 && !v.chars().any(char::is_control))
    };
    if get("x-novel-tool-name") != Some(tool)
        || get("x-novel-root-thread") != Some(task.thread_id.as_str())
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let caller = get("x-novel-caller-thread").ok_or(RepositoryError::TaskBindingInvalid)?;
    let parent = get("x-novel-parent-thread");
    if (caller == task.thread_id && parent.is_some())
        || (caller != task.thread_id && parent != Some(task.thread_id.as_str()))
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    Ok(caller.into())
}
fn bound(
    state: &RuntimeState,
    actor: Actor,
    headers: &HeaderMap,
    project: &str,
    id: &str,
    thread: &str,
    revision: i64,
) -> Result<(SqliteStoryRepository, TaskRecord), Response> {
    let project = ProjectId::parse(project).map_err(|e| bad_request("invalid_project", e))?;
    let repo = SqliteStoryRepository::open(&state.database_path).map_err(repository_error)?;
    let task = candidate_task(
        &repo,
        actor,
        headers,
        &project,
        Some(id),
        Some(thread),
        Some(revision),
    )
    .map_err(|r| *r)?
    .ok_or_else(|| {
        error(
            StatusCode::FORBIDDEN,
            "task_required",
            "An active bound writer task is required",
        )
    })?;
    if task.operation != "generateChapter" {
        return Err(repository_error(RepositoryError::TaskOutputForbidden));
    }
    Ok((repo, task))
}
pub(crate) async fn read(
    State(state): State<RuntimeState>,
    Path((project, id)): Path<(String, String)>,
    Extension(actor): Extension<Actor>,
    headers: HeaderMap,
    Query(q): Query<Read>,
) -> Response {
    let (repo, task) = match bound(
        &state,
        actor,
        &headers,
        &project,
        &id,
        &q.thread_id,
        q.expected_revision,
    ) {
        Ok(v) => v,
        Err(e) => return e,
    };
    if q.view.as_deref() == Some("payload") {
        if let Err(e) =
            crate::recovery_binding::require_root(&headers, &task, "save_draft_checkpoint")
        {
            return repository_error(e);
        }
        // Internal parent reference integration; never expose full payload to a child.
        if headers.get("x-novel-tool-name").is_none() || q.cursor.is_some() {
            return repository_error(RepositoryError::TaskBindingInvalid);
        }
        return match q
            .candidate_id
            .as_deref()
            .ok_or(RepositoryError::TaskBindingInvalid)
            .and_then(|id| repo.delegated_draft_payload(&task, id))
        {
            Ok(payload) => Json(json!({"payload":payload})).into_response(),
            Err(e) => repository_error(e),
        };
    }
    if q.view.is_some() {
        return bad_request(
            "invalid_candidate_view",
            "Omit view for isolated candidate reading",
        );
    }
    let caller = match lineage(&headers, &task, "read_delegated_draft") {
        Ok(v) => v,
        Err(e) => return repository_error(e),
    };
    match repo.read_delegated_draft(&task, &caller, q.candidate_id.as_deref(), q.cursor) {
        Ok(v) => Json(v).into_response(),
        Err(e) => repository_error(e),
    }
}
pub(crate) async fn save(
    State(state): State<RuntimeState>,
    Path((project, id)): Path<(String, String)>,
    Extension(actor): Extension<Actor>,
    headers: HeaderMap,
    Json(q): Json<Write>,
) -> Response {
    let (mut repo, task) = match bound(
        &state,
        actor,
        &headers,
        &project,
        &id,
        &q.thread_id,
        q.expected_revision,
    ) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let caller = match lineage(&headers, &task, "save_delegated_draft") {
        Ok(v) => v,
        Err(e) => return repository_error(e),
    };
    let get = |key: &str| {
        headers
            .get(key)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_owned()
    };
    let caller = DelegatedDraftCaller {
        caller_thread_id: caller,
        parent_thread_id: get("x-novel-parent-thread"),
        turn_id: get("x-novel-turn"),
        call_id: get("x-novel-tool-call"),
    };
    if caller.caller_thread_id==task.thread_id {
        if let Err(e)=crate::recovery_binding::bind(&mut repo,&headers,Some(&task),"save_delegated_draft",Some(&q.request.idempotency_key)) {return repository_error(e);}
    }
    match repo.save_delegated_draft(&task, &caller, q.request) {
        Ok(v) => Json(v).into_response(),
        Err(e) => repository_error(e),
    }
}
pub(crate) async fn grant(
    State(state): State<RuntimeState>,
    Path((project, id)): Path<(String, String)>,
    Extension(actor): Extension<Actor>,
    headers: HeaderMap,
    Json(q): Json<Grant>,
) -> Response {
    let (mut repo, task) = match bound(
        &state,
        actor,
        &headers,
        &project,
        &id,
        &q.thread_id,
        q.expected_revision,
    ) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let caller = match lineage(&headers, &task, "authorize_delegated_writer") {
        Ok(v) => v,
        Err(e) => return repository_error(e),
    };
    if caller != task.thread_id {
        return repository_error(RepositoryError::TaskOutputForbidden);
    }
    if let Err(e)=crate::recovery_binding::bind(&mut repo,&headers,Some(&task),"authorize_delegated_writer",None) {return repository_error(e);}
    match repo.authorize_delegated_writer(&task, &q.child_thread_id) {
        Ok(v) => Json(v).into_response(),
        Err(e) => repository_error(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn no_untrusted_lineage_can_claim_an_isolated_child() {
        let task = TaskRecord {
            task_id: "t".into(),
            project_id: ProjectId::parse("p").unwrap(),
            thread_id: "root".into(),
            operation: "generateChapter".into(),
            task_kind: "generateChapter".into(),
            purpose: "writer".into(),
            required_output_kind: Some("draft".into()),
            target_json: "{}".into(),
            input_manifest_json: "{}".into(),
            input_revision: 0,
            model: None,
            status: "running".into(),
            stage: "running".into(),
            delivery_status: "pending".into(),
            output_refs_json: "[]".into(),
            result_summary: "".into(),
            error: None,
            capability_hash: "a".into(),
            capability_expires_at: 1,
            capability_revoked_at: None,
            turn_id: None,
        };
        let mut headers = HeaderMap::new();
        assert!(lineage(&headers, &task, "save_delegated_draft").is_err());
        for (key, value) in [
            ("x-novel-tool-name", "save_delegated_draft"),
            ("x-novel-root-thread", "root"),
            ("x-novel-caller-thread", "child"),
            ("x-novel-parent-thread", "other"),
        ] {
            headers.insert(key, value.parse().unwrap());
        }
        assert!(lineage(&headers, &task, "save_delegated_draft").is_err());
        headers.insert("x-novel-parent-thread", "root".parse().unwrap());
        assert_eq!(
            lineage(&headers, &task, "save_delegated_draft").unwrap(),
            "child"
        );
    }
}
