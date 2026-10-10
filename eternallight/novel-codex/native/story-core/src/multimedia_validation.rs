use crate::{RepositoryError, multimedia_types::*};
use std::collections::HashSet;

pub(crate) fn invalid(code: &str) -> RepositoryError {
    RepositoryError::InvalidContent(code.into())
}
pub(crate) fn key(value: &str) -> Result<(), RepositoryError> {
    if value.is_empty() || value.len() > 160 || value.chars().any(char::is_control) {
        return Err(invalid("invalid_media_identity"));
    }
    Ok(())
}
pub(crate) fn text(value: &str, max: usize, nonempty: bool) -> Result<(), RepositoryError> {
    if (nonempty && value.trim().is_empty()) || value.chars().count() > max || value.contains('\0')
    {
        return Err(invalid("invalid_media_text"));
    }
    Ok(())
}
pub(crate) fn rational(v: &Rational) -> Result<(), RepositoryError> {
    if v.numerator <= 0
        || v.denominator <= 0
        || v.numerator > 1_000_000
        || v.denominator > 1_000_000
    {
        return Err(invalid("invalid_media_rate"));
    }
    Ok(())
}
pub(crate) fn time(v: &MediaTime, positive: bool) -> Result<(), RepositoryError> {
    rational(&v.rate)?;
    if v.value < 0 || (positive && v.value == 0) || v.value > 1_000_000_000_000 {
        return Err(invalid("invalid_media_time"));
    }
    Ok(())
}
pub(crate) fn rect(v: &MediaRect, width: f64, height: f64) -> Result<(), RepositoryError> {
    if [v.x, v.y, v.width, v.height, width, height]
        .iter()
        .any(|n| !n.is_finite())
        || v.x < 0.0
        || v.y < 0.0
        || v.width <= 0.0
        || v.height <= 0.0
        || v.x + v.width > width + 0.000001
        || v.y + v.height > height + 0.000001
    {
        return Err(invalid("invalid_media_region"));
    }
    Ok(())
}
fn dimensions(w: i64, h: i64) -> Result<(), RepositoryError> {
    if !(1..=100_000).contains(&w) || !(1..=100_000).contains(&h) {
        return Err(invalid("invalid_media_dimensions"));
    }
    Ok(())
}
fn unique(ids: &mut HashSet<String>, id: &str) -> Result<(), RepositoryError> {
    key(id)?;
    if !ids.insert(id.into()) {
        return Err(invalid("duplicate_media_object"));
    }
    Ok(())
}
pub(crate) fn document(v: &MultimediaDocument) -> Result<(), RepositoryError> {
    let mut ids = HashSet::new();
    match v {
        MultimediaDocument::Comic { title, pages, .. } => {
            text(title, 160, true)?;
            if pages.len() > 1000 {
                return Err(invalid("media_document_limit"));
            }
            for page in pages {
                unique(&mut ids, &page.id)?;
                text(&page.title, 160, false)?;
                dimensions(page.width, page.height)?;
                if page.panels.len() > 1000 {
                    return Err(invalid("media_document_limit"));
                }
                for panel in &page.panels {
                    unique(&mut ids, &panel.id)?;
                    text(&panel.description, 20000, false)?;
                    rect(&panel.rect, page.width as f64, page.height as f64)?;
                    if panel.balloons.len() > 1000 || panel.reference_asset_revision_ids.len() > 100
                    {
                        return Err(invalid("media_document_limit"));
                    }
                    for balloon in &panel.balloons {
                        unique(&mut ids, &balloon.id)?;
                        text(&balloon.text, 20000, false)?;
                        text(&balloon.speaker, 160, false)?;
                        rect(&balloon.rect, panel.rect.width, panel.rect.height)?;
                    }
                }
            }
        }
        MultimediaDocument::Animation {
            title,
            width,
            height,
            frame_rate,
            shots,
            timeline,
        } => {
            text(title, 160, true)?;
            dimensions(*width, *height)?;
            rational(frame_rate)?;
            if shots.len() > 10000 || timeline.tracks.len() > 200 {
                return Err(invalid("media_document_limit"));
            }
            let mut shot_ids = HashSet::new();
            for shot in shots {
                unique(&mut ids, &shot.id)?;
                shot_ids.insert(shot.id.as_str());
                text(&shot.title, 160, false)?;
                text(&shot.description, 20000, false)?;
                if shot.reference_asset_revision_ids.len() > 100 {
                    return Err(invalid("media_document_limit"));
                }
            }
            for track in &timeline.tracks {
                unique(&mut ids, &track.id)?;
                text(&track.title, 160, false)?;
                if track.clips.len() > 10000 {
                    return Err(invalid("media_document_limit"));
                }
                for clip in &track.clips {
                    unique(&mut ids, &clip.id)?;
                    time(&clip.start, false)?;
                    time(&clip.duration, true)?;
                    time(&clip.source_start, false)?;
                    rational(&clip.playback_rate)?;
                    text(&clip.caption, 20000, false)?;
                    if clip
                        .shot_id
                        .as_ref()
                        .is_some_and(|id| !shot_ids.contains(id.as_str()))
                    {
                        return Err(invalid("media_shot_not_found"));
                    }
                    if track.kind == TrackKind::Subtitle
                        && (clip.asset_revision_id.is_some() || clip.shot_id.is_some())
                    {
                        return Err(invalid("invalid_subtitle_clip"));
                    }
                }
            }
        }
    }
    Ok(())
}
pub(crate) fn preserve_locks(
    old: &MultimediaDocument,
    new: &MultimediaDocument,
    allow_unlock: bool,
) -> Result<(), RepositoryError> {
    match (old, new) {
        (
            MultimediaDocument::Comic { pages: before, .. },
            MultimediaDocument::Comic { pages: after, .. },
        ) => {
            for page in before {
                for panel in page.panels.iter().filter(|p| p.locked) {
                    let next = after
                        .iter()
                        .find(|p| p.id == page.id)
                        .and_then(|p| p.panels.iter().find(|p| p.id == panel.id));
                    if next.is_none_or(|next| {
                        let mut before = panel.clone();
                        if allow_unlock {
                            before.locked = next.locked;
                        }
                        before != *next
                    }) {
                        return Err(RepositoryError::ObjectLocked);
                    }
                }
            }
        }
        (
            MultimediaDocument::Animation { shots: before, .. },
            MultimediaDocument::Animation { shots: after, .. },
        ) => {
            for shot in before.iter().filter(|s| s.locked) {
                let next = after.iter().find(|s| s.id == shot.id);
                if next.is_none_or(|next| {
                    let mut before = shot.clone();
                    if allow_unlock {
                        before.locked = next.locked;
                    }
                    before != *next
                }) {
                    return Err(RepositoryError::ObjectLocked);
                }
            }
        }
        _ => return Err(invalid("media_kind_mismatch")),
    }
    Ok(())
}
pub(crate) fn asset(v: &MediaAssetInput) -> Result<(), RepositoryError> {
    text(&v.name, 255, true)?;
    if v.digest.len() != 64
        || !v
            .digest
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        || v.bytes <= 0
        || v.bytes > 64 * 1024 * 1024 * 1024
    {
        return Err(invalid("invalid_media_blob"));
    }
    match v.kind {
        MediaAssetKind::Image => {
            if !matches!(
                v.mime_type.as_str(),
                "image/png" | "image/jpeg" | "image/webp" | "image/gif"
            ) {
                return Err(invalid("invalid_media_mime"));
            }
            dimensions(
                v.width.ok_or_else(|| invalid("media_width_required"))?,
                v.height.ok_or_else(|| invalid("media_height_required"))?,
            )?;
            if v.duration.is_some() || v.sample_rate.is_some() || v.channels.is_some() {
                return Err(invalid("invalid_image_metadata"));
            }
        }
        MediaAssetKind::Video => {
            if !matches!(
                v.mime_type.as_str(),
                "video/mp4" | "video/webm" | "video/quicktime"
            ) {
                return Err(invalid("invalid_media_mime"));
            }
            dimensions(
                v.width.ok_or_else(|| invalid("media_width_required"))?,
                v.height.ok_or_else(|| invalid("media_height_required"))?,
            )?;
            time(
                v.duration
                    .as_ref()
                    .ok_or_else(|| invalid("media_duration_required"))?,
                true,
            )?;
        }
        MediaAssetKind::Audio => {
            if !matches!(
                v.mime_type.as_str(),
                "audio/wav"
                    | "audio/x-wav"
                    | "audio/mpeg"
                    | "audio/mp4"
                    | "audio/aac"
                    | "audio/ogg"
                    | "audio/flac"
                    | "audio/webm"
            ) {
                return Err(invalid("invalid_media_mime"));
            }
            time(
                v.duration
                    .as_ref()
                    .ok_or_else(|| invalid("media_duration_required"))?,
                true,
            )?;
            if v.width.is_some() || v.height.is_some() {
                return Err(invalid("invalid_audio_metadata"));
            }
            if v.sample_rate.is_none() || v.channels.is_none() {
                return Err(invalid("media_audio_metadata_required"));
            }
        }
    }
    if v.sample_rate.is_some_and(|n| !(1..=768000).contains(&n))
        || v.channels.is_some_and(|n| !(1..=64).contains(&n))
    {
        return Err(invalid("invalid_audio_metadata"));
    }
    Ok(())
}
pub(crate) fn select_document(
    v: &MultimediaDocument,
    s: &MediaSelector,
) -> Result<(), RepositoryError> {
    match s {
        MediaSelector::Whole => Ok(()),
        MediaSelector::FieldText { .. } => {
            let doc = serde_json::to_value(v).map_err(|_| RepositoryError::CorruptState)?;
            crate::multimedia::text::validate_selector(&doc, s)
        }
        MediaSelector::Panel {
            page_id,
            panel_id,
            region,
        } => match v {
            MultimediaDocument::Comic { pages, .. } => {
                let panel = pages
                    .iter()
                    .find(|p| &p.id == page_id)
                    .and_then(|p| p.panels.iter().find(|p| &p.id == panel_id))
                    .ok_or_else(|| invalid("media_panel_not_found"))?;
                if let Some(r) = region {
                    rect(r, panel.rect.width, panel.rect.height)?;
                }
                Ok(())
            }
            _ => Err(invalid("invalid_media_selector")),
        },
        MediaSelector::Shot { shot_id, range } => match v {
            MultimediaDocument::Animation { shots, .. } => {
                if !shots.iter().any(|s| &s.id == shot_id) {
                    return Err(invalid("media_shot_not_found"));
                }
                if let Some(r) = range {
                    time(&r.start, false)?;
                    time(&r.duration, true)?;
                }
                Ok(())
            }
            _ => Err(invalid("invalid_media_selector")),
        },
        MediaSelector::Clip {
            track_id,
            clip_id,
            range,
        } => match v {
            MultimediaDocument::Animation { timeline, .. } => {
                let clip = timeline
                    .tracks
                    .iter()
                    .find(|t| &t.id == track_id)
                    .and_then(|t| t.clips.iter().find(|c| &c.id == clip_id))
                    .ok_or_else(|| invalid("media_clip_not_found"))?;
                if let Some(r) = range {
                    time(&r.start, false)?;
                    time(&r.duration, true)?;
                    if !within(
                        &r.start,
                        &r.duration,
                        &Rational {
                            numerator: 1,
                            denominator: 1,
                        },
                        &clip.duration,
                    ) {
                        return Err(invalid("media_range_outside_source"));
                    }
                }
                Ok(())
            }
            _ => Err(invalid("invalid_media_selector")),
        },
        _ => Err(invalid("invalid_media_selector")),
    }
}
/// Compare exact rational seconds without floating-point drift.
pub(crate) fn within(
    start: &MediaTime,
    duration: &MediaTime,
    rate: &Rational,
    total: &MediaTime,
) -> bool {
    let a = (start.value as i128) * (start.rate.denominator as i128);
    let ad = start.rate.numerator as i128;
    let b =
        (duration.value as i128) * (duration.rate.denominator as i128) * (rate.numerator as i128);
    let bd = (duration.rate.numerator as i128) * (rate.denominator as i128);
    let c = (total.value as i128) * (total.rate.denominator as i128);
    let cd = total.rate.numerator as i128;
    // Values are bounded; checked arithmetic also rejects malicious limit products.
    a.checked_mul(bd)
        .and_then(|x| b.checked_mul(ad).and_then(|y| x.checked_add(y)))
        .and_then(|x| x.checked_mul(cd))
        .zip(c.checked_mul(ad).and_then(|x| x.checked_mul(bd)))
        .is_some_and(|(left, right)| left <= right)
}
