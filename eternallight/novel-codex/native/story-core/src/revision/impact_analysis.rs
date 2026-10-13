use std::collections::{BTreeMap, VecDeque};

use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};

use crate::dependency_rules::{DEPENDENCY_RULE_VERSION, RelationTraversalPolicy};

use crate::impact_text::literal_impact_candidates;
use crate::{
    BranchId, ProjectId, RelationDirection, RelationGraphQuery, RelationGraphScope,
    RepositoryError, RevisionId, SnapshotId, SqliteStoryRepository,
};

const MAX_PATH_STATES: usize = 1_000;
const MAX_FINDING_BYTES: usize = 64_000;

#[derive(Clone, Copy)]
pub enum ImpactAnalysisScope<'a> {
    Current,
    Snapshot(&'a SnapshotId),
}

pub struct ImpactAnalysisQuery<'a> {
    pub project_id: &'a ProjectId,
    pub scope: ImpactAnalysisScope<'a>,
    pub changed_entity_id: &'a str,
    pub world_time: Option<i64>,
    pub max_hops: usize,
    pub limit: usize,
    pub revision_pair: Option<ImpactRevisionPair<'a>>,
}

#[derive(Clone, Copy)]
pub struct ImpactRevisionPair<'a> {
    pub from_revision_id: &'a RevisionId,
    pub to_revision_id: &'a RevisionId,
}

struct PathState<'a> {
    node: &'a str,
    visited: Vec<&'a str>,
    edges: Vec<&'a Value>,
}

impl SqliteStoryRepository {
    

    pub(crate) fn analyze_explicit_impact_in_snapshot(
        &self,
        query: ImpactAnalysisQuery<'_>,
    ) -> Result<Value, RepositoryError> {
        let main = BranchId::parse("main").expect("main branch id");
        let base_snapshot_id = match query.scope {
            ImpactAnalysisScope::Current => self
                .branch_head(query.project_id, &main)?
                .ok_or(RepositoryError::ProjectNotFound)?
                .snapshot_id
                .as_str()
                .to_owned(),
            ImpactAnalysisScope::Snapshot(snapshot_id) => snapshot_id.as_str().to_owned(),
        };
        let graph = self.relation_graph_with_policy(
            RelationGraphQuery {
                project_id: query.project_id,
                scope: match query.scope {
                    ImpactAnalysisScope::Current => RelationGraphScope::Current,
                    ImpactAnalysisScope::Snapshot(snapshot_id) => {
                        RelationGraphScope::Snapshot(snapshot_id)
                    }
                },
                entity_id: query.changed_entity_id,
                relation_type: None,
                direction: RelationDirection::Outgoing,
                world_time: query.world_time,
                max_hops: query.max_hops,
                limit: 100,
            },
            RelationTraversalPolicy::ExplicitDependencies,
        )?;
        let seed = graph["canonicalEntityId"]
            .as_str()
            .ok_or(RepositoryError::CorruptState)?;
        let mut outgoing: BTreeMap<&str, Vec<&Value>> = BTreeMap::new();
        for edge in graph["results"]
            .as_array()
            .ok_or(RepositoryError::CorruptState)?
        {
            let source = edge["fromId"]
                .as_str()
                .ok_or(RepositoryError::CorruptState)?;
            let _ = edge["toId"].as_str().ok_or(RepositoryError::CorruptState)?;
            outgoing.entry(source).or_default().push(edge);
        }
        let mut queue = VecDeque::from([PathState {
            node: seed,
            visited: vec![seed],
            edges: Vec::new(),
        }]);
        let mut findings = Vec::new();
        let mut finding_bytes = 0;
        let mut path_states = 0;
        let mut truncated_paths = false;
        while let Some(path) = queue.pop_front() {
            path_states += 1;
            if path_states > MAX_PATH_STATES {
                truncated_paths = true;
                break;
            }
            if path.edges.len() == query.max_hops {
                continue;
            }
            for edge in outgoing.get(path.node).into_iter().flatten() {
                let target = edge["toId"].as_str().ok_or(RepositoryError::CorruptState)?;
                if path.visited.contains(&target) {
                    continue;
                }
                let mut steps = path.edges.clone();
                steps.push(edge);
                let finding = json!({
                    "affectedEntityId": target,
                    "classification": "possibleImpact",
                    "hopCount": steps.len(),
                    "evidencePath": steps.iter().map(|step| Ok(json!({
                        "fromId": step["fromId"], "toId": step["toId"],
                        "relationType": step["relationType"],
                        "dependencyRuleId": format!("{DEPENDENCY_RULE_VERSION}/{}", step["relationType"].as_str().ok_or(RepositoryError::CorruptState)?),
                        "propositionId": step["propositionId"],
                        "sourceObjectId": step["sourceObjectId"],
                        "sourceRevisionId": step["sourceRevisionId"],
                        "modelingBatchId": step["modelingBatchId"],
                        "assertionIndex": step["assertionIndex"],
                        "confirmationState": step["confirmationState"],
                        "evidence": step["evidence"]
                    }))).collect::<Result<Vec<_>, RepositoryError>>()?
                });
                let bytes = finding.to_string().len();
                if findings.len() == query.limit || finding_bytes + bytes > MAX_FINDING_BYTES {
                    truncated_paths = true;
                    break;
                }
                finding_bytes += bytes;
                findings.push(finding);
                let mut visited = path.visited.clone();
                visited.push(target);
                queue.push_back(PathState {
                    node: target,
                    visited,
                    edges: steps,
                });
            }
            if truncated_paths {
                break;
            }
        }
        let explicit_graph_complete = graph["coverage"]["complete"] == true && !truncated_paths;
        let literal = literal_impact_candidates(self, query.project_id, query.scope, seed)?;
        let mut revision_delta = query
            .revision_pair
            .map(|pair| {
                self.draft_revision_delta(
                    query.project_id,
                    pair.from_revision_id,
                    pair.to_revision_id,
                )
            })
            .transpose()?;
        if let (Some(delta), Some(pair)) = (revision_delta.as_mut(), query.revision_pair) {
            let object_id = delta["objectId"]
                .as_str()
                .ok_or(RepositoryError::CorruptState)?;
            let (selected, required, binding) = match query.scope {
                ImpactAnalysisScope::Current => {
                    let selected = self.connection.query_row(
                            "SELECT COALESCE(candidate_revision_id, accepted_revision_id) FROM story_object_heads_v2 WHERE project_id = ?1 AND object_id = ?2",
                            params![query.project_id.as_str(), object_id],
                            |row| row.get::<_, Option<String>>(0),
                        ).optional().map_err(storage_error)?.flatten();
                    (selected, pair.to_revision_id.as_str(), "currentToRevision")
                }
                ImpactAnalysisScope::Snapshot(snapshot_id) => {
                    let selected = self.connection.query_row(
                            "SELECT revision_id FROM story_snapshot_members_v2 WHERE snapshot_id = ?1 AND object_id = ?2",
                            params![snapshot_id.as_str(), object_id],
                            |row| row.get::<_, String>(0),
                        ).optional().map_err(storage_error)?;
                    (
                        selected,
                        pair.from_revision_id.as_str(),
                        "snapshotFromRevision",
                    )
                }
            };
            if selected.as_deref() != Some(required) {
                return Err(RepositoryError::InvalidContent(
                    "revision pair does not match the analysis source".into(),
                ));
            }
            let model_candidate_delta = self.draft_model_candidate_delta(
                query.project_id,
                object_id,
                pair.from_revision_id,
                pair.to_revision_id,
            )?;
            delta["scopeBinding"] = json!(binding);
            delta["modelCandidateDelta"] = model_candidate_delta;
        }
        Ok(json!({
            "projectId": query.project_id.as_str(),
            "branchId": "main", "baseSnapshotId": base_snapshot_id,
            "changedEntityId": query.changed_entity_id,
            "canonicalEntityId": seed,
            "worldTime": query.world_time,
            "dependencyRuleVersion": DEPENDENCY_RULE_VERSION,
            "maxHops": query.max_hops,
            "findings": findings,
            "revisionDelta": revision_delta,
            "literalTextCandidates": literal["candidates"],
            "coverage": {
                "explicitGraphComplete": explicit_graph_complete,
                "globalImpactComplete": false,
                "graphCoverage": graph["coverage"],
                "truncatedPaths": truncated_paths,
                "pathStatesExamined": path_states.min(MAX_PATH_STATES),
                "literalSearch": literal["coverage"],
                "searchedNames": literal["searchedNames"],
                "missingScope": ["unlinkedProse", "semanticDependencies", "alternativeInterpretations"]
            },
            "authority": "readOnlyHypothesis"
        }))
    }
}

fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}

#[cfg(test)]
#[path = "impact_analysis_tests.rs"]
mod tests;

/// Public impact analysis capability; SQLite retains transaction ownership.
pub trait ImpactAnalysisRepository {
    /// Returns bounded, evidence-bearing possibilities, never a proof of complete impact.
    fn analyze_explicit_impact(
        &self,
        query: ImpactAnalysisQuery<'_>,
    ) -> Result<Value, RepositoryError>;
}

impl ImpactAnalysisRepository for SqliteStoryRepository {
    /// Returns bounded, evidence-bearing possibilities, never a proof of complete impact.
    fn analyze_explicit_impact(
        &self,
        query: ImpactAnalysisQuery<'_>,
    ) -> Result<Value, RepositoryError> {
        if !(1..=3).contains(&query.max_hops) || !(1..=50).contains(&query.limit) {
            return Err(RepositoryError::InvalidContent(
                "invalid impact analysis limits".into(),
            ));
        }
        self.with_read_snapshot(|| self.analyze_explicit_impact_in_snapshot(query))?
    }
}

use crate::{ProjectRevisionRepository, ReadSnapshotRepository, RevisionDeltaRepository};
