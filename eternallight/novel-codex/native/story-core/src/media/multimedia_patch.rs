//! Bounded typed edits reuse the document revision transaction and its authority.
use super::*;

pub(crate) const MAX_PATCH_BYTES: usize = 256 * 1024;

pub(crate) fn validate_request(r: &PatchMultimediaDocument) -> Result<(), RepositoryError> {
    validate::key(&r.idempotency_key)?;
    if r.operations.is_empty() || r.operations.len() > 100 {
        return Err(fail("media_patch_operation_limit"));
    }
    if r.read_set.len() > 500 || r.sources.len() > 1000 {
        return Err(fail("media_source_limit"));
    }
    if encode(r)?.len() > MAX_PATCH_BYTES {
        return Err(fail("media_patch_too_large"));
    }
    Ok(())
}

fn index<T>(items: &[T], id: &str, key: impl Fn(&T) -> &str) -> Result<usize, RepositoryError> {
    validate::key(id)?;
    items
        .iter()
        .position(|v| key(v) == id)
        .ok_or_else(|| fail("media_patch_object_not_found"))
}
fn remove<T>(
    items: &mut Vec<T>,
    id: &str,
    key: impl Fn(&T) -> &str,
) -> Result<(), RepositoryError> {
    let i = index(items, id, key)?;
    items.remove(i);
    Ok(())
}
fn move_before<T>(
    items: &mut Vec<T>,
    id: &str,
    before: &Option<String>,
    key: impl Fn(&T) -> &str,
) -> Result<(), RepositoryError> {
    let from = index(items, id, &key)?;
    let to = match before {
        Some(before) => {
            if before == id {
                return Err(fail("media_patch_move_self"));
            }
            index(items, before, &key)?
        }
        None => items.len(),
    };
    let item = items.remove(from);
    items.insert(if from < to { to - 1 } else { to }, item);
    Ok(())
}
fn pages(document: &mut MultimediaDocument) -> Result<&mut Vec<ComicPage>, RepositoryError> {
    match document {
        MultimediaDocument::Comic { pages, .. } => Ok(pages),
        _ => Err(fail("media_kind_mismatch")),
    }
}
fn page<'a>(
    document: &'a mut MultimediaDocument,
    id: &str,
) -> Result<&'a mut ComicPage, RepositoryError> {
    let pages = pages(document)?;
    let i = index(pages, id, |p| &p.id)?;
    Ok(&mut pages[i])
}
fn panel<'a>(
    document: &'a mut MultimediaDocument,
    page_id: &str,
    panel_id: &str,
) -> Result<&'a mut ComicPanel, RepositoryError> {
    let panels = &mut page(document, page_id)?.panels;
    let i = index(panels, panel_id, |p| &p.id)?;
    Ok(&mut panels[i])
}
fn shots(document: &mut MultimediaDocument) -> Result<&mut Vec<AnimationShot>, RepositoryError> {
    match document {
        MultimediaDocument::Animation { shots, .. } => Ok(shots),
        _ => Err(fail("media_kind_mismatch")),
    }
}
fn tracks(document: &mut MultimediaDocument) -> Result<&mut Vec<AnimationTrack>, RepositoryError> {
    match document {
        MultimediaDocument::Animation { timeline, .. } => Ok(&mut timeline.tracks),
        _ => Err(fail("media_kind_mismatch")),
    }
}
fn track<'a>(
    document: &'a mut MultimediaDocument,
    id: &str,
) -> Result<&'a mut AnimationTrack, RepositoryError> {
    let tracks = tracks(document)?;
    let i = index(tracks, id, |t| &t.id)?;
    Ok(&mut tracks[i])
}

pub(crate) struct AppliedPatch {
    pub request: SaveMultimediaDocument,
    pub canon_snapshot_id: String,
    pub edition_sources: Vec<MediaSourceRef>,
}
pub(crate) fn apply(
    db: &Connection,
    current: &MultimediaWorkspace,
    r: &PatchMultimediaDocument,
    author: bool,
) -> Result<AppliedPatch, RepositoryError> {
    let mut edition_sources = current.edition.sources.clone();
    let mut document = current.document.clone();
    let mut sources = current.sources.clone();
    let mut canon_snapshot_id = current
        .edition
        .canon_snapshot_id
        .clone()
        .ok_or(RepositoryError::CorruptState)?;
    for operation in &r.operations {
        use MediaDocumentOperation::*;
        match operation {
            ReplaceTextRanges {
                field,
                expected_text_digest,
                edits,
            } => {
                if r.operations
                    .iter()
                    .filter(|op| matches!(op, ReplaceTextRanges { field: f, .. } if f == field))
                    .count()
                    != 1
                {
                    return Err(fail("media_text_field_repeated"));
                }
                text::apply(&mut document, field, expected_text_digest, edits)?;
            }
            AdoptCanonSnapshot {
                canon_snapshot_id: next,
            } => {
                if !author {
                    return Err(RepositoryError::TaskOutputForbidden);
                }
                validate::key(next)?;
                canon::at(db, &current.project.id, Some(next))?;
                canon_snapshot_id = next.clone();
            }
            ReplaceEditionSource { previous, next } => {
                if !author { return Err(RepositoryError::TaskOutputForbidden); }
                if !edition_sources.contains(previous) { return Err(fail("media_source_baseline_changed")); }
                if previous.project_id != next.project_id || previous.scope_id != next.scope_id
                    || previous.branch_id != next.branch_id || previous.object_id != next.object_id
                    || std::mem::discriminant(&previous.selector) != std::mem::discriminant(&next.selector) {
                    return Err(fail("media_source_replacement_scope_mismatch"));
                }
                validate_source(db, &current.project.id, next, MediaFreshness::Pinned)?;
                if previous == next { return Err(fail("media_source_replacement_unchanged")); }
                for source in &mut edition_sources { if source == previous { *source = next.clone(); } }
                let mut unique = Vec::new();
                for source in edition_sources.drain(..) { if !unique.contains(&source) { unique.push(source); } }
                edition_sources = unique;
            }
            SetScreenProduction { production: next } => match &mut document {
                MultimediaDocument::Animation { production, .. } => *production = next.clone(),
                _ => return Err(fail("media_kind_mismatch")),
            },
            SetCreativeDesign { text } => match &mut document {
                MultimediaDocument::Comic { creative_design, .. }
                | MultimediaDocument::Animation { creative_design, .. } => *creative_design = text.clone(),
            },
            SetTitle { title: next } => match &mut document {
                MultimediaDocument::Comic { title, .. }
                | MultimediaDocument::Animation { title, .. } => *title = next.clone(),
            },
            SetComicFormat { format: next } => match &mut document {
                MultimediaDocument::Comic { format, .. } => *format = next.clone(),
                _ => return Err(fail("media_kind_mismatch")),
            },
            UpsertPage { page: v } => {
                let pages = pages(&mut document)?;
                if let Some(p) = pages.iter_mut().find(|p| p.id == v.id) {
                    p.title = v.title.clone();
                    p.width = v.width;
                    p.height = v.height;
                    p.reading_direction = v.reading_direction.clone();
                } else {
                    pages.push(ComicPage {
                        id: v.id.clone(),
                        title: v.title.clone(),
                        width: v.width,
                        height: v.height,
                        reading_direction: v.reading_direction.clone(),
                        panels: vec![],
                    });
                }
            }
            RemovePage { page_id } => remove(pages(&mut document)?, page_id, |p| &p.id)?,
            MovePage { page_id, before_id } => {
                move_before(pages(&mut document)?, page_id, before_id, |p| &p.id)?
            }
            UpsertPanel { page_id, panel: v } => {
                let panels = &mut page(&mut document, page_id)?.panels;
                if let Some(p) = panels.iter_mut().find(|p| p.id == v.id) {
                    p.description = v.description.clone();
                    p.rect = v.rect.clone();
                    p.asset_revision_id = v.asset_revision_id.clone();
                    p.reference_asset_revision_ids = v.reference_asset_revision_ids.clone();
                } else {
                    panels.push(ComicPanel {
                        id: v.id.clone(),
                        description: v.description.clone(),
                        rect: v.rect.clone(),
                        asset_revision_id: v.asset_revision_id.clone(),
                        reference_asset_revision_ids: v.reference_asset_revision_ids.clone(),
                        balloons: vec![],
                        locked: false,
                    });
                }
            }
            RemovePanel { page_id, panel_id } => {
                remove(&mut page(&mut document, page_id)?.panels, panel_id, |p| {
                    &p.id
                })?
            }
            MovePanel {
                page_id,
                panel_id,
                before_id,
            } => move_before(
                &mut page(&mut document, page_id)?.panels,
                panel_id,
                before_id,
                |p| &p.id,
            )?,
            SetPanelLock {
                page_id,
                panel_id,
                locked,
            } => {
                if !author {
                    return Err(RepositoryError::TaskOutputForbidden);
                }
                panel(&mut document, page_id, panel_id)?.locked = *locked;
            }
            UpsertBalloon {
                page_id,
                panel_id,
                balloon,
            } => {
                let balloons = &mut panel(&mut document, page_id, panel_id)?.balloons;
                if let Some(v) = balloons.iter_mut().find(|v| v.id == balloon.id) {
                    *v = balloon.clone();
                } else {
                    balloons.push(balloon.clone());
                }
            }
            RemoveBalloon {
                page_id,
                panel_id,
                balloon_id,
            } => remove(
                &mut panel(&mut document, page_id, panel_id)?.balloons,
                balloon_id,
                |v| &v.id,
            )?,
            MoveBalloon {
                page_id,
                panel_id,
                balloon_id,
                before_id,
            } => move_before(
                &mut panel(&mut document, page_id, panel_id)?.balloons,
                balloon_id,
                before_id,
                |v| &v.id,
            )?,
            SetAnimationSettings {
                width: w,
                height: h,
                frame_rate: f,
            } => match &mut document {
                MultimediaDocument::Animation {
                    width,
                    height,
                    frame_rate,
                    ..
                } => {
                    *width = *w;
                    *height = *h;
                    *frame_rate = f.clone();
                }
                _ => return Err(fail("media_kind_mismatch")),
            },
            UpsertShot { shot: v } => {
                let shots = shots(&mut document)?;
                if let Some(s) = shots.iter_mut().find(|s| s.id == v.id) {
                    s.title = v.title.clone();
                    s.description = v.description.clone();
                    s.asset_revision_id = v.asset_revision_id.clone();
                    s.reference_asset_revision_ids = v.reference_asset_revision_ids.clone();
                } else {
                    shots.push(AnimationShot {
                        id: v.id.clone(),
                        title: v.title.clone(),
                        description: v.description.clone(),
                        asset_revision_id: v.asset_revision_id.clone(),
                        reference_asset_revision_ids: v.reference_asset_revision_ids.clone(),
                        locked: false,
                    });
                }
            }
            RemoveShot { shot_id } => remove(shots(&mut document)?, shot_id, |s| &s.id)?,
            MoveShot { shot_id, before_id } => {
                move_before(shots(&mut document)?, shot_id, before_id, |s| &s.id)?
            }
            SetShotLock { shot_id, locked } => {
                if !author {
                    return Err(RepositoryError::TaskOutputForbidden);
                }
                let shots = shots(&mut document)?;
                let i = index(shots, shot_id, |s| &s.id)?;
                shots[i].locked = *locked;
            }
            UpsertTrack { track: v } => {
                let tracks = tracks(&mut document)?;
                if let Some(t) = tracks.iter_mut().find(|t| t.id == v.id) {
                    t.title = v.title.clone();
                    t.kind = v.kind.clone();
                    t.muted = v.muted;
                } else {
                    tracks.push(AnimationTrack {
                        id: v.id.clone(),
                        title: v.title.clone(),
                        kind: v.kind.clone(),
                        muted: v.muted,
                        clips: vec![],
                    });
                }
            }
            RemoveTrack { track_id } => remove(tracks(&mut document)?, track_id, |t| &t.id)?,
            MoveTrack {
                track_id,
                before_id,
            } => move_before(tracks(&mut document)?, track_id, before_id, |t| &t.id)?,
            UpsertClip { track_id, clip } => {
                let clips = &mut track(&mut document, track_id)?.clips;
                if let Some(v) = clips.iter_mut().find(|v| v.id == clip.id) {
                    *v = clip.clone();
                } else {
                    clips.push(clip.clone());
                }
            }
            RemoveClip { track_id, clip_id } => {
                remove(&mut track(&mut document, track_id)?.clips, clip_id, |v| {
                    &v.id
                })?
            }
            MoveClip {
                track_id,
                clip_id,
                before_id,
            } => move_before(
                &mut track(&mut document, track_id)?.clips,
                clip_id,
                before_id,
                |v| &v.id,
            )?,
            RemoveSourceLink { link } => {
                let i = sources
                    .iter()
                    .position(|v| v == link)
                    .ok_or_else(|| fail("media_source_link_not_found"))?;
                sources.remove(i);
            }
        }
    }
    let mut read_set = Vec::<MediaReadDependency>::new();
    if let Some(revision) = &current.revision_id {
        let mut statement = db.prepare("SELECT dependency_json FROM story_media_read_dependencies_v1 WHERE revision_id=?1 ORDER BY ordinal").map_err(storage)?;
        for row in statement
            .query_map([revision], |row| row.get::<_, String>(0))
            .map_err(storage)?
        {
            read_set.push(decode(&row.map_err(storage)?)?);
        }
    }
    for dependency in &r.read_set {
        if let Some(v) = read_set.iter_mut().find(|v| v.source == dependency.source) {
            v.freshness = dependency.freshness;
        } else {
            read_set.push(dependency.clone());
        }
    }
    for link in &r.sources {
        if !sources.contains(link) {
            sources.push(link.clone());
        }
    }
    Ok(AppliedPatch {
        request: SaveMultimediaDocument {
            expected_revision_id: r.expected_revision_id.clone(),
            document,
            idempotency_key: r.idempotency_key.clone(),
            read_set,
            sources,
        },
        canon_snapshot_id,
        edition_sources,
    })
}
