use super::*;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(untagged)]
pub enum HttpMediaProjectCreatedAt {
    Text(String),
    Number(f64),
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(untagged)]
pub enum HttpMediaProjectUpdatedAt {
    Text(String),
    Number(f64),
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpMediaProject {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "title")]
    pub r#title: String,
    #[serde(rename = "logline")]
    pub r#logline: String,
    #[serde(rename = "created_at")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#created_at: Option<HttpMediaProjectCreatedAt>,
    #[serde(rename = "updated_at")]
    pub r#updated_at: HttpMediaProjectUpdatedAt,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpMediaSelectorWholeKind { #[serde(rename = "whole")] Whole }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpMediaSelectorWhole {
    #[serde(rename = "kind")]
    pub r#kind: HttpMediaSelectorWholeKind,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpMediaSelectorFieldTextKind { #[serde(rename = "fieldText")] FieldText }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpMediaTextFieldCreativeDesignKind { #[serde(rename = "creativeDesign")] CreativeDesign }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpMediaTextFieldCreativeDesign {
    #[serde(rename = "kind")]
    pub r#kind: HttpMediaTextFieldCreativeDesignKind,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpMediaTextFieldBalloonTextKind { #[serde(rename = "balloonText")] BalloonText }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpMediaTextFieldBalloonText {
    #[serde(rename = "kind")]
    pub r#kind: HttpMediaTextFieldBalloonTextKind,
    #[serde(rename = "pageId")]
    pub r#page_id: String,
    #[serde(rename = "panelId")]
    pub r#panel_id: String,
    #[serde(rename = "balloonId")]
    pub r#balloon_id: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpMediaTextFieldPanelDescriptionKind { #[serde(rename = "panelDescription")] PanelDescription }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpMediaTextFieldPanelDescription {
    #[serde(rename = "kind")]
    pub r#kind: HttpMediaTextFieldPanelDescriptionKind,
    #[serde(rename = "pageId")]
    pub r#page_id: String,
    #[serde(rename = "panelId")]
    pub r#panel_id: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpMediaTextFieldShotDescriptionKind { #[serde(rename = "shotDescription")] ShotDescription }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpMediaTextFieldShotDescription {
    #[serde(rename = "kind")]
    pub r#kind: HttpMediaTextFieldShotDescriptionKind,
    #[serde(rename = "shotId")]
    pub r#shot_id: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpMediaTextFieldClipCaptionKind { #[serde(rename = "clipCaption")] ClipCaption }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpMediaTextFieldClipCaption {
    #[serde(rename = "kind")]
    pub r#kind: HttpMediaTextFieldClipCaptionKind,
    #[serde(rename = "trackId")]
    pub r#track_id: String,
    #[serde(rename = "clipId")]
    pub r#clip_id: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpMediaTextFieldCanonStatementKind { #[serde(rename = "canonStatement")] CanonStatement }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpMediaTextFieldCanonStatement {
    #[serde(rename = "kind")]
    pub r#kind: HttpMediaTextFieldCanonStatementKind,
    #[serde(rename = "entryId")]
    pub r#entry_id: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(untagged)]
pub enum HttpMediaTextField {
    MediaTextFieldCreativeDesign(HttpMediaTextFieldCreativeDesign),
    MediaTextFieldBalloonText(HttpMediaTextFieldBalloonText),
    MediaTextFieldPanelDescription(HttpMediaTextFieldPanelDescription),
    MediaTextFieldShotDescription(HttpMediaTextFieldShotDescription),
    MediaTextFieldClipCaption(HttpMediaTextFieldClipCaption),
    MediaTextFieldCanonStatement(HttpMediaTextFieldCanonStatement),
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpMediaSelectorFieldText {
    #[serde(rename = "kind")]
    pub r#kind: HttpMediaSelectorFieldTextKind,
    #[serde(rename = "field")]
    pub r#field: HttpMediaTextField,
    #[serde(rename = "startChar")]
    pub r#start_char: f64,
    #[serde(rename = "endChar")]
    pub r#end_char: f64,
    #[serde(rename = "quote")]
    pub r#quote: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpMediaSelectorTextKind { #[serde(rename = "text")] Text }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpMediaSelectorText {
    #[serde(rename = "kind")]
    pub r#kind: HttpMediaSelectorTextKind,
    #[serde(rename = "startByte")]
    pub r#start_byte: f64,
    #[serde(rename = "endByte")]
    pub r#end_byte: f64,
    #[serde(rename = "quote")]
    pub r#quote: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpMediaSelectorPanelKind { #[serde(rename = "panel")] Panel }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpMediaRect {
    #[serde(rename = "x")]
    pub r#x: f64,
    #[serde(rename = "y")]
    pub r#y: f64,
    #[serde(rename = "width")]
    pub r#width: f64,
    #[serde(rename = "height")]
    pub r#height: f64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpMediaSelectorPanel {
    #[serde(rename = "kind")]
    pub r#kind: HttpMediaSelectorPanelKind,
    #[serde(rename = "pageId")]
    pub r#page_id: String,
    #[serde(rename = "panelId")]
    pub r#panel_id: String,
    #[serde(rename = "region")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#region: Option<HttpMediaRect>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpMediaSelectorShotKind { #[serde(rename = "shot")] Shot }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpMediaTime {
    #[serde(rename = "value")]
    pub r#value: f64,
    #[serde(rename = "rate")]
    pub r#rate: HttpRational,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpMediaSelectorVariant4Range {
    #[serde(rename = "start")]
    pub r#start: HttpMediaTime,
    #[serde(rename = "duration")]
    pub r#duration: HttpMediaTime,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpMediaSelectorShot {
    #[serde(rename = "kind")]
    pub r#kind: HttpMediaSelectorShotKind,
    #[serde(rename = "shotId")]
    pub r#shot_id: String,
    #[serde(rename = "range")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#range: Option<HttpMediaSelectorVariant4Range>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpMediaSelectorClipKind { #[serde(rename = "clip")] Clip }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpMediaSelectorVariant5Range {
    #[serde(rename = "start")]
    pub r#start: HttpMediaTime,
    #[serde(rename = "duration")]
    pub r#duration: HttpMediaTime,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpMediaSelectorClip {
    #[serde(rename = "kind")]
    pub r#kind: HttpMediaSelectorClipKind,
    #[serde(rename = "trackId")]
    pub r#track_id: String,
    #[serde(rename = "clipId")]
    pub r#clip_id: String,
    #[serde(rename = "range")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#range: Option<HttpMediaSelectorVariant5Range>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpMediaSelectorRegionKind { #[serde(rename = "region")] Region }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpMediaSelectorRegion {
    #[serde(rename = "kind")]
    pub r#kind: HttpMediaSelectorRegionKind,
    #[serde(rename = "region")]
    pub r#region: HttpMediaRect,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub enum HttpMediaSelectorTimeKind { #[serde(rename = "time")] Time }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpMediaSelectorTime {
    #[serde(rename = "kind")]
    pub r#kind: HttpMediaSelectorTimeKind,
    #[serde(rename = "start")]
    pub r#start: HttpMediaTime,
    #[serde(rename = "duration")]
    pub r#duration: HttpMediaTime,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
#[serde(untagged)]
pub enum HttpMediaSelector {
    MediaSelectorWhole(HttpMediaSelectorWhole),
    MediaSelectorFieldText(HttpMediaSelectorFieldText),
    MediaSelectorText(HttpMediaSelectorText),
    MediaSelectorPanel(HttpMediaSelectorPanel),
    MediaSelectorShot(HttpMediaSelectorShot),
    MediaSelectorClip(HttpMediaSelectorClip),
    MediaSelectorRegion(HttpMediaSelectorRegion),
    MediaSelectorTime(HttpMediaSelectorTime),
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpMediaSourceRef {
    #[serde(rename = "projectId")]
    pub r#project_id: String,
    #[serde(rename = "scopeId")]
    pub r#scope_id: String,
    #[serde(rename = "branchId")]
    pub r#branch_id: String,
    #[serde(rename = "objectId")]
    pub r#object_id: String,
    #[serde(rename = "revisionId")]
    pub r#revision_id: String,
    #[serde(rename = "digest")]
    pub r#digest: String,
    #[serde(rename = "selector")]
    pub r#selector: HttpMediaSelector,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpCreativeEdition {
    #[serde(rename = "id")]
    pub r#id: String,
    #[serde(rename = "medium")]
    pub r#medium: HttpCreativeMedium,
    #[serde(rename = "title")]
    pub r#title: String,
    #[serde(rename = "canonSnapshotId")]
    #[serde(deserialize_with = "required_nullable")]
    pub r#canon_snapshot_id: Option<String>,
    #[serde(rename = "sources")]
    pub r#sources: Vec<HttpMediaSourceRef>,
    #[serde(rename = "projectId")]
    pub r#project_id: String,
    #[serde(rename = "editionId")]
    pub r#edition_id: String,
    #[serde(rename = "scopeId")]
    pub r#scope_id: String,
    #[serde(rename = "branchId")]
    pub r#branch_id: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/")]
pub struct HttpNovelEdition {
    #[serde(rename = "chapters")]
    pub r#chapters: Vec<HttpChapter>,
}
