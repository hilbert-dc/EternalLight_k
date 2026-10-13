use super::*;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpCreateHypothesisResponse {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "baseSnapshotId")]
    pub r#base_snapshot_id: String,
    #[serde(rename = "replayed")]
    pub r#replayed: bool,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererCreateHypothesisResponse(pub HttpCreateHypothesisResponse);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpHypothesisAssumptionSuppressClaimKind { #[serde(rename = "suppressClaim")] SuppressClaim }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpHypothesisAssumptionSuppressClaim {
    #[serde(rename = "kind")]
    pub r#kind: HttpHypothesisAssumptionSuppressClaimKind,
    #[serde(rename = "modelingBatchId")]
    pub r#modeling_batch_id: String,
    #[serde(rename = "sourceRevisionId")]
    pub r#source_revision_id: String,
    #[serde(rename = "assertionIndex")]
    pub r#assertion_index: i64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpHypothesisAssumptionAssumeClaimKind { #[serde(rename = "assumeClaim")] AssumeClaim }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(untagged)]
pub enum HttpHypothesisAssumptionVariant1ValueValue {
    Text(String),
    Number(f64),
    WireFalse(WireFalse),
    WireTrue(WireTrue),
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpHypothesisAssumptionAssumeClaim {
    #[serde(rename = "kind")]
    pub r#kind: HttpHypothesisAssumptionAssumeClaimKind,
    #[serde(rename = "subjectId")]
    pub r#subject_id: String,
    #[serde(rename = "predicate")]
    pub r#predicate: String,
    #[serde(rename = "value")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#value: Option<HttpHypothesisAssumptionVariant1ValueValue>,
    #[serde(rename = "storyTime")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#story_time: Option<f64>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(untagged)]
pub enum HttpHypothesisAssumption {
    HypothesisAssumptionSuppressClaim(HttpHypothesisAssumptionSuppressClaim),
    HypothesisAssumptionAssumeClaim(HttpHypothesisAssumptionAssumeClaim),
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpHypothesisResultAuthority { #[serde(rename = "hypothesisOnly")] HypothesisOnly }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpHypothesisResultBaseline {
    #[serde(rename = "entryCount")]
    pub r#entry_count: i64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpHypothesisResultSimulationEntriesItemAssertion {
    #[serde(rename = "predicate")]
    pub r#predicate: String,
    #[serde(rename = "value")]
    pub r#value: serde_json::Value,
    #[serde(rename = "storyTime")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#story_time: Option<f64>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpHypothesisResultSimulationEntriesItem {
    #[serde(rename = "canonicalSubjectId")]
    pub r#canonical_subject_id: String,
    #[serde(rename = "confirmationState")]
    pub r#confirmation_state: String,
    #[serde(rename = "assertion")]
    pub r#assertion: HttpHypothesisResultSimulationEntriesItemAssertion,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpHypothesisResultSimulationFindingsItemVariant0LeftEvidence {
    #[serde(rename = "quote")]
    pub r#quote: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpHypothesisResultSimulationFindingsItemVariant0RightEvidence {
    #[serde(rename = "quote")]
    pub r#quote: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpHypothesisResultSimulationFindingsItemVariant0Type { #[serde(rename = "polarityConflict")] PolarityConflict }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpHypothesisResultSimulationFindingsItemVariant0 {
    #[serde(rename = "subjectId")]
    pub r#subject_id: String,
    #[serde(rename = "predicate")]
    pub r#predicate: String,
    #[serde(rename = "confirmationState")]
    pub r#confirmation_state: String,
    #[serde(rename = "leftSourceRevisionId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#left_source_revision_id: Option<String>,
    #[serde(rename = "rightSourceRevisionId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#right_source_revision_id: Option<String>,
    #[serde(rename = "leftEvidence")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#left_evidence: Option<HttpHypothesisResultSimulationFindingsItemVariant0LeftEvidence>,
    #[serde(rename = "rightEvidence")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#right_evidence: Option<HttpHypothesisResultSimulationFindingsItemVariant0RightEvidence>,
    #[serde(rename = "type")]
    pub r#type: HttpHypothesisResultSimulationFindingsItemVariant0Type,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpHypothesisResultSimulationFindingsItemVariant1Type { #[serde(rename = "epistemicDisagreement")] EpistemicDisagreement }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpHypothesisResultSimulationFindingsItemVariant1PropositionRef {
    #[serde(rename = "batchId")]
    pub r#batch_id: String,
    #[serde(rename = "propositionId")]
    pub r#proposition_id: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpHypothesisResultSimulationFindingsItemVariant1 {
    #[serde(rename = "subjectId")]
    pub r#subject_id: String,
    #[serde(rename = "predicate")]
    pub r#predicate: String,
    #[serde(rename = "confirmationState")]
    pub r#confirmation_state: String,
    #[serde(rename = "leftSourceRevisionId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#left_source_revision_id: Option<String>,
    #[serde(rename = "rightSourceRevisionId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#right_source_revision_id: Option<String>,
    #[serde(rename = "leftEvidence")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#left_evidence: Option<HttpHypothesisResultSimulationFindingsItemVariant0LeftEvidence>,
    #[serde(rename = "rightEvidence")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#right_evidence: Option<HttpHypothesisResultSimulationFindingsItemVariant0RightEvidence>,
    #[serde(rename = "type")]
    pub r#type: HttpHypothesisResultSimulationFindingsItemVariant1Type,
    #[serde(rename = "ownerId")]
    pub r#owner_id: String,
    #[serde(rename = "propositionRef")]
    pub r#proposition_ref: HttpHypothesisResultSimulationFindingsItemVariant1PropositionRef,
    #[serde(rename = "leftState")]
    pub r#left_state: String,
    #[serde(rename = "rightState")]
    pub r#right_state: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpHypothesisResultSimulationFindingsItemVariant2Type {
    #[serde(rename = "stateValueConflict")] StateValueConflict,
    #[serde(rename = "stateSchemaDisagreement")] StateSchemaDisagreement,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpHypothesisResultSimulationFindingsItemVariant2SchemaAuthority {
    #[serde(rename = "modelDeclared")] ModelDeclared,
    #[serde(rename = "authorConfirmed")] AuthorConfirmed,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpHypothesisResultSimulationFindingsItemVariant2LeftValueVariant4 {
    #[serde(rename = "entityId")]
    pub r#entity_id: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(untagged)]
pub enum HttpHypothesisResultSimulationFindingsItemVariant2LeftValue {
    Text(String),
    Number(f64),
    WireFalse(WireFalse),
    WireTrue(WireTrue),
    HypothesisResultSimulationFindingsItemVariant2LeftValueVariant4(HttpHypothesisResultSimulationFindingsItemVariant2LeftValueVariant4),
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpHypothesisResultSimulationFindingsItemVariant2RightValueVariant4 {
    #[serde(rename = "entityId")]
    pub r#entity_id: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(untagged)]
pub enum HttpHypothesisResultSimulationFindingsItemVariant2RightValue {
    Text(String),
    Number(f64),
    WireFalse(WireFalse),
    WireTrue(WireTrue),
    HypothesisResultSimulationFindingsItemVariant2RightValueVariant4(HttpHypothesisResultSimulationFindingsItemVariant2RightValueVariant4),
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpHypothesisResultSimulationFindingsItemVariant2 {
    #[serde(rename = "subjectId")]
    pub r#subject_id: String,
    #[serde(rename = "predicate")]
    pub r#predicate: String,
    #[serde(rename = "confirmationState")]
    pub r#confirmation_state: String,
    #[serde(rename = "leftSourceRevisionId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#left_source_revision_id: Option<String>,
    #[serde(rename = "rightSourceRevisionId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#right_source_revision_id: Option<String>,
    #[serde(rename = "leftEvidence")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#left_evidence: Option<HttpHypothesisResultSimulationFindingsItemVariant0LeftEvidence>,
    #[serde(rename = "rightEvidence")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#right_evidence: Option<HttpHypothesisResultSimulationFindingsItemVariant0RightEvidence>,
    #[serde(rename = "type")]
    pub r#type: HttpHypothesisResultSimulationFindingsItemVariant2Type,
    #[serde(rename = "schemaAuthority")]
    pub r#schema_authority: HttpHypothesisResultSimulationFindingsItemVariant2SchemaAuthority,
    #[serde(rename = "propertyRulesRevisionId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#property_rules_revision_id: Option<String>,
    #[serde(rename = "leftSchema")]
    pub r#left_schema: HttpPropertyRuleDefinitionSchema,
    #[serde(rename = "rightSchema")]
    pub r#right_schema: HttpPropertyRuleDefinitionSchema,
    #[serde(rename = "leftValue")]
    pub r#left_value: HttpHypothesisResultSimulationFindingsItemVariant2LeftValue,
    #[serde(rename = "rightValue")]
    pub r#right_value: HttpHypothesisResultSimulationFindingsItemVariant2RightValue,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(untagged)]
pub enum HttpHypothesisResultSimulationFindingsItem {
    HypothesisResultSimulationFindingsItemVariant0(HttpHypothesisResultSimulationFindingsItemVariant0),
    HypothesisResultSimulationFindingsItemVariant1(HttpHypothesisResultSimulationFindingsItemVariant1),
    HypothesisResultSimulationFindingsItemVariant2(HttpHypothesisResultSimulationFindingsItemVariant2),
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpHypothesisResultSimulationCoverage {
    #[serde(rename = "projectionComplete")]
    pub r#projection_complete: bool,
    #[serde(rename = "globalImpactComplete")]
    pub r#global_impact_complete: WireFalse,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpHypothesisResultSimulation {
    #[serde(rename = "entries")]
    pub r#entries: Vec<HttpHypothesisResultSimulationEntriesItem>,
    #[serde(rename = "findings")]
    pub r#findings: Vec<HttpHypothesisResultSimulationFindingsItem>,
    #[serde(rename = "coverage")]
    pub r#coverage: HttpHypothesisResultSimulationCoverage,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpHypothesisResult {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "projectId")]
    pub r#project_id: String,
    #[serde(rename = "assumptions")]
    pub r#assumptions: Vec<HttpHypothesisAssumption>,
    #[serde(rename = "baseSnapshotId")]
    pub r#base_snapshot_id: String,
    #[serde(rename = "intent")]
    pub r#intent: String,
    #[serde(rename = "authority")]
    pub r#authority: HttpHypothesisResultAuthority,
    #[serde(rename = "baseline")]
    pub r#baseline: HttpHypothesisResultBaseline,
    #[serde(rename = "simulation")]
    pub r#simulation: HttpHypothesisResultSimulation,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererGetHypothesisResponse(pub HttpHypothesisResult);
