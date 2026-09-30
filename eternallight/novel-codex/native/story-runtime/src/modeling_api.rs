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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BeginRequest {
    object_id: String,
    source_revision_id: String,
    expected_revision: i64,
    extractor_version: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PublishRequest {
    task_id: String,
    thread_id: String,
    expected_revision: i64,
    entities: Vec<Value>,
    assertions: Vec<Value>,
}

#[derive(Deserialize)]
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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EvolutionQueryRequest {
    dimension: String,
    project_revision: Option<i64>,
    snapshot_id: Option<String>,
    limit: Option<usize>,
    cursor: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EntityCatalogQueryRequest {
    snapshot_id: Option<String>,
    name: Option<String>,
}

#[derive(Deserialize)]
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
    let (repository, project_id) = match open(&state, project_id) {
        Ok(value) => value,
        Err(response) => return *response,
    };
    let snapshot_id = match query.snapshot_id.map(SnapshotId::parse).transpose() {
        Ok(value) => value,
        Err(source) => return bad_request("invalid_snapshot_id", source),
    };
    let direction = match RelationDirection::parse(query.direction.as_deref().unwrap_or("both")) {
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
}

pub(crate) async fn entity_catalog(
    State(state): State<RuntimeState>,
    Path(project_id): Path<String>,
    Query(query): Query<EntityCatalogQueryRequest>,
) -> Response {
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
}

pub(crate) async fn begin_batch(
    State(state): State<RuntimeState>,
    Path(project_id): Path<String>,
    Json(request): Json<BeginRequest>,
) -> Response {
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
}

pub(crate) async fn publish_batch(
    State(state): State<RuntimeState>,
    Path((project_id, batch_id)): Path<(String, String)>,
    Extension(actor): Extension<Actor>,
    headers: HeaderMap,
    Json(request): Json<PublishRequest>,
) -> Response {
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
        Ok(batch) => Json(batch.as_json()).into_response(),
        Err(source) => repository_error(source),
    }
}

pub(crate) async fn current_knowledge(
    State(state): State<RuntimeState>,
    Path(project_id): Path<String>,
) -> Response {
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
}

pub(crate) async fn snapshot_knowledge(
    State(state): State<RuntimeState>,
    Path((project_id, snapshot_id, object_id)): Path<(String, String, String)>,
) -> Response {
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
}

pub(crate) async fn knowledge_state(
    State(state): State<RuntimeState>,
    Path(project_id): Path<String>,
    Query(query): Query<StateQueryRequest>,
) -> Response {
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
}

pub(crate) async fn evolution_trace(
    State(state): State<RuntimeState>,
    Path(project_id): Path<String>,
    Query(query): Query<EvolutionQueryRequest>,
) -> Response {
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
}

fn open(
    state: &RuntimeState,
    project_id: String,
) -> Result<(SqliteStoryRepository, ProjectId), Box<Response>> {
    let project_id = ProjectId::parse(project_id)
        .map_err(|source| Box::new(bad_request("invalid_project_id", source)))?;
    let repository = SqliteStoryRepository::open(&state.database_path)
        .map_err(|source| Box::new(repository_error(source)))?;
    Ok((repository, project_id))
}
