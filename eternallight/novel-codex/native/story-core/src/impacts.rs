use rusqlite::OptionalExtension;
use rusqlite::params;
use serde_json::Value;

use crate::ProjectId;
use crate::RepositoryError;
use crate::SqliteStoryRepository;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImpactPage {
    pub items: Vec<Value>,
    pub next_cursor: Option<usize>,
    pub total: i64,
    pub open: i64,
    pub retained: i64,
    pub locked_conflict: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolveImpactReceipt {
    pub commit_id: String,
    pub result_revision: i64,
}

pub struct ResolvePlanningImpact<'a> {
    pub project_id: &'a ProjectId,
    pub impact_id: &'a str,
    pub expected_revision: i64,
    pub resolution: &'a str,
    pub note: Option<&'a str>,
    pub idempotency_key: &'a str,
    pub request_digest: &'a str,
}

struct OpenPlanningImpact {
    impact_id: String,
    source_fact_id: String,
    source_commit_id: String,
    target_node_id: String,
    dependency_key: String,
    predicate: String,
    evidence: String,
    linked_to_candidate: bool,
}

impl SqliteStoryRepository {
    pub fn planning_impacts(
        &self,
        project_id: &ProjectId,
        cursor: usize,
        limit: usize,
    ) -> Result<ImpactPage, RepositoryError> {
        let limit = limit.clamp(1, 50);
        let (total, open, retained, locked_conflict) = self
            .connection
            .query_row(
                "SELECT COUNT(*), COALESCE(SUM(state = 'open'), 0), COALESCE(SUM(state = 'resolved-kept'), 0), COALESCE(SUM(state = 'open' AND severity = 'locked-conflict'), 0) FROM story_planning_impacts_v2 WHERE project_id = ?1 AND reverted_by_commit_id IS NULL AND state IN ('open','resolved-kept')",
                [project_id.as_str()],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?, row.get::<_, i64>(3)?)),
            )
            .map_err(storage_error)?;
        let mut statement = self.connection.prepare(
            "SELECT impact_id, source_commit_id, target_artifact_kind, target_revision_id, target_node_id, target_label, dependency_key, predicate, evidence, severity, state, resolution_note FROM story_planning_impacts_v2 WHERE project_id = ?1 AND reverted_by_commit_id IS NULL AND state IN ('open','resolved-kept') ORDER BY state = 'open' DESC, severity = 'locked-conflict' DESC, created_revision DESC, impact_id LIMIT ?2 OFFSET ?3",
        ).map_err(storage_error)?;
        let items = statement
            .query_map(
                params![project_id.as_str(), limit as i64, cursor as i64],
                |row| {
                    let predicate = row.get::<_, String>(7)?;
                    let dependency_key = row.get::<_, String>(6)?;
                    let severity = row.get::<_, String>(9)?;
                    Ok(serde_json::json!({
                        "id": row.get::<_, String>(0)?,
                        "sourceCommitId": row.get::<_, String>(1)?,
                        "targetArtifactKind": row.get::<_, String>(2)?,
                        "targetRevisionId": row.get::<_, String>(3)?,
                        "targetNodeId": row.get::<_, String>(4)?,
                        "targetLabel": row.get::<_, String>(5)?,
                        "dependencyKey": dependency_key,
                        "predicate": predicate,
                        "evidence": row.get::<_, String>(8)?,
                        "severity": severity,
                        "status": severity,
                        "state": row.get::<_, String>(10)?,
                        "resolutionNote": row.get::<_, Option<String>>(11)?,
                        "reason": format!("新确认事实“{predicate}”命中规划依赖“{dependency_key}”"),
                        "sourceChapterId": null,
                        "sourceChapterNumber": null,
                        "sourceChapterTitle": null
                    }))
                },
            )
            .map_err(storage_error)?
            .map(|row| row.map_err(storage_error))
            .collect::<Result<Vec<_>, _>>()?;
        let next = cursor + items.len();
        Ok(ImpactPage {
            items,
            next_cursor: (next < total as usize).then_some(next),
            total,
            open,
            retained,
            locked_conflict,
        })
    }

    pub fn resolve_planning_impact(
        &mut self,
        request: ResolvePlanningImpact<'_>,
    ) -> Result<ResolveImpactReceipt, RepositoryError> {
        let ResolvePlanningImpact {
            project_id,
            impact_id,
            expected_revision,
            resolution,
            note,
            idempotency_key,
            request_digest,
        } = request;
        if !matches!(resolution, "kept" | "false-positive") {
            return Err(RepositoryError::InvalidContent(
                "invalid planning impact resolution".into(),
            ));
        }
        let transaction = self.connection.transaction().map_err(storage_error)?;
        if let Some(result) = transaction
            .query_row(
                "SELECT request_digest, result_json FROM story_domain_receipts_v2 WHERE project_id = ?1 AND operation = 'resolve-impact' AND idempotency_key = ?2",
                params![project_id.as_str(), idempotency_key],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()
            .map_err(storage_error)?
        {
            if result.0 != request_digest {
                return Err(RepositoryError::IdempotencyConflict);
            }
            return serde_json::from_str::<Value>(&result.1)
                .map_err(|_| RepositoryError::CorruptState)
                .and_then(|value| {
                    Ok(ResolveImpactReceipt {
                        commit_id: value["commitId"]
                            .as_str()
                            .ok_or(RepositoryError::CorruptState)?
                            .to_owned(),
                        result_revision: value["resultRevision"]
                            .as_i64()
                            .ok_or(RepositoryError::CorruptState)?,
                    })
                });
        }
        let revision = transaction
            .query_row(
                "SELECT revision FROM story_workspaces_v2 WHERE project_id = ?1",
                [project_id.as_str()],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(storage_error)?
            .ok_or(RepositoryError::ProjectNotFound)?;
        if revision != expected_revision {
            return Err(RepositoryError::StaleProjectRevision {
                expected: expected_revision,
                actual: revision,
            });
        }
        let state = transaction
            .query_row(
                "SELECT state FROM story_planning_impacts_v2 WHERE project_id = ?1 AND impact_id = ?2 AND reverted_by_commit_id IS NULL",
                params![project_id.as_str(), impact_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(storage_error)?
            .ok_or(RepositoryError::CandidateNotFound)?;
        if state != "open" {
            return Err(RepositoryError::InvalidContent(
                "planning impact is already resolved".into(),
            ));
        }
        let result_revision = revision + 1;
        let commit_id = format!("impact-{}", uuid::Uuid::new_v4().simple());
        let next_state = if resolution == "kept" {
            "resolved-kept"
        } else {
            "resolved-false-positive"
        };
        transaction
            .execute(
                "UPDATE story_planning_impacts_v2 SET state = ?1, resolution_note = ?2, resolved_revision = ?3 WHERE impact_id = ?4",
                params![next_state, note, result_revision, impact_id],
            )
            .map_err(storage_error)?;
        transaction
            .execute(
                "UPDATE story_workspaces_v2 SET revision = ?1 WHERE project_id = ?2",
                params![result_revision, project_id.as_str()],
            )
            .map_err(storage_error)?;
        let result = serde_json::json!({
            "commitId": commit_id,
            "resultRevision": result_revision
        });
        transaction
            .execute(
                "INSERT INTO story_domain_receipts_v2(project_id, operation, idempotency_key, request_digest, result_json) VALUES (?1, 'resolve-impact', ?2, ?3, ?4)",
                params![project_id.as_str(), idempotency_key, request_digest, result.to_string()],
            )
            .map_err(storage_error)?;
        transaction.commit().map_err(storage_error)?;
        Ok(ResolveImpactReceipt {
            commit_id,
            result_revision,
        })
    }
}

pub(crate) fn record_impacts_for_fact(
    connection: &rusqlite::Connection,
    project_id: &ProjectId,
    source_commit_id: &str,
    fact: &Value,
    project_revision: i64,
) -> Result<Vec<Value>, RepositoryError> {
    let Some(fact_id) = fact.get("id").and_then(Value::as_str) else {
        return Err(RepositoryError::InvalidContent("fact id is missing".into()));
    };
    let Some(predicate) = fact.get("predicate").and_then(Value::as_str) else {
        return Ok(Vec::new());
    };
    let evidence = fact.get("evidence").and_then(Value::as_str).unwrap_or("");
    let mut created = Vec::new();
    for kind in ["outline", "plan"] {
        let object_id = format!("artifact-{kind}");
        let revision_id = connection
            .query_row(
                "SELECT accepted_revision_id FROM story_object_heads_v2 WHERE project_id = ?1 AND object_id = ?2",
                params![project_id.as_str(), object_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()
            .map_err(storage_error)?
            .flatten();
        let Some(revision_id) = revision_id else {
            continue;
        };
        let content = connection
            .query_row(
                "SELECT content_json FROM story_revisions_v2 WHERE project_id = ?1 AND revision_id = ?2",
                params![project_id.as_str(), revision_id],
                |row| row.get::<_, String>(0),
            )
            .map_err(storage_error)?;
        let envelope: Value = serde_json::from_str(&content)
            .map_err(|error| RepositoryError::InvalidContent(error.to_string()))?;
        let payload = envelope.get("payload").unwrap_or(&envelope);
        let entries = payload
            .get(if kind == "outline" {
                "nodes"
            } else {
                "chapters"
            })
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for entry in entries {
            let dependencies = entry
                .get("dependencyKeys")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            if !dependencies
                .iter()
                .any(|value| value.as_str() == Some(predicate))
            {
                continue;
            }
            let node_id = entry
                .get(if kind == "outline" {
                    "id"
                } else {
                    "chapterPlanId"
                })
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    RepositoryError::InvalidContent("planning node id is missing".into())
                })?;
            let label = entry
                .get("publicTitle")
                .or_else(|| entry.get("title"))
                .and_then(Value::as_str)
                .unwrap_or("规划节点");
            let protected = if kind == "outline" {
                entry.get("protection").and_then(Value::as_str) != Some("free")
            } else {
                entry.get("locked").and_then(Value::as_bool) == Some(true)
            };
            let severity = if protected {
                "locked-conflict"
            } else {
                "affected"
            };
            let impact_id = format!("planning-impact-{}", uuid::Uuid::new_v4().simple());
            let changed = connection.execute(
                "INSERT OR IGNORE INTO story_planning_impacts_v2(impact_id, project_id, source_fact_id, source_commit_id, target_artifact_kind, target_revision_id, target_node_id, target_label, dependency_key, predicate, evidence, severity, state, created_revision) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, 'open', ?13)",
                params![impact_id, project_id.as_str(), fact_id, source_commit_id, kind, revision_id, node_id, label, predicate, predicate, evidence, severity, project_revision],
            ).map_err(storage_error)?;
            if changed == 1 {
                created.push(serde_json::json!({"id": impact_id}));
            }
        }
    }
    Ok(created)
}

pub(crate) fn link_candidate_impacts(
    connection: &rusqlite::Connection,
    project_id: &ProjectId,
    object_kind: &str,
    candidate_revision_id: &crate::RevisionId,
    content_json: &str,
    impact_ids: &[String],
) -> Result<(), RepositoryError> {
    if impact_ids.is_empty() {
        return Ok(());
    }
    if !matches!(object_kind, "outline" | "plan") || impact_ids.len() > 50 {
        return Err(RepositoryError::InvalidContent(
            "planning impact links are invalid".into(),
        ));
    }
    let envelope: Value = serde_json::from_str(content_json)
        .map_err(|error| RepositoryError::InvalidContent(error.to_string()))?;
    let payload = envelope.get("payload").unwrap_or(&envelope);
    let entries = payload
        .get(if object_kind == "outline" {
            "nodes"
        } else {
            "chapters"
        })
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let key = if object_kind == "outline" {
        "id"
    } else {
        "chapterPlanId"
    };
    for impact_id in impact_ids {
        let target = connection
            .query_row(
                "SELECT target_artifact_kind, target_node_id FROM story_planning_impacts_v2 WHERE project_id = ?1 AND impact_id = ?2 AND state = 'open' AND reverted_by_commit_id IS NULL",
                params![project_id.as_str(), impact_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()
            .map_err(storage_error)?
            .ok_or(RepositoryError::CandidateNotFound)?;
        if target.0 != object_kind
            || !entries
                .iter()
                .any(|entry| entry.get(key).and_then(Value::as_str) == Some(target.1.as_str()))
        {
            return Err(RepositoryError::InvalidContent(
                "candidate does not preserve the impacted planning node".into(),
            ));
        }
        connection.execute(
            "INSERT INTO story_planning_impact_links_v2(impact_id, candidate_revision_id) VALUES (?1, ?2) ON CONFLICT(impact_id) DO UPDATE SET candidate_revision_id = excluded.candidate_revision_id",
            params![impact_id, candidate_revision_id.as_str()],
        ).map_err(storage_error)?;
    }
    Ok(())
}

pub(crate) fn resolve_linked_impacts_on_accept(
    connection: &rusqlite::Connection,
    project_id: &ProjectId,
    object_kind: &str,
    revision_id: &crate::RevisionId,
    result_revision: i64,
) -> Result<Vec<String>, RepositoryError> {
    if !matches!(object_kind, "outline" | "plan") {
        return Ok(Vec::new());
    }
    let content = connection
        .query_row(
            "SELECT content_json FROM story_revisions_v2 WHERE project_id = ?1 AND revision_id = ?2",
            params![project_id.as_str(), revision_id.as_str()],
            |row| row.get::<_, String>(0),
        )
        .map_err(storage_error)?;
    let envelope: Value = serde_json::from_str(&content)
        .map_err(|error| RepositoryError::InvalidContent(error.to_string()))?;
    let payload = envelope.get("payload").unwrap_or(&envelope);
    let entries = payload
        .get(if object_kind == "outline" {
            "nodes"
        } else {
            "chapters"
        })
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let id_key = if object_kind == "outline" {
        "id"
    } else {
        "chapterPlanId"
    };
    let mut statement = connection.prepare(
        "SELECT impact.impact_id, impact.source_fact_id, impact.source_commit_id, impact.target_node_id, impact.dependency_key, impact.predicate, impact.evidence, EXISTS(SELECT 1 FROM story_planning_impact_links_v2 link WHERE link.impact_id = impact.impact_id AND link.candidate_revision_id = ?3) FROM story_planning_impacts_v2 impact WHERE impact.project_id = ?1 AND impact.target_artifact_kind = ?2 AND impact.state = 'open' AND impact.reverted_by_commit_id IS NULL",
    ).map_err(storage_error)?;
    let impacts = statement
        .query_map(
            params![project_id.as_str(), object_kind, revision_id.as_str()],
            |row| {
                Ok(OpenPlanningImpact {
                    impact_id: row.get(0)?,
                    source_fact_id: row.get(1)?,
                    source_commit_id: row.get(2)?,
                    target_node_id: row.get(3)?,
                    dependency_key: row.get(4)?,
                    predicate: row.get(5)?,
                    evidence: row.get(6)?,
                    linked_to_candidate: row.get(7)?,
                })
            },
        )
        .map_err(storage_error)?
        .map(|row| row.map_err(storage_error))
        .collect::<Result<Vec<_>, _>>()?;
    drop(statement);
    let mut resolved = Vec::new();
    for impact in impacts {
        let entry = entries
            .iter()
            .find(|entry| {
                entry.get(id_key).and_then(Value::as_str) == Some(impact.target_node_id.as_str())
            })
            .ok_or(RepositoryError::ReviewChecksFailed)?;
        if impact.linked_to_candidate {
            connection.execute(
                "UPDATE story_planning_impacts_v2 SET state = 'resolved-updated', resolution_note = '已由链接候选修改并采用', resolved_revision = ?1 WHERE impact_id = ?2",
                params![result_revision, impact.impact_id],
            ).map_err(storage_error)?;
            resolved.push(impact.impact_id.clone());
        } else {
            let label = entry
                .get("publicTitle")
                .or_else(|| entry.get("title"))
                .and_then(Value::as_str)
                .unwrap_or("规划节点");
            let protected = if object_kind == "outline" {
                entry.get("protection").and_then(Value::as_str) != Some("free")
            } else {
                entry.get("locked").and_then(Value::as_bool) == Some(true)
            };
            let severity = if protected {
                "locked-conflict"
            } else {
                "affected"
            };
            connection.execute(
                "UPDATE story_planning_impacts_v2 SET state = 'superseded', resolution_note = '规划已采用新版本，影响已迁移', resolved_revision = ?1 WHERE impact_id = ?2",
                params![result_revision, impact.impact_id],
            ).map_err(storage_error)?;
            let successor_id = format!("planning-impact-{}", uuid::Uuid::new_v4().simple());
            connection.execute(
                "INSERT INTO story_planning_impacts_v2(impact_id, project_id, source_fact_id, source_commit_id, target_artifact_kind, target_revision_id, target_node_id, target_label, dependency_key, predicate, evidence, severity, state, created_revision) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, 'open', ?13)",
                params![successor_id, project_id.as_str(), impact.source_fact_id, impact.source_commit_id, object_kind, revision_id.as_str(), impact.target_node_id, label, impact.dependency_key, impact.predicate, impact.evidence, severity, result_revision],
            ).map_err(storage_error)?;
        }
        connection
            .execute(
                "DELETE FROM story_planning_impact_links_v2 WHERE impact_id = ?1",
                [impact.impact_id],
            )
            .map_err(storage_error)?;
    }
    Ok(resolved)
}

fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}
