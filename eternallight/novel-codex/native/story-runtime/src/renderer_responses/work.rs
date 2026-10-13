use super::*;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpWorkTaskSummaryInterludePhase {
    #[serde(rename = "failed")] Failed,
    #[serde(rename = "waiting")] Waiting,
    #[serde(rename = "answering")] Answering,
    #[serde(rename = "answered")] Answered,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpWorkTaskSummaryInterlude {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "parentId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#parent_id: Option<String>,
    #[serde(rename = "messageId")]
    pub r#message_id: String,
    #[serde(rename = "stepId")]
    pub r#step_id: String,
    #[serde(rename = "controlRevision")]
    pub r#control_revision: i64,
    #[serde(rename = "revision")]
    pub r#revision: i64,
    #[serde(rename = "autoReturn")]
    pub r#auto_return: bool,
    #[serde(rename = "phase")]
    pub r#phase: HttpWorkTaskSummaryInterludePhase,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpWorkTaskSummaryPauseAfterStep {
    #[serde(rename = "controlRevision")]
    pub r#control_revision: i64,
    #[serde(rename = "remainingAttempts")]
    pub r#remaining_attempts: i64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpWorkTaskSummaryControlDeliveryCommand {
    #[serde(rename = "pause")] Pause,
    #[serde(rename = "pauseAfterStep")] PauseAfterStep,
    #[serde(rename = "resume")] Resume,
    #[serde(rename = "cancel")] Cancel,
    #[serde(rename = "recoverSavedResult")] RecoverSavedResult,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpWorkTaskSummaryControlDeliveryState { #[serde(rename = "deliveryUnknown")] DeliveryUnknown }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpWorkTaskSummaryControlDelivery {
    #[serde(rename = "commandId")]
    pub r#command_id: String,
    #[serde(rename = "command")]
    pub r#command: HttpWorkTaskSummaryControlDeliveryCommand,
    #[serde(rename = "state")]
    pub r#state: HttpWorkTaskSummaryControlDeliveryState,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpWorkTaskSummaryKind {
    #[serde(rename = "conversation")] Conversation,
    #[serde(rename = "direct")] Direct,
    #[serde(rename = "generation")] Generation,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpWorkTaskSummaryDeliveryStatus {
    #[serde(rename = "partial")] Partial,
    #[serde(rename = "pending")] Pending,
    #[serde(rename = "ready_for_review")] ReadyForReview,
    #[serde(rename = "discussion_ended")] DiscussionEnded,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpCreationIssueStatus {
    #[serde(rename = "stale")] Stale,
    #[serde(rename = "open")] Open,
    #[serde(rename = "disputed")] Disputed,
    #[serde(rename = "resolved")] Resolved,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpCreationIssueObservationStatus {
    #[serde(rename = "supported")] Supported,
    #[serde(rename = "unmet")] Unmet,
    #[serde(rename = "insufficientEvidence")] InsufficientEvidence,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpCreationIssueObservationSource {
    #[serde(rename = "revisionOutcome")] RevisionOutcome,
    #[serde(rename = "deliveryReview")] DeliveryReview,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpCreationIssueObservation {
    #[serde(rename = "issueId")]
    pub r#issue_id: String,
    #[serde(rename = "observationId")]
    pub r#observation_id: String,
    #[serde(rename = "taskId")]
    pub r#task_id: String,
    #[serde(rename = "reportRevisionId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#report_revision_id: Option<String>,
    #[serde(rename = "draftRevisionId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#draft_revision_id: Option<String>,
    #[serde(rename = "criterionId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#criterion_id: Option<String>,
    #[serde(rename = "status")]
    pub r#status: HttpCreationIssueObservationStatus,
    #[serde(rename = "reason")]
    pub r#reason: String,
    #[serde(rename = "source")]
    pub r#source: HttpCreationIssueObservationSource,
    #[serde(rename = "current")]
    pub r#current: bool,
    #[serde(rename = "recheckRequired")]
    pub r#recheck_required: bool,
    #[serde(rename = "superseded")]
    pub r#superseded: bool,
    #[serde(rename = "resolutionOf")]
    pub r#resolution_of: Vec<String>,
    #[serde(rename = "evidence")]
    pub r#evidence: Vec<HttpObservationEvidence>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpCreationIssue {
    #[serde(rename = "issueId")]
    pub r#issue_id: String,
    #[serde(rename = "summary")]
    pub r#summary: String,
    #[serde(rename = "status")]
    pub r#status: HttpCreationIssueStatus,
    #[serde(rename = "chapterId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#chapter_id: Option<String>,
    #[serde(rename = "currentDraftRevisionId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#current_draft_revision_id: Option<String>,
    #[serde(rename = "observations")]
    pub r#observations: Vec<HttpCreationIssueObservation>,
    #[serde(rename = "observationCount")]
    pub r#observation_count: i64,
    #[serde(rename = "hasMoreObservations")]
    pub r#has_more_observations: bool,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpCreationIssueSummary {
    #[serde(rename = "totalCount")]
    pub r#total_count: i64,
    #[serde(rename = "openCount")]
    pub r#open_count: i64,
    #[serde(rename = "staleCount")]
    pub r#stale_count: i64,
    #[serde(rename = "disputedCount")]
    pub r#disputed_count: i64,
    #[serde(rename = "entries")]
    pub r#entries: Vec<HttpCreationIssue>,
    #[serde(rename = "hasMore")]
    pub r#has_more: bool,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpChapterAcceptanceRequirement {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "text")]
    pub r#text: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpChapterAcceptanceReportChapterAcceptanceLength {
    #[serde(rename = "requirement")]
    pub r#requirement: String,
    #[serde(rename = "minCharacters")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#min_characters: Option<f64>,
    #[serde(rename = "maxCharacters")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#max_characters: Option<f64>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpChapterAcceptanceReportChapterAcceptanceVerificationMethod {
    #[serde(rename = "direct")] Direct,
    #[serde(rename = "independent")] Independent,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpChapterAcceptanceReportChapterAcceptanceVerification {
    #[serde(rename = "method")]
    pub r#method: HttpChapterAcceptanceReportChapterAcceptanceVerificationMethod,
    #[serde(rename = "reason")]
    pub r#reason: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpChapterAcceptanceReportChapterAcceptance {
    #[serde(rename = "requirements")]
    pub r#requirements: Vec<HttpChapterAcceptanceRequirement>,
    #[serde(rename = "length")]
    pub r#length: HttpChapterAcceptanceReportChapterAcceptanceLength,
    #[serde(rename = "verification")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#verification: Option<HttpChapterAcceptanceReportChapterAcceptanceVerification>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpChapterAcceptanceReportVerdict {
    #[serde(rename = "stale")] Stale,
    #[serde(rename = "pending")] Pending,
    #[serde(rename = "insufficientEvidence")] InsufficientEvidence,
    #[serde(rename = "notMet")] NotMet,
    #[serde(rename = "passed")] Passed,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpChapterAcceptanceReportLength {
    #[serde(rename = "characterCount")]
    pub r#character_count: i64,
    #[serde(rename = "minCharacters")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#min_characters: Option<f64>,
    #[serde(rename = "maxCharacters")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#max_characters: Option<f64>,
    #[serde(rename = "satisfied")]
    pub r#satisfied: bool,
    #[serde(rename = "countUnit")]
    pub r#count_unit: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpChapterAcceptanceReportChecksItem {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "status")]
    pub r#status: HttpCreationIssueObservationStatus,
    #[serde(rename = "reason")]
    pub r#reason: String,
    #[serde(rename = "issueId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#issue_id: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpChapterAcceptanceReport {
    #[serde(rename = "writerTaskId")]
    pub r#writer_task_id: String,
    #[serde(rename = "chapterId")]
    pub r#chapter_id: String,
    #[serde(rename = "draftRevisionId")]
    pub r#draft_revision_id: String,
    #[serde(rename = "chapterAcceptance")]
    pub r#chapter_acceptance: HttpChapterAcceptanceReportChapterAcceptance,
    #[serde(rename = "reportRevisionId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#report_revision_id: Option<String>,
    #[serde(rename = "current")]
    pub r#current: bool,
    #[serde(rename = "verdict")]
    pub r#verdict: HttpChapterAcceptanceReportVerdict,
    #[serde(rename = "length")]
    pub r#length: HttpChapterAcceptanceReportLength,
    #[serde(rename = "checks")]
    pub r#checks: Vec<HttpChapterAcceptanceReportChecksItem>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpWorkTaskSummaryDeliveryReportEvidenceItemState {
    #[serde(rename = "missing")] Missing,
    #[serde(rename = "pending")] Pending,
    #[serde(rename = "recorded")] Recorded,
    #[serde(rename = "superseded")] Superseded,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpWorkTaskSummaryDeliveryReportEvidenceItem {
    #[serde(rename = "stepId")]
    pub r#step_id: String,
    #[serde(rename = "attemptId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#attempt_id: Option<String>,
    #[serde(rename = "kind")]
    pub r#kind: String,
    #[serde(rename = "referenceId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#reference_id: Option<String>,
    #[serde(rename = "state")]
    pub r#state: HttpWorkTaskSummaryDeliveryReportEvidenceItemState,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpWorkTaskSummaryDeliveryReportDeliveryReviewGoalStatus {
    #[serde(rename = "partial")] Partial,
    #[serde(rename = "fulfilled")] Fulfilled,
    #[serde(rename = "needsAuthorDecision")] NeedsAuthorDecision,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpCreationIssueLinkNewIssue {
    #[serde(rename = "summary")]
    pub r#summary: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpCreationIssueLink {
    #[serde(rename = "issueId")]
    pub r#issue_id: String,
    #[serde(rename = "newIssue")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#new_issue: Option<HttpCreationIssueLinkNewIssue>,
    #[serde(rename = "resolutionOf")]
    pub r#resolution_of: Vec<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpWorkTaskSummaryDeliveryReportDeliveryReviewUnresolvedItem {
    #[serde(rename = "issue")]
    pub r#issue: HttpCreationIssueLink,
    #[serde(rename = "observationId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#observation_id: Option<String>,
    #[serde(rename = "reason")]
    pub r#reason: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpWorkTaskSummaryDeliveryReportDeliveryReviewIssueResolutionsItem {
    #[serde(rename = "issue")]
    pub r#issue: HttpCreationIssueLink,
    #[serde(rename = "reason")]
    pub r#reason: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpWorkTaskSummaryDeliveryReportDeliveryReview {
    #[serde(rename = "goalStatus")]
    pub r#goal_status: HttpWorkTaskSummaryDeliveryReportDeliveryReviewGoalStatus,
    #[serde(rename = "goalAssessment")]
    pub r#goal_assessment: String,
    #[serde(rename = "checkedScope")]
    pub r#checked_scope: String,
    #[serde(rename = "unresolved")]
    pub r#unresolved: Vec<HttpWorkTaskSummaryDeliveryReportDeliveryReviewUnresolvedItem>,
    #[serde(rename = "issueResolutions")]
    pub r#issue_resolutions: Vec<HttpWorkTaskSummaryDeliveryReportDeliveryReviewIssueResolutionsItem>,
    #[serde(rename = "authorDecisions")]
    pub r#author_decisions: Vec<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpWorkTaskSummaryDeliveryReport {
    #[serde(rename = "issues")]
    pub r#issues: HttpCreationIssueSummary,
    #[serde(rename = "chapterAcceptanceReports")]
    pub r#chapter_acceptance_reports: Vec<HttpChapterAcceptanceReport>,
    #[serde(rename = "evidence")]
    pub r#evidence: Vec<HttpWorkTaskSummaryDeliveryReportEvidenceItem>,
    #[serde(rename = "pendingActionIds")]
    pub r#pending_action_ids: Vec<String>,
    #[serde(rename = "unsettledAttemptIds")]
    pub r#unsettled_attempt_ids: Vec<String>,
    #[serde(rename = "deliveryReview")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#delivery_review: Option<HttpWorkTaskSummaryDeliveryReportDeliveryReview>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpWorkTaskSummaryResumeBlockReasonValue {
    #[serde(rename="budgetExhausted")] BudgetExhausted,
    #[serde(rename = "recoveryStalled")] RecoveryStalled,
    #[serde(rename = "partialOutput")] PartialOutput,
    #[serde(rename = "inputChanged")] InputChanged,
    #[serde(rename = "attemptBudget")] AttemptBudget,
    #[serde(rename = "pendingRecovery")] PendingRecovery,
    #[serde(rename = "pendingUsage")] PendingUsage,
    #[serde(rename = "pendingWriteAndUsage")] PendingWriteAndUsage,
    #[serde(rename = "executionBlocked")] ExecutionBlocked,
    #[serde(rename = "readerRestart")] ReaderRestart,
    #[serde(rename = "unknownUsage")] UnknownUsage,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpWorkTaskSummaryResumeModeValue {
    #[serde(rename = "recoverSavedResult")] RecoverSavedResult,
    #[serde(rename = "continueExecution")] ContinueExecution,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpWorkTaskSummaryStepsItemStatus {
    #[serde(rename = "running")] Running,
    #[serde(rename = "completed")] Completed,
    #[serde(rename = "cancelled")] Cancelled,
    #[serde(rename = "queued")] Queued,
    #[serde(rename = "blocked")] Blocked,
    #[serde(rename = "superseded")] Superseded,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpWorkTaskSummaryStepsItemReadingProgressValueVariant0 {
    #[serde(rename = "status")]
    pub r#status: HttpGenerationRunSummaryUsageCheckVariant0State,
    #[serde(rename = "version")]
    pub r#version: i64,
    #[serde(rename = "pages")]
    pub r#pages: f64,
    #[serde(rename = "acknowledgedCharacters")]
    pub r#acknowledged_characters: f64,
    #[serde(rename = "traversal")]
    pub r#traversal: novel_codex_story_core::ReadingTraversal,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpWorkTaskSummaryStepsItemReadingProgressValueVariant1Status { #[serde(rename = "stale")] Stale }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpWorkTaskSummaryStepsItemReadingProgressValueVariant1 {
    #[serde(rename = "status")]
    pub r#status: HttpWorkTaskSummaryStepsItemReadingProgressValueVariant1Status,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpWorkTaskSummaryStepsItemReadingProgressValueVariant2 {
    #[serde(rename = "status")]
    pub r#status: HttpGenerationRunSummaryUsageCheckVariant1State,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(untagged)]
pub enum HttpWorkTaskSummaryStepsItemReadingProgressValue {
    WorkTaskSummaryStepsItemReadingProgressValueVariant0(HttpWorkTaskSummaryStepsItemReadingProgressValueVariant0),
    WorkTaskSummaryStepsItemReadingProgressValueVariant1(HttpWorkTaskSummaryStepsItemReadingProgressValueVariant1),
    WorkTaskSummaryStepsItemReadingProgressValueVariant2(HttpWorkTaskSummaryStepsItemReadingProgressValueVariant2),
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpWorkTaskSummaryStepsItemAttemptsItemFailureObservationsItemIssuer { #[serde(rename = "engine")] Engine }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpWorkTaskSummaryStepsItemAttemptsItemFailureObservationsItemPhase { #[serde(rename = "modelRequest")] ModelRequest }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpWorkTaskSummaryStepsItemAttemptsItemFailureObservationsItem {
    #[serde(rename = "issuer")]
    pub r#issuer: HttpWorkTaskSummaryStepsItemAttemptsItemFailureObservationsItemIssuer,
    #[serde(rename = "phase")]
    pub r#phase: HttpWorkTaskSummaryStepsItemAttemptsItemFailureObservationsItemPhase,
    #[serde(rename = "runId")]
    pub r#run_id: String,
    #[serde(rename = "requestId")]
    pub r#request_id: String,
    #[serde(rename = "errorCode")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#error_code: Option<String>,
    #[serde(rename = "errorStatusCode")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#error_status_code: Option<f64>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpWorkTaskSummaryStepsItemAttemptsItem {
    #[serde(rename = "failureObservations")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#failure_observations: Option<Vec<HttpWorkTaskSummaryStepsItemAttemptsItemFailureObservationsItem>>,
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "attemptNumber")]
    pub r#attempt_number: i64,
    #[serde(rename = "startedAt")]
    pub r#started_at: i64,
    #[serde(rename = "completedAt")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#completed_at: Option<i64>,
    #[serde(rename = "threadId")]
    pub r#thread_id: String,
    #[serde(rename = "turnId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#turn_id: Option<String>,
    #[serde(rename = "status")]
    pub r#status: String,
    #[serde(rename = "error")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#error: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpWorkTaskSummaryStepsItemOutputRefsItem {
    #[serde(rename = "kind")]
    pub r#kind: String,
    #[serde(rename = "revisionId")]
    pub r#revision_id: String,
    #[serde(rename = "objectId")]
    pub r#object_id: String,
    #[serde(rename = "workingBaseRevisionId")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#working_base_revision_id: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpWorkTaskSummaryStepsItemChangesItem {
    #[serde(rename = "commitId")]
    pub r#commit_id: String,
    #[serde(rename = "operation")]
    pub r#operation: String,
    #[serde(rename = "baseRevision")]
    pub r#base_revision: i64,
    #[serde(rename = "resultRevision")]
    pub r#result_revision: i64,
    #[serde(rename = "undoneByCommitId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#undone_by_commit_id: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpWorkTaskSummaryStepsItem {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "operation")]
    pub r#operation: String,
    #[serde(rename = "expectedOutputKind")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#expected_output_kind: Option<String>,
    #[serde(rename = "instruction")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#instruction: Option<String>,
    #[serde(rename = "sourceTaskId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#source_task_id: Option<String>,
    #[serde(rename = "target")]
    pub r#target: HttpActionTarget,
    #[serde(rename = "status")]
    pub r#status: HttpWorkTaskSummaryStepsItemStatus,
    #[serde(rename = "inputRevision")]
    pub r#input_revision: i64,
    #[serde(rename = "dependsOnStepId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#depends_on_step_id: Option<String>,
    #[serde(rename = "currentAttemptId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#current_attempt_id: Option<String>,
    #[serde(rename = "readingProgress")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#reading_progress: Option<HttpWorkTaskSummaryStepsItemReadingProgressValue>,
    #[serde(rename = "attempts")]
    pub r#attempts: Vec<HttpWorkTaskSummaryStepsItemAttemptsItem>,
    #[serde(rename = "outputRefs")]
    pub r#output_refs: Vec<HttpWorkTaskSummaryStepsItemOutputRefsItem>,
    #[serde(rename = "changes")]
    pub r#changes: Vec<HttpWorkTaskSummaryStepsItemChangesItem>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpWorkTaskSummary {
    #[serde(rename = "interlude")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#interlude: Option<HttpWorkTaskSummaryInterlude>,
    #[serde(rename = "pauseAfterStep")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#pause_after_step: Option<HttpWorkTaskSummaryPauseAfterStep>,
    #[serde(rename = "controlDelivery")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#control_delivery: Option<HttpWorkTaskSummaryControlDelivery>,
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "projectId")]
    pub r#project_id: String,
    #[serde(rename = "threadId")]
    pub r#thread_id: String,
    #[serde(rename = "goal")]
    pub r#goal: String,
    #[serde(rename = "kind")]
    pub r#kind: HttpWorkTaskSummaryKind,
    #[serde(rename = "status")]
    pub r#status: HttpGenerationRunSummaryStatus,
    #[serde(rename = "deliveryStatus")]
    pub r#delivery_status: HttpWorkTaskSummaryDeliveryStatus,
    #[serde(rename = "version")]
    pub r#version: i64,
    #[serde(rename = "controlRevision")]
    pub r#control_revision: i64,
    #[serde(rename = "error")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#error: Option<String>,
    #[serde(rename = "deliveryReport")]
    pub r#delivery_report: HttpWorkTaskSummaryDeliveryReport,
    #[serde(rename = "resumeBlockReason")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#resume_block_reason: Option<HttpWorkTaskSummaryResumeBlockReasonValue>,
    #[serde(rename = "resumeMode")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#resume_mode: Option<HttpWorkTaskSummaryResumeModeValue>,
    #[serde(rename = "pendingContinuation")]
    pub r#pending_continuation: bool,
    #[serde(rename = "steps")]
    pub r#steps: Vec<HttpWorkTaskSummaryStepsItem>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpListWorkTasksResponse {
    #[serde(rename = "tasks")]
    pub r#tasks: Vec<HttpWorkTaskSummary>,
    #[serde(rename = "nextCursor")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#next_cursor: Option<String>,
    #[serde(rename = "activeCount")]
    pub r#active_count: i64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererListWorkTasksResponse(pub HttpListWorkTasksResponse);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererGetWorkTaskResponse(pub HttpWorkTaskSummary);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpCreationIssuePage {
    #[serde(rename = "entries")]
    pub r#entries: Vec<HttpCreationIssue>,
    #[serde(rename = "totalCount")]
    pub r#total_count: i64,
    #[serde(rename = "nextCursor")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#next_cursor: Option<i64>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpCreationIssueObservationPage {
    #[serde(rename = "issueId")]
    pub r#issue_id: String,
    #[serde(rename = "entries")]
    pub r#entries: Vec<HttpCreationIssueObservation>,
    #[serde(rename = "totalCount")]
    pub r#total_count: i64,
    #[serde(rename = "nextCursor")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#next_cursor: Option<i64>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(untagged)]
pub enum HttpGetWorkIssuesResponse {
    CreationIssuePage(HttpCreationIssuePage),
    CreationIssueObservationPage(HttpCreationIssueObservationPage),
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererGetWorkIssuesResponse(pub HttpGetWorkIssuesResponse);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererGetTaskFinalReplyResponse(pub Option<novel_codex_story_core::TaskFinalReply>);

/// Typed orchestration fields; operation-specific author inputs remain JSON.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpActionTarget {
 #[serde(rename="desiredOutcomes",default,skip_serializing_if="Option::is_none")]
 #[ts(optional)]
 pub desired_outcomes: Option<Vec<novel_codex_story_core::DesiredOutcome>>,
 #[serde(rename="chapterAcceptance",default,skip_serializing_if="Option::is_none")]
 #[ts(optional)]
 pub chapter_acceptance: Option<HttpChapterAcceptanceReportChapterAcceptance>,
 #[serde(flatten)]
 pub inputs: std::collections::BTreeMap<String,serde_json::Value>,
}

/// Evidence provenance is typed independently of a model's assessment.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to="responses/")]
#[serde(untagged)]
pub enum HttpObservationEvidence {
 Source(HttpSourceCheckEvidence),
 Quote(HttpObservationQuote),
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to="responses/")]
#[serde(rename_all="camelCase")]
pub struct HttpSourceCheckEvidence {
 pub source_id: String,
 pub revision_id: String,
 pub content_hash: String,
 pub evidence: HttpStoryEvidence,
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to="responses/")]
#[serde(rename_all="camelCase")]
pub struct HttpObservationQuote {
 pub quote: String,
 pub start: usize,
 pub end: usize,
 #[serde(default,skip_serializing_if="Option::is_none")]
 #[ts(optional)]
 pub group_id: Option<String>,
}
