use axum::{
    Json,
    extract::{Extension, Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use novel_codex_story_core::{
    BeginModelingBatch, EvolutionDimension, EvolutionTraceQuery, KnowledgePerspective,
    KnowledgeStateQuery, ObjectId, ProjectId, PublishModelingBatch, RelationDirection,
    RelationGraphQuery, RelationGraphScope, RevisionId, SnapshotId, SqliteStoryRepository,
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::server::{
    Actor, RuntimeState, bad_request, candidate_task, error, repository_error, unix_now,
};

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "ModelingApiBeginRequest", optional_fields = nullable)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BeginRequest {
    object_id: String,
    source_revision_id: String,
    expected_revision: i64,
    extractor_version: String,
}

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "ModelingApiPublishRequest", optional_fields = nullable)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PublishRequest {
    task_id: String,
    thread_id: String,
    expected_revision: i64,
    entities: Vec<Value>,
    assertions: Vec<Value>,
}

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "ModelingApiStateQueryRequest", optional_fields = nullable)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StateQueryRequest {
    perspective: String,
    snapshot_id: Option<String>,
    world_time: Option<i64>,
    epistemic_time: Option<i64>,
    reading_chapter: Option<i64>,
    owner_id: Option<String>,
    entity_id: Option<String>,
    predicate: Option<String>,
}

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "ModelingApiEvolutionQueryRequest", optional_fields = nullable)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EvolutionQueryRequest {
    dimension: String,
    project_revision: Option<i64>,
    snapshot_id: Option<String>,
    limit: Option<usize>,
    cursor: Option<String>,
}

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "ModelingApiEntityCatalogQueryRequest", optional_fields = nullable)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EntityCatalogQueryRequest {
    snapshot_id: Option<String>,
    name: Option<String>,
}

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "ModelingApiRelationGraphQueryRequest", optional_fields = nullable)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RelationGraphQueryRequest {
    entity_id: String,
    snapshot_id: Option<String>,
    relation_type: Option<String>,
    direction: Option<String>,
    world_time: Option<i64>,
    max_hops: Option<usize>,
    limit: Option<usize>,
}

pub(crate) async fn relation_graph(
    State(state): State<RuntimeState>,
    Path(project_id): Path<String>,
    Query(query): Query<RelationGraphQueryRequest>,
) -> Response {
    crate::repository_executor::run(state.clone(), move || {
        let (repository, project_id) = match open(&state, project_id) {
            Ok(value) => value,
            Err(response) => return *response,
        };
        let snapshot_id = match query.snapshot_id.map(SnapshotId::parse).transpose() {
            Ok(value) => value,
            Err(source) => return bad_request("invalid_snapshot_id", source),
        };
        let direction = match RelationDirection::parse(query.direction.as_deref().unwrap_or("both"))
        {
            Ok(value) => value,
            Err(source) => return repository_error(source),
        };
        match repository.query_relation_graph(RelationGraphQuery {
            project_id: &project_id,
            scope: snapshot_id
                .as_ref()
                .map_or(RelationGraphScope::Current, RelationGraphScope::Snapshot),
            entity_id: &query.entity_id,
            relation_type: query.relation_type.as_deref(),
            direction,
            world_time: query.world_time,
            max_hops: query.max_hops.unwrap_or(1),
            limit: query.limit.unwrap_or(40),
        }) {
            Ok(result) => Json(result).into_response(),
            Err(source) => repository_error(source),
        }
    })
    .await
}

pub(crate) async fn entity_catalog(
    State(state): State<RuntimeState>,
    Path(project_id): Path<String>,
    Query(query): Query<EntityCatalogQueryRequest>,
) -> Response {
    crate::repository_executor::run(state.clone(), move || {
        let (repository, project_id) = match open(&state, project_id) {
            Ok(value) => value,
            Err(response) => return *response,
        };
        let snapshot_id = match query.snapshot_id.map(SnapshotId::parse).transpose() {
            Ok(value) => value,
            Err(source) => return bad_request("invalid_snapshot_id", source),
        };
        let result = match (snapshot_id.as_ref(), query.name.as_deref()) {
            (Some(snapshot_id), Some(name)) => {
                repository.resolve_entity_name_at_snapshot(&project_id, snapshot_id, name, 60_000)
            }
            (Some(snapshot_id), None) => {
                repository.modeling_entity_catalog_at_snapshot(&project_id, snapshot_id, 60_000)
            }
            (None, Some(name)) => repository.resolve_entity_name(&project_id, name, 60_000),
            (None, None) => repository.modeling_entity_catalog(&project_id, 60_000),
        };
        match result {
            Ok(catalog) => Json(catalog).into_response(),
            Err(source) => repository_error(source),
        }
    })
    .await
}

pub(crate) async fn begin_batch(
    State(state): State<RuntimeState>,
    Path(project_id): Path<String>,
    Json(request): Json<BeginRequest>,
) -> Response {
    crate::repository_executor::run(state.clone(), move || {
        let (mut repository, project_id) = match open(&state, project_id) {
            Ok(value) => value,
            Err(response) => return *response,
        };
        let object_id = match ObjectId::parse(request.object_id) {
            Ok(value) => value,
            Err(source) => return bad_request("invalid_object_id", source),
        };
        let source_revision_id = match RevisionId::parse(request.source_revision_id) {
            Ok(value) => value,
            Err(source) => return bad_request("invalid_revision_id", source),
        };
        match repository.begin_modeling_batch(BeginModelingBatch {
            project_id: &project_id,
            object_id: &object_id,
            source_revision_id: &source_revision_id,
            expected_revision: request.expected_revision,
            extractor_version: &request.extractor_version,
            now: unix_now(),
        }) {
            Ok(batch) => (StatusCode::CREATED, Json(batch.as_json())).into_response(),
            Err(source) => repository_error(source),
        }
    })
    .await
}

pub(crate) async fn publish_batch(
    State(state): State<RuntimeState>,
    Path((project_id, batch_id)): Path<(String, String)>,
    Extension(actor): Extension<Actor>,
    headers: HeaderMap,
    Json(request): Json<PublishRequest>,
) -> Response {
    crate::repository_executor::run(state.clone(), move || {
    let (mut repository, project_id) = match open(&state, project_id) {
        Ok(value) => value,
        Err(response) => return *response,
    };
    let task = match candidate_task(
        &repository,
        actor,
        &headers,
        &project_id,
        Some(&request.task_id),
        Some(&request.thread_id),
        Some(request.expected_revision),
    ) {
        Ok(Some(task)) => task,
        Ok(None) => {
            return error(
                StatusCode::FORBIDDEN,
                "candidate_required",
                "A bound modeling task is required",
            );
        }
        Err(response) => return *response,
    };
    if task.operation != "modelKnowledge" {
        return error(
            StatusCode::FORBIDDEN,
            "task_scope_invalid",
            "Task does not permit knowledge modeling",
        );
    }
    if let Err(source) = crate::recovery_binding::bind(
        &mut repository,
        &headers,
        Some(&task),
        "publish_knowledge_batch",
        None,
    ) {
        return repository_error(source);
    }
    match repository.publish_modeling_batch(PublishModelingBatch {
        project_id: &project_id,
        batch_id: &batch_id,
        task_id: &task.task_id,
        thread_id: &request.thread_id,
        entities: &request.entities,
        assertions: &request.assertions,
        now: unix_now(),
    }) {
        Ok(batch) => Json(json!({
            "batchId": batch.id,
            "status": "ready",
            "publicationState": "committed",
            "receiptRef": format!("runtime:knowledge:{}", batch.id),
            "entityCount": batch.entities.len(),
            "assertionCount": batch.assertions.len(),
            "nextAction": "complete_task",
            "instruction": "Publication is durable and immutable. Do not publish again or change this batch. Complete the bound task now; candidate knowledge is not confirmed Canon."
        })).into_response(),
        Err(novel_codex_story_core::RepositoryError::IdempotencyConflict) => (
            StatusCode::CONFLICT,
            Json(json!({"error": {
                "code": "conflict",
                "message": "This batch was already published. Its persisted content cannot be changed by retrying publication.",
                "details": {"batchId": batch_id, "publicationState": "committed"},
                "recovery": {"nextAction": "complete_task", "retryPublication": false}
            }})),
        ).into_response(),
        Err(novel_codex_story_core::RepositoryError::InvalidContent(reason)) => {
            if let Some(feedback) = evidence_feedback(&reason) {
                (StatusCode::BAD_REQUEST, Json(feedback)).into_response()
            } else {
                repository_error(novel_codex_story_core::RepositoryError::InvalidContent(reason))
            }
        }
        Err(source) => repository_error(source),
    }

    }).await
}

pub(crate) async fn current_knowledge(
    State(state): State<RuntimeState>,
    Path(project_id): Path<String>,
) -> Response {
    crate::repository_executor::run(state.clone(), move || {
        let (repository, project_id) = match open(&state, project_id) {
            Ok(value) => value,
            Err(response) => return *response,
        };
        match repository.current_knowledge(&project_id) {
            Ok(items) => {
                Json(json!({"projectId": project_id.as_str(), "sources": items})).into_response()
            }
            Err(source) => repository_error(source),
        }
    })
    .await
}

pub(crate) async fn snapshot_knowledge(
    State(state): State<RuntimeState>,
    Path((project_id, snapshot_id, object_id)): Path<(String, String, String)>,
) -> Response {
    crate::repository_executor::run(state.clone(), move || {
        let (repository, project_id) = match open(&state, project_id) {
            Ok(value) => value,
            Err(response) => return *response,
        };
        let snapshot_id = match SnapshotId::parse(snapshot_id) {
            Ok(value) => value,
            Err(source) => return bad_request("invalid_snapshot_id", source),
        };
        let object_id = match ObjectId::parse(object_id) {
            Ok(value) => value,
            Err(source) => return bad_request("invalid_object_id", source),
        };
        match repository.knowledge_at_snapshot(&project_id, &snapshot_id, &object_id) {
            Ok(source) => Json(source).into_response(),
            Err(source) => repository_error(source),
        }
    })
    .await
}

pub(crate) async fn knowledge_state(
    State(state): State<RuntimeState>,
    Path(project_id): Path<String>,
    Query(query): Query<StateQueryRequest>,
) -> Response {
    crate::repository_executor::run(state.clone(), move || {
        let (repository, project_id) = match open(&state, project_id) {
            Ok(value) => value,
            Err(response) => return *response,
        };
        let perspective = match KnowledgePerspective::parse(&query.perspective) {
            Ok(value) => value,
            Err(source) => return repository_error(source),
        };
        let snapshot_id = match query.snapshot_id.map(SnapshotId::parse).transpose() {
            Ok(value) => value,
            Err(source) => return bad_request("invalid_snapshot_id", source),
        };
        match repository.knowledge_state(KnowledgeStateQuery {
            project_id: &project_id,
            snapshot_id: snapshot_id.as_ref(),
            perspective,
            world_time: query.world_time,
            epistemic_time: query.epistemic_time,
            reading_chapter: query.reading_chapter,
            owner_id: query.owner_id.as_deref(),
            entity_id: query.entity_id.as_deref(),
            predicate: query.predicate.as_deref(),
        }) {
            Ok(state) => Json(state).into_response(),
            Err(source) => repository_error(source),
        }
    })
    .await
}

pub(crate) async fn evolution_trace(
    State(state): State<RuntimeState>,
    Path(project_id): Path<String>,
    Query(query): Query<EvolutionQueryRequest>,
) -> Response {
    crate::repository_executor::run(state.clone(), move || {
        let (repository, project_id) = match open(&state, project_id) {
            Ok(value) => value,
            Err(response) => return *response,
        };
        let dimension = match EvolutionDimension::parse(&query.dimension) {
            Ok(value) => value,
            Err(source) => return repository_error(source),
        };
        let snapshot_id = match query.snapshot_id.map(SnapshotId::parse).transpose() {
            Ok(value) => value,
            Err(source) => return bad_request("invalid_snapshot_id", source),
        };
        match repository.evolution_trace(EvolutionTraceQuery {
            project_id: &project_id,
            expected_revision: query.project_revision,
            snapshot_id: snapshot_id.as_ref(),
            dimension,
            limit: query.limit.unwrap_or(50),
            cursor: query.cursor.as_deref(),
        }) {
            Ok(trace) => Json(trace).into_response(),
            Err(source) => repository_error(source),
        }
    })
    .await
}

fn open(
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

// These errors originate before publication, after checking the task's source.
// Give the agent a repair contract without modifying its requested evidence.
fn evidence_feedback(reason: &str) -> Option<Value> {
    let (field, code) = reason.split_once(": ")?;
    let instruction = match code {
        "evidence_anchor_mismatch" | "evidence_anchor_invalid" | "evidence_anchor_incomplete" => {
            "Use a verbatim unique quote with only {quote}; Runtime resolves its offsets. If the quote repeats, call resolve_story_evidence and use one exact returned match. Offsets count Unicode characters, not bytes or UTF-16 units."
        }
        "evidence_quote_not_found" => {
            "Read the bound source and copy a verbatim quote, including punctuation and whitespace. Do not paraphrase evidence or reuse another revision's offsets."
        }
        "evidence_quote_ambiguous" => {
            "Extend the verbatim quote until unique, or call resolve_story_evidence and choose the intended exact match. Do not guess offsets."
        }
        "evidence_shape_invalid" | "evidence_quote_invalid" => {
            "Evidence requires a nonempty verbatim quote of at most 2000 Unicode characters. Only quote and optional paired start/end fields are allowed."
        }
        _ => return None,
    };
    Some(json!({"error": {
        "code": "invalid_source_evidence", "message": reason,
        "details": {"field": field, "reason": code, "publicationState": "rejected"},
        "recovery": {"nextAction": "correct_evidence", "instruction": instruction,
            "preserveValidEntries": true, "retryUnchanged": false}
    }}))
}

#[cfg(test)]
mod evidence_feedback_tests {
    use super::*;

    #[test]
    fn evidence_error_identifies_the_failed_field_and_a_local_repair() {
        let feedback =
            evidence_feedback("assertions[3].evidence: evidence_anchor_mismatch").unwrap();
        assert_eq!(
            feedback["error"]["details"]["field"],
            "assertions[3].evidence"
        );
        assert_eq!(feedback["error"]["details"]["publicationState"], "rejected");
        assert_eq!(feedback["error"]["recovery"]["retryUnchanged"], false);
        assert!(
            feedback["error"]["recovery"]["instruction"]
                .as_str()
                .unwrap()
                .contains("{quote}")
        );
        assert!(evidence_feedback("entities[0].evidence: evidence_quote_ambiguous").is_some());
        assert!(evidence_feedback("task_input_stale").is_none());
        assert!(evidence_feedback("assertions[3]: unknown_error").is_none());
    }
}

/// Author/Host only; authentication is applied by server composition.
pub(crate) fn routes() -> axum::Router<crate::server::RuntimeState> {
    axum::Router::new()
        .route(
            "/api/projects/{project_id}/knowledge",
            axum::routing::get(crate::modeling_api::current_knowledge),
        )
        .route(
            "/api/projects/{project_id}/knowledge/snapshots/{snapshot_id}/objects/{object_id}",
            axum::routing::get(crate::modeling_api::snapshot_knowledge),
        )
        .route(
            "/api/projects/{project_id}/knowledge/state",
            axum::routing::get(crate::modeling_api::knowledge_state),
        )
        .route(
            "/api/projects/{project_id}/knowledge/entities",
            axum::routing::get(crate::modeling_api::entity_catalog),
        )
        .route(
            "/api/projects/{project_id}/knowledge/relations",
            axum::routing::get(crate::modeling_api::relation_graph),
        )
        .route(
            "/api/projects/{project_id}/knowledge/evolution",
            axum::routing::get(crate::modeling_api::evolution_trace),
        )
        .route(
            "/api/projects/{project_id}/modeling-batches",
            axum::routing::post(crate::modeling_api::begin_batch),
        )
}

/// Author or candidate; task capabilities remain checked by the handler.
pub(crate) fn writer_routes() -> axum::Router<crate::server::RuntimeState> {
    axum::Router::new().route(
        "/api/projects/{project_id}/modeling-batches/{batch_id}/publish",
        axum::routing::post(crate::modeling_api::publish_batch),
    )
}

use novel_codex_story_core::{EntityIdentityRepository, EvolutionRepository, KnowledgeProjectionRepository, ModelingOperationRepository, ModelingRepository, RelationGraphRepository, SnapshotKnowledgeRepository};
