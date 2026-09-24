use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use axum::Json;
use axum::extract::Path;
use axum::extract::Query;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::response::Response;
use novel_codex_story_core::ObjectHead;
use novel_codex_story_core::ProjectId;
use novel_codex_story_core::RevisionId;
use novel_codex_story_core::SqliteStoryRepository;
use serde::Deserialize;
use serde_json::Value;
use serde_json::json;

use crate::server::RuntimeState;
use crate::server::error;
use crate::server::repository_error;

#[derive(Deserialize)]
pub(crate) struct SearchQuery {
    #[serde(default)]
    q: String,
    #[serde(default = "default_search_limit")]
    limit: usize,
    kind: Option<String>,
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
    Query(query): Query<SearchQuery>,
) -> Response {
    if query.q.chars().count() > 240 || query.limit == 0 || query.limit > 50 {
        return error(
            StatusCode::BAD_REQUEST,
            "invalid_search",
            "Search query is invalid",
        );
    }
    let normalized = query.q.trim().to_lowercase();
    if normalized.is_empty() {
        return Json(json!({"results": []})).into_response();
    }
    let (repository, project_id) = match open(&state, project_id) {
        Ok(value) => value,
        Err(response) => return *response,
    };
    let heads = match repository.object_heads(&project_id) {
        Ok(value) => value,
        Err(source) => return repository_error(source),
    };
    let mut results = Vec::new();
    for head in heads {
        if query
            .kind
            .as_ref()
            .is_some_and(|kind| kind != &head.object_kind)
        {
            continue;
        }
        let Some(revision_id) = selected_revision(&head) else {
            continue;
        };
        let Some(content) = repository.revision_content(revision_id).ok().flatten() else {
            continue;
        };
        if !content.to_lowercase().contains(&normalized) {
            continue;
        }
        let title = serde_json::from_str::<Value>(&content)
            .ok()
            .and_then(|value| {
                value
                    .get("title")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| head.object_kind.clone());
        results.push(json!({
            "id": head.object_id.as_str(), "title": title, "kind": head.object_kind,
            "snippet": excerpt(&content, &normalized), "source": "story-revision"
        }));
        if results.len() == query.limit {
            break;
        }
    }
    Json(json!({"results": results})).into_response()
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

fn excerpt(content: &str, query: &str) -> String {
    let lower = content.to_lowercase();
    let position = lower.find(query).unwrap_or(0);
    let start = position.saturating_sub(80);
    content
        .get(start..)
        .unwrap_or(content)
        .chars()
        .take(240)
        .collect()
}

fn check(id: &str, label: &str, passed: bool, details: &str, severity: &str) -> Value {
    json!({"id": id, "label": label, "passed": passed, "severity": severity, "details": details})
}
