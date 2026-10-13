use super::*;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpIdentityDecisionSameKind { #[serde(rename = "same")] Same }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpIdentityDecisionSame {
    #[serde(rename = "kind")]
    pub r#kind: HttpIdentityDecisionSameKind,
    #[serde(rename = "fromId")]
    pub r#from_id: String,
    #[serde(rename = "toId")]
    pub r#to_id: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpIdentityDecisionDistinctKind { #[serde(rename = "distinct")] Distinct }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpIdentityDecisionDistinct {
    #[serde(rename = "kind")]
    pub r#kind: HttpIdentityDecisionDistinctKind,
    #[serde(rename = "leftId")]
    pub r#left_id: String,
    #[serde(rename = "rightId")]
    pub r#right_id: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(untagged)]
pub enum HttpIdentityDecision {
    IdentityDecisionSame(HttpIdentityDecisionSame),
    IdentityDecisionDistinct(HttpIdentityDecisionDistinct),
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpCreativeWorkspaceArtifactsIdentity {
    #[serde(rename = "candidate")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#candidate: Option<HttpCreativeRevision2>,
    #[serde(rename = "accepted")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#accepted: Option<HttpCreativeRevision3>,
    #[serde(rename = "working")]
    pub r#working: (),
    #[serde(rename = "locked")]
    pub r#locked: bool,
    #[serde(rename = "workingSelection")]
    pub r#working_selection: (),
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpPropertyRuleDefinitionSubjectKind {
    #[serde(rename = "character")] Character,
    #[serde(rename = "organization")] Organization,
    #[serde(rename = "location")] Location,
    #[serde(rename = "item")] Item,
    #[serde(rename = "proposition")] Proposition,
    #[serde(rename = "event")] Event,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpPropertyRuleDefinitionSchemaCardinality {
    #[serde(rename = "single")] Single,
    #[serde(rename = "multiple")] Multiple,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpPropertyRuleDefinitionSchemaValueTypeVariant0Type {
    #[serde(rename = "boolean")] Boolean,
    #[serde(rename = "text")] Text,
    #[serde(rename = "integer")] Integer,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpPropertyRuleDefinitionSchemaValueTypeVariant0 {
    #[serde(rename = "type")]
    pub r#type: HttpPropertyRuleDefinitionSchemaValueTypeVariant0Type,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpPropertyRuleDefinitionSchemaValueTypeVariant1Type { #[serde(rename = "entity")] Entity }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpPropertyRuleDefinitionSchemaValueTypeVariant1 {
    #[serde(rename = "type")]
    pub r#type: HttpPropertyRuleDefinitionSchemaValueTypeVariant1Type,
    #[serde(rename = "kind")]
    pub r#kind: HttpPropertyRuleDefinitionSubjectKind,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(untagged)]
pub enum HttpPropertyRuleDefinitionSchemaValueType {
    PropertyRuleDefinitionSchemaValueTypeVariant0(HttpPropertyRuleDefinitionSchemaValueTypeVariant0),
    PropertyRuleDefinitionSchemaValueTypeVariant1(HttpPropertyRuleDefinitionSchemaValueTypeVariant1),
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpPropertyRuleDefinitionSchema {
    #[serde(rename = "cardinality")]
    pub r#cardinality: HttpPropertyRuleDefinitionSchemaCardinality,
    #[serde(rename = "valueType")]
    pub r#value_type: HttpPropertyRuleDefinitionSchemaValueType,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpPropertyRuleDefinition {
    #[serde(rename = "predicate")]
    pub r#predicate: String,
    #[serde(rename = "subjectKind")]
    pub r#subject_kind: HttpPropertyRuleDefinitionSubjectKind,
    #[serde(rename = "schema")]
    pub r#schema: HttpPropertyRuleDefinitionSchema,
    #[serde(rename = "description")]
    pub r#description: String,
    #[serde(rename = "exceptions")]
    pub r#exceptions: Vec<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpEventConditionVariant0Type {
    #[serde(rename = "all")] All,
    #[serde(rename = "any")] Any,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpEventConditionVariant0 {
    #[serde(rename = "type")]
    pub r#type: HttpEventConditionVariant0Type,
    #[serde(rename = "conditions")]
    pub r#conditions: Vec<HttpEventCondition>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpEventConditionVariant1Type { #[serde(rename = "not")] Not }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpEventConditionVariant1 {
    #[serde(rename = "type")]
    pub r#type: HttpEventConditionVariant1Type,
    #[serde(rename = "condition")]
    pub r#condition: Box<HttpEventCondition>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpEventConditionVariant2Type { #[serde(rename = "state")] State }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpEventConditionVariant2Role {
    #[serde(rename = "location")] Location,
    #[serde(rename = "item")] Item,
    #[serde(rename = "actor")] Actor,
    #[serde(rename = "recipient")] Recipient,
    #[serde(rename = "affected")] Affected,
    #[serde(rename = "instrument")] Instrument,
    #[serde(rename = "witness")] Witness,
    #[serde(rename = "beneficiary")] Beneficiary,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpEventConditionVariant2Operator {
    #[serde(rename = "eq")] Eq,
    #[serde(rename = "ne")] Ne,
    #[serde(rename = "lt")] Lt,
    #[serde(rename = "le")] Le,
    #[serde(rename = "gt")] Gt,
    #[serde(rename = "ge")] Ge,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpEventConditionVariant2OperandVariant0Type { #[serde(rename = "boolean")] Boolean }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpEventConditionVariant2OperandVariant0 {
    #[serde(rename = "type")]
    pub r#type: HttpEventConditionVariant2OperandVariant0Type,
    #[serde(rename = "value")]
    pub r#value: bool,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpEventConditionVariant2OperandVariant1Type { #[serde(rename = "integer")] Integer }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpEventConditionVariant2OperandVariant1 {
    #[serde(rename = "type")]
    pub r#type: HttpEventConditionVariant2OperandVariant1Type,
    #[serde(rename = "value")]
    pub r#value: f64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpEventConditionVariant2OperandVariant2 {
    #[serde(rename = "type")]
    pub r#type: HttpMediaSelectorTextKind,
    #[serde(rename = "value")]
    pub r#value: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpEventConditionVariant2OperandVariant3Type { #[serde(rename = "participant")] Participant }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpEventConditionVariant2OperandVariant3 {
    #[serde(rename = "type")]
    pub r#type: HttpEventConditionVariant2OperandVariant3Type,
    #[serde(rename = "role")]
    pub r#role: HttpEventConditionVariant2Role,
    #[serde(rename = "quantifier")]
    pub r#quantifier: HttpEventConditionVariant0Type,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(untagged)]
pub enum HttpEventConditionVariant2Operand {
    EventConditionVariant2OperandVariant0(HttpEventConditionVariant2OperandVariant0),
    EventConditionVariant2OperandVariant1(HttpEventConditionVariant2OperandVariant1),
    EventConditionVariant2OperandVariant2(HttpEventConditionVariant2OperandVariant2),
    EventConditionVariant2OperandVariant3(HttpEventConditionVariant2OperandVariant3),
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpEventConditionVariant2 {
    #[serde(rename = "type")]
    pub r#type: HttpEventConditionVariant2Type,
    #[serde(rename = "role")]
    pub r#role: HttpEventConditionVariant2Role,
    #[serde(rename = "subjectKind")]
    pub r#subject_kind: HttpPropertyRuleDefinitionSubjectKind,
    #[serde(rename = "predicate")]
    pub r#predicate: String,
    #[serde(rename = "quantifier")]
    pub r#quantifier: HttpEventConditionVariant0Type,
    #[serde(rename = "operator")]
    pub r#operator: HttpEventConditionVariant2Operator,
    #[serde(rename = "operand")]
    pub r#operand: HttpEventConditionVariant2Operand,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(untagged)]
pub enum HttpEventCondition {
    EventConditionVariant0(HttpEventConditionVariant0),
    EventConditionVariant1(HttpEventConditionVariant1),
    EventConditionVariant2(HttpEventConditionVariant2),
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpEventRuleDefinition {
    #[serde(rename = "eventType")]
    pub r#event_type: String,
    #[serde(rename = "description")]
    pub r#description: String,
    #[serde(rename = "exceptions")]
    pub r#exceptions: Vec<String>,
    #[serde(rename = "condition")]
    pub r#condition: Box<HttpEventCondition>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpPropertyRulesPayload {
    #[serde(rename = "definitions")]
    pub r#definitions: Vec<HttpPropertyRuleDefinition>,
    #[serde(rename = "eventRules")]
    pub r#event_rules: Vec<HttpEventRuleDefinition>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpCreativeWorkspaceArtifactsPropertyRules {
    #[serde(rename = "candidate")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#candidate: Option<HttpCreativeRevision4>,
    #[serde(rename = "accepted")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#accepted: Option<HttpCreativeRevision4>,
    #[serde(rename = "working")]
    pub r#working: (),
    #[serde(rename = "locked")]
    pub r#locked: bool,
    #[serde(rename = "workingSelection")]
    pub r#working_selection: (),
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpStatePropertySchemaAuthority { #[serde(rename = "modelDeclared")] ModelDeclared }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpStateProperty {
    #[serde(rename = "schema")]
    pub r#schema: HttpPropertyRuleDefinitionSchema,
    #[serde(rename = "validFrom")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#valid_from: Option<f64>,
    #[serde(rename = "validUntilExclusive")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#valid_until_exclusive: Option<f64>,
    #[serde(rename = "schemaAuthority")]
    pub r#schema_authority: HttpStatePropertySchemaAuthority,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpStoryEventInterpretationParticipantsItem {
    #[serde(rename = "role")]
    pub r#role: HttpEventConditionVariant2Role,
    #[serde(rename = "entityId")]
    pub r#entity_id: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpEventClaimReference {
    #[serde(rename = "sourceBatchId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#source_batch_id: Option<String>,
    #[serde(rename = "assertionIndex")]
    pub r#assertion_index: i64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpEventTemporalAssessmentBasis { #[serde(rename = "sourceBoundTemporalReferences")] SourceBoundTemporalReferences }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpEventReferenceTiming {
    #[serde(rename = "applicableBeforeEvent")] ApplicableBeforeEvent,
    #[serde(rename = "inactiveBeforeEvent")] InactiveBeforeEvent,
    #[serde(rename = "startsAtEvent")] StartsAtEvent,
    #[serde(rename = "beforeEvent")] BeforeEvent,
    #[serde(rename = "atEvent")] AtEvent,
    #[serde(rename = "afterEvent")] AfterEvent,
    #[serde(rename = "unknownTime")] UnknownTime,
    #[serde(rename = "unsupportedConditionKind")] UnsupportedConditionKind,
    #[serde(rename = "unsupportedEffectKind")] UnsupportedEffectKind,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpEventTemporalAssessmentPreconditionsItem {
    #[serde(rename = "referenceIndex")]
    pub r#reference_index: i64,
    #[serde(rename = "timing")]
    pub r#timing: HttpEventReferenceTiming,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpEventTemporalAssessmentEffectsItem {
    #[serde(rename = "referenceIndex")]
    pub r#reference_index: i64,
    #[serde(rename = "timing")]
    pub r#timing: HttpEventReferenceTiming,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpEventTemporalAssessment {
    #[serde(rename = "basis")]
    pub r#basis: HttpEventTemporalAssessmentBasis,
    #[serde(rename = "preconditions")]
    pub r#preconditions: Vec<HttpEventTemporalAssessmentPreconditionsItem>,
    #[serde(rename = "effects")]
    pub r#effects: Vec<HttpEventTemporalAssessmentEffectsItem>,
    #[serde(rename = "semanticTruthInferred")]
    pub r#semantic_truth_inferred: WireFalse,
    #[serde(rename = "globalConditionsVerified")]
    pub r#global_conditions_verified: WireFalse,
    #[serde(rename = "stateChangesExecuted")]
    pub r#state_changes_executed: WireFalse,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpEventConditionAssessmentEvaluationJob {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "status")]
    pub r#status: novel_codex_story_core::ConditionEvaluationStatus,
    #[serde(rename = "version")]
    pub r#version: i64,
    #[serde(rename = "scannedSources")]
    pub r#scanned_sources: f64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpEventConditionAssessmentBasis { #[serde(rename = "authorRulesSourceClaims")] AuthorRulesSourceClaims }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpEventConditionAssessmentStatus {
    #[serde(rename = "noApplicableRule")] NoApplicableRule,
    #[serde(rename = "interpreted")] Interpreted,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpEventConditionAssessmentResult {
    #[serde(rename = "unknown")] Unknown,
    #[serde(rename = "supported")] Supported,
    #[serde(rename = "refuted")] Refuted,
    #[serde(rename = "both")] Both,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpEventConditionAssessmentCoverageSources {
    #[serde(rename = "pendingSources")]
    pub r#pending_sources: f64,
    #[serde(rename = "totalSources")]
    pub r#total_sources: i64,
    #[serde(rename = "scannedSources")]
    pub r#scanned_sources: f64,
    #[serde(rename = "omittedSources")]
    pub r#omitted_sources: f64,
    #[serde(rename = "sourceBytes")]
    pub r#source_bytes: i64,
    #[serde(rename = "relevantSources")]
    pub r#relevant_sources: f64,
    #[serde(rename = "sourceProofId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#source_proof_id: Option<String>,
    #[serde(rename = "entityKindsComplete")]
    pub r#entity_kinds_complete: bool,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpEventConditionAssessmentCoverage {
    #[serde(rename = "complete")]
    pub r#complete: bool,
    #[serde(rename = "sources")]
    pub r#sources: HttpEventConditionAssessmentCoverageSources,
    #[serde(rename = "unresolvedChecks")]
    pub r#unresolved_checks: f64,
    #[serde(rename = "evidenceTruncated")]
    pub r#evidence_truncated: bool,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpEventConditionAssessment {
    #[serde(rename = "explanationJobId")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#explanation_job_id: Option<String>,
    #[serde(rename = "sourceClaimsTruncated")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#source_claims_truncated: Option<bool>,
    #[serde(rename = "evaluationJob")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#evaluation_job: Option<HttpEventConditionAssessmentEvaluationJob>,
    #[serde(rename = "basis")]
    pub r#basis: HttpEventConditionAssessmentBasis,
    #[serde(rename = "status")]
    pub r#status: HttpEventConditionAssessmentStatus,
    #[serde(rename = "result")]
    pub r#result: HttpEventConditionAssessmentResult,
    #[serde(rename = "observedResult")]
    pub r#observed_result: HttpEventConditionAssessmentResult,
    #[serde(rename = "ruleRevisionId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#rule_revision_id: Option<String>,
    #[serde(rename = "identityRevisionId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#identity_revision_id: Option<String>,
    #[serde(rename = "sourceClaims")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#source_claims: Option<Vec<HttpEventClaimReference>>,
    #[serde(rename = "coverage")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#coverage: Option<HttpEventConditionAssessmentCoverage>,
    #[serde(rename = "stateChangesExecuted")]
    pub r#state_changes_executed: WireFalse,
    #[serde(rename = "worldTruthCertified")]
    pub r#world_truth_certified: WireFalse,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpStoryEventInterpretation {
    #[serde(rename = "evaluationTarget")]
    pub r#evaluation_target: novel_codex_story_core::ConditionEvaluationTarget,
    #[serde(rename = "eventId")]
    pub r#event_id: String,
    #[serde(rename = "eventType")]
    pub r#event_type: String,
    #[serde(rename = "storyTime")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#story_time: Option<f64>,
    #[serde(rename = "untilExclusive")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#until_exclusive: Option<f64>,
    #[serde(rename = "participants")]
    pub r#participants: Vec<HttpStoryEventInterpretationParticipantsItem>,
    #[serde(rename = "preconditions")]
    pub r#preconditions: Vec<HttpEventClaimReference>,
    #[serde(rename = "effects")]
    pub r#effects: Vec<HttpEventClaimReference>,
    #[serde(rename = "temporalAssessment")]
    pub r#temporal_assessment: HttpEventTemporalAssessment,
    #[serde(rename = "conditionAssessment")]
    pub r#condition_assessment: HttpEventConditionAssessment,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpHistoricalKnowledgeStateEntriesItemConfirmationState {
    #[serde(rename = "candidate")] Candidate,
    #[serde(rename = "confirmed")] Confirmed,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpHistoricalKnowledgeStateEntriesItemAssertionEvidence {
    #[serde(rename = "quote")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#quote: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpHistoricalKnowledgeStateEntriesItemAssertion {
    #[serde(rename = "kind")]
    pub r#kind: String,
    #[serde(rename = "predicate")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#predicate: Option<String>,
    #[serde(rename = "value")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#value: Option<serde_json::Value>,
    #[serde(rename = "storyTime")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub r#story_time: Option<f64>,
    #[serde(rename = "evidence")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#evidence: Option<HttpHistoricalKnowledgeStateEntriesItemAssertionEvidence>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpHistoricalKnowledgeStateEntriesItem {
    #[serde(rename = "canonicalSubjectId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#canonical_subject_id: Option<String>,
    #[serde(rename = "sourceObjectId")]
    pub r#source_object_id: String,
    #[serde(rename = "sourceRevisionId")]
    pub r#source_revision_id: String,
    #[serde(rename = "modelingBatchId")]
    pub r#modeling_batch_id: String,
    #[serde(rename = "assertionIndex")]
    pub r#assertion_index: i64,
    #[serde(rename = "confirmationState")]
    pub r#confirmation_state: HttpHistoricalKnowledgeStateEntriesItemConfirmationState,
    #[serde(rename = "assertion")]
    pub r#assertion: HttpHistoricalKnowledgeStateEntriesItemAssertion,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpHistoricalKnowledgeStateCoverage {
    #[serde(rename = "complete")]
    pub r#complete: bool,
    #[serde(rename = "truncatedEntries")]
    pub r#truncated_entries: bool,
    #[serde(rename = "pendingSources")]
    pub r#pending_sources: Vec<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpHistoricalKnowledgeState {
    #[serde(rename = "snapshotId")]
    pub r#snapshot_id: String,
    #[serde(rename = "entries")]
    pub r#entries: Vec<HttpHistoricalKnowledgeStateEntriesItem>,
    #[serde(rename = "coverage")]
    pub r#coverage: HttpHistoricalKnowledgeStateCoverage,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererGetHistoricalKnowledgeStateResponse(pub HttpHistoricalKnowledgeState);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpIdentityCatalogEntriesItemResolutionState {
    #[serde(rename = "candidate")] Candidate,
    #[serde(rename = "authorMerged")] AuthorMerged,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpIdentityCatalogEntriesItemClaimsItemEvidence {
    #[serde(rename = "quote")]
    pub r#quote: String,
    #[serde(rename = "start")]
    pub r#start: f64,
    #[serde(rename = "end")]
    pub r#end: f64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpIdentityCatalogEntriesItemClaimsItem {
    #[serde(rename = "displayName")]
    pub r#display_name: String,
    #[serde(rename = "aliases")]
    pub r#aliases: Vec<String>,
    #[serde(rename = "sourceObjectId")]
    pub r#source_object_id: String,
    #[serde(rename = "sourceRevisionId")]
    pub r#source_revision_id: String,
    #[serde(rename = "evidence")]
    pub r#evidence: HttpIdentityCatalogEntriesItemClaimsItemEvidence,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpIdentityCatalogEntriesItem {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "canonicalId")]
    pub r#canonical_id: String,
    #[serde(rename = "resolutionState")]
    pub r#resolution_state: HttpIdentityCatalogEntriesItemResolutionState,
    #[serde(rename = "kind")]
    pub r#kind: String,
    #[serde(rename = "claims")]
    pub r#claims: Vec<HttpIdentityCatalogEntriesItemClaimsItem>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpIdentityCatalogAmbiguousNamesItem {
    #[serde(rename = "name")]
    pub r#name: String,
    #[serde(rename = "candidateIds")]
    pub r#candidate_ids: Vec<String>,
    #[serde(rename = "canonicalIds")]
    pub r#canonical_ids: Vec<String>,
    #[serde(rename = "authorDecidedDistinct")]
    pub r#author_decided_distinct: bool,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpIdentityCatalog {
    #[serde(rename = "entries")]
    pub r#entries: Vec<HttpIdentityCatalogEntriesItem>,
    #[serde(rename = "ambiguousNames")]
    pub r#ambiguous_names: Vec<HttpIdentityCatalogAmbiguousNamesItem>,
    #[serde(rename = "identityRevisionId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#identity_revision_id: Option<String>,
    #[serde(rename = "identityDecisions")]
    pub r#identity_decisions: Vec<HttpIdentityDecision>,
    #[serde(rename = "truncated")]
    pub r#truncated: bool,
    #[serde(rename = "ambiguityScanComplete")]
    pub r#ambiguity_scan_complete: bool,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererGetIdentityCatalogResponse(pub HttpIdentityCatalog);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpCharacterKnowledgePerspective { #[serde(rename = "author")] Author, #[serde(rename="world")] World, #[serde(rename="character")] Character, #[serde(rename="reader")] Reader }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpCharacterKnowledgeSelectors {
    #[serde(rename = "entityId", deserialize_with = "required_nullable")]
    pub r#entity_id: Option<String>,
    #[serde(rename = "predicate", deserialize_with = "required_nullable")]
    pub r#predicate: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpCharacterKnowledgeEntriesItemPropositionRef {
    #[serde(rename = "batchId")]
    pub r#batch_id: String,
    #[serde(rename = "propositionId")]
    pub r#proposition_id: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpStoryEvidence {
    #[serde(rename = "quote")]
    pub r#quote: String,
    #[serde(rename = "start")]
    pub r#start: f64,
    #[serde(rename = "end")]
    pub r#end: f64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpStoryAssertionAssertionKind { #[serde(rename = "assertion")] Assertion }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpStoryAssertionAssertion {
    #[serde(rename = "subjectId")]
    pub r#subject_id: String,
    #[serde(rename = "predicate")]
    pub r#predicate: String,
    #[serde(rename = "evidence")]
    pub r#evidence: HttpStoryEvidence,
    #[serde(rename = "storyTime")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub r#story_time: Option<f64>,
    #[serde(rename = "kind")]
    pub r#kind: HttpStoryAssertionAssertionKind,
    #[serde(rename = "value")]
    pub r#value: serde_json::Value,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpStoryAssertionVariant1ValueVariant4 {
    #[serde(rename = "entityId")]
    pub r#entity_id: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(untagged)]
pub enum HttpStoryAssertionVariant1Value {
    Text(String),
    Number(f64),
    WireFalse(WireFalse),
    WireTrue(WireTrue),
    StoryAssertionVariant1ValueVariant4(HttpStoryAssertionVariant1ValueVariant4),
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpStoryAssertionVariant1 {
    #[serde(rename = "subjectId")]
    pub r#subject_id: String,
    #[serde(rename = "predicate")]
    pub r#predicate: String,
    #[serde(rename = "evidence")]
    pub r#evidence: HttpStoryEvidence,
    #[serde(rename = "storyTime")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub r#story_time: Option<f64>,
    #[serde(rename = "kind")]
    pub r#kind: HttpEventConditionVariant2Type,
    #[serde(rename = "value")]
    pub r#value: HttpStoryAssertionVariant1Value,
    #[serde(rename = "stateSchema")]
    pub r#state_schema: HttpPropertyRuleDefinitionSchema,
    #[serde(rename = "validFrom")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#valid_from: Option<f64>,
    #[serde(rename = "validUntilExclusive")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#valid_until_exclusive: Option<f64>,
    #[serde(rename = "validUntilEvidence")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#valid_until_evidence: Option<HttpStoryEvidence>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpStoryAssertionRelationKind { #[serde(rename = "relation")] Relation }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpStoryAssertionRelation {
    #[serde(rename = "subjectId")]
    pub r#subject_id: String,
    #[serde(rename = "predicate")]
    pub r#predicate: String,
    #[serde(rename = "evidence")]
    pub r#evidence: HttpStoryEvidence,
    #[serde(rename = "storyTime")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub r#story_time: Option<f64>,
    #[serde(rename = "kind")]
    pub r#kind: HttpStoryAssertionRelationKind,
    #[serde(rename = "value")]
    pub r#value: serde_json::Value,
    #[serde(rename = "targetId")]
    pub r#target_id: String,
    #[serde(rename = "relationType")]
    pub r#relation_type: String,
    #[serde(rename = "propositionId")]
    pub r#proposition_id: String,
    #[serde(rename = "validFrom")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#valid_from: Option<f64>,
    #[serde(rename = "validUntilExclusive")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#valid_until_exclusive: Option<f64>,
    #[serde(rename = "validUntilEvidence")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#valid_until_evidence: Option<HttpStoryEvidence>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpStoryAssertionEpistemicKind { #[serde(rename = "epistemic")] Epistemic }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpStoryAssertionVariant3EpistemicState {
    #[serde(rename = "suspected")] Suspected,
    #[serde(rename = "known")] Known,
    #[serde(rename = "false")] False,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpStoryAssertionEpistemic {
    #[serde(rename = "subjectId")]
    pub r#subject_id: String,
    #[serde(rename = "predicate")]
    pub r#predicate: String,
    #[serde(rename = "evidence")]
    pub r#evidence: HttpStoryEvidence,
    #[serde(rename = "storyTime")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub r#story_time: Option<f64>,
    #[serde(rename = "kind")]
    pub r#kind: HttpStoryAssertionEpistemicKind,
    #[serde(rename = "value")]
    pub r#value: serde_json::Value,
    #[serde(rename = "epistemicOwnerId")]
    pub r#epistemic_owner_id: String,
    #[serde(rename = "epistemicState")]
    pub r#epistemic_state: HttpStoryAssertionVariant3EpistemicState,
    #[serde(rename = "epistemicTime")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#epistemic_time: Option<f64>,
    #[serde(rename = "epistemicUntilExclusive")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#epistemic_until_exclusive: Option<f64>,
    #[serde(rename = "epistemicUntilEvidence")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#epistemic_until_evidence: Option<HttpStoryEvidence>,
    #[serde(rename = "propositionId")]
    pub r#proposition_id: String,
    #[serde(rename = "propositionSourceBatchId")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#proposition_source_batch_id: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpStoryAssertionEventKind { #[serde(rename = "event")] Event }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpStoryAssertionVariant4Value {
    #[serde(rename = "participants")]
    pub r#participants: Vec<HttpStoryEventInterpretationParticipantsItem>,
    #[serde(rename = "preconditions")]
    pub r#preconditions: Vec<HttpEventClaimReference>,
    #[serde(rename = "effects")]
    pub r#effects: Vec<HttpEventClaimReference>,
    #[serde(rename = "untilExclusive")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#until_exclusive: Option<f64>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpStoryAssertionEvent {
    #[serde(rename = "subjectId")]
    pub r#subject_id: String,
    #[serde(rename = "predicate")]
    pub r#predicate: String,
    #[serde(rename = "evidence")]
    pub r#evidence: HttpStoryEvidence,
    #[serde(rename = "storyTime")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub r#story_time: Option<f64>,
    #[serde(rename = "kind")]
    pub r#kind: HttpStoryAssertionEventKind,
    #[serde(rename = "value")]
    pub r#value: HttpStoryAssertionVariant4Value,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(untagged)]
pub enum HttpStoryAssertion {
    StoryAssertionAssertion(HttpStoryAssertionAssertion),
    StoryAssertionVariant1(HttpStoryAssertionVariant1),
    StoryAssertionRelation(HttpStoryAssertionRelation),
    StoryAssertionEpistemic(HttpStoryAssertionEpistemic),
    StoryAssertionEvent(HttpStoryAssertionEvent),
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpCharacterKnowledgeEntriesItemRelationDirection { #[serde(rename = "sourceToTarget")] SourceToTarget }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpCharacterKnowledgeEntriesItemRelation {
    #[serde(rename = "type")]
    pub r#type: String,
    #[serde(rename = "category")]
    pub r#category: String,
    #[serde(rename = "inverseLabel")]
    pub r#inverse_label: String,
    #[serde(rename = "transitive")]
    pub r#transitive: bool,
    #[serde(rename = "direction")]
    pub r#direction: HttpCharacterKnowledgeEntriesItemRelationDirection,
    #[serde(rename = "validFrom")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#valid_from: Option<f64>,
    #[serde(rename = "validUntilExclusive")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#valid_until_exclusive: Option<f64>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpCharacterKnowledgeEntriesItemCanonicalStateValueVariant4 {
    #[serde(rename = "entityId")]
    pub r#entity_id: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(untagged)]
pub enum HttpCharacterKnowledgeEntriesItemCanonicalStateValue {
    Text(String),
    Number(f64),
    WireFalse(WireFalse),
    WireTrue(WireTrue),
    CharacterKnowledgeEntriesItemCanonicalStateValueVariant4(HttpCharacterKnowledgeEntriesItemCanonicalStateValueVariant4),
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpCharacterKnowledgeEntriesItemStateSchemaAuthority {
    #[serde(rename = "modelDeclared")] ModelDeclared,
    #[serde(rename = "authorConfirmed")] AuthorConfirmed,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpCharacterKnowledgeEntriesItem {
    #[serde(rename = "sourceObjectId")]
    pub r#source_object_id: String,
    #[serde(rename = "sourceRevisionId")]
    pub r#source_revision_id: String,
    #[serde(rename = "modelingBatchId")]
    pub r#modeling_batch_id: String,
    #[serde(rename = "assertionIndex")]
    pub r#assertion_index: i64,
    #[serde(rename = "sourceChapter")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#source_chapter: Option<f64>,
    #[serde(rename = "canonicalSubjectId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#canonical_subject_id: Option<String>,
    #[serde(rename = "canonicalTargetId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#canonical_target_id: Option<String>,
    #[serde(rename = "canonicalEpistemicOwnerId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#canonical_epistemic_owner_id: Option<String>,
    #[serde(rename = "propositionRef")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#proposition_ref: Option<HttpCharacterKnowledgeEntriesItemPropositionRef>,
    #[serde(rename = "confirmationState")]
    pub r#confirmation_state: HttpHistoricalKnowledgeStateEntriesItemConfirmationState,
    #[serde(rename = "confirmedFactId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#confirmed_fact_id: Option<String>,
    #[serde(rename = "assertion")]
    pub r#assertion: HttpStoryAssertion,
    #[serde(rename = "relation")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#relation: Option<HttpCharacterKnowledgeEntriesItemRelation>,
    #[serde(rename = "canonicalStateValue")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#canonical_state_value: Option<HttpCharacterKnowledgeEntriesItemCanonicalStateValue>,
    #[serde(rename = "stateSchemaAuthority")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#state_schema_authority: Option<HttpCharacterKnowledgeEntriesItemStateSchemaAuthority>,
    #[serde(rename = "effectiveStateSchema")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#effective_state_schema: Option<HttpPropertyRuleDefinitionSchema>,
    #[serde(rename = "eventTemporalAssessment")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#event_temporal_assessment: Option<HttpEventTemporalAssessment>,
    #[serde(rename = "eventConditionAssessment")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#event_condition_assessment: Option<HttpEventConditionAssessment>,
    #[serde(rename = "eventEvolutionAssessment")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#event_evolution_assessment: Option<HttpEventEvolutionAssessment>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpCharacterKnowledgeCoverageSemanticCompleteness { #[serde(rename = "unknown")] Unknown }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpCharacterKnowledgeCoverage {
    #[serde(rename = "complete")]
    pub r#complete: bool,
    #[serde(rename = "pendingSources")]
    pub r#pending_sources: Vec<String>,
    #[serde(rename = "truncatedSources")]
    pub r#truncated_sources: bool,
    #[serde(rename = "pendingSourcesCount")]
    pub r#pending_sources_count: i64,
    #[serde(rename = "pendingSourceIdsTruncated")]
    pub r#pending_source_ids_truncated: bool,
    #[serde(rename = "matchingSourcesCount")]
    pub r#matching_sources_count: i64,
    #[serde(rename = "scannedSourcesCount")]
    pub r#scanned_sources_count: i64,
    #[serde(rename = "semanticCompleteness")]
    pub r#semantic_completeness: HttpCharacterKnowledgeCoverageSemanticCompleteness,
    #[serde(rename = "unknownChapterSources")]
    pub r#unknown_chapter_sources: f64,
    #[serde(rename = "truncatedEntries")]
    pub r#truncated_entries: bool,
    #[serde(rename = "unknownTimeSkipped")]
    pub r#unknown_time_skipped: f64,
    #[serde(rename = "unavailablePropositionSources")]
    pub r#unavailable_proposition_sources: i64,
    #[serde(rename = "findingsTruncated")]
    pub r#findings_truncated: bool,
    #[serde(rename = "uncertainConflictPairs")]
    pub r#uncertain_conflict_pairs: f64,
    #[serde(rename = "unknownStateTimes")]
    pub r#unknown_state_times: f64,
    #[serde(rename = "unappliedPropertyRules")]
    pub r#unapplied_property_rules: f64,
    #[serde(rename = "confirmedEntries")]
    pub r#confirmed_entries: f64,
    #[serde(rename = "candidateEntries")]
    pub r#candidate_entries: f64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpCharacterKnowledge {
    #[serde(rename = "projectId")]
    pub r#project_id: String,
    #[serde(rename = "snapshotId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#snapshot_id: Option<String>,
    #[serde(rename = "perspective")]
    pub r#perspective: HttpCharacterKnowledgePerspective,
    #[serde(rename = "identityRevisionId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#identity_revision_id: Option<String>,
    #[serde(rename = "propertyRulesRevisionId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#property_rules_revision_id: Option<String>,
    #[serde(rename = "propertyRulesPolicy")]
    pub r#property_rules_policy: String,
    #[serde(rename = "worldTime")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#world_time: Option<i64>,
    #[serde(rename = "epistemicTime")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#epistemic_time: Option<i64>,
    #[serde(rename = "readingChapter")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#reading_chapter: Option<i64>,
    #[serde(rename = "ownerId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#owner_id: Option<String>,
    #[serde(rename = "visibleThroughChapter")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#visible_through_chapter: Option<i64>,
    #[serde(rename = "selectors")]
    pub r#selectors: HttpCharacterKnowledgeSelectors,
    #[serde(rename = "entries")]
    pub r#entries: Vec<HttpCharacterKnowledgeEntriesItem>,
    #[serde(rename = "findings")]
    pub r#findings: Vec<HttpKnowledgeFinding>,
    #[serde(rename = "eventRuleCount")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#event_rule_count: Option<i64>,
    #[serde(rename = "eventConditionsPolicy")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#event_conditions_policy: Option<String>,
    #[serde(rename = "coverage")]
    pub r#coverage: HttpCharacterKnowledgeCoverage,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererGetCharacterKnowledgeResponse(pub HttpCharacterKnowledge);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpSaveIdentityDecisionsResponseArtifactKind { #[serde(rename = "identity")] Identity }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpSaveIdentityDecisionsResponsePayload {
    #[serde(rename = "decisions")]
    pub r#decisions: Vec<HttpIdentityDecision>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpSaveIdentityDecisionsResponse {
    #[serde(rename = "artifactKind")]
    pub r#artifact_kind: HttpSaveIdentityDecisionsResponseArtifactKind,
    #[serde(rename = "revisionId")]
    pub r#revision_id: String,
    #[serde(rename = "version")]
    pub r#version: i64,
    #[serde(rename = "payload")]
    pub r#payload: HttpSaveIdentityDecisionsResponsePayload,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererSaveIdentityDecisionsResponse(pub HttpSaveIdentityDecisionsResponse);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpSavePropertyRulesResponseArtifactKind { #[serde(rename = "propertyRules")] PropertyRules }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpSavePropertyRulesResponse {
    #[serde(rename = "artifactKind")]
    pub r#artifact_kind: HttpSavePropertyRulesResponseArtifactKind,
    #[serde(rename = "revisionId")]
    pub r#revision_id: String,
    #[serde(rename = "version")]
    pub r#version: i64,
    #[serde(rename = "payload")]
    pub r#payload: HttpPropertyRulesPayload,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererSavePropertyRulesResponse(pub HttpSavePropertyRulesResponse);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpConditionEvaluationJob {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "projectId")]
    pub r#project_id: String,
    #[serde(rename = "target")]
    pub r#target: novel_codex_story_core::ConditionEvaluationTarget,
    #[serde(rename = "bindingDigest")]
    pub r#binding_digest: String,
    #[serde(rename = "status")]
    pub r#status: novel_codex_story_core::ConditionEvaluationStatus,
    #[serde(rename = "version")]
    pub r#version: i64,
    #[serde(rename = "scannedSources")]
    pub r#scanned_sources: f64,
    #[serde(rename = "sourceBytes")]
    pub r#source_bytes: i64,
    #[serde(rename = "result")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#result: Option<HttpEventConditionAssessment>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererStartConditionEvaluationResponse(pub HttpConditionEvaluationJob);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererGetConditionEvaluationResponse(pub HttpConditionEvaluationJob);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererControlConditionEvaluationResponse(pub HttpConditionEvaluationJob);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpConditionExplanationPageDataItemAssessment {
    #[serde(rename = "result")]
    pub r#result: HttpEventConditionAssessmentResult,
    #[serde(rename = "support")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#support: Option<HttpEventClaimReference>,
    #[serde(rename = "refute")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#refute: Option<HttpEventClaimReference>,
    #[serde(rename = "unresolvedChecks")]
    pub r#unresolved_checks: f64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpConditionSourceExcerpt {
    #[serde(rename = "sourceBatchId")]
    pub r#source_batch_id: String,
    #[serde(rename = "assertionIndex")]
    pub r#assertion_index: i64,
    #[serde(rename = "sourceObjectId")]
    pub r#source_object_id: String,
    #[serde(rename = "sourceRevisionId")]
    pub r#source_revision_id: String,
    #[serde(rename = "quote")]
    pub r#quote: String,
    #[serde(rename = "start")]
    pub r#start: f64,
    #[serde(rename = "end")]
    pub r#end: f64,
    #[serde(rename = "truncated")]
    pub r#truncated: bool,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpConditionExplanationPageDataItemExcerpts {
    #[serde(rename = "support")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#support: Option<HttpConditionSourceExcerpt>,
    #[serde(rename = "refute")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#refute: Option<HttpConditionSourceExcerpt>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpConditionExplanationPageDataItem {
    #[serde(rename = "leafIndex")]
    pub r#leaf_index: i64,
    #[serde(rename = "subjectId")]
    pub r#subject_id: String,
    #[serde(rename = "operand")]
    pub r#operand: serde_json::Value,
    #[serde(rename = "assessment")]
    pub r#assessment: HttpConditionExplanationPageDataItemAssessment,
    #[serde(rename = "excerpts")]
    pub r#excerpts: HttpConditionExplanationPageDataItemExcerpts,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpConditionExplanationPage {
    #[serde(rename = "data")]
    pub r#data: Vec<HttpConditionExplanationPageDataItem>,
    #[serde(rename = "nextCursor")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#next_cursor: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererGetConditionExplanationsResponse(pub HttpConditionExplanationPage);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpRejectIdentityReviewResponseStatus { #[serde(rename = "rejected")] Rejected }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpRejectIdentityReviewResponse {
    #[serde(rename = "reviewId")]
    pub r#review_id: String,
    #[serde(rename = "status")]
    pub r#status: HttpRejectIdentityReviewResponseStatus,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererRejectIdentityReviewResponse(pub HttpRejectIdentityReviewResponse);

/// Candidate identities and provenance are structural; assertion values remain author data.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to="responses/")]
#[serde(rename_all="camelCase")]
pub struct HttpModeledEntity {
 pub id: String,
 pub kind: HttpModeledEntityKind,
 pub display_name: String,
 pub aliases: Vec<String>,
 pub evidence: HttpStoryEvidence,
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to="responses/")]
#[serde(rename_all="camelCase")]
pub enum HttpModeledEntityKind {Character, Organization, Location, Item, Proposition, Event}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to="responses/")]
#[serde(rename_all="camelCase")]
pub struct HttpEventEvolutionAssessment {
 pub basis: HttpEvolutionBasis,
 pub effects: Vec<HttpEventEvolutionEffect>,
 pub state_changes_executed: WireFalse,
 pub causality_certified: WireFalse,
 pub world_truth_certified: WireFalse,
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to="responses/")]
#[serde(rename_all="camelCase")]
pub enum HttpEvolutionBasis {DeclaredSourceEffectLinks}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to="responses/")]
#[serde(rename_all="camelCase")]
pub struct HttpEventEvolutionEffect {
 pub reference_index: usize,
 pub interpretation: HttpEvolutionInterpretation,
 pub condition_compatibility: HttpConditionCompatibility,
 pub confirmation_state: HttpConfirmationState,
 #[serde(deserialize_with="required_nullable")]
 pub confirmed_fact_id: Option<String>,
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to="responses/")]
#[serde(rename_all="camelCase")]
pub enum HttpEvolutionInterpretation {PreexistingClaim,CoincidentClaim,LaterClaim,UnknownTime,UnsupportedKind}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to="responses/")]
#[serde(rename_all="camelCase")]
pub enum HttpConditionCompatibility {Supported,Refuted,Conflicted,Unknown,Unruled}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to="responses/")]
#[serde(rename_all="camelCase")]
pub enum HttpConfirmationState {Confirmed,Candidate}
/// Both conflict variants share source-bound evidence and entry identities.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to="responses/")]
#[serde(rename_all="camelCase")]
pub struct HttpKnowledgeFinding {
 #[serde(rename="type")]
 pub kind: HttpKnowledgeFindingKind,
 pub subject_id: String,
 pub predicate: String,
 pub left_entry_index: usize,
 pub right_entry_index: usize,
 pub left_source_revision_id: String,
 pub right_source_revision_id: String,
 pub left_evidence: HttpStoryEvidence,
 pub right_evidence: HttpStoryEvidence,
 pub confirmation_state: String,
 #[serde(default,skip_serializing_if="Option::is_none")]
 #[ts(optional = nullable)]
 pub target_id: Option<String>,
 #[serde(default,skip_serializing_if="Option::is_none")]
 #[ts(optional)]
 pub owner_id: Option<String>,
 #[serde(default,skip_serializing_if="Option::is_none")]
 #[ts(optional)]
 pub proposition_ref: Option<HttpCharacterKnowledgeEntriesItemPropositionRef>,
 #[serde(default,skip_serializing_if="Option::is_none")]
 #[ts(optional)]
 pub left_claim_subject_id: Option<String>,
 #[serde(default,skip_serializing_if="Option::is_none")]
 #[ts(optional)]
 pub right_claim_subject_id: Option<String>,
 #[serde(default,skip_serializing_if="Option::is_none")]
 #[ts(optional)]
 pub left_state: Option<String>,
 #[serde(default,skip_serializing_if="Option::is_none")]
 #[ts(optional)]
 pub right_state: Option<String>,
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to="responses/")]
#[serde(rename_all="camelCase")]
pub enum HttpKnowledgeFindingKind {PolarityConflict,EpistemicDisagreement}
