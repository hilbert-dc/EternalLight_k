use super::*;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpQualityKind {
    #[serde(rename = "preference")] Preference,
    #[serde(rename = "goal")] Goal,
    #[serde(rename = "strength")] Strength,
    #[serde(rename = "observation")] Observation,
    #[serde(rename = "trial")] Trial,
    #[serde(rename = "method")] Method,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpQualityNoteScope {
    #[serde(rename = "work")] Work,
    #[serde(rename = "task")] Task,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpQualityNoteEvidenceItemRole {
    #[serde(rename = "current")] Current,
    #[serde(rename = "reference")] Reference,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpQualityNoteEvidenceItem {
    #[serde(rename = "revisionId")]
    pub r#revision_id: String,
    #[serde(rename = "quote")]
    pub r#quote: String,
    #[serde(rename = "role")]
    pub r#role: HttpQualityNoteEvidenceItemRole,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpQualityNoteVerdictValue {
    #[serde(rename = "supported")] Supported,
    #[serde(rename = "unmet")] Unmet,
    #[serde(rename = "insufficientEvidence")] InsufficientEvidence,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpQualityNote {
    #[serde(rename = "kind")]
    pub r#kind: HttpQualityKind,
    #[serde(rename = "scope")]
    pub r#scope: HttpQualityNoteScope,
    #[serde(rename = "text")]
    pub r#text: String,
    #[serde(rename = "evidence")]
    pub r#evidence: Vec<HttpQualityNoteEvidenceItem>,
    #[serde(rename = "question")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#question: Option<String>,
    #[serde(rename = "body")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#body: Option<String>,
    #[serde(rename = "relatesTo")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#relates_to: Option<String>,
    #[serde(rename = "verdict")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#verdict: Option<HttpQualityNoteVerdictValue>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpQualityRecordStatus {
    #[serde(rename = "confirmed")] Confirmed,
    #[serde(rename = "proposed")] Proposed,
    #[serde(rename = "withdrawn")] Withdrawn,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpQualityRecordAuthorKind {
    #[serde(rename = "author")] Author,
    #[serde(rename = "model")] Model,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpQualityRecordDecisionsItem {
    #[serde(rename = "version")]
    pub r#version: i64,
    #[serde(rename = "status")]
    pub r#status: String,
    #[serde(rename = "reason")]
    pub r#reason: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpQualityRecord {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "ordinal")]
    pub r#ordinal: i64,
    #[serde(rename = "note")]
    pub r#note: HttpQualityNote,
    #[serde(rename = "status")]
    pub r#status: HttpQualityRecordStatus,
    #[serde(rename = "sourceCurrent")]
    pub r#source_current: bool,
    #[serde(rename = "authorKind")]
    pub r#author_kind: HttpQualityRecordAuthorKind,
    #[serde(rename = "taskId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#task_id: Option<String>,
    #[serde(rename = "decisions")]
    pub r#decisions: Vec<HttpQualityRecordDecisionsItem>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpQualityPacketSummaryKind {
    #[serde(rename = "reading")] Reading,
    #[serde(rename = "comparison")] Comparison,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpQualityPacketSummary {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "kind")]
    pub r#kind: HttpQualityPacketSummaryKind,
    #[serde(rename = "labels")]
    pub r#labels: Vec<String>,
    #[serde(rename = "taskId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#task_id: Option<String>,
    #[serde(rename = "status")]
    pub r#status: String,
    #[serde(rename = "sourceCurrent")]
    pub r#source_current: bool,
    #[serde(rename = "hasReport")]
    pub r#has_report: bool,
    #[serde(rename = "feedbackCount")]
    pub r#feedback_count: i64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpQualityNotebookQualityGain { #[serde(rename = "unvalidated")] Unvalidated }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpQualityNotebook {
    #[serde(rename = "version")]
    pub r#version: i64,
    #[serde(rename = "notes")]
    pub r#notes: Vec<HttpQualityRecord>,
    #[serde(rename = "nextCursor")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#next_cursor: Option<i64>,
    #[serde(rename = "packets")]
    pub r#packets: Vec<HttpQualityPacketSummary>,
    #[serde(rename = "qualityGain")]
    pub r#quality_gain: HttpQualityNotebookQualityGain,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererGetQualityNotebookResponse(pub HttpQualityNotebook);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpChangeQualityNotebookResponseResult {
    #[serde(rename = "id")]
    pub r#id: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpChangeQualityNotebookResponse {
    #[serde(rename = "version")]
    pub r#version: i64,
    #[serde(rename = "result")]
    pub r#result: HttpChangeQualityNotebookResponseResult,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererChangeQualityNotebookResponse(pub HttpChangeQualityNotebookResponse);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpQualityPacketItemsItem {
    #[serde(rename = "label")]
    pub r#label: String,
    #[serde(rename = "text")]
    pub r#text: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpQualityPacket {
    #[serde(rename = "kind")]
    pub r#kind: HttpQualityPacketSummaryKind,
    #[serde(rename = "items")]
    pub r#items: Vec<HttpQualityPacketItemsItem>,
    #[serde(rename = "coverage")]
    pub r#coverage: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererGetQualityPacketResponse(pub HttpQualityPacket);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpQualityReaderReportFindingsItem {
    #[serde(rename = "label")]
    pub r#label: String,
    #[serde(rename = "quote")]
    pub r#quote: String,
    #[serde(rename = "effect")]
    pub r#effect: String,
    #[serde(rename = "confidence")]
    pub r#confidence: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpQualityReaderReport {
    #[serde(rename = "summary")]
    pub r#summary: String,
    #[serde(rename = "limitations")]
    pub r#limitations: String,
    #[serde(rename = "preference")]
    pub r#preference: String,
    #[serde(rename = "findings")]
    pub r#findings: Vec<HttpQualityReaderReportFindingsItem>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpQualityReviewReportReviewerKind { #[serde(rename = "model")] Model }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpQualityReviewReport {
    #[serde(rename = "report")]
    pub r#report: HttpQualityReaderReport,
    #[serde(rename = "reviewerKind")]
    pub r#reviewer_kind: HttpQualityReviewReportReviewerKind,
    #[serde(rename = "isolation")]
    pub r#isolation: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpQualityReviewLatestAuthorFeedbackItem {
    #[serde(rename = "preference")]
    pub r#preference: String,
    #[serde(rename = "reason")]
    pub r#reason: String,
    #[serde(rename = "version")]
    pub r#version: i64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpQualityReviewCalibration {
    #[serde(rename = "preferenceDisagreement")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#preference_disagreement: Option<bool>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpQualityReview {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "report")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#report: Option<HttpQualityReviewReport>,
    #[serde(rename = "latestAuthorFeedback")]
    pub r#latest_author_feedback: Vec<HttpQualityReviewLatestAuthorFeedbackItem>,
    #[serde(rename = "feedbackCount")]
    pub r#feedback_count: i64,
    #[serde(rename = "feedbackTruncated")]
    pub r#feedback_truncated: bool,
    #[serde(rename = "usageTaskId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#usage_task_id: Option<String>,
    #[serde(rename = "sourceCurrent")]
    pub r#source_current: bool,
    #[serde(rename = "calibration")]
    pub r#calibration: HttpQualityReviewCalibration,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererGetQualityReviewResponse(pub HttpQualityReview);
