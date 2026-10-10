//! Read-only relevance comparison of exact frozen references. It never rewrites an edition.
use super::*;

fn selected(value: &Value, selector: &MediaSelector) -> Option<Value> {
    Some(match selector {
        MediaSelector::Whole => value.clone(),
        MediaSelector::FieldText {
            field,
            start_char,
            end_char,
            ..
        } => {
            json!(text::slice(text::value(value, field).ok()?, *start_char, *end_char).ok()?)
        }
        MediaSelector::Text {
            start_byte,
            end_byte,
            ..
        } => json!(value["body"].as_str()?.get(*start_byte..*end_byte)?),
        MediaSelector::Panel {
            page_id,
            panel_id,
            region,
        } => {
            let page = value["pages"]
                .as_array()?
                .iter()
                .find(|p| p["id"].as_str() == Some(page_id))?;
            let panel = page["panels"]
                .as_array()?
                .iter()
                .find(|p| p["id"].as_str() == Some(panel_id))?;
            json!({"panel":panel,"pageWidth":page["width"],"pageHeight":page["height"],"readingDirection":page["readingDirection"],"region":region})
        }
        MediaSelector::Shot { shot_id, range } => {
            json!({"shot":value["shots"].as_array()?.iter().find(|s|s["id"].as_str()==Some(shot_id))?,"range":range})
        }
        MediaSelector::Clip {
            track_id,
            clip_id,
            range,
        } => {
            let track = value["timeline"]["tracks"]
                .as_array()?
                .iter()
                .find(|t| t["id"].as_str() == Some(track_id))?;
            json!({"clip":track["clips"].as_array()?.iter().find(|c|c["id"].as_str()==Some(clip_id))?,"trackKind":track["kind"],"range":range})
        }
        // Asset revisions are immutable. Their currentness is their verified identity.
        MediaSelector::Region { .. } | MediaSelector::Time { .. } => value.clone(),
    })
}
impl SqliteStoryRepository {
    pub fn media_source_impacts(
        &self,
        p: &ProjectId,
        e: &str,
        branch: Option<&str>,
        offset: usize,
        limit: usize,
    ) -> Result<Value, RepositoryError> {
        if limit == 0 || limit > 100 {
            return Err(fail("media_page_limit_invalid"));
        }
        let tx = self.connection.unchecked_transaction().map_err(storage)?;
        let w = workspace(&tx, p.as_str(), e, branch)?;
        let mut refs: Vec<(MediaSourceRef, Vec<Value>)> = vec![];
        let mut add = |s: MediaSourceRef, target: Option<Value>| {
            if let Some((_, targets)) = refs.iter_mut().find(|(v, _)| v == &s) {
                if let Some(t) = target {
                    if !targets.contains(&t) {
                        targets.push(t);
                    }
                }
            } else {
                refs.push((s, target.into_iter().collect()));
            }
        };
        for s in &w.edition.sources {
            add(s.clone(), None);
        }
        for l in &w.sources {
            add(
                l.source.clone(),
                Some(json!({"objectId":l.target_object_id,"selector":l.target_selector})),
            );
        }
        if let Some(r) = &w.revision_id {
            let mut q=tx.prepare("SELECT dependency_json FROM story_media_read_dependencies_v1 WHERE revision_id=?1 ORDER BY ordinal").map_err(storage)?;
            for row in q
                .query_map([r], |r| r.get::<_, String>(0))
                .map_err(storage)?
            {
                add(
                    decode::<MediaReadDependency>(&row.map_err(storage)?)?.source,
                    None,
                );
            }
        }
        if offset > refs.len() {
            return Err(fail("media_page_offset_invalid"));
        }
        let mut items = vec![];
        let mut bytes = 1500;
        for (source, targets) in refs.iter().skip(offset).take(limit) {
            validate_source(&tx, p.as_str(), source, MediaFreshness::Pinned)?;
            let (kind,content):(String,String)=tx.query_row("SELECT object_kind,content_json FROM story_revisions_v2 WHERE project_id=?1 AND revision_id=?2",params![p.as_str(),source.revision_id],|r|Ok((r.get(0)?,r.get(1)?))).map_err(storage)?;
            let current: Option<(String, String, String)> = if kind == "mediaAsset" {
                Some((
                    source.revision_id.clone(),
                    source.digest.clone(),
                    content.clone(),
                ))
            } else {
                tx.query_row("SELECT r.revision_id,r.content_digest,r.content_json FROM story_branches_v2 b JOIN story_snapshot_members_v2 m ON m.snapshot_id=b.head_snapshot_id JOIN story_revisions_v2 r ON r.revision_id=m.revision_id WHERE b.project_id=?1 AND b.branch_id=?2 AND m.object_id=?3",params![p.as_str(),source.branch_id,source.object_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(storage)?
            };
            let (status, latest) = match current {
                None => ("removed", None),
                Some((revision, hash, body)) => {
                    let mut latest = MediaSourceRef {
                        revision_id: revision,
                        digest: hash,
                        ..source.clone()
                    };
                    let old: Value = decode(&content)?;
                    let new: Value = decode(&body)?;
                    let mut status = if latest.revision_id == source.revision_id {
                        "current"
                    } else {
                        match (
                            selected(&old, &source.selector),
                            selected(&new, &source.selector),
                        ) {
                            (Some(a), Some(b)) if a == b => "current",
                            (_, None) => "selectionMissing",
                            _ => "changed",
                        }
                    };
                    if let MediaSelector::Text {
                        start_byte,
                        end_byte,
                        ..
                    } = &source.selector
                    {
                        if let Some(quote) = new["body"]
                            .as_str()
                            .and_then(|body| body.get(*start_byte..*end_byte))
                            .filter(|v| !v.is_empty())
                        {
                            latest.selector = MediaSelector::Text {
                                start_byte: *start_byte,
                                end_byte: *end_byte,
                                quote: quote.into(),
                            };
                        }
                    }
                    if let MediaSelector::FieldText {
                        field,
                        start_char,
                        end_char,
                        ..
                    } = &source.selector
                    {
                        if let Ok(quote) = text::value(&new, field)
                            .and_then(|v| text::slice(v, *start_char, *end_char))
                        {
                            latest.selector = MediaSelector::FieldText {
                                field: field.clone(),
                                start_char: *start_char,
                                end_char: *end_char,
                                quote,
                            };
                        }
                    }
                    match validate_source(&tx, p.as_str(), &latest, MediaFreshness::Pinned) {
                        Ok(()) => {}
                        Err(RepositoryError::InvalidContent(_)) => status = "selectionMissing",
                        Err(e) => return Err(e),
                    }
                    (
                        status,
                        if status == "selectionMissing" {
                            None
                        } else {
                            Some(latest)
                        },
                    )
                }
            };
            let item = json!({"source":source,"latest":latest,"status":status,"targets":targets});
            let n = item.to_string().len();
            if bytes + n > 60 * 1024 {
                if items.is_empty() {
                    return Err(fail("media_entity_too_large_select_smaller_scope"));
                }
                break;
            }
            bytes += n;
            items.push(item);
        }
        let next = offset + items.len();
        let current_canon = canon::at(&tx, p.as_str(), None)?;
        let result = json!({"projectId":p.as_str(),"editionId":e,"branchId":w.edition.branch_id,"revisionId":w.revision_id,"items":items,"offset":offset,"total":refs.len(),"nextOffset":if next<refs.len(){Some(next)}else{None},"canon":{"pinnedSnapshotId":w.edition.canon_snapshot_id,"currentSnapshotId":current_canon.snapshot_id,"changed":w.edition.canon_snapshot_id.as_deref()!=Some(current_canon.snapshot_id.as_str())}});
        tx.commit().map_err(storage)?;
        Ok(result)
    }
}
