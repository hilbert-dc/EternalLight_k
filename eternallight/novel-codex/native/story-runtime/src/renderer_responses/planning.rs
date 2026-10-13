use super::*;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpPlanningReviewArtifactKind {
    #[serde(rename = "outline")] Outline,
    #[serde(rename = "plan")] Plan,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpPlanningReviewRole {
    #[serde(rename = "character")] Character,
    #[serde(rename = "structure")] Structure,
    #[serde(rename = "experience")] Experience,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpPlanningReviewRunStatus {
    #[serde(rename = "running")] Running,
    #[serde(rename = "completed")] Completed,
    #[serde(rename = "partial")] Partial,
    #[serde(rename = "cancelled")] Cancelled,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpPlanningReviewRunCoverageStatus {
    #[serde(rename = "partial")] Partial,
    #[serde(rename = "pending")] Pending,
    #[serde(rename = "complete")] Complete,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpPlanningReviewRunChecksByRole {
    #[serde(rename = "character")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#character: Option<Vec<String>>,
    #[serde(rename = "structure")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#structure: Option<Vec<String>>,
    #[serde(rename = "experience")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#experience: Option<Vec<String>>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpPlanningReviewRunSeatsItem {
    #[serde(rename = "role")]
    pub r#role: HttpPlanningReviewRole,
    #[serde(rename = "taskId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#task_id: Option<String>,
    #[serde(rename = "status")]
    pub r#status: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpPlanningReviewReportStatus {
    #[serde(rename = "partial")] Partial,
    #[serde(rename = "complete")] Complete,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpPlanningReviewReportReportStrengthsItem {
    #[serde(rename = "quote")]
    pub r#quote: String,
    #[serde(rename = "reason")]
    pub r#reason: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpPlanningReviewFinding {
    #[serde(rename = "quote")]
    pub r#quote: String,
    #[serde(rename = "observation")]
    pub r#observation: String,
    #[serde(rename = "impact")]
    pub r#impact: String,
    #[serde(rename = "suggestion")]
    pub r#suggestion: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpPlanningReviewReportReportCoverageItemStatus {
    #[serde(rename = "checked")] Checked,
    #[serde(rename = "unknown")] Unknown,
    #[serde(rename = "notChecked")] NotChecked,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpPlanningReviewReportReportCoverageItem {
    #[serde(rename = "checkId")]
    pub r#check_id: String,
    #[serde(rename = "status")]
    pub r#status: HttpPlanningReviewReportReportCoverageItemStatus,
    #[serde(rename = "reason")]
    #[serde(default,skip_serializing_if="Option::is_none")]
    #[ts(optional)]
    pub r#reason: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpPlanningReviewReportReport {
    #[serde(rename = "summary")]
    pub r#summary: String,
    #[serde(rename = "strengths")]
    pub r#strengths: Vec<HttpPlanningReviewReportReportStrengthsItem>,
    #[serde(rename = "findings")]
    pub r#findings: Vec<HttpPlanningReviewFinding>,
    #[serde(rename = "coverage")]
    pub r#coverage: Vec<HttpPlanningReviewReportReportCoverageItem>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpPlanningReviewReport {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "runId")]
    pub r#run_id: String,
    #[serde(rename = "role")]
    pub r#role: HttpPlanningReviewRole,
    #[serde(rename = "status")]
    pub r#status: HttpPlanningReviewReportStatus,
    #[serde(rename = "report")]
    pub r#report: HttpPlanningReviewReportReport,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpPlanningReviewRunSynthesisPrioritiesItem {
    #[serde(rename = "role")]
    pub r#role: HttpPlanningReviewRole,
    #[serde(rename = "reportId")]
    pub r#report_id: String,
    #[serde(rename = "findingIndex")]
    pub r#finding_index: i64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpPlanningReviewRunSynthesis {
    #[serde(rename = "priorities")]
    pub r#priorities: Vec<HttpPlanningReviewRunSynthesisPrioritiesItem>,
    #[serde(rename = "disagreements")]
    pub r#disagreements: Vec<String>,
    #[serde(rename = "incompleteRoles")]
    pub r#incomplete_roles: Vec<HttpPlanningReviewRole>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpPlanningReviewRun {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "workTaskId")]
    pub r#work_task_id: String,
    #[serde(rename = "sourceTaskId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#source_task_id: Option<String>,
    #[serde(rename = "sourceIntentRevision")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#source_intent_revision: Option<i64>,
    #[serde(rename = "focus")]
    pub r#focus: String,
    #[serde(rename = "projectId")]
    pub r#project_id: String,
    #[serde(rename = "artifactKind")]
    pub r#artifact_kind: HttpPlanningReviewArtifactKind,
    #[serde(rename = "revisionId")]
    pub r#revision_id: String,
    #[serde(rename = "targetNodeId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#target_node_id: Option<String>,
    #[serde(rename = "authorIntent")]
    pub r#author_intent: String,
    #[serde(rename = "authorMessageId")]
    pub r#author_message_id: String,
    #[serde(rename = "inputRevision")]
    pub r#input_revision: i64,
    #[serde(rename = "roles")]
    pub r#roles: Vec<HttpPlanningReviewRole>,
    #[serde(rename = "sourcePacketHash")]
    pub r#source_packet_hash: String,
    #[serde(rename = "status")]
    pub r#status: HttpPlanningReviewRunStatus,
    #[serde(rename = "coverageStatus")]
    pub r#coverage_status: HttpPlanningReviewRunCoverageStatus,
    #[serde(rename = "checksByRole")]
    pub r#checks_by_role: HttpPlanningReviewRunChecksByRole,
    #[serde(rename = "seats")]
    pub r#seats: Vec<HttpPlanningReviewRunSeatsItem>,
    #[serde(rename = "reports")]
    pub r#reports: Vec<HttpPlanningReviewReport>,
    #[serde(rename = "synthesis")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#synthesis: Option<HttpPlanningReviewRunSynthesis>,
    #[serde(rename = "createdAt")]
    pub r#created_at: String,
    #[serde(rename = "updatedAt")]
    pub r#updated_at: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpPlanningImpactStatus {
    #[serde(rename = "affected")] Affected,
    #[serde(rename = "locked-conflict")] LockedConflict,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpPlanningImpactState {
    #[serde(rename = "open")] Open,
    #[serde(rename = "resolved-kept")] ResolvedKept,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpPlanningImpactTargetArtifactKind {
    #[serde(rename = "outline")] Outline,
    #[serde(rename = "plan")] Plan,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpPlanningImpact {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "status")]
    pub r#status: HttpPlanningImpactStatus,
    #[serde(rename = "reason")]
    pub r#reason: String,
    #[serde(rename = "evidence")]
    pub r#evidence: String,
    #[serde(rename = "state")]
    pub r#state: HttpPlanningImpactState,
    #[serde(rename = "severity")]
    pub r#severity: HttpPlanningImpactStatus,
    #[serde(rename = "targetArtifactKind")]
    pub r#target_artifact_kind: HttpPlanningImpactTargetArtifactKind,
    #[serde(rename = "targetRevisionId")]
    pub r#target_revision_id: String,
    #[serde(rename = "targetNodeId")]
    pub r#target_node_id: String,
    #[serde(rename = "targetLabel")]
    pub r#target_label: String,
    #[serde(rename = "dependencyKey")]
    pub r#dependency_key: String,
    #[serde(rename = "resolutionNote")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#resolution_note: Option<String>,
    #[serde(rename = "sourceChapterId")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#source_chapter_id: Option<String>,
    #[serde(rename = "sourceChapterNumber")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#source_chapter_number: Option<i64>,
    #[serde(rename = "sourceChapterTitle")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#source_chapter_title: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpWriterChapterProjection {
    #[serde(rename = "chapterPlanId")]
    pub r#chapter_plan_id: String,
    #[serde(rename = "primaryNodeId")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#primary_node_id: Option<String>,
    #[serde(rename = "chapterNumber")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#chapter_number: Option<i64>,
    #[serde(rename = "publicTitle")]
    pub r#public_title: String,
    #[serde(rename = "writerGuidance")]
    pub r#writer_guidance: String,
    #[serde(rename = "publicCommitments")]
    pub r#public_commitments: Vec<String>,
    #[serde(rename = "povCharacterId")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub r#pov_character_id: Option<String>,
    #[serde(rename = "storyTime")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub r#story_time: Option<f64>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpCreativeWorkspaceChapterPlanBindingsItem {
    #[serde(rename = "project_id")]
    pub r#project_id: String,
    #[serde(rename = "plan_revision_id")]
    pub r#plan_revision_id: String,
    #[serde(rename = "chapter_plan_id")]
    pub r#chapter_plan_id: String,
    #[serde(rename = "chapter_id")]
    pub r#chapter_id: String,
    #[serde(rename = "lineageStatus")]
    pub r#lineage_status: HttpDraftRevisionSourcePlanRefsLineageStatus,
    #[serde(rename = "writerProjection")]
    pub r#writer_projection: HttpWriterChapterProjection,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererGetPlanningReviewResponse(pub HttpPlanningReviewRun);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererCancelPlanningReviewResponse(pub HttpPlanningReviewRun);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpWriterContextPreviewPurpose { #[serde(rename = "writer")] Writer }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpWriterContextPreviewTargetPosition {
    #[serde(rename = "readingChapter")]
    pub r#reading_chapter: f64,
    #[serde(rename = "sceneOrder")]
    pub r#scene_order: f64,
    #[serde(rename = "storyTime")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#story_time: Option<f64>,
    #[serde(rename = "povCharacterId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#pov_character_id: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpWriterContextPreviewTarget {
    #[serde(rename = "projectId")]
    pub r#project_id: String,
    #[serde(rename = "projectRevision")]
    pub r#project_revision: i64,
    #[serde(rename = "position")]
    pub r#position: HttpWriterContextPreviewTargetPosition,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpWriterContextPreviewPlannedEffectsItemSource { #[serde(rename = "plan")] Plan }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpWriterContextPreviewPlannedEffectsItemValueChaptersItem {
    #[serde(rename = "chapterPlanId")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#chapter_plan_id: Option<String>,
    #[serde(rename = "primaryNodeId")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#primary_node_id: Option<String>,
    #[serde(rename = "chapterNumber")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#chapter_number: Option<i64>,
    #[serde(rename = "publicTitle")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#public_title: Option<String>,
    #[serde(rename = "writerGuidance")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#writer_guidance: Option<String>,
    #[serde(rename = "publicCommitments")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#public_commitments: Option<Vec<String>>,
    #[serde(rename = "povCharacterId")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub r#pov_character_id: Option<String>,
    #[serde(rename = "storyTime")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub r#story_time: Option<f64>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpWriterContextPreviewPlannedEffectsItemValue {
    #[serde(rename = "planRevisionId")]
    pub r#plan_revision_id: String,
    #[serde(rename = "chapters")]
    pub r#chapters: Vec<HttpWriterContextPreviewPlannedEffectsItemValueChaptersItem>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpWriterContextPreviewPlannedEffectsItem {
    #[serde(rename = "source")]
    pub r#source: HttpWriterContextPreviewPlannedEffectsItemSource,
    #[serde(rename = "value")]
    pub r#value: HttpWriterContextPreviewPlannedEffectsItemValue,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpWriterContextPreviewRelevantTextItemVariant0Authority {
    #[serde(rename = "candidate")] Candidate,
    #[serde(rename = "accepted")] Accepted,
    #[serde(rename = "working")] Working,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpWriterContextPreviewRelevantTextItemVariant0Content {
    #[serde(rename = "chapterId")]
    pub r#chapter_id: String,
    #[serde(rename = "chapterNumber")]
    pub r#chapter_number: i64,
    #[serde(rename = "title")]
    pub r#title: String,
    #[serde(rename = "body")]
    pub r#body: String,
    #[serde(rename = "sceneGroups")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#scene_groups: Option<Vec<HttpDraftRevisionSceneGroupsItem>>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpWriterContextPreviewRelevantTextItemVariant0TextRepresentation { #[serde(rename = "body")] Body }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpWriterContextPreviewRelevantTextItemVariant0 {
    #[serde(rename = "objectId")]
    pub r#object_id: String,
    #[serde(rename = "sourceRevisionId")]
    pub r#source_revision_id: String,
    #[serde(rename = "authority")]
    pub r#authority: HttpWriterContextPreviewRelevantTextItemVariant0Authority,
    #[serde(rename = "workingSelectionRevision")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#working_selection_revision: Option<i64>,
    #[serde(rename = "content")]
    pub r#content: HttpWriterContextPreviewRelevantTextItemVariant0Content,
    #[serde(rename = "textRepresentation")]
    pub r#text_representation: HttpWriterContextPreviewRelevantTextItemVariant0TextRepresentation,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpWriterContextPreviewRelevantTextItemVariant1Content {
    #[serde(rename = "chapterId")]
    pub r#chapter_id: String,
    #[serde(rename = "chapterNumber")]
    pub r#chapter_number: i64,
    #[serde(rename = "title")]
    pub r#title: String,
    #[serde(rename = "sceneGroups")]
    pub r#scene_groups: Vec<HttpDraftRevisionSceneGroupsItem>,
    #[serde(rename = "body")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#body: Option<()>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpWriterContextPreviewRelevantTextItemVariant1TextRepresentation { #[serde(rename = "sceneGroups")] SceneGroups }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpWriterContextPreviewRelevantTextItemVariant1 {
    #[serde(rename = "objectId")]
    pub r#object_id: String,
    #[serde(rename = "sourceRevisionId")]
    pub r#source_revision_id: String,
    #[serde(rename = "authority")]
    pub r#authority: HttpWriterContextPreviewRelevantTextItemVariant0Authority,
    #[serde(rename = "workingSelectionRevision")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#working_selection_revision: Option<i64>,
    #[serde(rename = "content")]
    pub r#content: HttpWriterContextPreviewRelevantTextItemVariant1Content,
    #[serde(rename = "textRepresentation")]
    pub r#text_representation: HttpWriterContextPreviewRelevantTextItemVariant1TextRepresentation,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(untagged)]
pub enum HttpWriterContextPreviewRelevantTextItem {
    WriterContextPreviewRelevantTextItemVariant0(HttpWriterContextPreviewRelevantTextItemVariant0),
    WriterContextPreviewRelevantTextItemVariant1(HttpWriterContextPreviewRelevantTextItemVariant1),
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpWriterContextPreviewCoverageScannedScope {
    #[serde(rename = "factsIncluded")]
    pub r#facts_included: f64,
    #[serde(rename = "draftsIncluded")]
    pub r#drafts_included: f64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpWriterContextPreviewCoverage {
    #[serde(rename = "complete")]
    pub r#complete: bool,
    #[serde(rename = "semanticCompleteness")]
    pub r#semantic_completeness: HttpCharacterKnowledgeCoverageSemanticCompleteness,
    #[serde(rename = "missingScope")]
    pub r#missing_scope: Vec<String>,
    #[serde(rename = "truncationReasons")]
    pub r#truncation_reasons: Vec<String>,
    #[serde(rename = "scannedScope")]
    pub r#scanned_scope: HttpWriterContextPreviewCoverageScannedScope,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpWriterContextPreviewBudgetsUsed {
    #[serde(rename = "byteLimit")]
    pub r#byte_limit: i64,
    #[serde(rename = "byteSize")]
    pub r#byte_size: f64,
    #[serde(rename = "entryLimit")]
    pub r#entry_limit: i64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpWriterContextPreview {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "purpose")]
    pub r#purpose: HttpWriterContextPreviewPurpose,
    #[serde(rename = "taskId")]
    pub r#task_id: (),
    #[serde(rename = "snapshotId")]
    pub r#snapshot_id: String,
    #[serde(rename = "contentHash")]
    pub r#content_hash: String,
    #[serde(rename = "target")]
    pub r#target: HttpWriterContextPreviewTarget,
    #[serde(rename = "plannedEffects")]
    pub r#planned_effects: Vec<HttpWriterContextPreviewPlannedEffectsItem>,
    #[serde(rename = "relevantText")]
    pub r#relevant_text: Vec<HttpWriterContextPreviewRelevantTextItem>,
    #[serde(rename = "coverage")]
    pub r#coverage: HttpWriterContextPreviewCoverage,
    #[serde(rename = "budgetsUsed")]
    pub r#budgets_used: HttpWriterContextPreviewBudgetsUsed,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpListPlanningImpactsResponseCounts {
    #[serde(rename = "total")]
    pub r#total: i64,
    #[serde(rename = "open")]
    pub r#open: f64,
    #[serde(rename = "retained")]
    pub r#retained: f64,
    #[serde(rename = "lockedConflict")]
    pub r#locked_conflict: f64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpListPlanningImpactsResponse {
    #[serde(rename = "data")]
    pub r#data: Vec<HttpPlanningImpact>,
    #[serde(rename = "nextCursor")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#next_cursor: Option<String>,
    #[serde(rename = "counts")]
    pub r#counts: HttpListPlanningImpactsResponseCounts,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererListPlanningImpactsResponse(pub HttpListPlanningImpactsResponse);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpCreativeArtifactKind {
    #[serde(rename = "brief")] Brief,
    #[serde(rename = "outline")] Outline,
    #[serde(rename = "plan")] Plan,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpStoryBriefPayload {
    #[serde(rename = "premise")]
    pub r#premise: String,
    #[serde(rename = "promise")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#promise: Option<String>,
    #[serde(rename = "stakes")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#stakes: Option<String>,
    #[serde(rename = "endingDirection")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#ending_direction: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpOutlineNodeKind {
    #[serde(rename = "book")] Book,
    #[serde(rename = "volume")] Volume,
    #[serde(rename = "unit")] Unit,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpOutlineNodeCertainty {
    #[serde(rename = "open")] Open,
    #[serde(rename = "tentative")] Tentative,
    #[serde(rename = "settled")] Settled,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpOutlineNodeProtection {
    #[serde(rename = "free")] Free,
    #[serde(rename = "preserve-intent")] PreserveIntent,
    #[serde(rename = "preserve-arrangement")] PreserveArrangement,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpOutlineNode {
    #[serde(rename = "id")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#id: Option<String>,
    #[serde(rename = "clientKey")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#client_key: Option<String>,
    #[serde(rename = "parentId")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#parent_id: Option<String>,
    #[serde(rename = "parentClientKey")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#parent_client_key: Option<String>,
    #[serde(rename = "kind")]
    pub r#kind: HttpOutlineNodeKind,
    #[serde(rename = "position")]
    pub r#position: i64,
    #[serde(rename = "title")]
    pub r#title: String,
    #[serde(rename = "publicTitle")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#public_title: Option<String>,
    #[serde(rename = "purpose")]
    pub r#purpose: String,
    #[serde(rename = "summary")]
    pub r#summary: String,
    #[serde(rename = "dependencyKeys")]
    pub r#dependency_keys: Vec<String>,
    #[serde(rename = "certainty")]
    pub r#certainty: HttpOutlineNodeCertainty,
    #[serde(rename = "protection")]
    pub r#protection: HttpOutlineNodeProtection,
    #[serde(rename = "writerGuidance")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#writer_guidance: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpOutlinePayload {
    #[serde(rename = "nodes")]
    pub r#nodes: Vec<HttpOutlineNode>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpPlannedBeat {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "description")]
    pub r#description: String,
    #[serde(rename = "locked")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#locked: Option<bool>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpPlannedEffectDimension {
    #[serde(rename = "world")] World,
    #[serde(rename = "knowledge")] Knowledge,
    #[serde(rename = "relationship")] Relationship,
    #[serde(rename = "narrative")] Narrative,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpPlannedEffect {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "dimension")]
    pub r#dimension: HttpPlannedEffectDimension,
    #[serde(rename = "description")]
    pub r#description: String,
    #[serde(rename = "commitmentRefs")]
    pub r#commitment_refs: Vec<HttpPlannedCommitmentRef>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpPlannedScene {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "title")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#title: Option<String>,
    #[serde(rename = "goal")]
    pub r#goal: String,
    #[serde(rename = "conflict")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#conflict: Option<String>,
    #[serde(rename = "beats")]
    pub r#beats: Vec<HttpPlannedBeat>,
    #[serde(rename = "expectedEffects")]
    pub r#expected_effects: Vec<HttpPlannedEffect>,
    #[serde(rename = "locked")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#locked: Option<bool>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpChapterPlanPayload {
    #[serde(rename = "chapterPlanId")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#chapter_plan_id: Option<String>,
    #[serde(rename = "clientKey")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#client_key: Option<String>,
    #[serde(rename = "primaryNodeId")]
    pub r#primary_node_id: String,
    #[serde(rename = "chapterNumber")]
    pub r#chapter_number: i64,
    #[serde(rename = "title")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#title: Option<String>,
    #[serde(rename = "goal")]
    pub r#goal: String,
    #[serde(rename = "conflict")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#conflict: Option<String>,
    #[serde(rename = "dependencyKeys")]
    pub r#dependency_keys: Vec<String>,
    #[serde(rename = "scenes")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#scenes: Option<Vec<HttpPlannedScene>>,
    #[serde(rename = "locked")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#locked: Option<bool>,
    #[serde(rename = "publicTitle")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#public_title: Option<String>,
    #[serde(rename = "writerGuidance")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#writer_guidance: Option<String>,
    #[serde(rename = "publicCommitments")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#public_commitments: Option<Vec<String>>,
    #[serde(rename = "allowances")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#allowances: Option<Vec<String>>,
    #[serde(rename = "povCharacterId")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub r#pov_character_id: Option<String>,
    #[serde(rename = "storyTime")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub r#story_time: Option<f64>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpRollingPlanPayload {
    #[serde(rename = "sourceOutlineRevisionId")]
    pub r#source_outline_revision_id: String,
    #[serde(rename = "window")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#window: Option<f64>,
    #[serde(rename = "chapters")]
    pub r#chapters: Vec<HttpChapterPlanPayload>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(untagged)]
pub enum HttpCreativeArtifactPayload {
    StoryBriefPayload(HttpStoryBriefPayload),
    OutlinePayload(HttpOutlinePayload),
    RollingPlanPayload(HttpRollingPlanPayload),
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpMaterializeChapterPlanResponse {
    #[serde(rename = "chapterId")]
    pub r#chapter_id: String,
    #[serde(rename = "resultRevision")]
    pub r#result_revision: i64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererMaterializeChapterPlanResponse(pub HttpMaterializeChapterPlanResponse);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpWorkCatalogVolumesItemOutlineStatus {
    #[serde(rename = "unavailable")] Unavailable,
    #[serde(rename = "independent")] Independent,
    #[serde(rename = "linked")] Linked,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpAssociateChapterPlanResponse {
    #[serde(rename = "chapterId")]
    pub r#chapter_id: String,
    #[serde(rename = "chapterPlanId")]
    pub r#chapter_plan_id: String,
    #[serde(rename = "resultRevision")]
    pub r#result_revision: i64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererAssociateChapterPlanResponse(pub HttpAssociateChapterPlanResponse);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpChapterPlanRebindReceiptBinding {
    #[serde(rename = "planRevisionId")]
    pub r#plan_revision_id: String,
    #[serde(rename = "chapterPlanId")]
    pub r#chapter_plan_id: String,
    #[serde(rename = "chapterId")]
    pub r#chapter_id: String,
    #[serde(rename = "chapterNumber")]
    pub r#chapter_number: i64,
    #[serde(rename = "resultRevision")]
    pub r#result_revision: i64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpChapterPlanRebindReceipt {
    #[serde(rename = "previousPlanRevisionId")]
    pub r#previous_plan_revision_id: String,
    #[serde(rename = "previousChapterPlanId")]
    pub r#previous_chapter_plan_id: String,
    #[serde(rename = "sourceRevisionId")]
    pub r#source_revision_id: String,
    #[serde(rename = "binding")]
    pub r#binding: HttpChapterPlanRebindReceiptBinding,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererRebindChapterPlanResponse(pub HttpChapterPlanRebindReceipt);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpResolvePlanningImpactResponse {
    #[serde(rename = "commitId")]
    pub r#commit_id: String,
    #[serde(rename = "resultRevision")]
    pub r#result_revision: i64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererResolvePlanningImpactResponse(pub HttpResolvePlanningImpactResponse);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpProvisionalPlanningImpactMatchedBy {
    #[serde(rename = "dependency-key")] DependencyKey,
    #[serde(rename = "author-link")] AuthorLink,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpProvisionalPlanningImpact {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "factProposalId")]
    pub r#fact_proposal_id: String,
    #[serde(rename = "predicate")]
    pub r#predicate: String,
    #[serde(rename = "evidence")]
    pub r#evidence: String,
    #[serde(rename = "targetArtifactKind")]
    pub r#target_artifact_kind: HttpPlanningImpactTargetArtifactKind,
    #[serde(rename = "targetRevisionId")]
    pub r#target_revision_id: String,
    #[serde(rename = "targetNodeId")]
    pub r#target_node_id: String,
    #[serde(rename = "targetLabel")]
    pub r#target_label: String,
    #[serde(rename = "dependencyKey")]
    pub r#dependency_key: String,
    #[serde(rename = "severity")]
    pub r#severity: HttpPlanningImpactStatus,
    #[serde(rename = "matchedBy")]
    pub r#matched_by: HttpProvisionalPlanningImpactMatchedBy,
}
