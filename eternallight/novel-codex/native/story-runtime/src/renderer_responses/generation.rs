use super::*;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpGenerationRunSummaryUsageCheckVariant0State { #[serde(rename = "current")] Current }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpGenerationRunSummaryUsageCheckVariant0 {
    #[serde(rename = "state")]
    pub r#state: HttpGenerationRunSummaryUsageCheckVariant0State,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpGenerationRunSummaryUsageCheckVariant1State { #[serde(rename = "unavailable")] Unavailable }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpGenerationRunSummaryUsageCheckVariant1Phase {
    #[serde(rename = "taskRead")] TaskRead,
    #[serde(rename = "usageSettlement")] UsageSettlement,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpGenerationRunSummaryUsageCheckVariant1 {
    #[serde(rename = "state")]
    pub r#state: HttpGenerationRunSummaryUsageCheckVariant1State,
    #[serde(rename = "phase")]
    pub r#phase: HttpGenerationRunSummaryUsageCheckVariant1Phase,
    #[serde(rename = "code")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#code: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(untagged)]
pub enum HttpGenerationRunSummaryUsageCheck {
    GenerationRunSummaryUsageCheckVariant0(HttpGenerationRunSummaryUsageCheckVariant0),
    GenerationRunSummaryUsageCheckVariant1(HttpGenerationRunSummaryUsageCheckVariant1),
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpGenerationRunSummaryStatus {
    #[serde(rename = "running")] Running,
    #[serde(rename = "completed")] Completed,
    #[serde(rename = "cancelled")] Cancelled,
    #[serde(rename = "paused")] Paused,
    #[serde(rename = "blocked")] Blocked,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpGenerationRunSummaryStage {
    #[serde(rename = "coordinate")] Coordinate,
    #[serde(rename = "finished")] Finished,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpGenerationRunSummaryProviderBudgetChargesItem {
    #[serde(rename = "taskId")]
    pub r#task_id: String,
    #[serde(rename = "budgetId")]
    pub r#budget_id: String,
    #[serde(rename = "status")]
    pub r#status: String,
    #[serde(rename = "requests")]
    pub r#requests: f64,
    #[serde(rename = "tokens")]
    pub r#tokens: f64,
    #[serde(rename = "unknownTokens")]
    pub r#unknown_tokens: f64,
    #[serde(rename = "overrun")]
    pub r#overrun: bool,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpGenerationRunSummaryProviderBudget {
    #[serde(rename = "maxRequests")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#max_requests: Option<f64>,
    #[serde(rename = "maxTokens")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#max_tokens: Option<f64>,
    #[serde(rename = "usedRequests")]
    pub r#used_requests: f64,
    #[serde(rename = "usedTokens")]
    pub r#used_tokens: f64,
    #[serde(rename = "unknownTokens")]
    pub r#unknown_tokens: f64,
    #[serde(rename = "overrun")]
    pub r#overrun: bool,
    #[serde(rename = "charges")]
    pub r#charges: Vec<HttpGenerationRunSummaryProviderBudgetChargesItem>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpGenerationRunSummary {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "revisionPredecessor")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#revision_predecessor: Option<novel_codex_story_core::RevisionPredecessor>,
    #[serde(rename = "authorMessageId")]
    pub r#author_message_id: String,
    #[serde(rename = "usageCheck")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#usage_check: Option<HttpGenerationRunSummaryUsageCheck>,
    #[serde(rename = "currentChapterId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#current_chapter_id: Option<String>,
    #[serde(rename = "currentChapterNumber")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#current_chapter_number: Option<i64>,
    #[serde(rename = "status")]
    pub r#status: HttpGenerationRunSummaryStatus,
    #[serde(rename = "stage")]
    pub r#stage: HttpGenerationRunSummaryStage,
    #[serde(rename = "activeTaskId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#active_task_id: Option<String>,
    #[serde(rename = "pendingCurrentIntentGeneration")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#pending_current_intent_generation: Option<f64>,
    #[serde(rename = "completedChapters")]
    pub r#completed_chapters: f64,
    #[serde(rename = "chapterDeliveries")]
    pub r#chapter_deliveries: Vec<novel_codex_story_core::GenerationChapterDelivery>,
    #[serde(rename = "startChapterNumber")]
    pub r#start_chapter_number: i64,
    #[serde(rename = "targetChapters")]
    pub r#target_chapters: f64,
    #[serde(rename = "blockedReason")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#blocked_reason: Option<String>,
    #[serde(rename = "feedbackRepairCount")]
    pub r#feedback_repair_count: i64,
    #[serde(rename = "feedbackRepairPrompt")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#feedback_repair_prompt: Option<String>,
    #[serde(rename = "maxTaskAttempts")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#max_task_attempts: Option<i64>,
    #[serde(rename = "usedTaskAttempts")]
    pub r#used_task_attempts: i64,
    #[serde(rename = "providerBudget")]
    pub r#provider_budget: HttpGenerationRunSummaryProviderBudget,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpListGenerationRunsResponse {
    #[serde(rename = "runs")]
    pub r#runs: Vec<HttpGenerationRunSummary>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererListGenerationRunsResponse(pub HttpListGenerationRunsResponse);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpGenerationReplanPreviewRun {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "revisionPredecessor")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#revision_predecessor: Option<novel_codex_story_core::RevisionPredecessor>,
    #[serde(rename = "authorMessageId")]
    pub r#author_message_id: String,
    #[serde(rename = "usageCheck")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#usage_check: Option<HttpGenerationRunSummaryUsageCheck>,
    #[serde(rename = "currentChapterId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#current_chapter_id: Option<String>,
    #[serde(rename = "currentChapterNumber")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#current_chapter_number: Option<i64>,
    #[serde(rename = "status")]
    pub r#status: HttpGenerationRunSummaryStatus,
    #[serde(rename = "stage")]
    pub r#stage: HttpGenerationRunSummaryStage,
    #[serde(rename = "activeTaskId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#active_task_id: Option<String>,
    #[serde(rename = "pendingCurrentIntentGeneration")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#pending_current_intent_generation: Option<f64>,
    #[serde(rename = "completedChapters")]
    pub r#completed_chapters: f64,
    #[serde(rename = "chapterDeliveries")]
    pub r#chapter_deliveries: Vec<novel_codex_story_core::GenerationChapterDelivery>,
    #[serde(rename = "startChapterNumber")]
    pub r#start_chapter_number: i64,
    #[serde(rename = "targetChapters")]
    pub r#target_chapters: f64,
    #[serde(rename = "blockedReason")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#blocked_reason: Option<String>,
    #[serde(rename = "feedbackRepairCount")]
    pub r#feedback_repair_count: i64,
    #[serde(rename = "feedbackRepairPrompt")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#feedback_repair_prompt: Option<String>,
    #[serde(rename = "maxTaskAttempts")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#max_task_attempts: Option<i64>,
    #[serde(rename = "usedTaskAttempts")]
    pub r#used_task_attempts: i64,
    #[serde(rename = "providerBudget")]
    pub r#provider_budget: HttpGenerationRunSummaryProviderBudget,
    #[serde(rename = "version")]
    pub r#version: i64,
    #[serde(rename = "projectId")]
    pub r#project_id: String,
    #[serde(rename = "deadlineAt")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#deadline_at: Option<f64>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpGenerationReplanPreview {
    #[serde(rename = "run")]
    pub r#run: HttpGenerationReplanPreviewRun,
    #[serde(rename = "projectRevision")]
    pub r#project_revision: i64,
    #[serde(rename = "sourceDigest")]
    pub r#source_digest: String,
    #[serde(rename = "availableFrom")]
    pub r#available_from: Vec<novel_codex_story_authoring::ReplanStage>,
    #[serde(rename = "activeAttemptStatus")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#active_attempt_status: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererPreviewGenerationReplanResponse(pub HttpGenerationReplanPreview);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererReplanGenerationResponse(pub HttpGenerationRunSummary);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpSceneAssessmentSummaryAuthority { #[serde(rename = "candidateAnalysis")] CandidateAnalysis }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpSceneAssessmentSummaryState {
    #[serde(rename = "current")] Current,
    #[serde(rename = "stale")] Stale,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpSceneAssessmentSummaryAssessmentAssessmentsItemVerdict {
    #[serde(rename = "insufficientEvidence")] InsufficientEvidence,
    #[serde(rename = "realized")] Realized,
    #[serde(rename = "deviated")] Deviated,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpSceneAssessmentSummaryAssessmentAssessmentsItemRecommendation {
    #[serde(rename = "continue")] Continue,
    #[serde(rename = "repairDraft")] RepairDraft,
    #[serde(rename = "replan")] Replan,
    #[serde(rename = "askAuthor")] AskAuthor,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpSceneEffectAssessment {
    #[serde(rename = "effectId")]
    pub r#effect_id: String,
    #[serde(rename = "verdict")]
    pub r#verdict: HttpSceneAssessmentSummaryAssessmentAssessmentsItemVerdict,
    #[serde(rename = "rationale")]
    pub r#rationale: String,
    #[serde(rename = "actualEventRefs")]
    pub r#actual_event_refs: Vec<HttpSceneEventRef>,
    #[serde(rename = "commitmentAssessments")]
    pub r#commitment_assessments: Vec<HttpCommitmentAssessment>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpSceneAssessmentSummaryAssessmentAssessmentsItem {
    #[serde(rename = "sceneId")]
    pub r#scene_id: String,
    #[serde(rename = "verdict")]
    pub r#verdict: HttpSceneAssessmentSummaryAssessmentAssessmentsItemVerdict,
    #[serde(rename = "recommendation")]
    pub r#recommendation: HttpSceneAssessmentSummaryAssessmentAssessmentsItemRecommendation,
    #[serde(rename = "rationale")]
    pub r#rationale: String,
    #[serde(rename = "effectAssessments")]
    pub r#effect_assessments: Vec<HttpSceneEffectAssessment>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpSceneAssessmentSummaryAssessment {
    #[serde(rename = "assessments")]
    pub r#assessments: Vec<HttpSceneAssessmentSummaryAssessmentAssessmentsItem>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpSceneAssessmentSummary {
    #[serde(rename = "revisionId")]
    pub r#revision_id: String,
    #[serde(rename = "authority")]
    pub r#authority: HttpSceneAssessmentSummaryAuthority,
    #[serde(rename = "state")]
    pub r#state: HttpSceneAssessmentSummaryState,
    #[serde(rename = "assessment")]
    pub r#assessment: HttpSceneAssessmentSummaryAssessment,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererGetSceneAssessmentResponse(pub HttpSceneAssessmentSummary);
