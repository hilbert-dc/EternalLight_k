//! Author-facing media HTTP receipts. Reuse Core's persisted document contract.
use super::{WireTrue, required_nullable};
use novel_codex_story_core::{
    CanonWorkspace, CreativeEdition, MediaFreshness, MediaSelector, MediaSourcePurpose,
    MediaSourceRef, MediaTextField, MultimediaWorkspace,
};

macro_rules! core_response {
    ($name:ident, $body:ty) => {
        #[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
        #[serde(transparent)]
        #[ts(export, export_to = "responses/")]
        pub struct $name(pub $body);
    };
}
core_response!(RendererMediaCreateEditionResponse, CreativeEdition);
core_response!(RendererMediaReadWorkspaceResponse, MultimediaWorkspace);
core_response!(RendererMediaSaveDocumentResponse, MultimediaWorkspace);
core_response!(RendererMediaPatchDocumentResponse, MultimediaWorkspace);
core_response!(RendererMediaReadCanonResponse, CanonWorkspace);
core_response!(RendererMediaSaveCanonResponse, CanonWorkspace);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "responses/")]
pub struct RendererMediaListEditionsResponse {
    pub editions: Vec<CreativeEdition>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "responses/")]
pub struct MediaCatalogItem {
    pub source: MediaSourceRef,
    pub kind: String,
    pub title: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "responses/")]
pub struct RendererMediaSourceCatalogResponse {
    pub project_id: String,
    pub edition_id: String,
    pub branch_id: String,
    pub snapshot_id: String,
    pub items: Vec<MediaCatalogItem>,
    pub offset: usize,
    pub total: usize,
    #[serde(deserialize_with = "required_nullable")]
    pub next_offset: Option<usize>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "responses/")]
pub enum MediaImpactStatus {
    Current,
    Changed,
    SelectionMissing,
    Removed,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "responses/")]
pub struct MediaImpactTarget {
    pub object_id: String,
    pub selector: MediaSelector,
    #[serde(deserialize_with = "required_nullable")]
    pub purpose: Option<MediaSourcePurpose>,
    #[serde(deserialize_with = "required_nullable")]
    pub decision: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "responses/")]
pub struct MediaImpactItem {
    pub source: MediaSourceRef,
    #[serde(deserialize_with = "required_nullable")]
    pub latest: Option<MediaSourceRef>,
    pub status: MediaImpactStatus,
    pub targets: Vec<MediaImpactTarget>,
    #[serde(deserialize_with = "required_nullable")]
    pub read_freshness: Option<MediaFreshness>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "responses/")]
pub struct MediaCanonImpact {
    #[serde(deserialize_with = "required_nullable")]
    pub pinned_snapshot_id: Option<String>,
    pub current_snapshot_id: String,
    pub changed: bool,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "responses/")]
pub struct RendererMediaSourceImpactsResponse {
    pub project_id: String,
    pub edition_id: String,
    pub branch_id: String,
    #[serde(deserialize_with = "required_nullable")]
    pub revision_id: Option<String>,
    pub items: Vec<MediaImpactItem>,
    pub offset: usize,
    pub total: usize,
    #[serde(deserialize_with = "required_nullable")]
    pub next_offset: Option<usize>,
    pub canon: MediaCanonImpact,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "responses/")]
pub enum MediaTextSection {
    Text,
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "responses/")]
pub enum MediaTextUnit {
    UnicodeScalar,
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "responses/")]
pub enum MediaTextVersion {
    Source,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "responses/")]
pub struct RendererMediaReadTextPageResponse {
    pub section: MediaTextSection,
    pub kind: MediaTextSection,
    pub unit: MediaTextUnit,
    pub partial_view: WireTrue,
    pub version: MediaTextVersion,
    pub revision_id: String,
    pub field: MediaTextField,
    pub source: MediaSourceRef,
    pub returned_source: MediaSourceRef,
    pub text: String,
    pub text_digest: String,
    pub slice_digest: String,
    pub start_char: usize,
    pub end_char: usize,
    pub total_chars: usize,
    pub authorized_start_char: usize,
    pub authorized_end_char: usize,
    pub char_offset: usize,
    #[serde(deserialize_with = "required_nullable")]
    pub next_char_offset: Option<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn media_pagination_requires_explicit_terminal_cursor_and_allows_unpinned_canon() {
        let mut catalog = json!({"projectId":"p","editionId":"e","branchId":"b","snapshotId":"s","items":[],"offset":0,"total":0,"nextOffset":null});
        assert!(
            serde_json::from_value::<RendererMediaSourceCatalogResponse>(catalog.clone()).is_ok()
        );
        catalog.as_object_mut().unwrap().remove("nextOffset");
        assert!(serde_json::from_value::<RendererMediaSourceCatalogResponse>(catalog).is_err());
        let mut impact = json!({"projectId":"p","editionId":"e","branchId":"b","revisionId":null,"items":[],"offset":0,"total":0,"nextOffset":null,"canon":{"pinnedSnapshotId":null,"currentSnapshotId":"s","changed":true}});
        assert!(
            serde_json::from_value::<RendererMediaSourceImpactsResponse>(impact.clone()).is_ok()
        );
        impact["canon"]
            .as_object_mut()
            .unwrap()
            .remove("pinnedSnapshotId");
        assert!(serde_json::from_value::<RendererMediaSourceImpactsResponse>(impact).is_err());
    }

    #[test]
    fn author_text_page_cannot_claim_complete_view_or_use_invalid_coordinates() {
        let source = json!({"projectId":"p","scopeId":"e","branchId":"b","objectId":"e","revisionId":"r","digest":"digest","selector":{"kind":"whole"}});
        let page = json!({"section":"text","kind":"text","unit":"unicodeScalar","partialView":true,"version":"source","revisionId":"r","field":{"kind":"creativeDesign"},"source":source,"returnedSource":source,"text":"","textDigest":"digest","sliceDigest":"digest","startChar":0,"endChar":0,"totalChars":0,"authorizedStartChar":0,"authorizedEndChar":0,"charOffset":0,"nextCharOffset":null});
        assert!(serde_json::from_value::<RendererMediaReadTextPageResponse>(page.clone()).is_ok());
        for (key, value) in [
            ("partialView", json!(false)),
            ("version", json!("working")),
            ("startChar", json!(-1)),
        ] {
            let mut invalid = page.clone();
            invalid[key] = value;
            assert!(
                serde_json::from_value::<RendererMediaReadTextPageResponse>(invalid).is_err(),
                "{key}"
            );
        }
    }
}
