use super::*;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpDraftRevisionSourcePlanRefsLineageStatus {
    #[serde(rename = "current")] Current,
    #[serde(rename = "historical")] Historical,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpDraftRevisionSourcePlanRefs {
    #[serde(rename = "lineageStatus")]
    pub r#lineage_status: HttpDraftRevisionSourcePlanRefsLineageStatus,
    #[serde(rename = "planRevisionId")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#plan_revision_id: Option<String>,
    #[serde(rename = "outlineRevisionId")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#outline_revision_id: Option<String>,
    #[serde(rename = "chapterPlanId")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#chapter_plan_id: Option<String>,
    #[serde(rename = "localOnly")]
    pub r#local_only: bool,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpDraftRevisionSceneGroupsItem {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "sceneId")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub r#scene_id: Option<String>,
    #[serde(rename = "body")]
    pub r#body: String,
    #[serde(rename = "locked")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#locked: Option<bool>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpDraftRevision {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "chapterId")]
    pub r#chapter_id: String,
    #[serde(rename = "title")]
    pub r#title: String,
    #[serde(rename = "version")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#version: Option<i64>,
    #[serde(rename = "parentRevisionId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#parent_revision_id: Option<String>,
    #[serde(rename = "sourcePlanRefs")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#source_plan_refs: Option<HttpDraftRevisionSourcePlanRefs>,
    #[serde(rename = "baseProjectRevision")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#base_project_revision: Option<i64>,
    #[serde(rename = "body")]
    pub r#body: String,
    #[serde(rename = "sceneGroups")]
    pub r#scene_groups: Vec<HttpDraftRevisionSceneGroupsItem>,
    #[serde(rename = "source")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#source: Option<String>,
    #[serde(rename = "createdAt")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#created_at: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpCreativeWorkspaceProject {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "title")]
    pub r#title: String,
    #[serde(rename = "logline")]
    pub r#logline: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpCreativeWorkspacePlanLineageStatus {
    #[serde(rename = "current")] Current,
    #[serde(rename = "missing")] Missing,
    #[serde(rename = "stale")] Stale,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpCreativeWorkspaceIndexStatus {
    #[serde(rename = "ready")] Ready,
    #[serde(rename = "stale")] Stale,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpCreativeWorkspaceIndex {
    #[serde(rename = "targetRevision")]
    pub r#target_revision: i64,
    #[serde(rename = "indexedRevision")]
    pub r#indexed_revision: i64,
    #[serde(rename = "status")]
    pub r#status: HttpCreativeWorkspaceIndexStatus,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpCreativeRevision {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "version")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#version: Option<i64>,
    #[serde(rename = "parentRevisionId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#parent_revision_id: Option<String>,
    #[serde(rename = "baseProjectRevision")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#base_project_revision: Option<i64>,
    #[serde(rename = "payload")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#payload: Option<std::collections::BTreeMap<String, serde_json::Value>>,
    #[serde(rename = "source")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#source: Option<String>,
    #[serde(rename = "createdAt")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#created_at: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpWorkingBaselineSelectionObjectKind {
    #[serde(rename = "draft")] Draft,
    #[serde(rename = "brief")] Brief,
    #[serde(rename = "outline")] Outline,
    #[serde(rename = "plan")] Plan,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpWorkingBaselineSelection {
    #[serde(rename = "objectKind")]
    pub r#object_kind: HttpWorkingBaselineSelectionObjectKind,
    #[serde(rename = "objectId")]
    pub r#object_id: String,
    #[serde(rename = "revisionId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#revision_id: Option<String>,
    #[serde(rename = "selectionRevision")]
    pub r#selection_revision: i64,
    #[serde(rename = "selectedAt")]
    pub r#selected_at: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpCreativeWorkspaceArtifactsBrief {
    #[serde(rename = "candidate")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#candidate: Option<HttpCreativeRevision>,
    #[serde(rename = "accepted")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#accepted: Option<HttpCreativeRevision>,
    #[serde(rename = "working")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#working: Option<HttpCreativeRevision>,
    #[serde(rename = "locked")]
    pub r#locked: bool,
    #[serde(rename = "workingSelection")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#working_selection: Option<HttpWorkingBaselineSelection>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpCreativeRevision2Payload {
    #[serde(rename = "decisions")]
    pub r#decisions: Vec<HttpIdentityDecision>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpCreativeRevision2 {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "version")]
    pub r#version: i64,
    #[serde(rename = "parentRevisionId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#parent_revision_id: Option<String>,
    #[serde(rename = "baseProjectRevision")]
    pub r#base_project_revision: i64,
    #[serde(rename = "payload")]
    pub r#payload: HttpCreativeRevision2Payload,
    #[serde(rename = "source")]
    pub r#source: String,
    #[serde(rename = "createdAt")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#created_at: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpCreativeRevision3Payload {
    #[serde(rename = "decisions")]
    pub r#decisions: Vec<HttpIdentityDecision>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpCreativeRevision3 {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "version")]
    pub r#version: i64,
    #[serde(rename = "parentRevisionId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#parent_revision_id: Option<String>,
    #[serde(rename = "baseProjectRevision")]
    pub r#base_project_revision: i64,
    #[serde(rename = "payload")]
    pub r#payload: HttpCreativeRevision3Payload,
    #[serde(rename = "source")]
    pub r#source: String,
    #[serde(rename = "createdAt")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#created_at: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpCreativeRevision4 {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "version")]
    pub r#version: i64,
    #[serde(rename = "parentRevisionId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#parent_revision_id: Option<String>,
    #[serde(rename = "baseProjectRevision")]
    pub r#base_project_revision: i64,
    #[serde(rename = "payload")]
    pub r#payload: HttpPropertyRulesPayload,
    #[serde(rename = "source")]
    pub r#source: String,
    #[serde(rename = "createdAt")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#created_at: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpCreativeWorkspaceArtifacts {
    #[serde(rename = "brief")]
    pub r#brief: HttpCreativeWorkspaceArtifactsBrief,
    #[serde(rename = "outline")]
    pub r#outline: HttpCreativeWorkspaceArtifactsBrief,
    #[serde(rename = "plan")]
    pub r#plan: HttpCreativeWorkspaceArtifactsBrief,
    #[serde(rename = "identity")]
    pub r#identity: HttpCreativeWorkspaceArtifactsIdentity,
    #[serde(rename = "propertyRules")]
    pub r#property_rules: HttpCreativeWorkspaceArtifactsPropertyRules,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpCreativeWorkspaceChaptersItem {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "number")]
    pub r#number: i64,
    #[serde(rename = "title")]
    pub r#title: String,
    #[serde(rename = "status")]
    pub r#status: String,
    #[serde(rename = "summary")]
    pub r#summary: String,
    #[serde(rename = "updated_at")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#updated_at: Option<String>,
    #[serde(rename = "candidate")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#candidate: Option<HttpDraftRevision>,
    #[serde(rename = "accepted")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#accepted: Option<HttpDraftRevision>,
    #[serde(rename = "working")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#working: Option<HttpDraftRevision>,
    #[serde(rename = "workingSelection")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#working_selection: Option<HttpWorkingBaselineSelection>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpCreativeWorkspaceContinuationIntentsItemStatus {
    #[serde(rename = "ready")] Ready,
    #[serde(rename = "launched")] Launched,
    #[serde(rename = "failed")] Failed,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpCreativeWorkspaceContinuationIntentsItem {
    #[serde(rename = "targetKind")]
    pub r#target_kind: HttpWorkingBaselineSelectionObjectKind,
    #[serde(rename = "targetId")]
    pub r#target_id: String,
    #[serde(rename = "workingRevisionId")]
    pub r#working_revision_id: String,
    #[serde(rename = "authorIntent")]
    pub r#author_intent: String,
    #[serde(rename = "status")]
    pub r#status: HttpCreativeWorkspaceContinuationIntentsItemStatus,
    #[serde(rename = "taskId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#task_id: Option<String>,
    #[serde(rename = "updatedAt")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#updated_at: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpCreativeWorkspaceModelingItemStatus {
    #[serde(rename = "ready")] Ready,
    #[serde(rename = "model_pending")] ModelPending,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpCreativeWorkspaceModelingItemConfirmationState { #[serde(rename = "candidate")] Candidate }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpCreativeWorkspaceModelingItem {
    #[serde(rename = "objectId")]
    pub r#object_id: String,
    #[serde(rename = "sourceRevisionId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#source_revision_id: Option<String>,
    #[serde(rename = "status")]
    pub r#status: HttpCreativeWorkspaceModelingItemStatus,
    #[serde(rename = "batchId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#batch_id: Option<String>,
    #[serde(rename = "entities")]
    pub r#entities: Vec<HttpModeledEntity>,
    #[serde(rename = "assertions")]
    pub r#assertions: Vec<HttpStoryAssertion>,
    #[serde(rename = "confirmationState")]
    pub r#confirmation_state: HttpCreativeWorkspaceModelingItemConfirmationState,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpCreativeWorkspaceFactProposalsItemFactKind {
    #[serde(rename = "character")] Character,
    #[serde(rename = "world")] World,
    #[serde(rename = "reader")] Reader,
    #[serde(rename = "author-secret")] AuthorSecret,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpCreativeWorkspaceFactProposalsItemEpistemicState {
    #[serde(rename = "unknown")] Unknown,
    #[serde(rename = "suspected")] Suspected,
    #[serde(rename = "known")] Known,
    #[serde(rename = "false")] False,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpCreativeWorkspaceFactProposalsItemSource {
    #[serde(rename = "author")] Author,
    #[serde(rename = "agent")] Agent,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpCreativeWorkspaceFactProposalsItem {
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
    #[serde(rename = "draft_revision_id")]
    pub r#draft_revision_id: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpCreativeWorkspaceCommitsItemOperationKind {
    #[serde(rename = "accept")] Accept,
    #[serde(rename = "materialize-chapter")] MaterializeChapter,
    #[serde(rename = "materialize-standalone")] MaterializeStandalone,
    #[serde(rename = "associate-plan")] AssociatePlan,
    #[serde(rename = "resolve-impact")] ResolveImpact,
    #[serde(rename = "undo")] Undo,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpCreativeWorkspaceCommitsItem {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "operation_kind")]
    pub r#operation_kind: HttpCreativeWorkspaceCommitsItemOperationKind,
    #[serde(rename = "proposalApplicationId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#proposal_application_id: Option<String>,
    #[serde(rename = "result_revision")]
    pub r#result_revision: i64,
    #[serde(rename = "receipt")]
    pub r#receipt: HttpAcceptanceReceipt,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpCreativeWorkspaceTasksItemPurpose {
    #[serde(rename = "planner")] Planner,
    #[serde(rename = "writer")] Writer,
    #[serde(rename = "critic")] Critic,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpCreativeWorkspaceTasksItemDeliveryStatus {
    #[serde(rename = "pending")] Pending,
    #[serde(rename = "ready_for_review")] ReadyForReview,
    #[serde(rename = "incomplete")] Incomplete,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpCreativeWorkspaceTasksItemTargetScopeVariant0Type { #[serde(rename = "wholeChapter")] WholeChapter }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpCreativeWorkspaceTasksItemTargetScopeVariant0 {
    #[serde(rename = "type")]
    pub r#type: HttpCreativeWorkspaceTasksItemTargetScopeVariant0Type,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpCreativeWorkspaceTasksItemTargetScopeVariant1Type { #[serde(rename = "throughScene")] ThroughScene }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpCreativeWorkspaceTasksItemTargetScopeVariant1 {
    #[serde(rename = "type")]
    pub r#type: HttpCreativeWorkspaceTasksItemTargetScopeVariant1Type,
    #[serde(rename = "endSceneOrder")]
    pub r#end_scene_order: f64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpCreativeWorkspaceTasksItemTargetScopeVariant2Type { #[serde(rename = "selectedScenes")] SelectedScenes }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpCreativeWorkspaceTasksItemTargetScopeVariant2BodyRangesItem {
    #[serde(rename = "groupId")]
    pub r#group_id: String,
    #[serde(rename = "start")]
    pub r#start: f64,
    #[serde(rename = "end")]
    pub r#end: f64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpCreativeWorkspaceTasksItemTargetScopeVariant2 {
    #[serde(rename = "type")]
    pub r#type: HttpCreativeWorkspaceTasksItemTargetScopeVariant2Type,
    #[serde(rename = "sourceRevisionId")]
    pub r#source_revision_id: String,
    #[serde(rename = "groupIds")]
    pub r#group_ids: Vec<String>,
    #[serde(rename = "bodyRanges")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#body_ranges: Option<Vec<HttpCreativeWorkspaceTasksItemTargetScopeVariant2BodyRangesItem>>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(untagged)]
pub enum HttpCreativeWorkspaceTasksItemTargetScope {
    CreativeWorkspaceTasksItemTargetScopeVariant0(HttpCreativeWorkspaceTasksItemTargetScopeVariant0),
    CreativeWorkspaceTasksItemTargetScopeVariant1(HttpCreativeWorkspaceTasksItemTargetScopeVariant1),
    CreativeWorkspaceTasksItemTargetScopeVariant2(HttpCreativeWorkspaceTasksItemTargetScopeVariant2),
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpCreativeWorkspaceTasksItemTargetReadScopeVariant0Type { #[serde(rename = "throughChapter")] ThroughChapter }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpCreativeWorkspaceTasksItemTargetReadScopeVariant0 {
    #[serde(rename = "type")]
    pub r#type: HttpCreativeWorkspaceTasksItemTargetReadScopeVariant0Type,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpCreativeWorkspaceTasksItemTargetReadScopeVariant1 {
    #[serde(rename = "type")]
    pub r#type: HttpCreativeWorkspaceTasksItemTargetScopeVariant1Type,
    #[serde(rename = "endSceneOrder")]
    pub r#end_scene_order: f64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(untagged)]
pub enum HttpCreativeWorkspaceTasksItemTargetReadScope {
    CreativeWorkspaceTasksItemTargetReadScopeVariant0(HttpCreativeWorkspaceTasksItemTargetReadScopeVariant0),
    CreativeWorkspaceTasksItemTargetReadScopeVariant1(HttpCreativeWorkspaceTasksItemTargetReadScopeVariant1),
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpCreativeWorkspaceTasksItemTarget {
    #[serde(rename = "type")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#type: Option<String>,
    #[serde(rename = "chapterId")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#chapter_id: Option<String>,
    #[serde(rename = "chapterNumber")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#chapter_number: Option<i64>,
    #[serde(rename = "chapterTitle")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#chapter_title: Option<String>,
    #[serde(rename = "sceneOrder")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#scene_order: Option<f64>,
    #[serde(rename = "scope")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#scope: Option<HttpCreativeWorkspaceTasksItemTargetScope>,
    #[serde(rename = "readScope")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#read_scope: Option<HttpCreativeWorkspaceTasksItemTargetReadScope>,
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
pub struct HttpCreativeWorkspaceTasksItemOutputRefsItem {
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
pub struct HttpCreativeWorkspaceTasksItem {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "taskKind")]
    pub r#task_kind: String,
    #[serde(rename = "purpose")]
    pub r#purpose: HttpCreativeWorkspaceTasksItemPurpose,
    #[serde(rename = "status")]
    pub r#status: String,
    #[serde(rename = "stage")]
    pub r#stage: String,
    #[serde(rename = "deliveryStatus")]
    pub r#delivery_status: HttpCreativeWorkspaceTasksItemDeliveryStatus,
    #[serde(rename = "threadId")]
    pub r#thread_id: String,
    #[serde(rename = "inputRevision")]
    pub r#input_revision: i64,
    #[serde(rename = "error")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#error: Option<String>,
    #[serde(rename = "target")]
    pub r#target: HttpCreativeWorkspaceTasksItemTarget,
    #[serde(rename = "outputRefs")]
    pub r#output_refs: Vec<HttpCreativeWorkspaceTasksItemOutputRefsItem>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpCreativeWorkspace {
    #[serde(rename = "project")]
    pub r#project: HttpCreativeWorkspaceProject,
    #[serde(rename = "revision")]
    pub r#revision: i64,
    #[serde(rename = "headSnapshotId")]
    pub r#head_snapshot_id: String,
    #[serde(rename = "sourceDigest")]
    pub r#source_digest: String,
    #[serde(rename = "planWindow")]
    pub r#plan_window: f64,
    #[serde(rename = "planLineageStatus")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#plan_lineage_status: Option<HttpCreativeWorkspacePlanLineageStatus>,
    #[serde(rename = "index")]
    pub r#index: HttpCreativeWorkspaceIndex,
    #[serde(rename = "artifacts")]
    pub r#artifacts: HttpCreativeWorkspaceArtifacts,
    #[serde(rename = "chapters")]
    pub r#chapters: Vec<HttpCreativeWorkspaceChaptersItem>,
    #[serde(rename = "workingBaselines")]
    pub r#working_baselines: Vec<HttpWorkingBaselineSelection>,
    #[serde(rename = "continuationIntents")]
    pub r#continuation_intents: Vec<HttpCreativeWorkspaceContinuationIntentsItem>,
    #[serde(rename = "planningReviews")]
    pub r#planning_reviews: Vec<HttpPlanningReviewRun>,
    #[serde(rename = "modeling")]
    pub r#modeling: Vec<HttpCreativeWorkspaceModelingItem>,
    #[serde(rename = "factProposals")]
    pub r#fact_proposals: Vec<HttpCreativeWorkspaceFactProposalsItem>,
    #[serde(rename = "reviews")]
    pub r#reviews: Vec<HttpCreativeReview>,
    #[serde(rename = "impacts")]
    pub r#impacts: Vec<HttpPlanningImpact>,
    #[serde(rename = "openImpactCount")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#open_impact_count: Option<i64>,
    #[serde(rename = "chapterPlanBindings")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#chapter_plan_bindings: Option<Vec<HttpCreativeWorkspaceChapterPlanBindingsItem>>,
    #[serde(rename = "commits")]
    pub r#commits: Vec<HttpCreativeWorkspaceCommitsItem>,
    #[serde(rename = "tasks")]
    pub r#tasks: Vec<HttpCreativeWorkspaceTasksItem>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererGetCreativeWorkspaceResponse(pub HttpCreativeWorkspace);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererSetWorkingBaselineResponse(pub HttpWorkingBaselineSelection);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpClearWorkingBaselineResponse {
    #[serde(rename = "objectKind")]
    pub r#object_kind: HttpWorkingBaselineSelectionObjectKind,
    #[serde(rename = "objectId")]
    pub r#object_id: String,
    #[serde(rename = "revisionId")]
    pub r#revision_id: (),
    #[serde(rename = "selectionRevision")]
    pub r#selection_revision: i64,
    #[serde(rename = "cleared")]
    pub r#cleared: WireTrue,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererClearWorkingBaselineResponse(pub HttpClearWorkingBaselineResponse);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpSaveContinuationIntentResponseStatus { #[serde(rename = "ready")] Ready }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpSaveContinuationIntentResponse {
    #[serde(rename = "targetKind")]
    pub r#target_kind: HttpWorkingBaselineSelectionObjectKind,
    #[serde(rename = "targetId")]
    pub r#target_id: String,
    #[serde(rename = "workingRevisionId")]
    pub r#working_revision_id: String,
    #[serde(rename = "authorIntent")]
    pub r#author_intent: String,
    #[serde(rename = "status")]
    pub r#status: HttpSaveContinuationIntentResponseStatus,
    #[serde(rename = "taskId")]
    pub r#task_id: (),
    #[serde(rename = "updatedAt")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#updated_at: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererSaveContinuationIntentResponse(pub HttpSaveContinuationIntentResponse);

/// The acceptance journal's stable identity receipt (project_api::creative_workspace).
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to="responses/")]
#[serde(rename_all="camelCase")]
pub struct HttpAcceptanceReceipt {
 pub commit_id: String,
 pub artifact_kind: String,
 pub artifact_revision_id: String,
 #[serde(deserialize_with="required_nullable")]
 pub previous_accepted_revision_id: Option<String>,
 pub base_revision: i64,
 pub result_revision: i64,
}
