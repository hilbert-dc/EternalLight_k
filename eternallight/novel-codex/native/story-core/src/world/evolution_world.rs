use std::cmp::Ordering;
use std::collections::BinaryHeap;

use serde_json::{Value, json};

use crate::evolution::{EvolutionTraceQuery, TraceManifest};
use crate::identity_decisions::IdentityResolution;
use crate::model_sources::{ModelSourceScope, validated_source};
use crate::{RepositoryError, SnapshotId, SqliteStoryRepository};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct WorldKey {
    undated: bool,
    time: i64,
    source: usize,
    assertion: usize,
    phase: usize,
}

struct WorldEntry {
    key: WorldKey,
    value: Value,
}

impl PartialEq for WorldEntry {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}

impl Eq for WorldEntry {}

impl PartialOrd for WorldEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for WorldEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        self.key.cmp(&other.key)
    }
}

impl SqliteStoryRepository {
    pub(crate) fn world_evolution(
        &self,
        query: &EvolutionTraceQuery<'_>,
        snapshot_id: &str,
        identity: &IdentityResolution,
    ) -> Result<(Vec<Value>, Value, Option<String>), RepositoryError> {
        let TraceManifest {
            sources,
            digest,
            pending_count,
            pending_sources,
        } = self.trace_manifest(query.project_id, query.snapshot_id)?;
        let after = query
            .cursor
            .map(|cursor| parse_cursor(cursor, snapshot_id, &digest))
            .transpose()?;
        let confirmed = self.trace_confirmed_facts(query.project_id, query.snapshot_id)?;
        let scope = query
            .snapshot_id
            .map_or(ModelSourceScope::Current, ModelSourceScope::Snapshot);
        let mut page = BinaryHeap::new();
        let mut cursor_found = after.is_none();
        let mut undated_count = 0usize;
        for (source_index, source) in sources.iter().enumerate() {
            if !source.ready {
                continue;
            }
            let batch_id = source
                .batch_id
                .as_deref()
                .ok_or(RepositoryError::CorruptState)?;
            let assertions = validated_source(&self.connection, query.project_id, scope, batch_id)?
                .ok_or(RepositoryError::CorruptState)?
                .assertions;
            for (index, assertion) in assertions.iter().enumerate() {
                if !crate::knowledge_projection::world_claim_visible(assertion) {
                    continue;
                }
                let state = assertion["kind"] == "state";
                if state {
                    crate::state_properties::schema(assertion)
                        .map_err(|_| RepositoryError::CorruptState)?;
                }
                let interval_end = (assertion["kind"] == "relation" || state)
                    && assertion["validUntilExclusive"].is_i64();
                for phase in 0..=usize::from(interval_end) {
                    let time = if phase == 1 {
                        assertion["validUntilExclusive"].as_i64()
                    } else {
                        assertion["storyTime"]
                            .as_i64()
                            .or_else(|| assertion["validFrom"].as_i64())
                    };
                    let key = WorldKey {
                        undated: time.is_none(),
                        time: time.unwrap_or_default(),
                        source: source_index,
                        assertion: index,
                        phase,
                    };
                    if key.undated {
                        undated_count += 1;
                    }
                    if after == Some(key) {
                        cursor_found = true;
                    }
                    if after.is_some_and(|cursor| key <= cursor) {
                        continue;
                    }
                    let confirmed_id = confirmed.get(&(batch_id.to_owned(), index));
                    let value = json!({
                        "dimension": "world",
                        "change": if phase == 1 { if state {"stateExpired"} else {"relationEnded"} } else { assertion["kind"].as_str().unwrap_or_default() },
                        "before": if phase == 1 { assertion["value"].clone() } else { Value::Null },
                        "after": if phase == 1 { Value::Null } else { assertion["value"].clone() },
                        "interpretation": if phase == 1 {"claimNoLongerApplies"} else {"sourceClaim"},
                        "stateSchema": assertion["stateSchema"],
                        "worldTime": time,
                        "timeStatus": if time.is_some() { "dated" } else { "undated" },
                        "epistemicTime": assertion["epistemicTime"],
                        "epistemicOwnerId": assertion["epistemicOwnerId"],
                        "canonicalEpistemicOwnerId": assertion["epistemicOwnerId"].as_str().map(|id| identity.canonical_id(id)),
                        "targetId": assertion["subjectId"],
                        "canonicalTargetId": assertion["subjectId"].as_str().map(|id| identity.canonical_id(id)),
                        "predicate": assertion["predicate"],
                        "relationType": assertion["relationType"],
                        "relatedEntityId": crate::state_properties::target_entity(assertion),
                        "canonicalRelatedEntityId": crate::state_properties::target_entity(assertion).map(|id| identity.canonical_id(id)),
                        "propositionId": assertion["propositionId"],
                        "sourceChapter": source.chapter,
                        "sourceObjectId": source.object_id,
                        "sourceRevisionId": source.revision_id,
                        "modelingBatchId":batch_id,"assertionIndex":index,
                        "confirmationState": if confirmed_id.is_some() { "confirmed" } else { "candidate" },
                        "confirmedFactId": confirmed_id,
                        "evidence": if phase == 1 { &assertion["validUntilEvidence"] } else { &assertion["evidence"] },
                    });
                    if page.len() < query.limit + 1 {
                        page.push(WorldEntry { key, value });
                    } else if page.peek().is_some_and(|largest| key < largest.key) {
                        page.pop();
                        page.push(WorldEntry { key, value });
                    }
                }
            }
        }
        if !cursor_found {
            return Err(invalid_cursor());
        }
        let mut page = page.into_vec();
        page.sort_unstable_by_key(|entry| entry.key);
        let truncated = page.len() > query.limit;
        page.truncate(query.limit);
        let next_cursor = if truncated {
            page.last()
                .map(|entry| format_cursor(snapshot_id, &digest, entry.key))
        } else {
            None
        };
        let rules =
            crate::property_rules::load(&self.connection, query.project_id, query.snapshot_id)?;
        let mut condition_sources = None;
        let mut condition_budget = 100_000;
        let mut unresolved_effects = 0;
        for entry in &mut page {
            if entry.value["change"] != "event" {
                continue;
            }
            let batch_id = entry.value["modelingBatchId"]
                .as_str()
                .ok_or(RepositoryError::CorruptState)?;
            let index = entry.key.assertion;
            let original = validated_source(&self.connection, query.project_id, scope, batch_id)?
                .ok_or(RepositoryError::CorruptState)?;
            let event = original
                .assertions
                .get(index)
                .ok_or(RepositoryError::CorruptState)?;
            let external = crate::event_sources::resolve(
                &self.connection,
                query.project_id,
                scope,
                batch_id,
                event,
                &original.entities,
            )?;
            let temporal = crate::event_conditions::assess(event, &original.assertions, &external)?;
            let target = crate::ConditionEvaluationTarget {
                snapshot_id: query.snapshot_id.map(ToString::to_string),
                source_batch_id: batch_id.into(),
                assertion_index: index,
            };
            let conditions = crate::condition_evaluation_jobs::resolve_assessment(
                &self.connection,
                query.project_id,
                target,
                || {
                    if condition_sources.is_none() && rules.event_rule_count() != 0 {
                        condition_sources = Some(crate::event_rule_sources::collect(
                            &self.connection,
                            query.project_id,
                            scope,
                            identity,
                            &rules,
                        )?);
                    }
                    crate::event_rule_evaluation::assess(
                        event,
                        &rules,
                        identity,
                        condition_sources.as_ref(),
                        &mut condition_budget,
                    )
                },
            )?;
            let evolution = crate::event_evolution::assess(
                event,
                batch_id,
                &temporal,
                &conditions,
                &confirmed,
            )?;
            unresolved_effects += evolution["effects"]
                .as_array()
                .ok_or(RepositoryError::CorruptState)?
                .iter()
                .filter(|effect| {
                    effect["conditionCompatibility"] != "supported"
                        || effect["confirmationState"] != "confirmed"
                        || matches!(
                            effect["interpretation"].as_str(),
                            Some("unknownTime" | "unsupportedKind")
                        )
                })
                .count();
            entry.value["eventTemporalAssessment"] = temporal;
            entry.value["eventConditionAssessment"] = conditions;
            entry.value["eventEvolutionAssessment"] = evolution;
        }
        let entries = page.into_iter().map(|entry| entry.value).collect();
        let coverage = json!({
            "complete": pending_count == 0 && !truncated && undated_count == 0,
            "chronologyComplete": pending_count == 0 && undated_count == 0,
            "truncated": truncated,
            "pendingSourceCount": pending_count,
            "pendingSources": pending_sources,
            "undatedEventCount": undated_count,
            "manifestDigest": digest,
            "ordering": "storyTimeThenSource"
            ,"unresolvedEffectsInPage":unresolved_effects,"semanticCompleteness":"unknown","stateChangesExecuted":false
        });
        Ok((entries, coverage, next_cursor))
    }
}

fn format_cursor(snapshot_id: &str, digest: &str, key: WorldKey) -> String {
    format!(
        "{snapshot_id}:world:{digest}:{}:{}:{}:{}:{}",
        if key.undated { "u" } else { "k" },
        key.time,
        key.source,
        key.assertion,
        key.phase
    )
}

fn parse_cursor(
    cursor: &str,
    snapshot_id: &str,
    digest: &str,
) -> Result<WorldKey, RepositoryError> {
    let parts = cursor.split(':').collect::<Vec<_>>();
    if parts.len() != 8 || parts[1] != "world" {
        return Err(invalid_cursor());
    }
    let expected = SnapshotId::parse(parts[0]).map_err(|_| invalid_cursor())?;
    let actual = SnapshotId::parse(snapshot_id).map_err(|_| RepositoryError::CorruptState)?;
    if expected != actual {
        return Err(RepositoryError::StaleSnapshot { expected, actual });
    }
    if parts[2] != digest {
        return Err(RepositoryError::StaleKnowledgeManifest);
    }
    let undated = match parts[3] {
        "k" => false,
        "u" => true,
        _ => return Err(invalid_cursor()),
    };
    let time = parts[4].parse::<i64>().map_err(|_| invalid_cursor())?;
    if undated && time != 0 {
        return Err(invalid_cursor());
    }
    let source = parts[5].parse::<usize>().map_err(|_| invalid_cursor())?;
    let assertion = parts[6].parse::<usize>().map_err(|_| invalid_cursor())?;
    let phase = parts[7].parse::<usize>().map_err(|_| invalid_cursor())?;
    if assertion >= 100 || phase > 1 {
        return Err(invalid_cursor());
    }
    Ok(WorldKey {
        undated,
        time,
        source,
        assertion,
        phase,
    })
}

fn invalid_cursor() -> RepositoryError {
    RepositoryError::InvalidContent("invalid evolution cursor".into())
}
