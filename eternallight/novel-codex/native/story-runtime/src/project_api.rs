use axum::Json;
use axum::extract::Path;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::response::Response;
use novel_codex_story_core::BranchId;
use novel_codex_story_core::ProjectId;
use novel_codex_story_core::RepositoryError;
use novel_codex_story_core::RevisionId;
use serde_json::Value;
use serde_json::json;

use crate::contracts::*;
use crate::server::{
    RuntimeState, bad_request, error, internal_error, repository_error, task_json,
};
use crate::workspace_projection::*;
pub(crate) async fn set_artifact_lock(
    State(state): State<RuntimeState>,
    Path((project_id, kind)): Path<(String, String)>,
    Json(request): Json<LockArtifactRequest>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |mut repository| {
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

        match repository.candidates().set_artifact_lock(
            &project_id,
            &kind,
            request.locked,
            request.expected_revision,
        ) {
            Ok(result_revision) => Json(json!({
                "artifactKind": kind, "locked": request.locked, "resultRevision": result_revision
            }))
            .into_response(),
            Err(source) => repository_error(source),
        }
    })
    .await
}

pub(crate) async fn workspace_state(
    State(state): State<RuntimeState>,
    Path(project_id): Path<String>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |repository| {
    let project_id = match ProjectId::parse(project_id) {
        Ok(project_id) => project_id,
        Err(error) => return bad_request("invalid_project_id", error),
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
        &BranchId::main(),
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

    }).await
}

pub(crate) async fn list_projects(State(state): State<RuntimeState>) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |repository| {
        match repository.list_projects() {
            Ok(projects) => Json(json!({
                "projects": projects.into_iter().map(ProjectResponse::from).collect::<Vec<_>>()
            }))
            .into_response(),
            Err(error) => internal_error(error),
        }
    })
    .await
}

/// Author/Host only; authentication is applied by server composition.
pub(crate) fn routes() -> axum::Router<crate::server::RuntimeState> {
    axum::Router::new()
        .route(
            "/api/projects",
            axum::routing::get(crate::project_api::list_projects)
                .post(crate::multimedia_api::create_project),
        )
        .route(
            "/api/projects/{project_id}/creative-workspace",
            axum::routing::get(crate::project_api::workspace_state),
        )
        .route(
            "/api/projects/{project_id}/artifacts/{kind}/lock",
            axum::routing::patch(crate::project_api::set_artifact_lock),
        )
}

use novel_codex_story_core::{BoundWriterPlanRepository, ChaptersRepository, ImpactsRepository, ModelingRepository, PlanningReviewsRepository, ProjectRevisionRepository, TaskInputRepository, TasksRepository, WorkflowRepository, WorkspaceSelectionRepository};
