use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use axum::Extension;
use axum::Json;
use axum::extract::Path;
use axum::extract::Query;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::response::Response;
use novel_codex_story_core::ObjectHead;
use novel_codex_story_core::ProjectId;
use novel_codex_story_core::RelationDirection;
use novel_codex_story_core::RelationGraphQuery;
use novel_codex_story_core::RelationGraphScope;
use novel_codex_story_core::RepositoryError;
use novel_codex_story_core::RevisionId;
use novel_codex_story_core::SearchQuery as DomainSearchQuery;
use novel_codex_story_core::SearchScope;
use novel_codex_story_core::SnapshotId;
use novel_codex_story_core::SqliteStoryRepository;
use novel_codex_story_core::TaskRecord;
use novel_codex_story_core::WriterKnowledgeScope;
use serde::Deserialize;
use serde_json::Value;
use serde_json::json;

use crate::server::Actor;
use crate::server::RuntimeState;
use crate::server::candidate_task;
use crate::server::error;
use crate::server::repository_error;

#[derive(Deserialize)]
pub(crate) struct SearchQueryRequest {
    #[serde(default)]
    q: String,
    #[serde(default = "default_search_limit")]
    limit: usize,
    kind: Option<String>,
    #[serde(rename = "snapshotId")]
    snapshot_id: Option<String>,
    #[serde(rename = "entityName")]
    entity_name: Option<String>,
    #[serde(rename = "entityId")]
    entity_id: Option<String>,
    cursor: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct TaskSearchQueryRequest {
    thread_id: String,
    expected_revision: String,
    #[serde(default)]
    q: String,
    limit: Option<String>,
    kind: Option<String>,
    snapshot_id: Option<String>,
    entity_name: Option<String>,
    entity_id: Option<String>,
    cursor: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct TaskRelationsQueryRequest {
    thread_id: String,
    expected_revision: String,
    entity_id: String,
    snapshot_id: Option<String>,
    relation_type: Option<String>,
    direction: Option<String>,
    world_time: Option<i64>,
    max_hops: Option<usize>,
    limit: Option<usize>,
}

pub(crate) async fn task_relations(
    State(state): State<RuntimeState>,
    Path((project_id, task_id)): Path<(String, String)>,
    Extension(actor): Extension<Actor>,
    headers: HeaderMap,
    Query(query): Query<TaskRelationsQueryRequest>,
) -> Response {
    if query.snapshot_id.is_some() {
        return error(
            StatusCode::BAD_REQUEST,
            "historical_relations_forbidden",
            "Task relations cannot select a historical snapshot",
        );
    }
    let Ok(revision) = query.expected_revision.parse::<i64>() else {
        return error(
            StatusCode::BAD_REQUEST,
            "invalid_relations",
            "Invalid task revision",
        );
    };
    if revision < 0 {
        return error(
            StatusCode::BAD_REQUEST,
            "invalid_relations",
            "Invalid task revision",
        );
    }
    let (repository, project_id) = match open(&state, project_id) {
        Ok(value) => value,
        Err(response) => return *response,
    };
    match repository.with_read_snapshot(|| {
        let task = match candidate_task(
            &repository,
            actor,
            &headers,
            &project_id,
            Some(&task_id),
            Some(&query.thread_id),
            Some(revision),
        ) {
            Ok(Some(task))
                if matches!(
                    task.operation.as_str(),
                    "generateChapter" | "assessRevisionOutcome"
                ) =>
            {
                task
            }
            Ok(_) => {
                return error(
                    StatusCode::FORBIDDEN,
                    "chapter_task_required",
                    "Bound chapter task required",
                );
            }
            Err(response) => return *response,
        };
        if let Err(source) = repository.assert_task_inputs_current(&task) {
            return repository_error(source);
        }
        let (reading_chapter, target) = match bound_task_chapter(&repository, &project_id, &task) {
            Ok(value) => value,
            Err(source) => return repository_error(source),
        };
        // Reuse the indexed, source-bound chapter projection, including this assessment's
        // current chapter but never future chapters. The assessor cannot choose the bound.
        let relation_bound = if task.operation == "assessRevisionOutcome" {
            match reading_chapter.checked_add(1) {
                Some(bound) => bound,
                None => return repository_error(RepositoryError::TaskBindingInvalid),
            }
        } else {
            reading_chapter
        };
        let pov = match target.get("povCharacterId") {
            None | Some(Value::Null) => None,
            Some(Value::String(id)) if !id.is_empty() && id.len() <= 160 => Some(id.as_str()),
            _ => return repository_error(RepositoryError::TaskBindingInvalid),
        };
        let target_story_time = match target.get("storyTime") {
            None | Some(Value::Null) => None,
            Some(Value::Number(value)) => match value.as_i64() {
                Some(time) => Some(time),
                None => return repository_error(RepositoryError::TaskBindingInvalid),
            },
            _ => return repository_error(RepositoryError::TaskBindingInvalid),
        };
        if pov.is_some()
            && query
                .world_time
                .is_some_and(|time| Some(time) != target_story_time)
        {
            return error(
                StatusCode::BAD_REQUEST,
                "pov_time_override_forbidden",
                "POV relation time is bound to the Writer task",
            );
        }
        let direction = match RelationDirection::parse(query.direction.as_deref().unwrap_or("both"))
        {
            Ok(value) => value,
            Err(source) => return repository_error(source),
        };
        match repository.query_relation_graph(RelationGraphQuery {
            project_id: &project_id,
            scope: pov.map_or(
                RelationGraphScope::WriterBeforeChapter(relation_bound),
                |character_id| RelationGraphScope::WriterPov {
                    chapter: reading_chapter,
                    character_id,
                },
            ),
            entity_id: &query.entity_id,
            relation_type: query.relation_type.as_deref(),
            direction,
            world_time: if pov.is_some() {
                target_story_time
            } else {
                query.world_time
            },
            max_hops: query.max_hops.unwrap_or(1),
            limit: query.limit.unwrap_or(20),
        }) {
            Ok(mut value) => {
                if task.operation == "assessRevisionOutcome" {
                    value["scope"] = json!("revisionOutcome");
                }
                Json(value).into_response()
            }
            Err(source) => repository_error(source),
        }
    }) {
        Ok(response) => response,
        Err(source) => repository_error(source),
    }
}

pub(crate) async fn task_search(
    State(state): State<RuntimeState>,
    Path((project_id, task_id)): Path<(String, String)>,
    Extension(actor): Extension<Actor>,
    headers: HeaderMap,
    Query(query): Query<TaskSearchQueryRequest>,
) -> Response {
    if query.snapshot_id.is_some() {
        return error(
            StatusCode::BAD_REQUEST,
            "historical_search_forbidden",
            "Task search cannot select a historical snapshot",
        );
    }
    let limit = query.limit.as_deref().unwrap_or("10").parse::<usize>();
    let revision = query.expected_revision.parse::<i64>();
    let (Ok(limit), Ok(revision)) = (limit, revision) else {
        return error(
            StatusCode::BAD_REQUEST,
            "invalid_search",
            "Search query is invalid",
        );
    };
    if query.q.chars().count() > 240 || !(1..=20).contains(&limit) || revision < 0 {
        return error(
            StatusCode::BAD_REQUEST,
            "invalid_search",
            "Search query is invalid",
        );
    }
    if query.entity_id.is_some() && query.entity_name.is_none() {
        return error(
            StatusCode::BAD_REQUEST,
            "invalid_search",
            "entityId requires entityName",
        );
    }
    let (repository, project_id) = match open(&state, project_id) {
        Ok(value) => value,
        Err(response) => return *response,
    };
    match repository.with_read_snapshot(|| {
        let task = match candidate_task(
            &repository,
            actor,
            &headers,
            &project_id,
            Some(&task_id),
            Some(&query.thread_id),
            Some(revision),
        ) {
            Ok(task) => task,
            Err(response) => return *response,
        };
        let mut writer_reading_chapter = None;
        let mut writer_pov = None;
        let mut writer_story_time = None;
        if let Some(task) = task.as_ref() {
            if let Err(source) = repository.assert_task_inputs_current(task) {
                return repository_error(source);
            }
            if matches!(
                task.operation.as_str(),
                "generateChapter" | "modelKnowledge"
            ) {
                let (chapter_number, target) =
                    match bound_task_chapter(&repository, &project_id, task) {
                        Ok(value) => value,
                        Err(source) => return repository_error(source),
                    };
                if task.operation == "generateChapter" {
                    writer_reading_chapter = Some(chapter_number);
                    writer_pov = match target.get("povCharacterId") {
                        None | Some(Value::Null) => None,
                        Some(Value::String(id)) if !id.is_empty() && id.len() <= 160 => {
                            Some(id.clone())
                        }
                        _ => return repository_error(RepositoryError::TaskBindingInvalid),
                    };
                    writer_story_time = match target.get("storyTime") {
                        None | Some(Value::Null) => None,
                        Some(Value::Number(value)) => match value.as_i64() {
                            Some(time) => Some(time),
                            None => return repository_error(RepositoryError::TaskBindingInvalid),
                        },
                        _ => return repository_error(RepositoryError::TaskBindingInvalid),
                    };
                }
            }
        }
        let scope = match task
            .as_ref()
            .map(|task| repository.task_text_search_scope(task))
            .transpose()
        {
            Ok(scope) => scope.unwrap_or(SearchScope::All),
            Err(source) => return repository_error(source),
        };
        if query.entity_name.is_some() && writer_reading_chapter.is_none() {
            return error(
                StatusCode::BAD_REQUEST,
                "invalid_search",
                "Structured task search requires a Writer task",
            );
        }
        if query.entity_name.is_some() && let Some(task) = task.as_ref() {
            match repository.task_source_projection_aligned(task) {
                Ok(true) => {}
                Ok(false) => return error(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "source_modeling_incomplete",
                    "Selected text source differs from the modeled draft; structured knowledge is unavailable",
                ),
                Err(source) => return repository_error(source),
            }
        }
        if writer_pov.is_some() && query.cursor.is_some() {
            return error(
                StatusCode::BAD_REQUEST,
                "pov_text_cursor_forbidden",
                "POV-limited text search cannot paginate unverified prose",
            );
        }
        let search = DomainSearchQuery {
            project_id: &project_id,
            snapshot_id: None,
            text: if writer_pov.is_some() { "" } else { &query.q },
            kind: query.kind.as_deref(),
            scope,
            limit,
            cursor: query.cursor.as_deref(),
        };
        let result = if let Some(task) = task.as_ref() {
            repository.search_documents_for_task(task, search)
        } else {
            repository.search_documents(search)
        };
        match result {
            Ok(mut result) => {
                if writer_pov.is_some() && !query.q.trim().is_empty() {
                    result["coverage"]["complete"] = json!(false);
                    result["coverage"]["povTextUnverified"] = json!(true);
                }
                if let (Some(name), Some(reading_chapter)) =
                    (query.entity_name.as_deref(), writer_reading_chapter)
                {
                    let knowledge = match repository.query_writer_knowledge_by_name(
                        &project_id,
                        writer_pov.as_deref().map_or(
                            WriterKnowledgeScope::PriorChapters(reading_chapter),
                            |character_id| WriterKnowledgeScope::CharacterPov {
                                chapter: reading_chapter,
                                character_id,
                                at_story_time: writer_story_time,
                            },
                        ),
                        name,
                        query.entity_id.as_deref(),
                        limit,
                    ) {
                        Ok(value) => value,
                        Err(source) => return repository_error(source),
                    };
                    result
                        .as_object_mut()
                        .expect("search response is an object")
                        .insert("knowledge".into(), knowledge);
                }
                Json(result).into_response()
            }
            Err(source) => repository_error(source),
        }
    }) {
        Ok(response) => response,
        Err(source) => repository_error(source),
    }
}

fn bound_task_chapter(
    repository: &SqliteStoryRepository,
    project_id: &ProjectId,
    task: &TaskRecord,
) -> Result<(i64, Value), RepositoryError> {
    let target: Value =
        serde_json::from_str(&task.target_json).map_err(|_| RepositoryError::CorruptState)?;
    let chapter_id = if task.operation == "modelKnowledge" {
        let batch_id = target["batchId"]
            .as_str()
            .ok_or(RepositoryError::TaskBindingInvalid)?;
        repository
            .modeling_batch(project_id, batch_id)?
            .object_id
            .strip_prefix("draft-")
            .ok_or(RepositoryError::TaskBindingInvalid)?
            .to_owned()
    } else {
        target["chapterId"]
            .as_str()
            .ok_or(RepositoryError::TaskBindingInvalid)?
            .to_owned()
    };
    let chapter = repository
        .chapters(project_id)?
        .into_iter()
        .find(|chapter| chapter.chapter_id == chapter_id)
        .ok_or(RepositoryError::TaskBindingInvalid)?;
    if target["chapterNumber"]
        .as_i64()
        .is_some_and(|number| number != chapter.number)
        || target["readingChapter"]
            .as_i64()
            .is_some_and(|number| number != chapter.number)
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    Ok((chapter.number, target))
}

#[derive(Deserialize)]
pub(crate) struct ExportRequest {
    scopes: Vec<String>,
    format: String,
}

#[derive(Deserialize)]
pub(crate) struct AbandonRequest {
    stage: String,
    #[serde(rename = "idempotencyKey")]
    idempotency_key: String,
}

fn default_search_limit() -> usize {
    20
}

pub(crate) async fn metrics(State(state): State<RuntimeState>) -> Response {
    let repository = match SqliteStoryRepository::open(&state.database_path) {
        Ok(value) => value,
        Err(error) => return repository_error(error),
    };
    match repository.local_metrics() {
        Ok(metrics) => Json(json!({"scope": "local-device", "metrics": metrics})).into_response(),
        Err(error) => repository_error(error),
    }
}

pub(crate) async fn record_abandonment(
    State(state): State<RuntimeState>,
    Json(request): Json<AbandonRequest>,
) -> Response {
    if !matches!(
        request.stage.as_str(),
        "brief" | "outline" | "plan" | "draft"
    ) || request.idempotency_key.len() != 36
    {
        return error(
            StatusCode::BAD_REQUEST,
            "invalid_metric_action",
            "Metric action is invalid",
        );
    }
    let mut repository = match SqliteStoryRepository::open(&state.database_path) {
        Ok(value) => value,
        Err(error) => return repository_error(error),
    };
    let metric = format!("stage.abandoned.{}", request.stage);
    match repository.record_local_metric(&metric, &request.idempotency_key) {
        Ok(metrics) => Json(json!({"scope": "local-device", "metrics": metrics})).into_response(),
        Err(error) => repository_error(error),
    }
}

pub(crate) async fn project_state(
    State(state): State<RuntimeState>,
    Path(project_id): Path<String>,
) -> Response {
    let (repository, project_id) = match open(&state, project_id) {
        Ok(value) => value,
        Err(response) => return *response,
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
    let heads = match repository.object_heads(&project_id) {
        Ok(value) => value,
        Err(source) => return repository_error(source),
    };
    let chapters = match repository.chapters(&project_id) {
        Ok(value) => value,
        Err(source) => return repository_error(source),
    };
    let chapter_values = chapters
        .into_iter()
        .map(|chapter| {
            let head = find_head(&heads, &format!("draft-{}", chapter.chapter_id));
            let revision = head.and_then(selected_revision);
            let draft = revision.and_then(|id| revision_json(&repository, id));
            json!({
                "id": chapter.chapter_id,
                "number": chapter.number,
                "title": chapter.title,
                "status": if head.and_then(|head| head.accepted_revision_id.as_ref()).is_some() { "accepted" } else { "draft" },
                "passages": draft.and_then(|value| value.get("body").and_then(Value::as_str).map(|body| vec![json!({"id": format!("passage-{}", chapter.chapter_id), "position": 1, "body": body, "beatIds": []})])).unwrap_or_default()
            })
        })
        .collect::<Vec<_>>();
    Json(json!({
        "project": {"id": project.project_id.as_str(), "title": project.title, "logline": project.logline, "created_at": project.created_at, "updated_at": project.updated_at},
        "canon": {"characters": [], "relationships": [], "events": [], "locations": [], "worldRules": [], "plotThreads": [], "notes": []},
        "narrativeIr": {"version": "2.0", "scenes": [], "anchors": []},
        "novelEdition": {"chapters": chapter_values}
    }))
    .into_response()
}

pub(crate) async fn search(
    State(state): State<RuntimeState>,
    Path(project_id): Path<String>,
    Query(query): Query<SearchQueryRequest>,
) -> Response {
    if query.q.chars().count() > 240
        || query.limit == 0
        || query.limit > 50
        || (query.entity_name.is_some() && query.snapshot_id.is_none())
        || (query.entity_id.is_some() && query.entity_name.is_none())
    {
        return error(
            StatusCode::BAD_REQUEST,
            "invalid_search",
            "Search query is invalid",
        );
    }
    let (repository, project_id) = match open(&state, project_id) {
        Ok(value) => value,
        Err(response) => return *response,
    };
    let snapshot_id = match query.snapshot_id.map(SnapshotId::parse).transpose() {
        Ok(value) => value,
        Err(source) => {
            return error(
                StatusCode::BAD_REQUEST,
                "invalid_snapshot_id",
                &source.to_string(),
            );
        }
    };
    match repository.with_read_snapshot(|| {
        match repository.search_documents(DomainSearchQuery {
            project_id: &project_id,
            snapshot_id: snapshot_id.as_ref(),
            text: &query.q,
            kind: query.kind.as_deref(),
            scope: SearchScope::All,
            limit: query.limit,
            cursor: query.cursor.as_deref(),
        }) {
            Ok(mut result) => {
                if let (Some(name), Some(snapshot_id)) =
                    (query.entity_name.as_deref(), snapshot_id.as_ref())
                {
                    let knowledge = match repository.query_knowledge_by_name(
                        &project_id,
                        snapshot_id,
                        name,
                        query.entity_id.as_deref(),
                        query.limit.min(40),
                    ) {
                        Ok(value) => value,
                        Err(source) => return repository_error(source),
                    };
                    result
                        .as_object_mut()
                        .expect("search response is an object")
                        .insert("knowledge".into(), knowledge);
                }
                Json(result).into_response()
            }
            Err(source) => repository_error(source),
        }
    }) {
        Ok(response) => response,
        Err(source) => repository_error(source),
    }
}

pub(crate) async fn chapter_diffs(
    State(state): State<RuntimeState>,
    Path((project_id, chapter_id)): Path<(String, String)>,
) -> Response {
    let (repository, project_id) = match open(&state, project_id) {
        Ok(value) => value,
        Err(response) => return *response,
    };
    let object_id = format!("draft-{chapter_id}");
    let head = match repository.object_heads(&project_id) {
        Ok(heads) => heads
            .into_iter()
            .find(|head| head.object_id.as_str() == object_id),
        Err(source) => return repository_error(source),
    };
    let Some(head) = head else {
        return error(
            StatusCode::NOT_FOUND,
            "chapter_not_found",
            "Chapter not found",
        );
    };
    let accepted = head
        .accepted_revision_id
        .as_ref()
        .and_then(|id| revision_body(&repository, id))
        .unwrap_or_default();
    let candidate = head
        .candidate_revision_id
        .as_ref()
        .and_then(|id| revision_body(&repository, id))
        .unwrap_or_default();
    let diffs = if accepted == candidate || candidate.is_empty() {
        vec![]
    } else {
        vec![json!({
            "id": format!("diff-{chapter_id}"), "chapter_id": chapter_id,
            "fromText": accepted, "toText": candidate,
            "summary": "候选正文与正式正文不同",
            "createdAt": null,
            "addedChars": candidate.chars().count().saturating_sub(accepted.chars().count()),
            "removedChars": accepted.chars().count().saturating_sub(candidate.chars().count())
        })]
    };
    Json(json!({"diffs": diffs})).into_response()
}

pub(crate) async fn story_tests(
    State(state): State<RuntimeState>,
    Path(project_id): Path<String>,
) -> Response {
    let (repository, project_id) = match open(&state, project_id) {
        Ok(value) => value,
        Err(response) => return *response,
    };
    let heads = match repository.object_heads(&project_id) {
        Ok(value) => value,
        Err(source) => return repository_error(source),
    };
    let chapters = match repository.chapters(&project_id) {
        Ok(value) => value,
        Err(source) => return repository_error(source),
    };
    let has_brief = find_head(&heads, "artifact-brief")
        .and_then(selected_revision)
        .is_some();
    let has_outline = find_head(&heads, "artifact-outline")
        .and_then(selected_revision)
        .is_some();
    let has_drafts = !chapters.is_empty();
    let checks = vec![
        check(
            "brief-exists",
            "作品简报已建立",
            has_brief,
            "先生成或保存 Brief。",
            "warning",
        ),
        check(
            "outline-exists",
            "故事大纲已建立",
            has_outline,
            "先生成或保存 Outline。",
            "warning",
        ),
        check(
            "draft-exists",
            "至少存在一个章节草稿",
            has_drafts,
            "生成章节后可执行正文检查。",
            "warning",
        ),
    ];
    Json(json!({"passed": checks.iter().all(|check| check["passed"] == true), "projectId": project_id.as_str(), "checks": checks})).into_response()
}

pub(crate) async fn export_project(
    State(state): State<RuntimeState>,
    Path(project_id): Path<String>,
    Json(request): Json<ExportRequest>,
) -> Response {
    if request.scopes.is_empty()
        || request.scopes.iter().any(|scope| {
            !matches!(
                scope.as_str(),
                "manuscript" | "planning" | "candidates" | "private"
            )
        })
        || !matches!(request.format.as_str(), "txt" | "markdown")
    {
        return error(
            StatusCode::BAD_REQUEST,
            "invalid_export",
            "Export selection is invalid",
        );
    }
    let (repository, project_id) = match open(&state, project_id) {
        Ok(value) => value,
        Err(response) => return *response,
    };
    let revision = match repository.project_revision(&project_id) {
        Ok(value) => value,
        Err(source) => return repository_error(source),
    };
    let heads = match repository.object_heads(&project_id) {
        Ok(value) => value,
        Err(source) => return repository_error(source),
    };
    let mut sections = Vec::new();
    for head in heads {
        let ids = if request.scopes.contains(&"candidates".to_owned()) {
            [
                head.accepted_revision_id.as_ref(),
                head.candidate_revision_id.as_ref(),
            ]
        } else {
            [head.accepted_revision_id.as_ref(), None]
        };
        for revision_id in ids.into_iter().flatten() {
            if let Ok(Some(content)) = repository.revision_content(revision_id) {
                sections.push(format!(
                    "## {} · {}\n\n{}",
                    head.object_kind,
                    revision_id.as_str(),
                    content
                ));
            }
        }
    }
    if sections.is_empty() {
        return error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "export_empty",
            "Selected scopes contain no saved content",
        );
    }
    let exported_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |value| value.as_secs());
    let extension = if request.format == "markdown" {
        "md"
    } else {
        "txt"
    };
    let content = format!(
        "# 作品导出\n\n作品 ID：{}\n作品版本：{}\n\n{}\n",
        project_id.as_str(),
        revision,
        sections.join("\n\n")
    );
    Json(json!({
        "fileName": format!("{}-r{}.{}", project_id.as_str(), revision, extension),
        "mimeType": if extension == "md" { "text/markdown;charset=utf-8" } else { "text/plain;charset=utf-8" },
        "content": content, "projectRevision": revision, "exportedAt": exported_at, "scopes": request.scopes
    })).into_response()
}

fn open(
    state: &RuntimeState,
    project_id: String,
) -> Result<(SqliteStoryRepository, ProjectId), Box<Response>> {
    let project_id = ProjectId::parse(project_id).map_err(|source| {
        Box::new(error(
            StatusCode::BAD_REQUEST,
            "invalid_project_id",
            &source.to_string(),
        ))
    })?;
    let repository = SqliteStoryRepository::open(&state.database_path)
        .map_err(|source| Box::new(repository_error(source)))?;
    Ok((repository, project_id))
}

fn selected_revision(head: &ObjectHead) -> Option<&RevisionId> {
    head.accepted_revision_id
        .as_ref()
        .or(head.candidate_revision_id.as_ref())
}

fn find_head<'a>(heads: &'a [ObjectHead], object_id: &str) -> Option<&'a ObjectHead> {
    heads
        .iter()
        .find(|head| head.object_id.as_str() == object_id)
}

fn revision_json(repository: &SqliteStoryRepository, revision_id: &RevisionId) -> Option<Value> {
    repository
        .revision_content(revision_id)
        .ok()
        .flatten()
        .and_then(|content| serde_json::from_str(&content).ok())
}

fn revision_body(repository: &SqliteStoryRepository, revision_id: &RevisionId) -> Option<String> {
    revision_json(repository, revision_id)?
        .get("body")?
        .as_str()
        .map(str::to_owned)
}

fn check(id: &str, label: &str, passed: bool, details: &str, severity: &str) -> Value {
    json!({"id": id, "label": label, "passed": passed, "severity": severity, "details": details})
}
