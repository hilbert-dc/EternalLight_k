use serde_json::{Value, json};

use crate::identity_decisions::{
    IdentityResolution, identity_resolution, identity_resolution_at_snapshot,
};
use crate::knowledge_conflicts::detect_conflicts;
use crate::model_sources::ModelSourceScope;
use crate::proposition_sources::proposition_source_visible;
use crate::{ProjectId, RepositoryError, SnapshotId, SqliteStoryRepository};

pub(crate) const MAX_PROJECTED_ENTRIES: usize = 200;
pub(crate) const MAX_PENDING_SOURCE_IDS: usize = 100;

#[derive(Clone, Copy)]
pub(crate) enum StateVisibility {
    Author,
    ThroughChapter(i64),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KnowledgePerspective {
    Author,
    World,
    Character,
    Reader,
}

impl KnowledgePerspective {
    pub fn parse(value: &str) -> Result<Self, RepositoryError> {
        match value {
            "author" => Ok(Self::Author),
            "world" => Ok(Self::World),
            "character" => Ok(Self::Character),
            "reader" => Ok(Self::Reader),
            _ => Err(RepositoryError::InvalidContent(
                "invalid knowledge perspective".into(),
            )),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Author => "author",
            Self::World => "world",
            Self::Character => "character",
            Self::Reader => "reader",
        }
    }
}

pub struct KnowledgeStateQuery<'a> {
    pub project_id: &'a ProjectId,
    pub snapshot_id: Option<&'a SnapshotId>,
    pub perspective: KnowledgePerspective,
    pub world_time: Option<i64>,
    pub epistemic_time: Option<i64>,
    pub reading_chapter: Option<i64>,
    pub owner_id: Option<&'a str>,
    pub entity_id: Option<&'a str>,
    pub predicate: Option<&'a str>,
}

pub(crate) use crate::relation_types::relation_definition;

pub(crate) fn world_claim_visible(assertion: &Value) -> bool {
    match assertion["kind"].as_str() {
        Some("assertion" | "state" | "event") => true,
        Some("relation") => {
            relation_definition(assertion["relationType"].as_str().unwrap_or_default())
                .is_some_and(|definition| matches!(definition.category, "world" | "causal"))
        }
        Some("epistemic") | None | Some(_) => false,
    }
}

impl SqliteStoryRepository {
    pub fn knowledge_state(
        &self,
        query: KnowledgeStateQuery<'_>,
    ) -> Result<Value, RepositoryError> {
        self.with_read_snapshot(|| self.knowledge_state_in_snapshot(query))?
    }

    pub(crate) fn knowledge_state_in_snapshot(
        &self,
        query: KnowledgeStateQuery<'_>,
    ) -> Result<Value, RepositoryError> {
        self.knowledge_state_with_visibility(query, StateVisibility::Author)
    }

    pub(crate) fn knowledge_state_with_visibility(
        &self,
        query: KnowledgeStateQuery<'_>,
        visibility: StateVisibility,
    ) -> Result<Value, RepositoryError> {
        if query
            .entity_id
            .is_some_and(|id| id.is_empty() || id.len() > 160)
            || query
                .predicate
                .is_some_and(|predicate| predicate.is_empty() || predicate.len() > 80)
        {
            return Err(RepositoryError::InvalidContent(
                "invalid state selector".into(),
            ));
        }
        if query.perspective == KnowledgePerspective::Character
            && query
                .owner_id
                .is_none_or(|owner| owner.is_empty() || owner.len() > 160)
        {
            return Err(RepositoryError::InvalidContent(
                "character perspective requires ownerId".into(),
            ));
        }
        if query.perspective == KnowledgePerspective::Reader && query.reading_chapter.is_none() {
            return Err(RepositoryError::InvalidContent(
                "reader perspective requires readingChapter".into(),
            ));
        }
        if query.reading_chapter.is_some_and(|chapter| chapter < 0) {
            return Err(RepositoryError::InvalidContent(
                "readingChapter must not be negative".into(),
            ));
        }
        if query.perspective == KnowledgePerspective::Character && query.world_time.is_some() {
            return Err(RepositoryError::InvalidContent(
                "character perspective uses epistemicTime, not worldTime".into(),
            ));
        }
        if query.perspective == KnowledgePerspective::World && query.epistemic_time.is_some() {
            return Err(RepositoryError::InvalidContent(
                "world perspective uses worldTime, not epistemicTime".into(),
            ));
        }
        let writer_through = match visibility {
            StateVisibility::Author => None,
            StateVisibility::ThroughChapter(chapter)
                if chapter >= 0 && query.snapshot_id.is_none() =>
            {
                Some(chapter)
            }
            StateVisibility::ThroughChapter(_) => {
                return Err(RepositoryError::TaskBindingInvalid);
            }
        };
        let identity = if writer_through.is_some() {
            IdentityResolution::unresolved()
        } else if let Some(snapshot_id) = query.snapshot_id {
            identity_resolution_at_snapshot(&self.connection, query.project_id, snapshot_id)?
        } else {
            identity_resolution(&self.connection, query.project_id)?
        };
        let apply_rules = writer_through.is_none()
            && matches!(
                query.perspective,
                KnowledgePerspective::Author | KnowledgePerspective::World
            );
        let rules = if apply_rules {
            crate::property_rules::load(&self.connection, query.project_id, query.snapshot_id)?
        } else {
            crate::property_rules::PropertyRules::empty()
        };
        let owner_canonical = query.owner_id.map(|owner| identity.canonical_id(owner));
        let selector = crate::state_selector::StateSelector {
            query: &query,
            entity_ids: query
                .entity_id
                .map(|id| identity.raw_ids_for_canonical(id))
                .unwrap_or_default(),
            owner_ids: if query.perspective == KnowledgePerspective::Character {
                identity.raw_ids_for_canonical(
                    owner_canonical
                        .as_deref()
                        .expect("validated character owner"),
                )
            } else {
                Vec::new()
            },
            visible_through_chapter: writer_through.or(match query.perspective {
                KnowledgePerspective::Reader | KnowledgePerspective::Character => {
                    query.reading_chapter
                }
                KnowledgePerspective::Author | KnowledgePerspective::World => None,
            }),
        };
        let selected = self.projected_state_sources(&selector)?;
        let pending_sources_count = selected.pending_sources;
        let (chapter_numbers, sources, total_sources, unknown_chapter_sources) = (
            selected.chapters,
            selected.sources,
            selected.total_sources,
            selected.unknown_chapter_sources,
        );
        let confirmed = if writer_through.is_some() {
            std::collections::HashMap::new()
        } else {
            self.trace_confirmed_facts(query.project_id, query.snapshot_id)?
        };
        let mut condition_sources = None;
        let mut condition_budget = 100_000;
        let mut entries = Vec::new();
        let mut pending_sources = Vec::new();
        let mut unknown_time_skipped = 0;
        let mut truncated_entries = false;
        let mut scoped_sources = 0;
        let mut confirmed_entries = 0;
        let mut unavailable_proposition_sources = 0;
        let mut unknown_state_times = 0;
        let mut unapplied_property_rules = 0;
        for source in &sources {
            let object_id = source["objectId"].as_str().unwrap_or_default();
            let source_chapter = chapter_numbers.get(object_id).copied();
            if matches!(
                query.perspective,
                KnowledgePerspective::Reader | KnowledgePerspective::Character
            ) && let Some(reading_chapter) = query.reading_chapter
                && source_chapter.is_none_or(|chapter| chapter > reading_chapter)
            {
                continue;
            }
            scoped_sources += 1;
            if source["status"] != "ready" {
                pending_sources.push(object_id.to_owned());
                continue;
            }
            for (index, assertion) in source["assertions"]
                .as_array()
                .into_iter()
                .flatten()
                .enumerate()
            {
                let kind = assertion["kind"].as_str().unwrap_or_default();
                if kind == "state" {
                    crate::state_properties::schema(assertion)
                        .map_err(|_| RepositoryError::CorruptState)?;
                }
                if !selector.matches_selectors(assertion) {
                    continue;
                }
                if kind == "epistemic"
                    && !crate::epistemic_time::epistemic_interval_valid(assertion)
                {
                    return Err(RepositoryError::CorruptState);
                }
                if query.perspective == KnowledgePerspective::World
                    && !world_claim_visible(assertion)
                {
                    continue;
                }
                if query.perspective == KnowledgePerspective::Character
                    && (kind != "epistemic"
                        || assertion["epistemicOwnerId"]
                            .as_str()
                            .map(|id| identity.canonical_id(id))
                            != owner_canonical)
                {
                    continue;
                }
                if kind == "epistemic"
                    && let Some(source_batch_id) = assertion["propositionSourceBatchId"].as_str()
                {
                    let proposition_id = assertion["propositionId"]
                        .as_str()
                        .ok_or(RepositoryError::CorruptState)?;
                    let scope = query
                        .snapshot_id
                        .map_or(ModelSourceScope::Current, ModelSourceScope::Snapshot);
                    if !proposition_source_visible(
                        &self.connection,
                        query.project_id,
                        scope,
                        source_batch_id,
                        proposition_id,
                    )? {
                        unavailable_proposition_sources += 1;
                        continue;
                    }
                }
                if kind == "state" && assertion["validFrom"].is_null() {
                    unknown_state_times += 1;
                }
                let time = if kind == "epistemic" {
                    assertion["epistemicTime"].as_i64()
                } else if matches!(kind, "relation" | "state") {
                    assertion["validFrom"]
                        .as_i64()
                        .or_else(|| assertion["storyTime"].as_i64())
                } else {
                    assertion["storyTime"].as_i64()
                };
                let time_limit = if kind == "epistemic" {
                    query.epistemic_time
                } else {
                    query.world_time
                };
                if let Some(limit) = time_limit {
                    match time {
                        Some(value) if value <= limit => {}
                        None => {
                            unknown_time_skipped += 1;
                            continue;
                        }
                        Some(_) => continue,
                    }
                    if kind == "epistemic" && !crate::epistemic_claim_active_at(assertion, limit) {
                        continue;
                    }
                    if matches!(kind, "relation" | "state")
                        && assertion["validUntilExclusive"]
                            .as_i64()
                            .is_some_and(|end| limit >= end)
                    {
                        continue;
                    }
                }
                if entries.len() == MAX_PROJECTED_ENTRIES {
                    truncated_entries = true;
                    break;
                }
                let relation = if kind == "relation" {
                    relation_definition(assertion["relationType"].as_str().unwrap_or_default())
                        .map(|definition| json!({
                            "type": assertion["relationType"], "category": definition.category,
                            "inverseLabel": definition.inverse, "transitive": definition.transitive,
                            "direction": "sourceToTarget",
                            "validFrom": assertion.get("validFrom"),
                            "validUntilExclusive": assertion.get("validUntilExclusive")
                        }))
                } else {
                    None
                };
                let confirmed_fact_id = source["batchId"]
                    .as_str()
                    .and_then(|batch_id| confirmed.get(&(batch_id.to_owned(), index)));
                if confirmed_fact_id.is_some() {
                    confirmed_entries += 1;
                }
                entries.push(json!({
                    "sourceObjectId": object_id, "sourceRevisionId": source["sourceRevisionId"],
                    "modelingBatchId": source["batchId"], "assertionIndex": index,
                    "sourceChapter": source_chapter,
                    "canonicalSubjectId": assertion["subjectId"].as_str().map(|id| identity.canonical_id(id)),
                    "canonicalTargetId": crate::state_properties::target_entity(assertion).map(|id| identity.canonical_id(id)),
                    "canonicalEpistemicOwnerId": assertion["epistemicOwnerId"].as_str().map(|id| identity.canonical_id(id)),
                    "propositionRef": assertion["propositionId"].as_str().map(|id| json!({
                        "batchId": assertion["propositionSourceBatchId"].as_str().or_else(|| source["batchId"].as_str()),
                        "propositionId": id
                    })),
                    "confirmationState": if confirmed_fact_id.is_some() { "confirmed" } else { "candidate" },
                    "confirmedFactId": confirmed_fact_id,
                    "assertion": assertion, "relation": relation
                }));
                if kind == "event" {
                    entries.last_mut().expect("inserted event entry")["eventTemporalAssessment"] =
                        crate::event_conditions::assess(
                            assertion,
                            source["assertions"]
                                .as_array()
                                .ok_or(RepositoryError::CorruptState)?,
                            &crate::event_sources::resolve(
                                &self.connection,
                                query.project_id,
                                query.snapshot_id.map_or(
                                    crate::model_sources::ModelSourceScope::Current,
                                    crate::model_sources::ModelSourceScope::Snapshot,
                                ),
                                source["batchId"]
                                    .as_str()
                                    .ok_or(RepositoryError::CorruptState)?,
                                assertion,
                                source["entities"]
                                    .as_array()
                                    .ok_or(RepositoryError::CorruptState)?,
                            )?,
                        )?;
                    if apply_rules {
                        let target = crate::ConditionEvaluationTarget {
                            snapshot_id: query.snapshot_id.map(ToString::to_string),
                            source_batch_id: source["batchId"]
                                .as_str()
                                .ok_or(RepositoryError::CorruptState)?
                                .into(),
                            assertion_index: index,
                        };
                        let assessment = crate::condition_evaluation_jobs::resolve_assessment(
                            &self.connection,
                            query.project_id,
                            target,
                            || {
                                if condition_sources.is_none() && rules.event_rule_count() != 0 {
                                    condition_sources = Some(crate::event_rule_sources::collect(
                                        &self.connection,
                                        query.project_id,
                                        query.snapshot_id.map_or(
                                            ModelSourceScope::Current,
                                            ModelSourceScope::Snapshot,
                                        ),
                                        &identity,
                                        &rules,
                                    )?);
                                }
                                crate::event_rule_evaluation::assess(
                                    assertion,
                                    &rules,
                                    &identity,
                                    condition_sources.as_ref(),
                                    &mut condition_budget,
                                )
                            },
                        )?;
                        let entry = entries.last_mut().expect("inserted event entry");
                        entry["eventConditionAssessment"] = assessment;
                        entry["eventEvolutionAssessment"] = crate::event_evolution::assess(
                            assertion,
                            source["batchId"]
                                .as_str()
                                .ok_or(RepositoryError::CorruptState)?,
                            &entry["eventTemporalAssessment"],
                            &entry["eventConditionAssessment"],
                            &confirmed,
                        )?;
                    }
                }
                if kind == "state" {
                    let value = crate::state_properties::target_entity(assertion).map_or_else(
                        || assertion["value"].clone(),
                        |id| json!({"entityId":identity.canonical_id(id)}),
                    );
                    let entry = entries.last_mut().expect("inserted state entry");
                    entry["canonicalStateValue"] = value;
                    entry["stateSchemaAuthority"] = json!("modelDeclared");
                    entry["effectiveStateSchema"] = assertion["stateSchema"].clone();
                    if apply_rules && rules.project(entry, &source["entities"])? {
                        unapplied_property_rules += 1;
                    }
                }
            }
            if truncated_entries {
                break;
            }
        }
        let (findings, findings_truncated, uncertain_conflict_pairs) = detect_conflicts(&entries);
        let mut projection = json!({
            "projectId": query.project_id.as_str(), "perspective": query.perspective.as_str(),
            "snapshotId": query.snapshot_id.map(SnapshotId::as_str),
            "identityRevisionId": identity.revision_id,
            "propertyRulesRevisionId": rules.revision_id,
            "propertyRulesPolicy": if apply_rules { "approvedStructuralRules" } else { "withheldForLimitedPerspective" },
            "worldTime": query.world_time, "epistemicTime": query.epistemic_time,
            "readingChapter": query.reading_chapter,
            "ownerId": query.owner_id, "entries": entries, "findings": findings,
            "visibleThroughChapter": writer_through,
            "selectors": {"entityId": query.entity_id, "predicate": query.predicate},
            "coverage": {
                "complete": pending_sources.is_empty() && scoped_sources == total_sources && !truncated_entries && unknown_time_skipped == 0 && !findings_truncated && uncertain_conflict_pairs == 0 && unknown_chapter_sources == 0 && unavailable_proposition_sources == 0 && unknown_state_times == 0 && unapplied_property_rules == 0,
                "pendingSources": pending_sources, "truncatedSources": total_sources > scoped_sources,
                "pendingSourcesCount": pending_sources_count,
                "pendingSourceIdsTruncated": pending_sources_count > pending_sources.len() as i64,
                "matchingSourcesCount": total_sources,
                "scannedSourcesCount": scoped_sources,
                "semanticCompleteness": "unknown",
                "unknownChapterSources": unknown_chapter_sources,
                "truncatedEntries": truncated_entries, "unknownTimeSkipped": unknown_time_skipped,
                "unavailablePropositionSources": unavailable_proposition_sources,
                "findingsTruncated": findings_truncated, "uncertainConflictPairs": uncertain_conflict_pairs,
                "unknownStateTimes": unknown_state_times,
                "unappliedPropertyRules": unapplied_property_rules,
                "confirmedEntries": confirmed_entries,
                "candidateEntries": entries.len() - confirmed_entries
            }
        });
        // The existing withheld policy covers both rule families. Do not invent a zero
        // count or duplicate global rule declarations in a restricted projection.
        if apply_rules {
            projection["eventRuleCount"] = json!(rules.event_rule_count());
            projection["eventConditionsPolicy"] = json!("authorRulesSourceClaims");
        }
        Ok(projection)
    }
}

#[cfg(test)]
#[path = "knowledge_projection_tests.rs"]
mod tests;
