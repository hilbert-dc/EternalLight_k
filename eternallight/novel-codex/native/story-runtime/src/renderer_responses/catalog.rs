use super::*;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpWorkVolumeSource {
    #[serde(rename = "outlineRevisionId")]
    pub r#outline_revision_id: String,
    #[serde(rename = "nodeId")]
    pub r#node_id: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpWorkCatalogVolumesItem {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "title")]
    pub r#title: String,
    #[serde(rename = "position")]
    pub r#position: i64,
    #[serde(rename = "source")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#source: Option<HttpWorkVolumeSource>,
    #[serde(rename = "outlineStatus")]
    pub r#outline_status: HttpWorkCatalogVolumesItemOutlineStatus,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpWorkCatalogPlacementsItem {
    #[serde(rename = "chapterId")]
    pub r#chapter_id: String,
    #[serde(rename = "volumeId")]
    pub r#volume_id: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpWorkCatalog {
    #[serde(rename = "revision")]
    pub r#revision: i64,
    #[serde(rename = "volumes")]
    pub r#volumes: Vec<HttpWorkCatalogVolumesItem>,
    #[serde(rename = "placements")]
    pub r#placements: Vec<HttpWorkCatalogPlacementsItem>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererGetWorkCatalogResponse(pub HttpWorkCatalog);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpChapterDeletionReceiptOperation { #[serde(rename = "delete-chapter")] DeleteChapter }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpChapterDeletionReceipt {
    #[serde(rename = "operation")]
    pub r#operation: HttpChapterDeletionReceiptOperation,
    #[serde(rename = "projectId")]
    pub r#project_id: String,
    #[serde(rename = "chapterId")]
    pub r#chapter_id: String,
    #[serde(rename = "chapterNumber")]
    pub r#chapter_number: i64,
    #[serde(rename = "title")]
    pub r#title: String,
    #[serde(rename = "baseRevision")]
    pub r#base_revision: i64,
    #[serde(rename = "resultRevision")]
    pub r#result_revision: i64,
    #[serde(rename = "catalogRevision")]
    pub r#catalog_revision: i64,
    #[serde(rename = "baseSnapshotId")]
    pub r#base_snapshot_id: String,
    #[serde(rename = "resultSnapshotId")]
    pub r#result_snapshot_id: String,
    #[serde(rename = "structureChangeId")]
    pub r#structure_change_id: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpChapterStructureChangeKind { #[serde(rename = "chapterDeleted")] ChapterDeleted }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpChapterStructureChangeSource {
    #[serde(rename = "objectId")]
    pub r#object_id: String,
    #[serde(rename = "savedRevisionId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#saved_revision_id: Option<String>,
    #[serde(rename = "acceptedRevisionId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#accepted_revision_id: Option<String>,
    #[serde(rename = "workingRevisionId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#working_revision_id: Option<String>,
    #[serde(rename = "knowledgeBatchId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#knowledge_batch_id: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpChapterStructureChangeAssessmentState {
    #[serde(rename = "pending")] Pending,
    #[serde(rename = "skipped")] Skipped,
    #[serde(rename = "bound")] Bound,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpChapterStructureChangeAssessmentSkipReason { #[serde(rename = "noSavedEvidence")] NoSavedEvidence }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpChapterStructureChangeAssessment {
    #[serde(rename = "state")]
    pub r#state: HttpChapterStructureChangeAssessmentState,
    #[serde(rename = "skipReason")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#skip_reason: Option<HttpChapterStructureChangeAssessmentSkipReason>,
    #[serde(rename = "taskId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#task_id: Option<String>,
    #[serde(rename = "workTaskId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#work_task_id: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpChapterStructureChange {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "projectId")]
    pub r#project_id: String,
    #[serde(rename = "kind")]
    pub r#kind: HttpChapterStructureChangeKind,
    #[serde(rename = "chapterId")]
    pub r#chapter_id: String,
    #[serde(rename = "chapterNumber")]
    pub r#chapter_number: i64,
    #[serde(rename = "title")]
    pub r#title: String,
    #[serde(rename = "createdAt")]
    pub r#created_at: i64,
    #[serde(rename = "baseRevision")]
    pub r#base_revision: i64,
    #[serde(rename = "resultRevision")]
    pub r#result_revision: i64,
    #[serde(rename = "baseSnapshotId")]
    pub r#base_snapshot_id: String,
    #[serde(rename = "resultSnapshotId")]
    pub r#result_snapshot_id: String,
    #[serde(rename = "source")]
    pub r#source: HttpChapterStructureChangeSource,
    #[serde(rename = "previousChapter")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#previous_chapter: Option<HttpChapterStructureChangePreviousChapter>,
    #[serde(rename = "nextChapter")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#next_chapter: Option<HttpChapterStructureChangeNextChapter>,
    #[serde(rename = "assessment")]
    pub r#assessment: HttpChapterStructureChangeAssessment,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpListChapterStructureChangesResponse {
    #[serde(rename = "projectId")]
    pub r#project_id: String,
    #[serde(rename = "items")]
    pub r#items: Vec<HttpChapterStructureChange>,
    #[serde(rename = "nextCursor")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#next_cursor: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererListChapterStructureChangesResponse(pub HttpListChapterStructureChangesResponse);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererReadChapterStructureChangeResponse(pub HttpChapterStructureChange);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpUpdateWorkCatalogResponse {
    #[serde(rename = "catalog")]
    pub r#catalog: HttpWorkCatalog,
    #[serde(rename = "volumeId")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#volume_id: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererUpdateWorkCatalogResponse(pub HttpUpdateWorkCatalogResponse);
