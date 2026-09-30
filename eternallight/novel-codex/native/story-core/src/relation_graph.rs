use std::collections::{BTreeSet, HashMap, HashSet};

use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};

use crate::dependency_rules::RelationTraversalPolicy;

use crate::identity_decisions::{
    IdentityResolution, identity_resolution, identity_resolution_at_snapshot,
};
use crate::knowledge_projection::relation_definition;
use crate::relation_graph_index::{
    epistemic_visibility, neighbors, seed_is_visible, source_coverage,
};
use crate::{ProjectId, RepositoryError, SnapshotId, SqliteStoryRepository};

const MAX_SCANNED_EDGES: usize = 1_000;
const MAX_VISITED_NODES: usize = 500;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RelationDirection {
    Outgoing,
    Incoming,
    Both,
}

impl RelationDirection {
    pub fn parse(value: &str) -> Result<Self, RepositoryError> {
        match value {
            "outgoing" => Ok(Self::Outgoing),
            "incoming" => Ok(Self::Incoming),
            "both" => Ok(Self::Both),
            _ => Err(RepositoryError::InvalidContent(
                "invalid relation direction".into(),
            )),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Outgoing => "outgoing",
            Self::Incoming => "incoming",
            Self::Both => "both",
        }
    }
}

pub struct RelationGraphQuery<'a> {
    pub project_id: &'a ProjectId,
    pub scope: RelationGraphScope<'a>,
    pub entity_id: &'a str,
    pub relation_type: Option<&'a str>,
    pub direction: RelationDirection,
    pub world_time: Option<i64>,
    pub max_hops: usize,
    pub limit: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RelationGraphScope<'a> {
    Current,
    Snapshot(&'a SnapshotId),
    /// Only source-bound claims from chapters strictly before this Writer target.
    WriterBeforeChapter(i64),
    /// Character-scoped Writer view; the caller must bind this to the trusted task target.
    WriterPov {
        chapter: i64,
        character_id: &'a str,
    },
}

impl SqliteStoryRepository {
    /// Expands indexed source-bound relation claims; a multi-hop path is never an inferred fact.
    pub fn query_relation_graph(
        &self,
        query: RelationGraphQuery<'_>,
    ) -> Result<Value, RepositoryError> {
        self.relation_graph_with_policy(query, RelationTraversalPolicy::AllRelations)
    }

    pub(crate) fn relation_graph_with_policy(
        &self,
        query: RelationGraphQuery<'_>,
        policy: RelationTraversalPolicy,
    ) -> Result<Value, RepositoryError> {
        if query.entity_id.is_empty()
            || query.entity_id.len() > 160
            || !(1..=3).contains(&query.max_hops)
            || !(1..=100).contains(&query.limit)
            || matches!(query.scope, RelationGraphScope::WriterBeforeChapter(number) if number < 1)
            || matches!(query.scope, RelationGraphScope::WriterPov { chapter, character_id } if chapter < 1 || character_id.is_empty() || character_id.len() > 160)
            || query
                .relation_type
                .is_some_and(|kind| relation_definition(kind).is_none())
        {
            return Err(RepositoryError::InvalidContent(
                "invalid relation graph query".into(),
            ));
        }
        if self.project(query.project_id)?.is_none() {
            return Err(RepositoryError::CandidateNotFound);
        }
        if let RelationGraphScope::Snapshot(snapshot_id) = query.scope {
            let exists = self.connection.query_row(
                "SELECT 1 FROM story_snapshots_v2 WHERE project_id = ?1 AND branch_id = 'main' AND snapshot_id = ?2",
                params![query.project_id.as_str(), snapshot_id.as_str()],
                |row| row.get::<_, i64>(0),
            ).optional().map_err(storage_error)?;
            if exists.is_none() {
                return Err(RepositoryError::CandidateNotFound);
            }
        }
        let identity = match query.scope {
            RelationGraphScope::Current => identity_resolution(&self.connection, query.project_id)?,
            RelationGraphScope::Snapshot(snapshot_id) => {
                identity_resolution_at_snapshot(&self.connection, query.project_id, snapshot_id)?
            }
            RelationGraphScope::WriterBeforeChapter(_) | RelationGraphScope::WriterPov { .. } => {
                IdentityResolution::unresolved()
            }
        };
        let seed_id = identity.canonical_id(query.entity_id);
        let (total_sources, modeled_sources) =
            source_coverage(&self.connection, query.project_id, query.scope)?;
        let seed_found = seed_is_visible(
            &self.connection,
            query.project_id,
            query.scope,
            &identity.raw_ids_for_canonical(&seed_id),
        )?;
        let confirmed = match query.scope {
            RelationGraphScope::Current => self.trace_confirmed_facts(query.project_id, None)?,
            RelationGraphScope::Snapshot(snapshot_id) => {
                self.trace_confirmed_facts(query.project_id, Some(snapshot_id))?
            }
            RelationGraphScope::WriterBeforeChapter(_) | RelationGraphScope::WriterPov { .. } => {
                HashMap::new()
            }
        };
        let mut frontier = BTreeSet::from([seed_id.clone()]);
        let mut visited = HashSet::from([seed_id.clone()]);
        let mut seen_claims = HashSet::new();
        let mut results = Vec::new();
        let mut truncated_edges = false;
        let mut truncated_nodes = false;
        let mut truncated_matches = false;
        let mut unknown_time_skipped = 0;
        let mut uncertain_knowledge = matches!(query.scope, RelationGraphScope::WriterPov { .. })
            && query.world_time.is_none();
        for hop in 1..=query.max_hops {
            if frontier.is_empty() || !seed_found {
                break;
            }
            let raw_ids = frontier
                .iter()
                .flat_map(|id| identity.raw_ids_for_canonical(id))
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            let mut next = BTreeSet::new();
            let endpoints: &[&str] = match query.direction {
                RelationDirection::Outgoing => &["subject"],
                RelationDirection::Incoming => &["target"],
                RelationDirection::Both => &["subject", "target"],
            };
            for endpoint in endpoints {
                let (rows, query_truncated) = neighbors(
                    &self.connection,
                    query.project_id,
                    query.scope,
                    &raw_ids,
                    query.relation_type,
                    endpoint,
                    policy,
                )?;
                truncated_edges |= query_truncated;
                for row in rows {
                    if !seen_claims.insert((row.batch_id.clone(), row.assertion_index)) {
                        continue;
                    }
                    if seen_claims.len() > MAX_SCANNED_EDGES {
                        truncated_edges = true;
                        continue;
                    }
                    let assertion = &row.assertion;
                    if assertion["kind"] != "relation"
                        || assertion["subjectId"] != row.subject_id
                        || assertion["targetId"] != row.target_id
                        || assertion["relationType"] != row.relation_type
                        || assertion["propositionId"]
                            .as_str()
                            .is_none_or(str::is_empty)
                    {
                        return Err(RepositoryError::CorruptState);
                    }
                    let definition = relation_definition(&row.relation_type)
                        .ok_or(RepositoryError::CorruptState)?;
                    let from_id = identity.canonical_id(&row.subject_id);
                    let to_id = identity.canonical_id(&row.target_id);
                    let outgoing = frontier.contains(&from_id) && *endpoint == "subject";
                    let incoming = frontier.contains(&to_id) && *endpoint == "target";
                    if !outgoing && !incoming {
                        return Err(RepositoryError::CorruptState);
                    }
                    if let Some(time) = query.world_time {
                        let start = assertion["validFrom"]
                            .as_i64()
                            .or_else(|| assertion["storyTime"].as_i64());
                        match start {
                            Some(start) if start <= time => {}
                            None => {
                                unknown_time_skipped += 1;
                                continue;
                            }
                            Some(_) => continue,
                        }
                        if assertion["validUntilExclusive"]
                            .as_i64()
                            .is_some_and(|end| time >= end)
                        {
                            continue;
                        }
                    }
                    if let RelationGraphScope::WriterPov {
                        chapter,
                        character_id,
                    } = query.scope
                    {
                        let proposition_id = assertion["propositionId"]
                            .as_str()
                            .ok_or(RepositoryError::CorruptState)?;
                        if !epistemic_visibility(
                            &self.connection,
                            query.project_id,
                            &row.batch_id,
                            chapter,
                            character_id,
                            proposition_id,
                            query.world_time,
                        )? {
                            uncertain_knowledge = true;
                            continue;
                        }
                    }
                    if results.len() == query.limit {
                        truncated_matches = true;
                    } else {
                        let fact_id = confirmed.get(&(row.batch_id.clone(), row.assertion_index));
                        results.push(json!({
                            "fromId": from_id, "toId": to_id,
                            "rawSubjectId": row.subject_id, "rawTargetId": row.target_id,
                            "relationType": row.relation_type, "category": definition.category,
                            "propositionId": assertion["propositionId"],
                            "direction": "sourceToTarget", "transitive": definition.transitive,
                            "value": assertion["value"], "storyTime": assertion["storyTime"],
                            "validFrom": assertion["validFrom"],
                            "validUntilExclusive": assertion["validUntilExclusive"],
                            "sourceObjectId": row.source_object_id,
                            "sourceRevisionId": row.source_revision_id,
                            "modelingBatchId": row.batch_id, "assertionIndex": row.assertion_index,
                            "evidence": assertion["evidence"], "hop": hop,
                            "confirmationState": if fact_id.is_some() { "confirmed" } else { "candidate" },
                            "confirmedFactId": fact_id
                        }));
                    }
                    let neighbor = if outgoing { &to_id } else { &from_id };
                    if !visited.contains(neighbor) {
                        if visited.len() == MAX_VISITED_NODES {
                            truncated_nodes = true;
                        } else {
                            visited.insert(neighbor.clone());
                            next.insert(neighbor.clone());
                        }
                    }
                }
            }
            frontier = next;
        }
        Ok(json!({
            "projectId": query.project_id.as_str(),
            "scope": match query.scope {
                RelationGraphScope::Current => "current",
                RelationGraphScope::Snapshot(_) => "snapshot",
                RelationGraphScope::WriterBeforeChapter(_) => "writerPriorChapters",
                RelationGraphScope::WriterPov { .. } => "writerCharacterKnowledge",
            },
            "snapshotId": match query.scope { RelationGraphScope::Snapshot(id) => Some(id.as_str()), _ => None },
            "visibleThroughChapter": match query.scope { RelationGraphScope::WriterBeforeChapter(number) => Some(number - 1), RelationGraphScope::WriterPov { chapter, .. } => Some(chapter - 1), _ => None },
            "identityRevisionId": identity.revision_id, "entityId": query.entity_id,
            "canonicalEntityId": seed_id, "seedFound": if matches!(query.scope, RelationGraphScope::WriterPov { .. }) { None } else { Some(seed_found) },
            "relationType": query.relation_type, "direction": query.direction.as_str(),
            "worldTime": query.world_time, "maxHops": query.max_hops, "results": results,
            "coverage": {
                "complete": seed_found && modeled_sources == total_sources && !uncertain_knowledge && !truncated_edges
                    && !truncated_nodes && !truncated_matches && unknown_time_skipped == 0,
                "totalSources": total_sources, "modeledSources": modeled_sources,
                "pendingSourceCount": total_sources - modeled_sources,
                "truncatedEdges": if matches!(query.scope, RelationGraphScope::WriterPov { .. }) { None } else { Some(truncated_edges) }, "truncatedNodes": truncated_nodes,
                "truncatedMatches": truncated_matches, "unknownTimeSkipped": if matches!(query.scope, RelationGraphScope::WriterPov { .. }) { None } else { Some(unknown_time_skipped) },
                "scannedEdges": if matches!(query.scope, RelationGraphScope::WriterPov { .. }) { None } else { Some(seen_claims.len()) }, "visitedNodes": visited.len(),
                "claimsOnly": true
            }
        }))
    }
}

fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}
