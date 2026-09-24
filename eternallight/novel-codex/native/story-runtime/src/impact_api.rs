use axum::Json;
use axum::extract::Path;
use axum::extract::Query;
use axum::extract::State;
use axum::response::IntoResponse;
use axum::response::Response;
use novel_codex_story_core::ProjectId;
use novel_codex_story_core::ResolvePlanningImpact;
use novel_codex_story_core::SqliteStoryRepository;
use serde::Deserialize;
use serde_json::json;
use sha2::Digest;

use crate::RuntimeState;
use crate::server::bad_request;
use crate::server::internal_error;
use crate::server::repository_error;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImpactQuery {
    cursor: Option<String>,
    limit: Option<usize>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ResolveImpactRequest {
    expected_revision: i64,
    resolution: String,
    note: Option<String>,
    idempotency_key: String,
}

pub(crate) async fn list_impacts(
    State(state): State<RuntimeState>,
    Path(project_id): Path<String>,
    Query(query): Query<ImpactQuery>,
) -> Response {
    let project_id = match ProjectId::parse(project_id) {
        Ok(value) => value,
        Err(error) => return bad_request("invalid_project_id", error),
    };
    let cursor = match query.cursor.as_deref().unwrap_or("0").parse::<usize>() {
        Ok(value) => value,
        Err(error) => return bad_request("invalid_impact_cursor", error),
    };
    let repository = match SqliteStoryRepository::open(&state.database_path) {
        Ok(value) => value,
        Err(error) => return internal_error(error),
    };
    match repository.planning_impacts(&project_id, cursor, query.limit.unwrap_or(25)) {
        Ok(page) => Json(json!({
            "data": page.items,
            "nextCursor": page.next_cursor.map(|value| value.to_string()),
            "counts": {
                "total": page.total,
                "open": page.open,
                "retained": page.retained,
                "lockedConflict": page.locked_conflict
            }
        }))
        .into_response(),
        Err(error) => repository_error(error),
    }
}

pub(crate) async fn resolve_impact(
    State(state): State<RuntimeState>,
    Path((project_id, impact_id)): Path<(String, String)>,
    Json(request): Json<ResolveImpactRequest>,
) -> Response {
    let project_id = match ProjectId::parse(project_id) {
        Ok(value) => value,
        Err(error) => return bad_request("invalid_project_id", error),
    };
    let digest = format!(
        "{:x}",
        sha2::Sha256::digest(
            json!({
                "impactId": impact_id,
                "expectedRevision": request.expected_revision,
                "resolution": request.resolution,
                "note": request.note
            })
            .to_string()
            .as_bytes()
        )
    );
    let mut repository = match SqliteStoryRepository::open(&state.database_path) {
        Ok(value) => value,
        Err(error) => return internal_error(error),
    };
    match repository.resolve_planning_impact(ResolvePlanningImpact {
        project_id: &project_id,
        impact_id: &impact_id,
        expected_revision: request.expected_revision,
        resolution: &request.resolution,
        note: request.note.as_deref(),
        idempotency_key: &request.idempotency_key,
        request_digest: &digest,
    }) {
        Ok(receipt) => Json(json!({
            "commitId": receipt.commit_id,
            "resultRevision": receipt.result_revision
        }))
        .into_response(),
        Err(error) => repository_error(error),
    }
}
