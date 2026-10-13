//! Current typed multimedia contract. All media identities refer to immutable revisions.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub enum CreativeMedium {
    Novel,
    Comic,
    Animation,
}
impl CreativeMedium {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Novel => "novel",
            Self::Comic => "comic",
            Self::Animation => "animation",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Rational {
    pub numerator: i64,
    pub denominator: i64,
}
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MediaTime {
    pub value: i64,
    pub rate: Rational,
}
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MediaRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MediaRange {
    pub start: MediaTime,
    pub duration: MediaTime,
}
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum MediaSelector {
    Whole,
    FieldText {
        field: MediaTextField,
        #[serde(rename = "startChar")]
        start_char: usize,
        #[serde(rename = "endChar")]
        end_char: usize,
        quote: String,
    },
    Text {
        #[serde(rename = "startByte")]
        start_byte: usize,
        #[serde(rename = "endByte")]
        end_byte: usize,
        quote: String,
    },
    Panel {
        #[serde(rename = "pageId")]
        page_id: String,
        #[serde(rename = "panelId")]
        panel_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        region: Option<MediaRect>,
    },
    Shot {
        #[serde(rename = "shotId")]
        shot_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        range: Option<MediaRange>,
    },
    Clip {
        #[serde(rename = "trackId")]
        track_id: String,
        #[serde(rename = "clipId")]
        clip_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        range: Option<MediaRange>,
    },
    Region {
        region: MediaRect,
    },
    Time {
        start: MediaTime,
        duration: MediaTime,
    },
}
/// Stable field identities; character coordinates count Unicode scalar values.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum MediaTextField {
    CreativeDesign,
    BalloonText {
        page_id: String,
        panel_id: String,
        balloon_id: String,
    },
    PanelDescription {
        page_id: String,
        panel_id: String,
    },
    ShotDescription {
        shot_id: String,
    },
    ClipCaption {
        track_id: String,
        clip_id: String,
    },
    CanonStatement {
        entry_id: String,
    },
}
#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MediaTextEdit {
    pub start_char: usize,
    pub end_char: usize,
    pub expected_slice_digest: String,
    pub replacement: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReadMediaText {
    pub revision_id: String,
    pub field: MediaTextField,
    pub char_offset: Option<usize>,
    pub char_limit: Option<usize>,
}
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MediaSourceRef {
    pub project_id: String,
    pub scope_id: String,
    pub branch_id: String,
    pub object_id: String,
    pub revision_id: String,
    pub digest: String,
    pub selector: MediaSelector,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub enum MediaFreshness {
    Pinned,
    MustBeCurrent,
}
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MediaReadDependency {
    pub source: MediaSourceRef,
    pub freshness: MediaFreshness,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub enum MediaRelation {
    Adapts,
    Depicts,
    UsesReference,
    DerivedFrom,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub enum MediaSourcePurpose { Plot, Character, Visual, Audio, Timing, Reference }

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MediaSourceLink {
    /// Model-owned adaptation explanation; never an author approval.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub purpose: Option<MediaSourcePurpose>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub decision: Option<String>,
    pub relation: MediaRelation,
    pub source: MediaSourceRef,
    pub target_object_id: String,
    pub target_selector: MediaSelector,
}
#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreativeEdition {
    pub id: String,
    pub project_id: String,
    pub edition_id: String,
    pub scope_id: String,
    pub branch_id: String,
    pub medium: CreativeMedium,
    pub title: String,
    pub canon_snapshot_id: Option<String>,
    pub sources: Vec<MediaSourceRef>,
}
#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(deny_unknown_fields)]
pub struct MediaProject {
    pub id: String,
    pub title: String,
    pub logline: String,
    pub created_at: i64,
    pub updated_at: i64,
}
#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreativeProjectCreated {
    pub project: MediaProject,
    pub editions: Vec<CreativeEdition>,
    pub active_edition_id: String,
}
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub enum BalloonKind {
    Speech,
    Thought,
    Caption,
    Sound,
}
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComicBalloon {
    pub id: String,
    pub text: String,
    pub speaker: String,
    pub rect: MediaRect,
    pub kind: BalloonKind,
}
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComicPanel {
    pub id: String,
    pub description: String,
    pub rect: MediaRect,
    pub asset_revision_id: Option<String>,
    pub reference_asset_revision_ids: Vec<String>,
    pub balloons: Vec<ComicBalloon>,
    pub locked: bool,
}
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub enum ReadingDirection {
    LeftToRight,
    RightToLeft,
    Vertical,
}
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComicPage {
    pub id: String,
    pub title: String,
    pub width: i64,
    pub height: i64,
    pub reading_direction: ReadingDirection,
    pub panels: Vec<ComicPanel>,
}
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub enum ComicFormat {
    Pages,
    Strip,
}
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnimationShot {
    pub id: String,
    pub title: String,
    pub description: String,
    pub asset_revision_id: Option<String>,
    pub reference_asset_revision_ids: Vec<String>,
    pub locked: bool,
}
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnimationClip {
    pub id: String,
    pub shot_id: Option<String>,
    pub asset_revision_id: Option<String>,
    pub start: MediaTime,
    pub duration: MediaTime,
    pub source_start: MediaTime,
    pub playback_rate: Rational,
    pub caption: String,
}
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub enum TrackKind {
    Video,
    Audio,
    Subtitle,
}
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnimationTrack {
    pub id: String,
    pub title: String,
    pub kind: TrackKind,
    pub muted: bool,
    pub clips: Vec<AnimationClip>,
}
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnimationTimeline {
    pub tracks: Vec<AnimationTrack>,
}
/// A model-owned production target; never proof of author approval or delivery.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScreenProduction {
    pub form: ScreenForm,
    pub expression: ScreenExpression,
    pub deliverable: ScreenDeliverable,
    pub target_duration: Option<MediaTime>,
}
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub enum ScreenForm { ShortFilm, FeatureFilm, Episode }
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub enum ScreenExpression { Animation, LiveAction, Hybrid }
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub enum ScreenDeliverable { Screenplay, Storyboard, Animatic, FinishedVideo }
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum MultimediaDocument {
    Comic {
        title: String,
        #[serde(rename = "creativeDesign", skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        creative_design: Option<String>,
        format: ComicFormat,
        pages: Vec<ComicPage>,
    },
    Animation {
        title: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        production: Option<ScreenProduction>,
        #[serde(rename = "creativeDesign", skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        creative_design: Option<String>,
        width: i64,
        height: i64,
        #[serde(rename = "frameRate")]
        frame_rate: Rational,
        shots: Vec<AnimationShot>,
        timeline: AnimationTimeline,
    },
}
impl MultimediaDocument {
    pub fn creative_design(&self) -> Option<&str> {
        match self {
            Self::Comic { creative_design, .. } | Self::Animation { creative_design, .. } => creative_design.as_deref(),
        }
    }
    pub fn medium(&self) -> CreativeMedium {
        match self {
            Self::Comic { .. } => CreativeMedium::Comic,
            Self::Animation { .. } => CreativeMedium::Animation,
        }
    }
    pub fn empty(medium: CreativeMedium, title: String) -> Option<Self> {
        match medium {
            CreativeMedium::Novel => None,
            CreativeMedium::Comic => Some(Self::Comic {
                title,
                creative_design: None,
                format: ComicFormat::Pages,
                pages: vec![],
            }),
            CreativeMedium::Animation => Some(Self::Animation {
                title,
                production: None,
                creative_design: None,
                width: 1920,
                height: 1080,
                frame_rate: Rational {
                    numerator: 24,
                    denominator: 1,
                },
                shots: vec![],
                timeline: AnimationTimeline { tracks: vec![] },
            }),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub enum MediaAssetKind {
    Image,
    Audio,
    Video,
}
#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MediaAsset {
    pub id: String,
    pub asset_revision_id: String,
    pub project_id: String,
    pub scope_id: String,
    pub name: String,
    pub kind: MediaAssetKind,
    pub mime_type: String,
    pub digest: String,
    pub bytes: i64,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub duration: Option<MediaTime>,
    pub sample_rate: Option<i64>,
    pub channels: Option<i64>,
    pub preview_url: String,
    pub created_at: i64,
}
#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MultimediaWorkspace {
    pub project: MediaProject,
    pub edition: CreativeEdition,
    pub revision: i64,
    pub source_digest: String,
    pub revision_id: Option<String>,
    pub document_source: Option<MediaSourceRef>,
    /// Exact immutable content_json bytes for digest verification and offline delivery.
    pub document_encoding: Option<String>,
    pub head_snapshot_id: String,
    pub document: MultimediaDocument,
    pub assets: Vec<MediaAsset>,
    pub sources: Vec<MediaSourceLink>,
}
#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveMultimediaDocument {
    pub expected_revision_id: Option<String>,
    pub document: MultimediaDocument,
    pub idempotency_key: String,
    pub read_set: Vec<MediaReadDependency>,
    pub sources: Vec<MediaSourceLink>,
}
#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PatchMultimediaDocument {
    pub expected_revision_id: Option<String>,
    pub idempotency_key: String,
    pub operations: Vec<MediaDocumentOperation>,
    /// New reads merge into the current revision; an explicit repeat can change freshness.
    pub read_set: Vec<MediaReadDependency>,
    /// New links append without dropping existing links not included in this edit.
    pub sources: Vec<MediaSourceLink>,
}
#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComicPageMetadata {
    pub id: String,
    pub title: String,
    pub width: i64,
    pub height: i64,
    pub reading_direction: ReadingDirection,
}
#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComicPanelMetadata {
    pub id: String,
    pub description: String,
    pub rect: MediaRect,
    pub asset_revision_id: Option<String>,
    pub reference_asset_revision_ids: Vec<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnimationShotMetadata {
    pub id: String,
    pub title: String,
    pub description: String,
    pub asset_revision_id: Option<String>,
    pub reference_asset_revision_ids: Vec<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnimationTrackMetadata {
    pub id: String,
    pub title: String,
    pub kind: TrackKind,
    pub muted: bool,
}
/// Typed local operations. Metadata upserts retain children, order, and locks.
/// All operations apply sequentially against one CAS baseline, then commit once.
#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum MediaDocumentOperation {
    ReplaceTextRanges {
        field: MediaTextField,
        #[serde(rename = "expectedTextDigest")]
        expected_text_digest: String,
        edits: Vec<MediaTextEdit>,
    },
    /// Author-only adoption creates a new document revision with this exact canon pin.
    AdoptCanonSnapshot {
        canon_snapshot_id: String,
    },
    /// Author-only replacement of the input baseline for future tasks.
    ReplaceEditionSource {
        previous: MediaSourceRef,
        next: MediaSourceRef,
    },
    SetScreenProduction {
        production: Option<ScreenProduction>,
    },
    SetCreativeDesign {
        text: Option<String>,
    },
    SetTitle {
        title: String,
    },
    SetComicFormat {
        format: ComicFormat,
    },
    UpsertPage {
        page: ComicPageMetadata,
    },
    RemovePage {
        page_id: String,
    },
    MovePage {
        page_id: String,
        before_id: Option<String>,
    },
    UpsertPanel {
        page_id: String,
        panel: ComicPanelMetadata,
    },
    RemovePanel {
        page_id: String,
        panel_id: String,
    },
    MovePanel {
        page_id: String,
        panel_id: String,
        before_id: Option<String>,
    },
    SetPanelLock {
        page_id: String,
        panel_id: String,
        locked: bool,
    },
    UpsertBalloon {
        page_id: String,
        panel_id: String,
        balloon: ComicBalloon,
    },
    RemoveBalloon {
        page_id: String,
        panel_id: String,
        balloon_id: String,
    },
    MoveBalloon {
        page_id: String,
        panel_id: String,
        balloon_id: String,
        before_id: Option<String>,
    },
    SetAnimationSettings {
        width: i64,
        height: i64,
        frame_rate: Rational,
    },
    UpsertShot {
        shot: AnimationShotMetadata,
    },
    RemoveShot {
        shot_id: String,
    },
    MoveShot {
        shot_id: String,
        before_id: Option<String>,
    },
    SetShotLock {
        shot_id: String,
        locked: bool,
    },
    UpsertTrack {
        track: AnimationTrackMetadata,
    },
    RemoveTrack {
        track_id: String,
    },
    MoveTrack {
        track_id: String,
        before_id: Option<String>,
    },
    UpsertClip {
        track_id: String,
        clip: AnimationClip,
    },
    RemoveClip {
        track_id: String,
        clip_id: String,
    },
    MoveClip {
        track_id: String,
        clip_id: String,
        before_id: Option<String>,
    },
    RemoveSourceLink {
        link: MediaSourceLink,
    },
}
#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateEditionRequest {
    pub title: String,
    pub medium: CreativeMedium,
    pub idempotency_key: String,
    pub sources: Vec<MediaSourceRef>,
}
#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateCreativeProjectRequest {
    pub title: String,
    pub logline: String,
    pub medium: CreativeMedium,
    pub idempotency_key: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MediaAssetInput {
    pub name: String,
    pub kind: MediaAssetKind,
    pub mime_type: String,
    pub digest: String,
    pub bytes: i64,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub duration: Option<MediaTime>,
    pub sample_rate: Option<i64>,
    pub channels: Option<i64>,
}
#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BeginMediaImport {
    pub operation_id: String,
    pub idempotency_key: String,
    pub name: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RegisterMediaAsset {
    pub operation_id: String,
    pub idempotency_key: String,
    pub asset: MediaAssetInput,
    pub sources: Vec<MediaSourceRef>,
}
#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateMediaBranch {
    pub source_branch_id: String,
    pub title: String,
    pub idempotency_key: String,
}
