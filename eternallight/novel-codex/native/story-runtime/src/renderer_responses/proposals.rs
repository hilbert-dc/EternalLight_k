use super::*;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpProposalSetSummaryStatus {
    #[serde(rename = "proposed")] Proposed,
    #[serde(rename = "applied")] Applied,
    #[serde(rename = "undone")] Undone,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpProposalSetSummary {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "intent")]
    pub r#intent: String,
    #[serde(rename = "createdAt")]
    pub r#created_at: i64,
    #[serde(rename = "status")]
    pub r#status: HttpProposalSetSummaryStatus,
    #[serde(rename = "applicationId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#application_id: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpListProposalSetsResponse {
    #[serde(rename = "data")]
    pub r#data: Vec<HttpProposalSetSummary>,
    #[serde(rename = "nextCursor")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#next_cursor: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererListProposalSetsResponse(pub HttpListProposalSetsResponse);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpCreateProposalSetResponse {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "baseSnapshotId")]
    pub r#base_snapshot_id: String,
    #[serde(rename = "replayed")]
    pub r#replayed: bool,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererCreateProposalSetResponse(pub HttpCreateProposalSetResponse);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpProposalSetDetailStatus {
    #[serde(rename = "ready")] Ready,
    #[serde(rename = "stale")] Stale,
    #[serde(rename = "applied")] Applied,
    #[serde(rename = "undone")] Undone,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpProposalSetDetailOperationsItem {
    #[serde(rename = "operationId")]
    pub r#operation_id: String,
    #[serde(rename = "reviewId")]
    pub r#review_id: String,
    #[serde(rename = "objectId")]
    pub r#object_id: String,
    #[serde(rename = "objectKind")]
    pub r#object_kind: String,
    #[serde(rename = "candidateRevisionId")]
    pub r#candidate_revision_id: String,
    #[serde(rename = "expectedAcceptedRevisionId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#expected_accepted_revision_id: Option<String>,
    #[serde(rename = "dependsOn")]
    pub r#depends_on: Vec<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpProposalSetDetailReviewOptionsValue {
    #[serde(rename = "sceneGroupIds")]
    pub r#scene_group_ids: Vec<String>,
    #[serde(rename = "factProposals")]
    pub r#fact_proposals: Vec<HttpFactProposal>,
    #[serde(rename = "selections")]
    pub r#selections: std::collections::BTreeMap<String, serde_json::Value>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpProposalSetDetailSelectionMissingDependenciesItem {
    #[serde(rename = "operationId")]
    pub r#operation_id: String,
    #[serde(rename = "requires")]
    pub r#requires: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpProposalSetDetailSelection {
    #[serde(rename = "operationIds")]
    pub r#operation_ids: Vec<String>,
    #[serde(rename = "missingDependencies")]
    pub r#missing_dependencies: Vec<HttpProposalSetDetailSelectionMissingDependenciesItem>,
    #[serde(rename = "declaredClosureReady")]
    pub r#declared_closure_ready: bool,
    #[serde(rename = "applyReady")]
    pub r#apply_ready: bool,
    #[serde(rename = "selectionsByOperationId")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#selections_by_operation_id: Option<std::collections::BTreeMap<String, std::collections::BTreeMap<String, serde_json::Value>>>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpProposalSetDetailApplicationStatus {
    #[serde(rename = "applied")] Applied,
    #[serde(rename = "undone")] Undone,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpProposalSetDetailApplication {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "status")]
    pub r#status: HttpProposalSetDetailApplicationStatus,
    #[serde(rename = "resultRevision")]
    pub r#result_revision: i64,
    #[serde(rename = "resultSnapshotId")]
    pub r#result_snapshot_id: String,
    #[serde(rename = "undoneByApplicationId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#undone_by_application_id: Option<String>,
    #[serde(rename = "selectedOperationIds")]
    pub r#selected_operation_ids: Vec<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpProposalSetDetailCoverageDependencyClosure { #[serde(rename = "declaredOnly")] DeclaredOnly }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpProposalSetDetailCoverage {
    #[serde(rename = "dependencyClosure")]
    pub r#dependency_closure: HttpProposalSetDetailCoverageDependencyClosure,
    #[serde(rename = "semanticDependenciesComplete")]
    pub r#semantic_dependencies_complete: WireFalse,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpProposalSetDetail {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "projectId")]
    pub r#project_id: String,
    #[serde(rename = "baseSnapshotId")]
    pub r#base_snapshot_id: String,
    #[serde(rename = "baseProjectRevision")]
    pub r#base_project_revision: i64,
    #[serde(rename = "intent")]
    pub r#intent: String,
    #[serde(rename = "createdAt")]
    pub r#created_at: i64,
    #[serde(rename = "status")]
    pub r#status: HttpProposalSetDetailStatus,
    #[serde(rename = "operations")]
    pub r#operations: Vec<HttpProposalSetDetailOperationsItem>,
    #[serde(rename = "reviewOptions")]
    pub r#review_options: std::collections::BTreeMap<String, HttpProposalSetDetailReviewOptionsValue>,
    #[serde(rename = "selection")]
    pub r#selection: HttpProposalSetDetailSelection,
    #[serde(rename = "staleOperations")]
    pub r#stale_operations: Vec<String>,
    #[serde(rename = "application")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#application: Option<HttpProposalSetDetailApplication>,
    #[serde(rename = "coverage")]
    pub r#coverage: HttpProposalSetDetailCoverage,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererGetProposalSetResponse(pub HttpProposalSetDetail);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererPreviewProposalSetResponse(pub HttpProposalSetDetail);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpApplyProposalSetResponse {
    #[serde(rename = "applicationId")]
    pub r#application_id: String,
    #[serde(rename = "resultRevision")]
    pub r#result_revision: i64,
    #[serde(rename = "resultSnapshotId")]
    pub r#result_snapshot_id: String,
    #[serde(rename = "replayed")]
    pub r#replayed: bool,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererApplyProposalSetResponse(pub HttpApplyProposalSetResponse);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpUndoProposalSetResponse {
    #[serde(rename = "undoApplicationId")]
    pub r#undo_application_id: String,
    #[serde(rename = "resultRevision")]
    pub r#result_revision: i64,
    #[serde(rename = "resultSnapshotId")]
    pub r#result_snapshot_id: String,
    #[serde(rename = "replayed")]
    pub r#replayed: bool,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererUndoProposalSetResponse(pub HttpUndoProposalSetResponse);
