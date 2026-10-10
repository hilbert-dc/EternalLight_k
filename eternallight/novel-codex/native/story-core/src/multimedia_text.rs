//! Exact text windows and edits over immutable revisions, never over an inferred HEAD.
use super::*;

pub(crate) fn hash(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}
fn entity<'a>(items: &'a Value, key: &str) -> Result<&'a Value, RepositoryError> {
    validate::key(key)?;
    items
        .as_array()
        .and_then(|items| items.iter().find(|v| v["id"].as_str() == Some(key)))
        .ok_or_else(|| fail("media_text_field_not_found"))
}
pub(crate) fn value<'a>(
    doc: &'a Value,
    field: &MediaTextField,
) -> Result<&'a str, RepositoryError> {
    use MediaTextField::*;
    let selected = match field {
        BalloonText {
            page_id,
            panel_id,
            balloon_id,
        } => {
            let page = entity(&doc["pages"], page_id)?;
            let panel = entity(&page["panels"], panel_id)?;
            &entity(&panel["balloons"], balloon_id)?["text"]
        }
        PanelDescription { page_id, panel_id } => {
            &entity(&entity(&doc["pages"], page_id)?["panels"], panel_id)?["description"]
        }
        ShotDescription { shot_id } => &entity(&doc["shots"], shot_id)?["description"],
        ClipCaption { track_id, clip_id } => &entity(
            &entity(&doc["timeline"]["tracks"], track_id)?["clips"],
            clip_id,
        )?["caption"],
        CanonStatement { entry_id } => &entity(&doc["entries"], entry_id)?["statement"],
    };
    selected
        .as_str()
        .ok_or_else(|| fail("media_text_field_not_found"))
}
pub(crate) fn slice(text: &str, start: usize, end: usize) -> Result<String, RepositoryError> {
    if start > end || end > text.chars().count() {
        return Err(fail("media_text_range_invalid"));
    }
    Ok(text.chars().skip(start).take(end - start).collect())
}
/// Parent prose selections may authorize descendants; image/time selections never do.
pub(crate) fn contains(grant: &MediaSelector, selected: &MediaSelector) -> bool {
    if *grant == MediaSelector::Whole || grant == selected {
        return true;
    }
    let MediaSelector::FieldText {
        field,
        start_char,
        end_char,
        ..
    } = selected
    else {
        return false;
    };
    match (grant, field) {
        (
            MediaSelector::Panel {
                page_id: a,
                panel_id: b,
                region: None,
            },
            MediaTextField::PanelDescription { page_id, panel_id }
            | MediaTextField::BalloonText {
                page_id, panel_id, ..
            },
        ) => a == page_id && b == panel_id,
        (
            MediaSelector::Shot {
                shot_id: a,
                range: None,
            },
            MediaTextField::ShotDescription { shot_id },
        ) => a == shot_id,
        (
            MediaSelector::Clip {
                track_id: a,
                clip_id: b,
                range: None,
            },
            MediaTextField::ClipCaption { track_id, clip_id },
        ) => a == track_id && b == clip_id,
        (
            MediaSelector::FieldText {
                field: a,
                start_char: lo,
                end_char: hi,
                ..
            },
            _,
        ) => a == field && lo <= start_char && hi >= end_char,
        _ => false,
    }
}
pub(crate) fn validate_selector(doc: &Value, s: &MediaSelector) -> Result<(), RepositoryError> {
    let MediaSelector::FieldText {
        field,
        start_char,
        end_char,
        quote,
    } = s
    else {
        return Err(fail("invalid_media_selector"));
    };
    if quote.chars().count() > 8000 {
        return Err(fail("media_source_quote_limit"));
    }
    if slice(value(doc, field)?, *start_char, *end_char)? != *quote {
        return Err(fail("media_source_text_mismatch"));
    }
    Ok(())
}
pub(crate) fn descriptor(field: MediaTextField, text: &str) -> Value {
    json!({"field":field,"totalChars":text.chars().count(),"textDigest":hash(text),"readSection":"text","unit":"unicodeScalar"})
}
pub(crate) fn page(
    source: &MediaSourceRef,
    doc: &Value,
    r: &crate::MediaWorkspacePageRequest,
) -> Result<Value, RepositoryError> {
    r.validate_text_parameters()?;
    let field = r
        .text_field
        .as_ref()
        .ok_or_else(|| fail("media_text_field_required"))?;
    let (probe_start, probe_end) = match &source.selector {
        MediaSelector::FieldText {
            start_char,
            end_char,
            ..
        } => (*start_char, *end_char),
        _ => (0, 0),
    };
    let probe = MediaSelector::FieldText {
        field: field.clone(),
        start_char: probe_start,
        end_char: probe_end,
        quote: String::new(),
    };
    if !contains(&source.selector, &probe) {
        return Err(RepositoryError::TaskOutputForbidden);
    }
    let text = value(doc, field)?;
    let full_len = text.chars().count();
    let (lo, hi) = match &source.selector {
        MediaSelector::FieldText {
            field: granted,
            start_char,
            end_char,
            ..
        } if granted == field => (*start_char, *end_char),
        _ => (0, full_len),
    };
    let selection = MediaSelector::FieldText {
        field: field.clone(),
        start_char: lo,
        end_char: hi,
        quote: slice(text, lo, hi)?,
    };
    if !contains(&source.selector, &selection) {
        return Err(RepositoryError::TaskOutputForbidden);
    }
    let offset = r.char_offset.unwrap_or(0);
    let limit = r.char_limit.unwrap_or(4000);
    if offset > hi - lo {
        return Err(fail("media_text_offset_invalid"));
    }
    let start = lo + offset;
    let count = limit.min(hi - start);
    let make = |count: usize| -> Result<Value, RepositoryError> {
        let end = start + count;
        let text_page = slice(text, start, end)?;
        let mut returned = source.clone();
        returned.selector = MediaSelector::FieldText {
            field: field.clone(),
            start_char: start,
            end_char: end,
            quote: text_page.clone(),
        };
        Ok(
            json!({"section":"text","kind":"text","version":"working","partialView":true,"unit":"unicodeScalar","source":returned,"returnedSource":returned,"field":field,"revisionId":source.revision_id,"text":text_page,"textDigest":hash(text),"sliceDigest":hash(&text_page),"startChar":start,"endChar":end,"totalChars":hi-lo,"authorizedStartChar":lo,"authorizedEndChar":hi,"charOffset":offset,"nextCharOffset":if end<hi {Some(end-lo)} else {None}}),
        )
    };
    let mut low = 0;
    let mut high = count;
    // Count actual JSON bytes, including escaped text and exact source quotes.
    while low < high {
        let mid = low + (high - low + 1) / 2;
        if encode(&make(mid)?)?.len() <= 64 * 1024 {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    let result = make(low)?;
    if encode(&result)?.len() > 64 * 1024 || (count > 0 && low == 0) {
        return Err(fail("media_text_page_metadata_too_large"));
    }
    Ok(result)
}
pub(crate) fn replace(
    text: &str,
    expected: &str,
    edits: &[MediaTextEdit],
) -> Result<String, RepositoryError> {
    if hash(text) != expected {
        return Err(fail("media_text_digest_changed"));
    }
    if edits.is_empty() || edits.len() > 32 {
        return Err(fail("media_text_edit_limit"));
    }
    let mut sorted = edits.iter().collect::<Vec<_>>();
    sorted.sort_by_key(|e| (e.start_char, e.end_char));
    for (i, edit) in sorted.iter().enumerate() {
        let selected = slice(text, edit.start_char, edit.end_char)?;
        if hash(&selected) != edit.expected_slice_digest {
            return Err(fail("media_text_slice_changed"));
        }
        if edit.replacement.contains('\0') {
            return Err(fail("invalid_media_text"));
        }
        if i > 0
            && (sorted[i - 1].end_char > edit.start_char
                || sorted[i - 1].start_char == edit.start_char)
        {
            return Err(fail("media_text_ranges_overlap"));
        }
    }
    let mut chars = text.chars().collect::<Vec<_>>();
    for edit in sorted.into_iter().rev() {
        chars.splice(edit.start_char..edit.end_char, edit.replacement.chars());
    }
    Ok(chars.into_iter().collect())
}
pub(crate) fn apply(
    doc: &mut MultimediaDocument,
    field: &MediaTextField,
    expected: &str,
    edits: &[MediaTextEdit],
) -> Result<(), RepositoryError> {
    use MediaTextField::*;
    let target = match (doc, field) {
        (
            MultimediaDocument::Comic { pages, .. },
            BalloonText {
                page_id,
                panel_id,
                balloon_id,
            },
        ) => {
            let panel = pages
                .iter_mut()
                .find(|v| v.id == *page_id)
                .and_then(|p| p.panels.iter_mut().find(|v| v.id == *panel_id))
                .ok_or_else(|| fail("media_text_field_not_found"))?;
            &mut panel
                .balloons
                .iter_mut()
                .find(|v| v.id == *balloon_id)
                .ok_or_else(|| fail("media_text_field_not_found"))?
                .text
        }
        (MultimediaDocument::Comic { pages, .. }, PanelDescription { page_id, panel_id }) => {
            &mut pages
                .iter_mut()
                .find(|v| v.id == *page_id)
                .and_then(|p| p.panels.iter_mut().find(|v| v.id == *panel_id))
                .ok_or_else(|| fail("media_text_field_not_found"))?
                .description
        }
        (MultimediaDocument::Animation { shots, .. }, ShotDescription { shot_id }) => {
            &mut shots
                .iter_mut()
                .find(|v| v.id == *shot_id)
                .ok_or_else(|| fail("media_text_field_not_found"))?
                .description
        }
        (MultimediaDocument::Animation { timeline, .. }, ClipCaption { track_id, clip_id }) => {
            &mut timeline
                .tracks
                .iter_mut()
                .find(|v| v.id == *track_id)
                .and_then(|t| t.clips.iter_mut().find(|v| v.id == *clip_id))
                .ok_or_else(|| fail("media_text_field_not_found"))?
                .caption
        }
        _ => return Err(RepositoryError::TaskOutputForbidden),
    };
    *target = replace(target, expected, edits)?;
    Ok(())
}
