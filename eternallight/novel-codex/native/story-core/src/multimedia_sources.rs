//! Frozen, exact cross-edition read grants. No media query escalates to Project.
use super::*;
use crate::MediaWorkspacePageRequest;

pub(super) fn grants(
    db: &Connection,
    task: &crate::TaskRecord,
    e: &CreativeEdition,
) -> Result<Vec<MediaSourceRef>, RepositoryError> {
    let snapshot: String = db
        .query_row(
            "SELECT snapshot_id FROM story_media_task_bindings_v1 WHERE task_id=?1",
            [&task.task_id],
            |r| r.get(0),
        )
        .map_err(storage)?;
    let mut sources = e.sources.clone();
    let frozen_canon = canon::at(db, task.project_id.as_str(), e.canon_snapshot_id.as_deref())?;
    if let Some(canon) = frozen_canon.document_source {
        sources.push(canon);
    }
    for entry in frozen_canon.document.entries {
        sources.extend(entry.sources);
    }
    if let Some(revision) = member(db, &snapshot, &e.id)? {
        let digest: String = db
            .query_row(
                "SELECT content_digest FROM story_revisions_v2 WHERE revision_id=?1",
                [&revision],
                |r| r.get(0),
            )
            .map_err(storage)?;
        sources.push(MediaSourceRef {
            project_id: e.project_id.clone(),
            scope_id: e.scope_id.clone(),
            branch_id: e.branch_id.clone(),
            object_id: e.id.clone(),
            revision_id: revision.clone(),
            digest,
            selector: MediaSelector::Whole,
        });
        let mut query=db.prepare("SELECT source_json FROM story_media_source_links_v1 WHERE revision_id=?1 ORDER BY ordinal").map_err(storage)?;
        for row in query
            .query_map([&revision], |r| r.get::<_, String>(0))
            .map_err(storage)?
        {
            sources.push(decode::<MediaSourceLink>(&row.map_err(storage)?)?.source);
        }
        let mut query=db.prepare("SELECT dependency_json FROM story_media_read_dependencies_v1 WHERE revision_id=?1 ORDER BY ordinal").map_err(storage)?;
        for row in query
            .query_map([&revision], |r| r.get::<_, String>(0))
            .map_err(storage)?
        {
            sources.push(decode::<MediaReadDependency>(&row.map_err(storage)?)?.source);
        }
    }
    let mut outputs = db.prepare("SELECT r.revision_id,r.content_digest FROM story_task_outputs_v2 o JOIN story_revisions_v2 r ON r.revision_id=o.revision_id JOIN story_media_revision_scopes_v1 s ON s.revision_id=r.revision_id WHERE o.task_id=?1 AND r.project_id=?2 AND r.object_id=?3 AND s.scope_id=?4 ORDER BY o.ordinal").map_err(storage)?;
    for row in outputs
        .query_map(params![task.task_id, e.project_id, e.id, e.scope_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(storage)?
    {
        let (revision, digest) = row.map_err(storage)?;
        sources.push(MediaSourceRef {
            project_id: e.project_id.clone(),
            scope_id: e.scope_id.clone(),
            branch_id: e.branch_id.clone(),
            object_id: e.id.clone(),
            revision_id: revision,
            digest,
            selector: MediaSelector::Whole,
        });
    }
    // Asset input grants are the frozen library plus this Work's actual ready receipts.
    for revision in operations::allowed_input_assets(db, task, e)? {
        let stored:Option<String>=db.query_row("SELECT metadata_json FROM story_media_assets_v1 WHERE project_id=?1 AND scope_id=?2 AND asset_revision_id=?3",params![e.project_id,e.scope_id,revision],|r|r.get(0)).optional().map_err(storage)?;
        if let Some(stored) = stored {
            let a: MediaAsset = decode(&stored)?;
            sources.push(MediaSourceRef {
                project_id: e.project_id.clone(),
                scope_id: e.scope_id.clone(),
                branch_id: e.branch_id.clone(),
                object_id: a.id,
                revision_id: revision,
                digest: a.digest,
                selector: MediaSelector::Whole,
            });
        }
    }
    Ok(sources)
}
pub(super) fn exact_scope(g: &MediaSourceRef, s: &MediaSourceRef) -> bool {
    g.project_id == s.project_id
        && g.scope_id == s.scope_id
        && g.branch_id == s.branch_id
        && g.object_id == s.object_id
        && g.revision_id == s.revision_id
        && g.digest == s.digest
        && text::contains(&g.selector, &s.selector)
}
fn text_page(
    source: &MediaSourceRef,
    text: &str,
    r: &MediaWorkspacePageRequest,
) -> Result<Value, RepositoryError> {
    let offset = r.offset.unwrap_or(0);
    let limit = r.limit.unwrap_or(4000);
    if limit == 0 || limit > 8000 {
        return Err(fail("media_page_limit_invalid"));
    }
    let chars = text.chars().count();
    if offset > chars {
        return Err(fail("media_page_offset_invalid"));
    }
    let selected: String = text.chars().skip(offset).take(limit).collect();
    let next = offset + selected.chars().count();
    Ok(
        json!({"source":source,"kind":"text","text":selected,"offset":offset,"totalChars":chars,"nextOffset":if next<chars{Some(next)}else{None}}),
    )
}
impl SqliteStoryRepository {
    pub fn read_task_media_source(
        &self,
        task: &crate::TaskRecord,
        source: &MediaSourceRef,
        r: &MediaWorkspacePageRequest,
    ) -> Result<Value, RepositoryError> {
        let tx = self.connection.unchecked_transaction().map_err(storage)?;
        let target: Value = decode(&task.target_json)?;
        let e = media_target(&tx, task.project_id.as_str(), &target)?
            .ok_or(RepositoryError::TaskBindingInvalid)?;
        let e = assert_task_scope(&tx, task, &e.id, Some(&e.branch_id))?;
        if !grants(&tx, task, &e)?
            .iter()
            .any(|g| exact_scope(g, source))
        {
            return Err(RepositoryError::TaskOutputForbidden);
        }
        validate_source(
            &tx,
            task.project_id.as_str(),
            source,
            MediaFreshness::Pinned,
        )?;
        let(kind,content):(String,String)=tx.query_row("SELECT object_kind,content_json FROM story_revisions_v2 WHERE project_id=?1 AND revision_id=?2 AND object_id=?3",params![task.project_id.as_str(),source.revision_id,source.object_id],|row|Ok((row.get(0)?,row.get(1)?))).map_err(storage)?;
        let data: Value = decode(&content)?;
        r.validate_section_parameters()?;
        if matches!(source.selector, MediaSelector::FieldText { .. })
            && r.section.as_deref() != Some("text")
        {
            return Err(fail("media_text_section_required"));
        }
        let result = if r.section.as_deref() == Some("text") {
            if !matches!(kind.as_str(), "comic" | "animation" | "canon") {
                return Err(fail("media_source_text_unavailable"));
            }
            let mut result = text::page(source, &data, r)?;
            result["version"] = json!("source");
            result
        } else if kind == "mediaAsset" {
            json!({"source":source,"kind":"assetMetadata","asset":data,"visualInspection":false})
        } else if matches!(kind.as_str(), "comic" | "animation") {
            let document: MultimediaDocument = decode(&content)?;
            match &source.selector {
                MediaSelector::Whole => {
                    let edition = edition(
                        &tx,
                        task.project_id.as_str(),
                        &source.object_id,
                        Some(&source.branch_id),
                    )?;
                    let snapshot:Option<String>=tx.query_row("WITH RECURSIVE chain(id) AS (SELECT head_snapshot_id FROM story_branches_v2 WHERE project_id=?1 AND branch_id=?2 UNION SELECT s.parent_snapshot_id FROM story_snapshots_v2 s JOIN chain c ON s.snapshot_id=c.id WHERE s.parent_snapshot_id IS NOT NULL) SELECT m.snapshot_id FROM chain c JOIN story_snapshot_members_v2 m ON m.snapshot_id=c.id WHERE m.object_id=?3 AND m.revision_id=?4 LIMIT 1",params![task.project_id.as_str(),source.branch_id,source.object_id,source.revision_id],|r|r.get(0)).optional().map_err(storage)?;
                    let snapshot = snapshot.ok_or(RepositoryError::CorruptState)?;
                    let mut selected = workspace_at(
                        &tx,
                        task.project_id.as_str(),
                        &edition.id,
                        Some(&edition.branch_id),
                        Some(&snapshot),
                        Some(&[]),
                    )?
                    .page(r)?;
                    selected["source"] = json!(source);
                    selected
                }
                MediaSelector::Panel {
                    page_id,
                    panel_id,
                    region,
                } => {
                    let MultimediaDocument::Comic { pages, .. } = document else {
                        return Err(fail("media_selector_invalid"));
                    };
                    let page = pages
                        .iter()
                        .find(|p| p.id == *page_id)
                        .ok_or_else(|| fail("media_page_not_found"))?;
                    let panel = page
                        .panels
                        .iter()
                        .find(|p| p.id == *panel_id)
                        .ok_or_else(|| fail("media_panel_not_found"))?;
                    // A selected panel grants no sibling pages/panels.
                    if let Some(region) = region {
                        json!({"source":source,"kind":"panelRegion","pageId":page.id,"panelId":panel.id,"region":region,"assetRevisionId":panel.asset_revision_id,"visualInspection":false})
                    } else if r.section.as_deref() == Some("balloons") {
                        let offset = r.offset.unwrap_or(0);
                        let limit = r.limit.unwrap_or(20);
                        if limit == 0 || limit > 100 || offset > panel.balloons.len() {
                            return Err(fail("media_page_limit_invalid"));
                        }
                        let items = panel
                            .balloons
                            .iter()
                            .skip(offset)
                            .take(limit)
                            .map(|b|json!({"id":b.id,"speaker":b.speaker,"rect":b.rect,"kind":b.kind,"textFields":[text::descriptor(MediaTextField::BalloonText{page_id:page.id.clone(),panel_id:panel.id.clone(),balloon_id:b.id.clone()},&b.text)]}))
                            .collect::<Vec<_>>();
                        let next = offset + items.len();
                        json!({"source":source,"kind":"balloons","items":items,"offset":offset,"nextOffset":if next<panel.balloons.len(){Some(next)}else{None}})
                    } else {
                        json!({"source":source,"kind":"panel","panel":{"id":panel.id,"rect":panel.rect,"locked":panel.locked,"assetRevisionId":panel.asset_revision_id,"referenceAssetRevisionIds":panel.reference_asset_revision_ids,"balloonCount":panel.balloons.len(),"textFields":[text::descriptor(MediaTextField::PanelDescription{page_id:page.id.clone(),panel_id:panel.id.clone()},&panel.description)]},"page":{"id":page.id,"width":page.width,"height":page.height,"readingDirection":page.reading_direction}})
                    }
                }
                MediaSelector::Shot { shot_id, range } => {
                    let MultimediaDocument::Animation { shots, .. } = document else {
                        return Err(fail("media_selector_invalid"));
                    };
                    let shot = shots
                        .iter()
                        .find(|s| s.id == *shot_id)
                        .ok_or_else(|| fail("media_shot_not_found"))?;
                    if let Some(range) = range {
                        json!({"source":source,"kind":"shotRange","shotId":shot.id,"assetRevisionId":shot.asset_revision_id,"range":range,"visualInspection":false})
                    } else {
                        json!({"source":source,"kind":"shot","shot":{"id":shot.id,"title":shot.title,"locked":shot.locked,"assetRevisionId":shot.asset_revision_id,"referenceAssetRevisionIds":shot.reference_asset_revision_ids,"textFields":[text::descriptor(MediaTextField::ShotDescription{shot_id:shot.id.clone()},&shot.description)]}})
                    }
                }
                MediaSelector::Clip {
                    track_id,
                    clip_id,
                    range,
                } => {
                    let MultimediaDocument::Animation { timeline, .. } = document else {
                        return Err(fail("media_selector_invalid"));
                    };
                    let track = timeline
                        .tracks
                        .iter()
                        .find(|t| t.id == *track_id)
                        .ok_or_else(|| fail("media_track_not_found"))?;
                    let clip = track
                        .clips
                        .iter()
                        .find(|c| c.id == *clip_id)
                        .ok_or_else(|| fail("media_clip_not_found"))?;
                    if let Some(range) = range {
                        json!({"source":source,"kind":"clipRange","clipId":clip.id,"trackId":track.id,"shotId":clip.shot_id,"assetRevisionId":clip.asset_revision_id,"range":range,"visualInspection":false})
                    } else {
                        let mut metadata = serde_json::to_value(clip)
                            .map_err(|_| RepositoryError::CorruptState)?;
                        metadata
                            .as_object_mut()
                            .ok_or(RepositoryError::CorruptState)?
                            .remove("caption");
                        metadata["textFields"] = json!([text::descriptor(
                            MediaTextField::ClipCaption {
                                track_id: track.id.clone(),
                                clip_id: clip.id.clone()
                            },
                            &clip.caption
                        )]);
                        json!({"source":source,"kind":"clip","clip":metadata,"track":{"id":track.id,"kind":track.kind}})
                    }
                }
                MediaSelector::FieldText { .. } => return Err(fail("media_text_section_required")),
                _ => return Err(fail("media_selector_invalid")),
            }
        } else if kind == "canon" {
            canon::page_document(&decode::<CanonDocument>(&content)?, Some(source), r)?
        } else if let Some(body) = data["body"].as_str() {
            let selected = match &source.selector {
                MediaSelector::Whole => body,
                MediaSelector::Text {
                    start_byte,
                    end_byte,
                    ..
                } => body
                    .get(*start_byte as usize..*end_byte as usize)
                    .ok_or_else(|| fail("media_selector_invalid"))?,
                _ => return Err(fail("media_selector_invalid")),
            };
            text_page(source, selected, r)?
        } else {
            json!({"source":source,"kind":kind,"content":data})
        };
        if result.to_string().len() > 64 * 1024 {
            return Err(fail("media_entity_too_large_select_smaller_scope"));
        }
        tx.commit().map_err(storage)?;
        Ok(result)
    }
}
