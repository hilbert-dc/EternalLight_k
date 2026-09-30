use std::collections::BTreeMap;

use novel_codex_story_core::{ObjectHead, RevisionId, WorkingBaseline};
use serde_json::{Value, json};

/// A source choice never changes its acceptance authority; selection is a separate fact.
#[derive(Clone, Copy)]
pub(crate) enum SourcePolicy {
    AuthorSelected,
    RunOrBoundTask,
}

pub(crate) struct ContextSource<'a> {
    pub revision_id: &'a RevisionId,
    pub authority: &'static str,
    pub selection_revision: Option<i64>,
}

pub(crate) fn selected_baseline<'a>(
    baselines: &'a [WorkingBaseline],
    object_kind: &str,
    object_id: &str,
) -> Option<&'a WorkingBaseline> {
    baselines
        .iter()
        .find(|selection| selection.object_kind == object_kind && selection.object_id == object_id)
}

pub(crate) fn context_source<'a>(
    head: &'a ObjectHead,
    baseline: Option<&'a WorkingBaseline>,
    policy: SourcePolicy,
) -> Option<ContextSource<'a>> {
    if matches!(policy, SourcePolicy::AuthorSelected)
        && let Some(selection) = baseline
        && let Some(revision_id) = selection.revision_id.as_ref()
    {
        return Some(ContextSource {
            revision_id,
            authority: if head.accepted_revision_id.as_ref() == Some(revision_id) {
                "accepted"
            } else if head.candidate_revision_id.as_ref() == Some(revision_id) {
                "candidate"
            } else {
                "working"
            },
            selection_revision: Some(selection.selection_revision),
        });
    }
    let revision_id = if matches!(policy, SourcePolicy::RunOrBoundTask) {
        head.candidate_revision_id
            .as_ref()
            .or(head.accepted_revision_id.as_ref())
    } else {
        head.accepted_revision_id
            .as_ref()
            .or(head.candidate_revision_id.as_ref())
    }?;
    Some(ContextSource {
        revision_id,
        authority: if head.accepted_revision_id.as_ref() == Some(revision_id) {
            "accepted"
        } else {
            "candidate"
        },
        selection_revision: None,
    })
}

pub(crate) fn draft_sources(
    heads: &[ObjectHead],
    baselines: &[WorkingBaseline],
    policy: SourcePolicy,
) -> BTreeMap<String, String> {
    heads
        .iter()
        .filter(|head| head.object_kind == "draft")
        .filter_map(|head| {
            let chapter_id = head.object_id.as_str().strip_prefix("draft-")?;
            let source = context_source(
                head,
                selected_baseline(baselines, "draft", chapter_id),
                policy,
            )?;
            Some((
                head.object_id.as_str().to_owned(),
                source.revision_id.as_str().to_owned(),
            ))
        })
        .collect()
}

pub(crate) fn withhold_mismatched_commitments(
    commitments: &mut Value,
    sources: &BTreeMap<String, String>,
) {
    let mut removed = false;
    if let Some(entries) = commitments["entries"].as_array_mut() {
        let before = entries.len();
        entries.retain(|entry| {
            ["setupEvidenceRefs", "fulfillmentEvidenceRefs"]
                .iter()
                .all(|key| {
                    entry[*key].as_array().is_some_and(|refs| {
                        refs.iter().all(|anchor| {
                            anchor["objectId"]
                                .as_str()
                                .and_then(|object_id| sources.get(object_id))
                                .is_some_and(|revision| anchor["revisionId"] == *revision)
                        })
                    })
                })
        });
        removed |= entries.len() != before;
        let retained = entries.len();
        commitments["coverage"]["includedEntries"] = json!(retained);
    }
    if let Some(entries) = commitments["proposalContext"]["entries"].as_array_mut() {
        let before = entries.len();
        entries.retain(|entry| {
            entry["chapterId"]
                .as_str()
                .and_then(|chapter_id| sources.get(&format!("draft-{chapter_id}")))
                .is_some_and(|revision| entry["sourceRevisionId"] == *revision)
        });
        removed |= entries.len() != before;
        let retained = entries.len();
        commitments["proposalContext"]["coverage"]["includedEntries"] = json!(retained);
    }
    if removed {
        commitments["coverage"]["complete"] = json!(false);
        commitments["coverage"]["sourceModelingIncomplete"] = json!(true);
        commitments["proposalContext"]["coverage"]["complete"] = json!(false);
        commitments["proposalContext"]["coverage"]["sourceModelingIncomplete"] = json!(true);
    }
}
