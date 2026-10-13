use super::*;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpImpactAnalysisResultFindingsItemClassification { #[serde(rename = "possibleImpact")] PossibleImpact }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpImpactAnalysisResultFindingsItemEvidencePathItemEvidence {
    #[serde(rename = "quote")]
    pub r#quote: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpImpactAnalysisResultFindingsItemEvidencePathItem {
    #[serde(rename = "fromId")]
    pub r#from_id: String,
    #[serde(rename = "toId")]
    pub r#to_id: String,
    #[serde(rename = "relationType")]
    pub r#relation_type: String,
    #[serde(rename = "dependencyRuleId")]
    pub r#dependency_rule_id: String,
    #[serde(rename = "sourceObjectId")]
    pub r#source_object_id: String,
    #[serde(rename = "sourceRevisionId")]
    pub r#source_revision_id: String,
    #[serde(rename = "evidence")]
    pub r#evidence: HttpImpactAnalysisResultFindingsItemEvidencePathItemEvidence,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpImpactAnalysisResultFindingsItem {
    #[serde(rename = "affectedEntityId")]
    pub r#affected_entity_id: String,
    #[serde(rename = "classification")]
    pub r#classification: HttpImpactAnalysisResultFindingsItemClassification,
    #[serde(rename = "hopCount")]
    pub r#hop_count: i64,
    #[serde(rename = "evidencePath")]
    pub r#evidence_path: Vec<HttpImpactAnalysisResultFindingsItemEvidencePathItem>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpImpactAnalysisResultLiteralTextCandidatesItemEvidence {
    #[serde(rename = "quote")]
    pub r#quote: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpImpactAnalysisResultLiteralTextCandidatesItem {
    #[serde(rename = "sourceObjectId")]
    pub r#source_object_id: String,
    #[serde(rename = "sourceRevisionId")]
    pub r#source_revision_id: String,
    #[serde(rename = "sourcePath")]
    pub r#source_path: String,
    #[serde(rename = "matchedName")]
    pub r#matched_name: String,
    #[serde(rename = "evidence")]
    pub r#evidence: HttpImpactAnalysisResultLiteralTextCandidatesItemEvidence,
    #[serde(rename = "snippet")]
    pub r#snippet: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpImpactAnalysisResultRevisionDeltaSceneChangesItem {
    #[serde(rename = "sceneId")]
    pub r#scene_id: String,
    #[serde(rename = "status")]
    pub r#status: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpImpactAnalysisResultRevisionDeltaModelCandidateDeltaStatus {
    #[serde(rename = "unavailable")] Unavailable,
    #[serde(rename = "comparedCandidates")] ComparedCandidates,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpImpactAnalysisResultRevisionDeltaModelCandidateDeltaCandidateChangesItemChange {
    #[serde(rename = "newlyExtracted")] NewlyExtracted,
    #[serde(rename = "noLongerExtracted")] NoLongerExtracted,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpImpactAnalysisResultRevisionDeltaModelCandidateDeltaCandidateChangesItemClaim {
    #[serde(rename = "subjectId")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#subject_id: Option<String>,
    #[serde(rename = "targetId")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#target_id: Option<String>,
    #[serde(rename = "predicate")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#predicate: Option<String>,
    #[serde(rename = "value")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#value: Option<serde_json::Value>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpImpactAnalysisResultRevisionDeltaModelCandidateDeltaCandidateChangesItemEvidence {
    #[serde(rename = "quote")]
    pub r#quote: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpImpactAnalysisResultRevisionDeltaModelCandidateDeltaCandidateChangesItem {
    #[serde(rename = "change")]
    pub r#change: HttpImpactAnalysisResultRevisionDeltaModelCandidateDeltaCandidateChangesItemChange,
    #[serde(rename = "claim")]
    pub r#claim: HttpImpactAnalysisResultRevisionDeltaModelCandidateDeltaCandidateChangesItemClaim,
    #[serde(rename = "sourceRevisionId")]
    pub r#source_revision_id: String,
    #[serde(rename = "modelingBatchId")]
    pub r#modeling_batch_id: String,
    #[serde(rename = "evidence")]
    pub r#evidence: HttpImpactAnalysisResultRevisionDeltaModelCandidateDeltaCandidateChangesItemEvidence,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpImpactAnalysisResultRevisionDeltaModelCandidateDeltaCoverage {
    #[serde(rename = "from")]
    pub r#from: String,
    #[serde(rename = "to")]
    pub r#to: String,
    #[serde(rename = "complete")]
    pub r#complete: bool,
    #[serde(rename = "truncated")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#truncated: Option<bool>,
    #[serde(rename = "totalCandidateChanges")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#total_candidate_changes: Option<i64>,
    #[serde(rename = "evidenceReanchoredClaimCount")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#evidence_reanchored_claim_count: Option<i64>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpImpactAnalysisResultRevisionDeltaModelCandidateDelta {
    #[serde(rename = "status")]
    pub r#status: HttpImpactAnalysisResultRevisionDeltaModelCandidateDeltaStatus,
    #[serde(rename = "candidateChanges")]
    pub r#candidate_changes: Vec<HttpImpactAnalysisResultRevisionDeltaModelCandidateDeltaCandidateChangesItem>,
    #[serde(rename = "coverage")]
    pub r#coverage: HttpImpactAnalysisResultRevisionDeltaModelCandidateDeltaCoverage,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpImpactAnalysisResultRevisionDelta {
    #[serde(rename = "bodyChanged")]
    pub r#body_changed: bool,
    #[serde(rename = "sceneChanges")]
    pub r#scene_changes: Vec<HttpImpactAnalysisResultRevisionDeltaSceneChangesItem>,
    #[serde(rename = "modelCandidateDelta")]
    pub r#model_candidate_delta: HttpImpactAnalysisResultRevisionDeltaModelCandidateDelta,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpImpactAnalysisResultCoverageGraphCoverage {
    #[serde(rename = "complete")]
    pub r#complete: bool,
    #[serde(rename = "pendingSourceCount")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#pending_source_count: Option<i64>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpImpactAnalysisResultCoverageLiteralSearch {
    #[serde(rename = "literalNameSearchComplete")]
    pub r#literal_name_search_complete: bool,
    #[serde(rename = "namesTruncated")]
    pub r#names_truncated: bool,
    #[serde(rename = "resultsTruncated")]
    pub r#results_truncated: bool,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpImpactAnalysisResultCoverage {
    #[serde(rename = "globalImpactComplete")]
    pub r#global_impact_complete: WireFalse,
    #[serde(rename = "explicitGraphComplete")]
    pub r#explicit_graph_complete: bool,
    #[serde(rename = "truncatedPaths")]
    pub r#truncated_paths: bool,
    #[serde(rename = "graphCoverage")]
    pub r#graph_coverage: HttpImpactAnalysisResultCoverageGraphCoverage,
    #[serde(rename = "literalSearch")]
    pub r#literal_search: HttpImpactAnalysisResultCoverageLiteralSearch,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpImpactAnalysisResult {
    #[serde(rename = "dependencyRuleVersion")]
    pub r#dependency_rule_version: String,
    #[serde(rename = "baseSnapshotId")]
    pub r#base_snapshot_id: String,
    #[serde(rename = "canonicalEntityId")]
    pub r#canonical_entity_id: String,
    #[serde(rename = "findings")]
    pub r#findings: Vec<HttpImpactAnalysisResultFindingsItem>,
    #[serde(rename = "literalTextCandidates")]
    pub r#literal_text_candidates: Vec<HttpImpactAnalysisResultLiteralTextCandidatesItem>,
    #[serde(rename = "revisionDelta")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#revision_delta: Option<HttpImpactAnalysisResultRevisionDelta>,
    #[serde(rename = "coverage")]
    pub r#coverage: HttpImpactAnalysisResultCoverage,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererGetImpactAnalysisResponse(pub HttpImpactAnalysisResult);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpChapterDiff {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "chapter_id")]
    pub r#chapter_id: String,
    #[serde(rename = "fromText")]
    pub r#from_text: String,
    #[serde(rename = "toText")]
    pub r#to_text: String,
    #[serde(rename = "summary")]
    pub r#summary: String,
    #[serde(rename = "createdAt")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#created_at: Option<String>,
    #[serde(rename = "addedChars")]
    pub r#added_chars: f64,
    #[serde(rename = "removedChars")]
    pub r#removed_chars: f64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpGetDiffsResponse {
    #[serde(rename = "diffs")]
    pub r#diffs: Vec<HttpChapterDiff>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererGetDiffsResponse(pub HttpGetDiffsResponse);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpSearchKind {
    #[serde(rename = "draft")] Draft,
    #[serde(rename = "brief")] Brief,
    #[serde(rename = "outline")] Outline,
    #[serde(rename = "plan")] Plan,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpSearchResultSource { #[serde(rename = "story-revision")] StoryRevision }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpSearchResultEvidence {
    #[serde(rename = "quote")]
    pub r#quote: String,
    #[serde(rename = "start")]
    pub r#start: f64,
    #[serde(rename = "end")]
    pub r#end: f64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpSearchResult {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "title")]
    pub r#title: String,
    #[serde(rename = "kind")]
    pub r#kind: HttpSearchKind,
    #[serde(rename = "snippet")]
    pub r#snippet: String,
    #[serde(rename = "source")]
    pub r#source: HttpSearchResultSource,
    #[serde(rename = "sourceRevisionId")]
    pub r#source_revision_id: String,
    #[serde(rename = "sourcePath")]
    pub r#source_path: String,
    #[serde(rename = "evidence")]
    pub r#evidence: HttpSearchResultEvidence,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpSearchResponseCoverage {
    #[serde(rename = "complete")]
    pub r#complete: bool,
    #[serde(rename = "snapshotId")]
    pub r#snapshot_id: String,
    #[serde(rename = "scannedCandidates")]
    pub r#scanned_candidates: f64,
    #[serde(rename = "matchedDocuments")]
    pub r#matched_documents: f64,
    #[serde(rename = "candidateCapReached")]
    pub r#candidate_cap_reached: bool,
    #[serde(rename = "truncatedMatches")]
    pub r#truncated_matches: bool,
    #[serde(rename = "indexVersion")]
    pub r#index_version: i64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpSearchResponse {
    #[serde(rename = "results")]
    pub r#results: Vec<HttpSearchResult>,
    #[serde(rename = "nextCursor")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#next_cursor: Option<String>,
    #[serde(rename = "coverage")]
    pub r#coverage: HttpSearchResponseCoverage,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererSearchResponse(pub HttpSearchResponse);

