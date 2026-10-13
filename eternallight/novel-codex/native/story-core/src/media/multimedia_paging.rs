use crate::{MediaTextField, MultimediaDocument, MultimediaWorkspace, RepositoryError};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const PAGE_BYTES: usize = 64 * 1024;
#[derive(Clone, Debug, Default, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MediaWorkspacePageRequest {
    pub section: Option<String>,
    pub page_id: Option<String>,
    pub panel_id: Option<String>,
    pub track_id: Option<String>,
    pub offset: Option<usize>,
    pub limit: Option<usize>,
    pub text_field: Option<MediaTextField>,
    pub char_offset: Option<usize>,
    pub char_limit: Option<usize>,
    pub entry_id: Option<String>,
}
impl MediaWorkspacePageRequest {
    pub(crate) fn validate_text_parameters(&self) -> Result<(), RepositoryError> {
        if self.section.as_deref() != Some("text")
            || self.text_field.is_none()
            || self.offset.is_some()
            || self.limit.is_some()
            || self.page_id.is_some()
            || self.panel_id.is_some()
            || self.track_id.is_some()
            || self.entry_id.is_some()
            || self.char_limit.is_some_and(|n| n == 0 || n > 8000)
        {
            return Err(invalid("media_text_page_parameters_invalid"));
        }
        Ok(())
    }
    pub fn validate_section_parameters(&self) -> Result<(), RepositoryError> {
        if (self.section.as_deref() == Some("canonSources")) != self.entry_id.is_some() {
            return Err(invalid("media_canon_source_parameters_invalid"));
        }
        if self.section.as_deref() == Some("text") {
            self.validate_text_parameters()
        } else if self.text_field.is_some()
            || self.char_offset.is_some()
            || self.char_limit.is_some()
        {
            Err(invalid("media_text_page_parameters_invalid"))
        } else {
            Ok(())
        }
    }
}
fn invalid(code: &str) -> RepositoryError {
    RepositoryError::InvalidContent(code.into())
}
fn value<T: Serialize>(v: &T) -> Result<Value, RepositoryError> {
    serde_json::to_value(v).map_err(|_| RepositoryError::CorruptState)
}
impl MultimediaWorkspace {
    /// This is an explicitly partial read view, never a truncated document that
    /// could accidentally be submitted as a complete replacement.
    pub fn page(&self, r: &MediaWorkspacePageRequest) -> Result<Value, RepositoryError> {
        r.validate_section_parameters()?;
        if r.section.as_deref() == Some("text") {
            let source = self
                .document_source
                .as_ref()
                .ok_or_else(|| invalid("media_text_unsaved"))?;
            return crate::multimedia::text::page(source, &value(&self.document)?, r);
        }
        let section = r.section.as_deref().unwrap_or("overview");
        let offset = r.offset.unwrap_or(0);
        let limit = r.limit.unwrap_or(20);
        if limit == 0 || limit > 100 {
            return Err(invalid("media_page_limit_invalid"));
        }
        let (mut doc, counts) = match &self.document {
            MultimediaDocument::Comic {
                title,
                format,
                pages,
                ..
            } => (
                json!({"kind":"comic","title":title,"format":format}),
                json!({"pages":pages.len(),"panels":pages.iter().map(|p|p.panels.len()).sum::<usize>(),"balloons":pages.iter().flat_map(|p|&p.panels).map(|p|p.balloons.len()).sum::<usize>(),"assets":self.assets.len(),"sources":self.sources.len(),"editionSources":self.edition.sources.len()}),
            ),
            MultimediaDocument::Animation {
                production,
                title,
                width,
                height,
                frame_rate,
                shots,
                timeline,
                ..
            } => (
                json!({"kind":"animation","title":title,"production":production,"width":width,"height":height,"frameRate":frame_rate}),
                json!({"shots":shots.len(),"tracks":timeline.tracks.len(),"clips":timeline.tracks.iter().map(|t|t.clips.len()).sum::<usize>(),"assets":self.assets.len(),"sources":self.sources.len(),"editionSources":self.edition.sources.len()}),
            ),
        };
        doc["textFields"] = json!(self.document.creative_design().map(|text| crate::multimedia::text::descriptor(MediaTextField::CreativeDesign, text)).into_iter().collect::<Vec<_>>());
        let mut response = json!({"section":section,"project":{"id":self.project.id,"title":self.project.title},"edition":{"id":self.edition.id,"editionId":self.edition.edition_id,"projectId":self.edition.project_id,"scopeId":self.edition.scope_id,"branchId":self.edition.branch_id,"medium":self.edition.medium,"title":self.edition.title,"canonSnapshotId":self.edition.canon_snapshot_id},"revision":self.revision,"revisionId":self.revision_id,"documentSource":self.document_source,"headSnapshotId":self.head_snapshot_id,"sourceDigest":self.source_digest,"documentHeader":doc,"counts":counts,"pageId":r.page_id,"panelId":r.panel_id,"trackId":r.track_id});
        let items:Vec<Value>=match (section,&self.document){
            ("overview",_)=>vec![],
            ("assets",_)=>self.assets.iter().map(value).collect::<Result<_,_>>()?,
            ("sources",_)=>self.sources.iter().map(value).collect::<Result<_,_>>()?,
            ("editionSources",_)=>self.edition.sources.iter().map(value).collect::<Result<_,_>>()?,
            ("pages",MultimediaDocument::Comic{pages,..})=>pages.iter().map(|p|json!({"id":p.id,"title":p.title,"width":p.width,"height":p.height,"readingDirection":p.reading_direction,"panelCount":p.panels.len()})).collect(),
            ("panels",MultimediaDocument::Comic{pages,..})=>{
                let page=pages.iter().find(|p|Some(p.id.as_str())==r.page_id.as_deref()).ok_or_else(||invalid("media_page_not_found"))?;
                page.panels.iter().map(|p|json!({"id":p.id,"textFields":[crate::multimedia::text::descriptor(MediaTextField::PanelDescription{page_id:page.id.clone(),panel_id:p.id.clone()},&p.description)],"rect":p.rect,"assetRevisionId":p.asset_revision_id,"referenceAssetRevisionIds":p.reference_asset_revision_ids,"locked":p.locked,"balloonCount":p.balloons.len()})).collect()
            },
            ("balloons",MultimediaDocument::Comic{pages,..})=>{
                let page=pages.iter().find(|p|Some(p.id.as_str())==r.page_id.as_deref()).ok_or_else(||invalid("media_page_not_found"))?;
                let panel=page.panels.iter().find(|p|Some(p.id.as_str())==r.panel_id.as_deref()).ok_or_else(||invalid("media_panel_not_found"))?;
                panel.balloons.iter().map(|b|json!({"id":b.id,"speaker":b.speaker,"rect":b.rect,"kind":b.kind,"textFields":[crate::multimedia::text::descriptor(MediaTextField::BalloonText{page_id:page.id.clone(),panel_id:panel.id.clone(),balloon_id:b.id.clone()},&b.text)]})).collect()
            },
            ("shots",MultimediaDocument::Animation{shots,..})=>shots.iter().map(|s|json!({"id":s.id,"title":s.title,"locked":s.locked,"assetRevisionId":s.asset_revision_id,"referenceAssetRevisionIds":s.reference_asset_revision_ids,"textFields":[crate::multimedia::text::descriptor(MediaTextField::ShotDescription{shot_id:s.id.clone()},&s.description)]})).collect(),
            ("tracks",MultimediaDocument::Animation{timeline,..})=>timeline.tracks.iter().map(|t|json!({"id":t.id,"title":t.title,"kind":t.kind,"muted":t.muted,"clipCount":t.clips.len()})).collect(),
            ("clips",MultimediaDocument::Animation{timeline,..})=>{
                let track=timeline.tracks.iter().find(|t|Some(t.id.as_str())==r.track_id.as_deref()).ok_or_else(||invalid("media_track_not_found"))?;
                track.clips.iter().map(|c|{let mut v=serde_json::to_value(c).expect("typed clip");v.as_object_mut().unwrap().remove("caption");v["textFields"]=json!([crate::multimedia::text::descriptor(MediaTextField::ClipCaption{track_id:track.id.clone(),clip_id:c.id.clone()},&c.caption)]);v}).collect()
            },
            _=>return Err(invalid("media_section_invalid")),
        };
        if offset > items.len() {
            return Err(invalid("media_page_offset_invalid"));
        }
        let mut bytes = response.to_string().len() + 200;
        let mut page = vec![];
        for item in items.iter().skip(offset).take(limit) {
            let size = item.to_string().len() + 1;
            if bytes + size > PAGE_BYTES {
                if page.is_empty() {
                    return Err(invalid("media_entity_too_large_select_smaller_scope"));
                }
                break;
            }
            bytes += size;
            page.push(item.clone());
        }
        let next = offset + page.len();
        response["items"] = json!(page);
        response["offset"] = json!(offset);
        response["total"] = json!(items.len());
        response["nextOffset"] = if next < items.len() {
            json!(next)
        } else {
            Value::Null
        };
        response["partialView"] = json!(true);
        Ok(response)
    }
}

