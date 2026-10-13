use super::*;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpServerProjectResponse {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "title")]
    pub r#title: String,
    #[serde(rename = "logline")]
    pub r#logline: String,
    #[serde(rename = "created_at")]
    pub r#created_at: i64,
    #[serde(rename = "updated_at")]
    pub r#updated_at: i64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpListProjectsResponse {
    #[serde(rename = "projects")]
    pub r#projects: Vec<HttpServerProjectResponse>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererListProjectsResponse(pub HttpListProjectsResponse);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpCreativeMedium {
    #[serde(rename = "novel")] Novel,
    #[serde(rename = "comic")] Comic,
    #[serde(rename = "animation")] Animation,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpRational {
    #[serde(rename = "numerator")]
    pub r#numerator: f64,
    #[serde(rename = "denominator")]
    pub r#denominator: f64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpCreativeProjectCreated {
    #[serde(rename = "project")]
    pub r#project: novel_codex_story_core::MediaProject,
    #[serde(rename = "editions")]
    pub r#editions: Vec<novel_codex_story_core::CreativeEdition>,
    #[serde(rename = "activeEditionId")]
    pub r#active_edition_id: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererCreateProjectResponse(pub HttpCreativeProjectCreated);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpSceneStatus {
    #[serde(rename = "draft")] Draft,
    #[serde(rename = "review")] Review,
    #[serde(rename = "ready")] Ready,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpSceneParticipantsItem {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "name")]
    pub r#name: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpSceneStateChangesItem {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "description")]
    pub r#description: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpSceneCanonConstraintsItem {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "statement")]
    pub r#statement: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpLocalMetricsSummaryScope { #[serde(rename = "local-device")] LocalDevice }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpLocalMetricsSummaryMetricsItem {
    #[serde(rename = "metric")]
    pub r#metric: String,
    #[serde(rename = "count")]
    pub r#count: i64,
    #[serde(rename = "totalSeconds")]
    pub r#total_seconds: i64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpLocalMetricsSummary {
    #[serde(rename = "scope")]
    pub r#scope: HttpLocalMetricsSummaryScope,
    #[serde(rename = "metrics")]
    pub r#metrics: Vec<HttpLocalMetricsSummaryMetricsItem>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererGetLocalMetricsResponse(pub HttpLocalMetricsSummary);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererRecordStageAbandonmentResponse(pub HttpLocalMetricsSummary);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpAuthorExportScope {
    #[serde(rename = "manuscript")] Manuscript,
    #[serde(rename = "planning")] Planning,
    #[serde(rename = "candidates")] Candidates,
    #[serde(rename = "private")] Private,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpAuthorExportResult {
    #[serde(rename = "fileName")]
    pub r#file_name: String,
    #[serde(rename = "mimeType")]
    pub r#mime_type: String,
    #[serde(rename = "content")]
    pub r#content: String,
    #[serde(rename = "projectRevision")]
    pub r#project_revision: i64,
    #[serde(rename = "exportedAt")]
    pub r#exported_at: String,
    #[serde(rename = "scopes")]
    pub r#scopes: Vec<HttpAuthorExportScope>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererExportProjectResponse(pub HttpAuthorExportResult);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpGetChapterDraftHistoryResponse {
    #[serde(rename = "projectId")]
    pub r#project_id: String,
    #[serde(rename = "revision")]
    pub r#revision: HttpDraftRevision,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererGetChapterDraftHistoryResponse(pub HttpGetChapterDraftHistoryResponse);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpFactProposal {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "sceneGroupId")]
    pub r#scene_group_id: String,
    #[serde(rename = "sceneId")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#scene_id: Option<String>,
    #[serde(rename = "factKind")]
    pub r#fact_kind: HttpCreativeWorkspaceFactProposalsItemFactKind,
    #[serde(rename = "subjectId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#subject_id: Option<String>,
    #[serde(rename = "predicate")]
    pub r#predicate: String,
    #[serde(rename = "value")]
    pub r#value: serde_json::Value,
    #[serde(rename = "epistemicOwnerId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#epistemic_owner_id: Option<String>,
    #[serde(rename = "epistemicState")]
    pub r#epistemic_state: HttpCreativeWorkspaceFactProposalsItemEpistemicState,
    #[serde(rename = "storyTime")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub r#story_time: Option<f64>,
    #[serde(rename = "stateProperty")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#state_property: Option<HttpStateProperty>,
    #[serde(rename = "storyEvent")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#story_event: Option<HttpStoryEventInterpretation>,
    #[serde(rename = "evidence")]
    pub r#evidence: String,
    #[serde(rename = "source")]
    pub r#source: HttpCreativeWorkspaceFactProposalsItemSource,
    #[serde(rename = "reviewDecision")]
    pub r#review_decision: HttpCreativeWorkspaceFactProposalsItemReviewDecision,
    #[serde(rename = "readingChapter")]
    pub r#reading_chapter: f64,
    #[serde(rename = "sceneOrder")]
    pub r#scene_order: f64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpSceneEventRef {
    #[serde(rename = "modelingBatchId")]
    pub r#modeling_batch_id: String,
    #[serde(rename = "assertionIndex")]
    pub r#assertion_index: i64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererGetWriterPreviewResponse(pub HttpWriterContextPreview);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpSaveArtifactResponse {
    #[serde(rename = "artifactKind")]
    pub r#artifact_kind: HttpCreativeArtifactKind,
    #[serde(rename = "revisionId")]
    pub r#revision_id: String,
    #[serde(rename = "version")]
    pub r#version: i64,
    #[serde(rename = "payload")]
    pub r#payload: HttpCreativeArtifactPayload,
    #[serde(rename = "linkedImpactIds")]
    pub r#linked_impact_ids: Vec<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererSaveArtifactResponse(pub HttpSaveArtifactResponse);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererDeleteChapterResponse(pub HttpChapterDeletionReceipt);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpMaterializeStandaloneChapterResponse {
    #[serde(rename = "chapterId")]
    pub r#chapter_id: String,
    #[serde(rename = "resultRevision")]
    pub r#result_revision: i64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererMaterializeStandaloneChapterResponse(pub HttpMaterializeStandaloneChapterResponse);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpSetArtifactLockResponse {
    #[serde(rename = "artifactKind")]
    pub r#artifact_kind: HttpCreativeArtifactKind,
    #[serde(rename = "locked")]
    pub r#locked: bool,
    #[serde(rename = "resultRevision")]
    pub r#result_revision: i64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererSetArtifactLockResponse(pub HttpSetArtifactLockResponse);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpSaveDraftCheckpointResponse {
    #[serde(rename = "chapterId")]
    pub r#chapter_id: String,
    #[serde(rename = "revisionId")]
    pub r#revision_id: String,
    #[serde(rename = "version")]
    pub r#version: i64,
    #[serde(rename = "body")]
    pub r#body: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererSaveDraftCheckpointResponse(pub HttpSaveDraftCheckpointResponse);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpUndoCommitResponse {
    #[serde(rename = "commitId")]
    pub r#commit_id: String,
    #[serde(rename = "resultRevision")]
    pub r#result_revision: i64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererUndoCommitResponse(pub HttpUndoCommitResponse);
