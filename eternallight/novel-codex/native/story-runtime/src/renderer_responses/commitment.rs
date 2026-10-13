use super::*;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpPlannedCommitmentRefVariant0Type { #[serde(rename = "commitment")] Commitment }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpPlannedCommitmentRefVariant0 {
    #[serde(rename = "type")]
    pub r#type: HttpPlannedCommitmentRefVariant0Type,
    #[serde(rename = "commitmentId")]
    pub r#commitment_id: String,
    #[serde(rename = "revisionId")]
    pub r#revision_id: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpPlannedCommitmentRefVariant1Type { #[serde(rename = "proposal")] Proposal }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpPlannedCommitmentRefVariant1 {
    #[serde(rename = "type")]
    pub r#type: HttpPlannedCommitmentRefVariant1Type,
    #[serde(rename = "reportRevisionId")]
    pub r#report_revision_id: String,
    #[serde(rename = "proposalId")]
    pub r#proposal_id: String,
    #[serde(rename = "sourceRevisionId")]
    pub r#source_revision_id: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(untagged)]
pub enum HttpPlannedCommitmentRef {
    PlannedCommitmentRefVariant0(HttpPlannedCommitmentRefVariant0),
    PlannedCommitmentRefVariant1(HttpPlannedCommitmentRefVariant1),
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpCommitmentAssessmentReferenceState {
    #[serde(rename = "current")] Current,
    #[serde(rename = "sourceChanged")] SourceChanged,
    #[serde(rename = "revisionSuperseded")] RevisionSuperseded,
    #[serde(rename = "authorEstablished")] AuthorEstablished,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpCommitmentAssessmentOutcome {
    #[serde(rename = "insufficientEvidence")] InsufficientEvidence,
    #[serde(rename = "advanced")] Advanced,
    #[serde(rename = "partiallyFulfilled")] PartiallyFulfilled,
    #[serde(rename = "fulfilled")] Fulfilled,
    #[serde(rename = "contradicted")] Contradicted,
    #[serde(rename = "unchanged")] Unchanged,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpCommitmentAssessment {
    #[serde(rename = "reference")]
    pub r#reference: HttpPlannedCommitmentRef,
    #[serde(rename = "referenceState")]
    pub r#reference_state: HttpCommitmentAssessmentReferenceState,
    #[serde(rename = "outcome")]
    pub r#outcome: HttpCommitmentAssessmentOutcome,
    #[serde(rename = "rationale")]
    pub r#rationale: String,
    #[serde(rename = "actualEventRefs")]
    pub r#actual_event_refs: Vec<HttpSceneEventRef>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpCommitmentExtractionReportEstablishedItem {
    #[serde(rename = "proposalId")]
    pub r#proposal_id: String,
    #[serde(rename = "commitmentId")]
    pub r#commitment_id: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpCommitmentExtractionReportAuthority { #[serde(rename = "candidateProposalBatch")] CandidateProposalBatch }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpCommitmentExtractionReportPayloadExaminedRangesItem {
    #[serde(rename = "start")]
    pub r#start: f64,
    #[serde(rename = "end")]
    pub r#end: f64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpCommitmentExtractionReportPayload {
    #[serde(rename = "chapterId")]
    pub r#chapter_id: String,
    #[serde(rename = "sourceRevisionId")]
    pub r#source_revision_id: String,
    #[serde(rename = "examinedRanges")]
    pub r#examined_ranges: Vec<HttpCommitmentExtractionReportPayloadExaminedRangesItem>,
    #[serde(rename = "candidates")]
    pub r#candidates: Vec<novel_codex_story_core::NarrativeCommitment>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpCommitmentExtractionReportCoverage {
    #[serde(rename = "semanticCompleteness")]
    pub r#semantic_completeness: HttpCharacterKnowledgeCoverageSemanticCompleteness,
    #[serde(rename = "modelReportedRanges")]
    pub r#model_reported_ranges: WireTrue,
    #[serde(rename = "literaryFulfillmentVerified")]
    pub r#literary_fulfillment_verified: WireFalse,
    #[serde(rename = "globalCompletenessClaimed")]
    pub r#global_completeness_claimed: WireFalse,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpCommitmentExtractionReport {
    #[serde(rename = "established")]
    pub r#established: Vec<HttpCommitmentExtractionReportEstablishedItem>,
    #[serde(rename = "revisionId")]
    pub r#revision_id: String,
    #[serde(rename = "snapshotId")]
    pub r#snapshot_id: String,
    #[serde(rename = "projectRevision")]
    pub r#project_revision: i64,
    #[serde(rename = "authority")]
    pub r#authority: HttpCommitmentExtractionReportAuthority,
    #[serde(rename = "sourceCurrent")]
    pub r#source_current: bool,
    #[serde(rename = "reportCurrent")]
    pub r#report_current: bool,
    #[serde(rename = "payload")]
    pub r#payload: HttpCommitmentExtractionReportPayload,
    #[serde(rename = "coverage")]
    pub r#coverage: HttpCommitmentExtractionReportCoverage,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererGetCommitmentExtractionResponse(pub Option<HttpCommitmentExtractionReport>);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpEstablishCommitmentResponse {
    #[serde(rename = "commitmentId")]
    pub r#commitment_id: String,
    #[serde(rename = "revisionId")]
    pub r#revision_id: String,
    #[serde(rename = "snapshotId")]
    pub r#snapshot_id: String,
    #[serde(rename = "confirmationState")]
    pub r#confirmation_state: HttpCreativeWorkspaceModelingItemConfirmationState,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererEstablishCommitmentResponse(pub HttpEstablishCommitmentResponse);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpCommitmentDetailAuthorControls {
    #[serde(rename = "projectRevision")]
    pub r#project_revision: i64,
    #[serde(rename = "undoAvailable")]
    pub r#undo_available: bool,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpCommitmentDetailInterpretation { #[serde(rename = "sourceBoundNarrativeClaim")] SourceBoundNarrativeClaim }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpCommitmentDetail {
    #[serde(rename = "authorControls")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#author_controls: Option<HttpCommitmentDetailAuthorControls>,
    #[serde(rename = "confirmationCommitId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#confirmation_commit_id: Option<String>,
    #[serde(rename = "projectId")]
    pub r#project_id: String,
    #[serde(rename = "snapshotId")]
    pub r#snapshot_id: String,
    #[serde(rename = "objectId")]
    pub r#object_id: String,
    #[serde(rename = "revisionId")]
    pub r#revision_id: String,
    #[serde(rename = "payload")]
    pub r#payload: novel_codex_story_core::NarrativeCommitment,
    #[serde(rename = "extractionOrigin")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#extraction_origin: Option<novel_codex_story_core::CommitmentExtractionOrigin>,
    #[serde(rename = "sourceCurrent")]
    pub r#source_current: bool,
    #[serde(rename = "confirmationState")]
    pub r#confirmation_state: HttpHistoricalKnowledgeStateEntriesItemConfirmationState,
    #[serde(rename = "interpretation")]
    pub r#interpretation: HttpCommitmentDetailInterpretation,
    #[serde(rename = "literaryFulfillmentVerified")]
    pub r#literary_fulfillment_verified: WireFalse,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererGetCommitmentResponse(pub HttpCommitmentDetail);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpCommitmentPageDataItem {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "summary")]
    pub r#summary: String,
    #[serde(rename = "kind")]
    pub r#kind: novel_codex_story_core::CommitmentKind,
    #[serde(rename = "status")]
    pub r#status: novel_codex_story_core::CommitmentStatus,
    #[serde(rename = "revisionId")]
    pub r#revision_id: String,
    #[serde(rename = "confirmationState")]
    pub r#confirmation_state: HttpHistoricalKnowledgeStateEntriesItemConfirmationState,
    #[serde(rename = "sourceCurrent")]
    pub r#source_current: bool,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpCommitmentPage {
    #[serde(rename = "snapshotId")]
    pub r#snapshot_id: String,
    #[serde(rename = "nextCursor")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#next_cursor: Option<String>,
    #[serde(rename = "data")]
    pub r#data: Vec<HttpCommitmentPageDataItem>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererListCommitmentsResponse(pub HttpCommitmentPage);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpSaveCommitmentResponse {
    #[serde(rename = "revisionId")]
    pub r#revision_id: String,
    #[serde(rename = "snapshotId")]
    pub r#snapshot_id: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererSaveCommitmentResponse(pub HttpSaveCommitmentResponse);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpCreateCommitmentReviewResponse {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "artifactKind")]
    pub r#artifact_kind: HttpPlannedCommitmentRefVariant0Type,
    #[serde(rename = "artifactRevisionId")]
    pub r#artifact_revision_id: String,
    #[serde(rename = "projectRevision")]
    pub r#project_revision: i64,
    #[serde(rename = "status")]
    pub r#status: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererCreateCommitmentReviewResponse(pub HttpCreateCommitmentReviewResponse);
