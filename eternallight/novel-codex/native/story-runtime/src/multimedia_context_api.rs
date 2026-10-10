use super::multimedia_api::{failure, open};
use crate::server::RuntimeState;
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    response::{IntoResponse, Response},
    routing::get,
};
use novel_codex_story_core::SaveCanonDocument;
use serde::Deserialize;

pub(crate) fn routes() -> Router<RuntimeState> {
    Router::new()
        .route(
            "/api/projects/{project_id}/canon",
            get(read_canon).post(save_canon),
        )
        .route(
            "/api/projects/{project_id}/editions/{edition_id}/source-impacts",
            get(impacts),
        )
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CanonQuery {
    snapshot_id: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ImpactQuery {
    branch_id: Option<String>,
    offset: Option<usize>,
    limit: Option<usize>,
}
async fn read_canon(
    State(s): State<RuntimeState>,
    Path(p): Path<String>,
    Query(q): Query<CanonQuery>,
) -> Response {
    let (db, p) = match open(&s, p) {
        Ok(v) => v,
        Err(e) => return e,
    };
    match db.canon_workspace(&p, q.snapshot_id.as_deref()) {
        Ok(v) => Json(v).into_response(),
        Err(e) => failure(e),
    }
}
async fn save_canon(
    State(s): State<RuntimeState>,
    Path(p): Path<String>,
    Json(r): Json<SaveCanonDocument>,
) -> Response {
    let (mut db, p) = match open(&s, p) {
        Ok(v) => v,
        Err(e) => return e,
    };
    match db.save_canon_document(&p, &r) {
        Ok(v) => Json(v).into_response(),
        Err(e) => failure(e),
    }
}
async fn impacts(
    State(s): State<RuntimeState>,
    Path((p, e)): Path<(String, String)>,
    Query(q): Query<ImpactQuery>,
) -> Response {
    let (db, p) = match open(&s, p) {
        Ok(v) => v,
        Err(e) => return e,
    };
    match db.media_source_impacts(
        &p,
        &e,
        q.branch_id.as_deref(),
        q.offset.unwrap_or(0),
        q.limit.unwrap_or(20),
    ) {
        Ok(v) => Json(v).into_response(),
        Err(e) => failure(e),
    }
}
