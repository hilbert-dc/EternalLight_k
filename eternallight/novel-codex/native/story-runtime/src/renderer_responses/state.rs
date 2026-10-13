use super::*;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpRuntimeStateProject {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "title")]
    pub r#title: String,
    #[serde(rename = "logline")]
    pub r#logline: String,
    #[serde(rename = "updated_at")]
    pub r#updated_at: i64,
    #[serde(rename = "created_at")]
    pub r#created_at: i64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpCharacter {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "name")]
    pub r#name: String,
    #[serde(rename = "role")]
    pub r#role: String,
    #[serde(rename = "status")]
    pub r#status: String,
    #[serde(rename = "color")]
    pub r#color: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpRelationship {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "sourceId")]
    pub r#source_id: String,
    #[serde(rename = "targetId")]
    pub r#target_id: String,
    #[serde(rename = "kind")]
    pub r#kind: String,
    #[serde(rename = "state")]
    pub r#state: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpStoryEvent {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "title")]
    pub r#title: String,
    #[serde(rename = "outcome")]
    pub r#outcome: String,
    #[serde(rename = "occurredAt")]
    pub r#occurred_at: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpLocation {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "name")]
    pub r#name: String,
    #[serde(rename = "constraints")]
    pub r#constraints: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpWorldRule {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "statement")]
    pub r#statement: String,
    #[serde(rename = "severity")]
    pub r#severity: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpPlotThreadTone {
    #[serde(rename = "amber")] Amber,
    #[serde(rename = "blue")] Blue,
    #[serde(rename = "violet")] Violet,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpPlotThread {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "title")]
    pub r#title: String,
    #[serde(rename = "state")]
    pub r#state: String,
    #[serde(rename = "tone")]
    pub r#tone: HttpPlotThreadTone,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpStoryCanon {
    #[serde(rename = "characters")]
    pub r#characters: Vec<HttpCharacter>,
    #[serde(rename = "relationships")]
    pub r#relationships: Vec<HttpRelationship>,
    #[serde(rename = "events")]
    pub r#events: Vec<HttpStoryEvent>,
    #[serde(rename = "locations")]
    pub r#locations: Vec<HttpLocation>,
    #[serde(rename = "worldRules")]
    pub r#world_rules: Vec<HttpWorldRule>,
    #[serde(rename = "plotThreads")]
    pub r#plot_threads: Vec<HttpPlotThread>,
    #[serde(rename = "notes")]
    pub r#notes: Vec<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpBeat {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "position")]
    pub r#position: i64,
    #[serde(rename = "description")]
    pub r#description: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpScene {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "title")]
    pub r#title: String,
    #[serde(rename = "location")]
    pub r#location: String,
    #[serde(rename = "time")]
    pub r#time: String,
    #[serde(rename = "summary")]
    pub r#summary: String,
    #[serde(rename = "status")]
    pub r#status: HttpSceneStatus,
    #[serde(rename = "beats")]
    pub r#beats: Vec<HttpBeat>,
    #[serde(rename = "participants")]
    pub r#participants: Vec<HttpSceneParticipantsItem>,
    #[serde(rename = "stateChanges")]
    pub r#state_changes: Vec<HttpSceneStateChangesItem>,
    #[serde(rename = "canonConstraints")]
    pub r#canon_constraints: Vec<HttpSceneCanonConstraintsItem>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpNarrativeIrAnchorsItem {
    #[serde(rename = "chapterId")]
    pub r#chapter_id: String,
    #[serde(rename = "passageId")]
    pub r#passage_id: String,
    #[serde(rename = "sceneId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#scene_id: Option<String>,
    #[serde(rename = "beatIds")]
    pub r#beat_ids: Vec<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpNarrativeIr {
    #[serde(rename = "version")]
    pub r#version: String,
    #[serde(rename = "scenes")]
    pub r#scenes: Vec<HttpScene>,
    #[serde(rename = "anchors")]
    pub r#anchors: Vec<HttpNarrativeIrAnchorsItem>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpPassage {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "position")]
    pub r#position: i64,
    #[serde(rename = "body")]
    pub r#body: String,
    #[serde(rename = "scene_id")]
    #[serde(default, skip_serializing_if="Option::is_none")]
    #[ts(optional)]
    pub r#scene_id: Option<String>,
    #[serde(rename = "beatIds")]
    pub r#beat_ids: Vec<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpChapter {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "number")]
    pub r#number: i64,
    #[serde(rename = "title")]
    pub r#title: String,
    #[serde(rename = "status")]
    pub r#status: String,
    #[serde(rename = "summary")]
    #[serde(default, skip_serializing_if="Option::is_none")]
    #[ts(optional)]
    pub r#summary: Option<String>,
    #[serde(rename = "updated_at")]
    #[serde(default, skip_serializing_if="Option::is_none")]
    #[ts(optional)]
    pub r#updated_at: Option<String>,
    #[serde(rename = "passages")]
    pub r#passages: Vec<HttpPassage>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpRuntimeState {
    #[serde(rename = "project")]
    pub r#project: HttpRuntimeStateProject,
    #[serde(rename = "canon")]
    pub r#canon: HttpStoryCanon,
    #[serde(rename = "narrativeIr")]
    pub r#narrative_ir: HttpNarrativeIr,
    #[serde(rename = "novelEdition")]
    pub r#novel_edition: HttpNovelEdition,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(transparent)]
pub struct RendererGetStateResponse(pub HttpRuntimeState);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpChapterStructureChangePreviousChapter {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "number")]
    pub r#number: i64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpChapterStructureChangeNextChapter {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "number")]
    pub r#number: i64,
}
