use axum::Json;
use axum::extract::Path;
use axum::extract::Query;
use axum::extract::State;
use axum::response::IntoResponse;
use axum::response::Response;
use novel_codex_story_core::ProjectId;
use novel_codex_story_core::ResolvePlanningImpact;
use novel_codex_story_core::{
    ImpactAnalysisQuery, ImpactAnalysisScope, ImpactRevisionPair, RevisionId, SnapshotId,
};
use serde::Deserialize;
use serde_json::json;
use sha2::Digest;

use crate::RuntimeState;
use crate::server::bad_request;
use crate::server::repository_error;

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "ImpactApiImpactQuery", optional_fields = nullable)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImpactQuery {
    cursor: Option<String>,
    limit: Option<usize>,
}

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "ImpactApiAnalyzeImpactQuery", optional_fields = nullable)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AnalyzeImpactQuery {
    entity_id: String,
    snapshot_id: Option<String>,
    from_revision_id: Option<String>,
    to_revision_id: Option<String>,
    world_time: Option<i64>,
    max_hops: Option<usize>,
    limit: Option<usize>,
}

pub(crate) async fn analyze_impact(
    State(state): State<RuntimeState>,
    Path(project_id): Path<String>,
    Query(query): Query<AnalyzeImpactQuery>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |repository| {
        let project_id = match ProjectId::parse(project_id) {
            Ok(value) => value,
            Err(error) => return bad_request("invalid_project_id", error),
        };
        let snapshot_id = match query.snapshot_id.map(SnapshotId::parse).transpose() {
            Ok(value) => value,
            Err(error) => return bad_request("invalid_snapshot_id", error),
        };
        let (from_revision_id, to_revision_id) =
            match (query.from_revision_id, query.to_revision_id) {
                (None, None) => (None, None),
                (Some(from), Some(to)) => {
                    let from = match RevisionId::parse(from) {
                        Ok(value) => value,
                        Err(error) => return bad_request("invalid_from_revision_id", error),
                    };
                    let to = match RevisionId::parse(to) {
                        Ok(value) => value,
                        Err(error) => return bad_request("invalid_to_revision_id", error),
                    };
                    (Some(from), Some(to))
                }
                _ => return bad_request("invalid_revision_pair", "Both revision IDs are required"),
            };

        match repository.analyze_explicit_impact(ImpactAnalysisQuery {
            project_id: &project_id,
            scope: snapshot_id
                .as_ref()
                .map_or(ImpactAnalysisScope::Current, ImpactAnalysisScope::Snapshot),
            changed_entity_id: &query.entity_id,
            world_time: query.world_time,
            max_hops: query.max_hops.unwrap_or(3),
            limit: query.limit.unwrap_or(25),
            revision_pair: from_revision_id.as_ref().zip(to_revision_id.as_ref()).map(
                |(from_revision_id, to_revision_id)| ImpactRevisionPair {
                    from_revision_id,
                    to_revision_id,
                },
            ),
        }) {
            Ok(report) => Json(report).into_response(),
            Err(error) => repository_error(error),
        }
    })
    .await
}

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "ImpactApiResolveImpactRequest", optional_fields = nullable)]
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
    crate::repository_executor::with_repo(state.clone(), move |repository| {
        let project_id = match ProjectId::parse(project_id) {
            Ok(value) => value,
            Err(error) => return bad_request("invalid_project_id", error),
        };
        let cursor = match query.cursor.as_deref().unwrap_or("0").parse::<usize>() {
            Ok(value) => value,
            Err(error) => return bad_request("invalid_impact_cursor", error),
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
    })
    .await
}

pub(crate) async fn resolve_impact(
    State(state): State<RuntimeState>,
    Path((project_id, impact_id)): Path<(String, String)>,
    Json(request): Json<ResolveImpactRequest>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |mut repository| {
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
    })
    .await
}

/// Author/Host only; authentication is applied by server composition.
pub(crate) fn routes() -> axum::Router<crate::server::RuntimeState> {
    axum::Router::new()
        .route(
            "/api/projects/{project_id}/impacts",
            axum::routing::get(crate::impact_api::list_impacts),
        )
        .route(
            "/api/projects/{project_id}/impact-analysis",
            axum::routing::get(crate::impact_api::analyze_impact),
        )
        .route(
            "/api/projects/{project_id}/impacts/{impact_id}/resolve",
            axum::routing::post(crate::impact_api::resolve_impact),
        )
}

use novel_codex_story_core::{ImpactAnalysisRepository, ImpactsRepository};
