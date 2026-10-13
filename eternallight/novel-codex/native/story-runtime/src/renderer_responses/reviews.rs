use super::*;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpCreativeWorkspaceFactProposalsItemReviewDecision {
    #[serde(rename = "candidate")] Candidate,
    #[serde(rename = "confirmed")] Confirmed,
    #[serde(rename = "rejected")] Rejected,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpCreativeReviewArtifactKind {
    #[serde(rename = "draft")] Draft,
    #[serde(rename = "brief")] Brief,
    #[serde(rename = "outline")] Outline,
    #[serde(rename = "plan")] Plan,
    #[serde(rename = "identity")] Identity,
    #[serde(rename = "propertyRules")] PropertyRules,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpCreativeReviewStatus {
    #[serde(rename = "ready")] Ready,
    #[serde(rename = "stale")] Stale,
    #[serde(rename = "rejected")] Rejected,
    #[serde(rename = "accepted")] Accepted,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpCreativeReviewChecksItem {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "passed")]
    pub r#passed: bool,
    #[serde(rename = "blocking")]
    pub r#blocking: bool,
    #[serde(rename = "message")]
    pub r#message: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpCreativeReview {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "artifactKind")]
    pub r#artifact_kind: HttpCreativeReviewArtifactKind,
    #[serde(rename = "artifactRevisionId")]
    pub r#artifact_revision_id: String,
    #[serde(rename = "projectRevision")]
    pub r#project_revision: i64,
    #[serde(rename = "status")]
    pub r#status: HttpCreativeReviewStatus,
    #[serde(rename = "checks")]
    pub r#checks: Vec<HttpCreativeReviewChecksItem>,
    #[serde(rename = "selections")]
    pub r#selections: std::collections::BTreeMap<String, serde_json::Value>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpRevisionOutcomeReportUncoveredRequirementsItemAuthorSourceChecksItem {
    #[serde(rename = "sourceId")]
    pub r#source_id: String,
    #[serde(rename = "contentHash")]
    pub r#content_hash: String,
    #[serde(rename = "quote")]
    pub r#quote: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpRevisionOutcomeReportUncoveredRequirementsItem {
    #[serde(rename = "issue")]
    pub r#issue: HttpCreationIssueLink,
    #[serde(rename = "requirement")]
    pub r#requirement: String,
    #[serde(rename = "reason")]
    pub r#reason: String,
    #[serde(rename = "authorSourceChecks")]
    pub r#author_source_checks: Vec<HttpRevisionOutcomeReportUncoveredRequirementsItemAuthorSourceChecksItem>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpRevisionOutcomeReportOutcomesItemEvidenceItem {
    #[serde(rename = "groupId")]
    pub r#group_id: String,
    #[serde(rename = "quote")]
    pub r#quote: String,
    #[serde(rename = "start")]
    pub r#start: f64,
    #[serde(rename = "end")]
    pub r#end: f64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpRevisionOutcomeReportOutcomesItem {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "status")]
    pub r#status: HttpCreationIssueObservationStatus,
    #[serde(rename = "reason")]
    pub r#reason: String,
    #[serde(rename = "evidence")]
    pub r#evidence: Vec<HttpRevisionOutcomeReportOutcomesItemEvidenceItem>,
    #[serde(rename = "issue")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#issue: Option<HttpCreationIssueLink>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpRevisionOutcomeReport {
    #[serde(rename = "uncoveredRequirements")]
    pub r#uncovered_requirements: Vec<HttpRevisionOutcomeReportUncoveredRequirementsItem>,
    #[serde(rename = "writerTaskId")]
    pub r#writer_task_id: String,
    #[serde(rename = "chapterId")]
    pub r#chapter_id: String,
    #[serde(rename = "originalRevisionId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#original_revision_id: Option<String>,
    #[serde(rename = "draftRevisionId")]
    pub r#draft_revision_id: String,
    #[serde(rename = "knowledgeBatchId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#knowledge_batch_id: Option<String>,
    #[serde(rename = "outcomes")]
    pub r#outcomes: Vec<HttpRevisionOutcomeReportOutcomesItem>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpRevisionOutcome {
    #[serde(rename = "revisionId")]
    pub r#revision_id: String,
    #[serde(rename = "authority")]
    pub r#authority: HttpSceneAssessmentSummaryAuthority,
    #[serde(rename = "state")]
    pub r#state: HttpSceneAssessmentSummaryState,
    #[serde(rename = "assessment")]
    pub r#assessment: HttpRevisionOutcomeReport,
    #[serde(rename = "issues")]
    pub r#issues: Vec<HttpCreationIssue>,
    #[serde(rename = "decisions")]
    pub r#decisions: Vec<novel_codex_story_core::RevisionOutcomeDecision>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererGetRevisionOutcomeResponse(pub Option<HttpRevisionOutcome>);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererDecideRevisionOutcomeResponse(pub novel_codex_story_core::RevisionOutcomeDecision);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpCreateReviewResponse {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "artifactKind")]
    pub r#artifact_kind: HttpCreativeReviewArtifactKind,
    #[serde(rename = "artifactRevisionId")]
    pub r#artifact_revision_id: String,
    #[serde(rename = "projectRevision")]
    pub r#project_revision: i64,
    #[serde(rename = "status")]
    pub r#status: String,
    #[serde(rename = "checks")]
    pub r#checks: Vec<HttpCreativeReviewChecksItem>,
    #[serde(rename = "factProposals")]
    pub r#fact_proposals: Vec<HttpFactProposal>,
    #[serde(rename = "provisionalImpacts")]
    pub r#provisional_impacts: Vec<HttpProvisionalPlanningImpact>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererCreateReviewResponse(pub HttpCreateReviewResponse);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpAcceptReviewResponse {
    #[serde(rename = "commitId")]
    pub r#commit_id: String,
    #[serde(rename = "resultRevision")]
    pub r#result_revision: i64,
    #[serde(rename = "impacts")]
    pub r#impacts: Vec<HttpPlanningImpact>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererAcceptReviewResponse(pub HttpAcceptReviewResponse);
