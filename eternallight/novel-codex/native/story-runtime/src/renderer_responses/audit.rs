use super::*;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpGetSearchAuditResponse {
    #[serde(rename = "job")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#job: Option<novel_codex_story_core::SearchAuditJob>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererGetSearchAuditResponse(pub HttpGetSearchAuditResponse);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererStartSearchAuditResponse(pub novel_codex_story_core::SearchAuditJob);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererControlSearchAuditResponse(pub novel_codex_story_core::SearchAuditJob);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpGetEntityAuditResponse {
    #[serde(rename = "job")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#job: Option<novel_codex_story_core::EntityAuditJob>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererGetEntityAuditResponse(pub HttpGetEntityAuditResponse);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererStartEntityAuditResponse(pub novel_codex_story_core::EntityAuditJob);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererControlEntityAuditResponse(pub novel_codex_story_core::EntityAuditJob);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpGetRelationAuditResponse {
    #[serde(rename = "job")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#job: Option<novel_codex_story_core::RelationAuditJob>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererGetRelationAuditResponse(pub HttpGetRelationAuditResponse);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererStartRelationAuditResponse(pub novel_codex_story_core::RelationAuditJob);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererControlRelationAuditResponse(pub novel_codex_story_core::RelationAuditJob);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpGetKnowledgeAuditResponse {
    #[serde(rename = "job")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#job: Option<novel_codex_story_core::KnowledgeAuditJob>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererGetKnowledgeAuditResponse(pub HttpGetKnowledgeAuditResponse);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererStartKnowledgeAuditResponse(pub novel_codex_story_core::KnowledgeAuditJob);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererControlKnowledgeAuditResponse(pub novel_codex_story_core::KnowledgeAuditJob);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpGetSnapshotKnowledgeAuditResponse {
    #[serde(rename = "job")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#job: Option<novel_codex_story_core::SnapshotKnowledgeAuditJob>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererGetSnapshotKnowledgeAuditResponse(pub HttpGetSnapshotKnowledgeAuditResponse);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererStartSnapshotKnowledgeAuditResponse(pub novel_codex_story_core::SnapshotKnowledgeAuditJob);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererControlSnapshotKnowledgeAuditResponse(pub novel_codex_story_core::SnapshotKnowledgeAuditJob);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpGetFactAuditResponse {
    #[serde(rename = "job")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#job: Option<novel_codex_story_core::FactAuditJob>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererGetFactAuditResponse(pub HttpGetFactAuditResponse);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererStartFactAuditResponse(pub novel_codex_story_core::FactAuditJob);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererControlFactAuditResponse(pub novel_codex_story_core::FactAuditJob);
