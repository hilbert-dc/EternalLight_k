use axum::Extension;
use axum::Json;
use axum::extract::Path;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::response::Response;
use novel_codex_story_core::IdempotencyKey;
use novel_codex_story_core::ProjectId;
use novel_codex_story_core::RepositoryError;
use novel_codex_story_core::RevisionId;
use novel_codex_story_core::SqliteStoryRepository;
use novel_codex_story_core::TaskOutputBinding;
use serde_json::Value;
use serde_json::json;

use crate::contracts::*;
use crate::server::{
    Actor, RuntimeState, bad_request, candidate_task, error, internal_error, repository_error,
};
use crate::workspace_projection::*;
pub(crate) fn prepare_draft_request(
    repo: &mut SqliteStoryRepository,
    actor: Actor,
    headers: &HeaderMap,
    project: &str,
    mut input: Value,
) -> Result<SaveArtifactRequest, Response> {
    if input.get("patch").is_some() {
        if input.get("payload").is_some() {
            return Err(bad_request(
                "invalid_draft_patch",
                "Supply exactly one of payload or patch",
            ));
        }
        let project =
            ProjectId::parse(project.to_owned()).map_err(|e| bad_request("invalid_project", e))?;
        let task = candidate_task(
            &repo,
            actor,
            headers,
            &project,
            input["taskId"].as_str(),
            input["threadId"].as_str(),
            input["expectedRevision"].as_i64(),
        )
        .map_err(|e| *e)?
        .ok_or_else(|| {
            bad_request(
                "draft_patch_task_required",
                "Patch requires an authorized writer task",
            )
        })?;
        crate::recovery_binding::bind(
            repo,
            headers,
            Some(&task),
            "save_draft_checkpoint",
            input["idempotencyKey"].as_str(),
        )
        .map_err(repository_error)?;
        let mut payload = repo
            .expand_draft_patch(
                &project,
                &task.task_id,
                input["expectedCandidateRevisionId"].as_str(),
                &input["patch"],
            )
            .map_err(repository_error)?;
        let target: Value = serde_json::from_str(&task.target_json)
            .map_err(|_| repository_error(RepositoryError::CorruptState))?;
        if let Some(end) =
            crate::context_visibility::writer_scene_limit(&target).map_err(repository_error)?
        {
            crate::context_visibility::visible_draft_prefix(&mut payload, end)
                .map_err(repository_error)?;
        }
        input.as_object_mut().unwrap().remove("patch");
        input["payload"] = payload;
    }
    serde_json::from_value(input)
        .map_err(|_| bad_request("invalid_draft_request", "Draft save envelope is invalid"))
}

pub(crate) async fn save_draft_checkpoint(
    State(state): State<RuntimeState>,
    Path(project_id): Path<String>,
    Extension(actor): Extension<Actor>,
    headers: HeaderMap,
    Json(input): Json<Value>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |mut repository| {
    let request = match prepare_draft_request(&mut repository, actor, &headers, &project_id, input) {
        Ok(request) => request,
        Err(response) => return response,
    };
    let novel_codex_story_core::DraftContent { chapter_id, number: chapter_number, title, body: draft_body, mut scene_groups } = match novel_codex_story_core::DraftContent::from_payload(&request.payload) {
        Ok(content) => content,
        Err(source) => return candidate_write_error(source),
    };
    let chapter_title = title.as_str();
    let body = draft_body.as_str();
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
    let mut source_plan_refs = request.payload
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
            if let Some(binding) = bindings
                .iter()
                .find(|binding| binding.chapter_id == chapter_id)
            {
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
    let request_digest = crate::draft_identity::draft_request_digest(
        &request.payload, request.expected_revision, expected_candidate.as_ref().map(RevisionId::as_str),
    );
    if let Err(source) = crate::recovery_binding::bind(
        &mut repository,
        &headers,
        task.as_ref(),
        "save_draft_checkpoint",
        Some(idempotency_key.as_str()),
    ) {
        return repository_error(source);
    }
    let completeness = request.payload.get("completeness").cloned().unwrap_or_else(|| json!("chapterComplete"));
    let result = repository.candidates().save_draft(novel_codex_story_core::DraftSubmission {
        context: novel_codex_story_core::CandidateContext {
            project_id: project_id.clone(), expected_revision: request.expected_revision,
            expected_candidate, idempotency_key, author: actor == Actor::Author,
            task_output: task.as_ref().map(|task| TaskOutputBinding {
                task_id: task.task_id.clone(), thread_id: task.thread_id.clone(), output_kind: "draft".into(),
                completeness: completeness.as_str().map(str::to_owned),
            }),
        },
        chapter_id: chapter_id.clone(), number: chapter_number, title: chapter_title.to_owned(),
        body, scene_groups, source_plan_refs, completeness, request_digest,
    });
    match result {
        Ok(receipt) => match draft_revision(&repository, &receipt.revision_id) {
            Ok(mut stored) => {
                let saved_source = task.as_ref().map(|task| {
                    match repository.task_saved_draft_source(task, receipt.revision_id.as_str()) {
                        Ok(source) => json!({"status":"available","sourceId":source["sourceId"],"revisionId":source["revisionId"],"contentRead":false}),
                        Err(_) => json!({"status":"unavailable","contentRead":false}),
                    }
                });
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
                        "completeness": stored["completeness"],
                        "savedSource": saved_source,
                        "checkpointId": format!("checkpoint-{}", receipt.revision_id.as_str())
                    })),
                )
                    .into_response()
            }
            Err(error) => repository_error(error),
        },
        Err(error) => candidate_write_error(error),
    }

    }).await
}

pub(crate) async fn save_artifact(
    State(state): State<RuntimeState>,
    Path((project_id, kind)): Path<(String, String)>,
    Extension(actor): Extension<Actor>,
    headers: HeaderMap,
    Json(request): Json<SaveArtifactRequest>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |mut repository| {
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
        let expected_candidate = match request.expected_candidate_revision_id {
            Some(revision_id) => match RevisionId::parse(revision_id) {
                Ok(revision_id) => Some(revision_id),
                Err(error) => return bad_request("invalid_revision_id", error),
            },
            None => None,
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
        let idempotency_key = match IdempotencyKey::parse(request.idempotency_key) {
            Ok(key) => key,
            Err(source) => return bad_request("invalid_idempotency_key", source),
        };
        let result =
            repository
                .candidates()
                .save_artifact(novel_codex_story_core::ArtifactSubmission {
                    context: novel_codex_story_core::CandidateContext {
                        project_id: project_id.clone(),
                        expected_revision: request.expected_revision,
                        expected_candidate,
                        idempotency_key,
                        author: actor == Actor::Author,
                        task_output: task.map(|task| TaskOutputBinding {
                            task_id: task.task_id,
                            thread_id: task.thread_id,
                            output_kind: kind.clone(),
                            completeness: None,
                        }),
                    },
                    kind: kind.clone(),
                    payload: request.payload,
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
            Err(error) => candidate_write_error(error),
        }
    })
    .await
}

pub(crate) async fn chapter_draft_history(
    State(state): State<RuntimeState>,
    Path((project_id, chapter_id, revision_id)): Path<(String, String, String)>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |repository| {
        let project_id = match ProjectId::parse(project_id) {
            Ok(value) => value,
            Err(source) => return bad_request("invalid_project_id", source),
        };
        let revision_id = match RevisionId::parse(revision_id) {
            Ok(value) => value,
            Err(source) => return bad_request("invalid_revision_id", source),
        };

        if let Err(source) =
            repository.chapter_draft_history(&project_id, &chapter_id, &revision_id)
        {
            return repository_error(source);
        }
        match draft_revision(&repository, &revision_id) {
            Ok(revision) => Json(json!({ "projectId": project_id.as_str(), "revision": revision }))
                .into_response(),
            Err(source) => repository_error(source),
        }
    })
    .await
}

pub(crate) fn candidate_write_error(
    source: novel_codex_story_core::CandidateWriteError,
) -> Response {
    use novel_codex_story_core::CandidateWriteError;
    match source {
        CandidateWriteError::Repository(source) => repository_error(source),
        CandidateWriteError::InvalidDraft { code, message } => {
            error(StatusCode::BAD_REQUEST, code, message)
        }
        CandidateWriteError::DraftTooLarge => error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "draft_too_large",
            "Draft exceeds 120000 characters",
        ),
        CandidateWriteError::ArtifactTooLarge => error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "artifact_too_large",
            "Artifact exceeds 48 KiB",
        ),
        CandidateWriteError::MissingExpectedRevision => error(
            StatusCode::CONFLICT,
            "candidate_revision_conflict",
            "expectedCandidateRevisionId does not identify a saved revision. Copy the frozen manifest CAS or the last successful save receipt exactly. For a first save, use JSON null, not the string \"null\"; do not invent an ID.",
        ),
    }
}

/// Author/Host only; authentication is applied by server composition.
pub(crate) fn routes() -> axum::Router<crate::server::RuntimeState> {
    axum::Router::new().route(
        "/api/projects/{project_id}/chapters/{chapter_id}/revisions/{revision_id}",
        axum::routing::get(crate::candidate_api::chapter_draft_history),
    )
}

/// Author or candidate; task capabilities remain checked by the handler.
pub(crate) fn writer_routes() -> axum::Router<crate::server::RuntimeState> {
    axum::Router::new()
        .route(
            "/api/projects/{project_id}/artifacts/{kind}",
            axum::routing::post(crate::candidate_api::save_artifact),
        )
        .route(
            "/api/projects/{project_id}/drafts/checkpoints",
            axum::routing::post(crate::candidate_api::save_draft_checkpoint),
        )
}

use novel_codex_story_core::{BoundWriterPlanRepository, ChapterHistoryRepository, ChaptersRepository, DraftPatchRepository, ProjectRevisionRepository, ProtectedScenesRepository, TaskWorkspaceRepository};
