use serde_json::{Value, json};

/// Remove only optional material. Frozen targets, plans, facts and modeling input stay intact;
/// the caller rejects the pack if mandatory items alone exceed its byte or model-item budget.
pub(crate) fn fit_optional_context(pack: &mut Value, byte_limit: usize) {
    loop {
        crate::context_pack::finalize_context_pack(pack);
        if pack.to_string().len() + 200 <= byte_limit
            && crate::context_model_item_budget::fits(pack)
        {
            break;
        }
        if pack["candidateCommitmentProposals"]
            .as_array()
            .is_some_and(|entries| entries.len() > 1)
            && trim_proposal(pack)
        {
            continue;
        }
        if pack["relevantCommitments"]
            .as_array()
            .is_some_and(|entries| entries.len() > 1)
            && trim_commitment(pack)
        {
            continue;
        }
        let protected_source = pack["operationContext"]["modelingBatch"]["sourceRevisionId"]
            .as_str()
            .or_else(|| pack["target"]["operation"]["sourceRevisionId"].as_str());
        let target_chapter = pack["target"]["position"]["readingChapter"]
            .as_i64()
            .or_else(|| pack["target"]["operation"]["chapterNumber"].as_i64());
        let removable = pack["relevantText"].as_array().and_then(|drafts| {
            drafts.iter().position(|draft| {
                protected_source.is_none_or(|id| draft["sourceRevisionId"] != id)
                    && target_chapter
                        .is_none_or(|chapter| draft["content"]["chapterNumber"] != chapter)
            })
        });
        if let Some(index) = removable {
            pack["relevantText"]
                .as_array_mut()
                .expect("draft array")
                .remove(index);
            mark_truncated(pack, "chapterText", "contextTextBudget");
            continue;
        }
        if let Some(sources) = pack
            .get_mut("operationContext")
            .and_then(|context| context.get_mut("modeling"))
            .and_then(Value::as_array_mut)
            && !sources.is_empty()
        {
            sources.remove(0);
            mark_truncated(pack, "knowledgeModeling", "contextSourceBudget");
            continue;
        }
        if let Some(entries) = pack
            .get_mut("operationContext")
            .and_then(|context| context.get_mut("entityCatalog"))
            .and_then(|catalog| catalog.get_mut("entries"))
            .and_then(Value::as_array_mut)
            && entries.pop().is_some()
        {
            let catalog = &mut pack["operationContext"]["entityCatalog"];
            catalog["truncated"] = json!(true);
            catalog["ambiguityScanComplete"] = json!(false);
            // Removed identities may be referenced by ambiguity groups; do not retain a partial decision.
            catalog["ambiguousNames"] = json!([]);
            mark_truncated(pack, "entityCatalog", "contextEntityBudget");
            continue;
        }
        if let Some(entries) = pack
            .get_mut("operationContext")
            .and_then(|context| context.get_mut("workingKnowledge"))
            .and_then(|projection| projection.get_mut("entries"))
            .and_then(Value::as_array_mut)
            && entries.pop().is_some()
        {
            let projection = &mut pack["operationContext"]["workingKnowledge"];
            let entries = projection["entries"].as_array().expect("entry array");
            let confirmed = entries
                .iter()
                .filter(|entry| entry["confirmationState"] == "confirmed")
                .count();
            let candidates = entries.len() - confirmed;
            projection["findings"] = json!([]);
            projection["coverage"]["complete"] = json!(false);
            projection["coverage"]["truncatedEntries"] = json!(true);
            projection["coverage"]["findingsTruncated"] = json!(true);
            projection["coverage"]["confirmedEntries"] = json!(confirmed);
            projection["coverage"]["candidateEntries"] = json!(candidates);
            mark_truncated(pack, "workingKnowledge", "contextKnowledgeBudget");
            continue;
        }
        // An empty optional projection carries no claim/evidence references. Compact only
        // repeated identity and inactive selectors, preserving active scope and rule policy.
        if let Some(projection) = pack
            .get_mut("operationContext")
            .and_then(|context| context.get_mut("workingKnowledge"))
            .and_then(Value::as_object_mut)
            && projection
                .get("entries")
                .and_then(Value::as_array)
                .is_some_and(Vec::is_empty)
            && projection.get("metadataTruncated") != Some(&json!(true))
        {
            let mut removed = projection.remove("projectId").is_some();
            for field in [
                "snapshotId",
                "identityRevisionId",
                "worldTime",
                "epistemicTime",
                "ownerId",
            ] {
                if projection.get(field).is_some_and(Value::is_null) {
                    projection.remove(field);
                    removed = true;
                }
            }
            if projection
                .get("selectors")
                .and_then(Value::as_object)
                .is_some_and(|selectors| selectors.values().all(Value::is_null))
            {
                projection.remove("selectors");
                removed = true;
            }
            if removed {
                projection.insert("metadataTruncated".into(), json!(true));
                pack["operationContext"]["workingKnowledge"]["coverage"]["complete"] = json!(false);
                mark_truncated(pack, "workingKnowledge", "contextKnowledgeMetadataBudget");
                continue;
            }
        }
        // Keep the highest-ranked promise while lower-priority optional material
        // can be removed, but never let it displace mandatory task/source inputs.
        if trim_proposal(pack) {
            continue;
        }
        if trim_commitment(pack) {
            continue;
        }
        break;
    }
}

fn trim_proposal(pack: &mut Value) -> bool {
    let Some(entries) = pack
        .get_mut("candidateCommitmentProposals")
        .and_then(Value::as_array_mut)
    else {
        return false;
    };
    if entries.pop().is_none() {
        return false;
    }
    let included = entries.len();
    let coverage = &mut pack["operationContext"]["commitmentProposalCoverage"];
    coverage["complete"] = json!(false);
    coverage["truncated"] = json!(true);
    coverage["includedEntries"] = json!(included);
    mark_truncated(pack, "commitmentProposals", "contextProposalBudget");
    true
}

fn trim_commitment(pack: &mut Value) -> bool {
    let Some(entries) = pack
        .get_mut("relevantCommitments")
        .and_then(Value::as_array_mut)
    else {
        return false;
    };
    if entries.pop().is_none() {
        return false;
    }
    let included = entries.len();
    let coverage = &mut pack["operationContext"]["commitmentCoverage"];
    coverage["complete"] = json!(false);
    coverage["truncated"] = json!(true);
    coverage["includedEntries"] = json!(included);
    mark_truncated(pack, "narrativeCommitments", "contextCommitmentBudget");
    true
}

pub(crate) fn mark_truncated(pack: &mut Value, scope: &str, reason: &str) {
    pack["coverage"]["complete"] = json!(false);
    for (field, value) in [("missingScope", scope), ("truncationReasons", reason)] {
        let entries = pack["coverage"][field]
            .as_array_mut()
            .expect("coverage array");
        if !entries.iter().any(|entry| entry == value) {
            entries.push(json!(value));
        }
    }
}

#[cfg(test)]
#[path = "context_budget_unit_tests.rs"]
mod tests;
