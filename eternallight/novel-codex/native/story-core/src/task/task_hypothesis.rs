use rusqlite::{OptionalExtension, TransactionBehavior, params};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::{
    CreateHypothesis, HypothesisAssumption, KnowledgePerspective, KnowledgeStateQuery,
    RepositoryError, SnapshotId, SqliteStoryRepository, TaskRecord,
};

#[derive(Deserialize, ts_rs::TS)]
#[serde(
    tag = "action",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum Action {
    ReadBaseline {
        entity_id: Option<String>,
        cursor: Option<String>,
        limit: Option<usize>,
    },
    Simulate {
        baseline_token: String,
        intent: String,
        assumptions: Vec<HypothesisAssumption>,
        logical_key: String,
        limit: Option<usize>,
    },
}

#[derive(Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Page {
    intent_revision: i64,
    snapshot: String,
    entity_id: Option<String>,
    offset: usize,
    count: usize,
    signature: String,
}



/// Canonical immutable analysis evidence for this exact task, including terminal tasks.
/// Work delivery validates attempt ownership; this read never trusts outputRefs or current heads.
pub(crate) fn saved_evidence(
    db: &rusqlite::Connection,
    task: &TaskRecord,
) -> Result<Vec<String>, RepositoryError> {
    if task.operation != "advisory" {
        return Err(RepositoryError::TaskOutputForbidden);
    }
    let prefix = record_prefix(task);
    let mut statement = db.prepare("SELECT h.hypothesis_id,h.base_snapshot_id,h.intent,h.assumptions_json,h.request_digest,h.idempotency_key,
        s.project_id,s.branch_id FROM story_hypotheses_v1 h LEFT JOIN story_snapshots_v2 s ON s.snapshot_id=h.base_snapshot_id
        WHERE h.project_id=?1 AND substr(h.idempotency_key,1,?2)=?3 ORDER BY h.created_at,h.hypothesis_id LIMIT 17").map_err(storage)?;
    let rows = statement
        .query_map(
            params![task.project_id.as_str(), prefix.len(), prefix],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, Option<String>>(6)?,
                    r.get::<_, Option<String>>(7)?,
                ))
            },
        )
        .map_err(storage)?;
    let mut ids = Vec::new();
    for row in rows {
        let (id, snapshot, intent, assumptions, digest, key, project, branch) =
            row.map_err(storage)?;
        let assumptions: Vec<HypothesisAssumption> =
            serde_json::from_str(&assumptions).map_err(|_| RepositoryError::CorruptState)?;
        let suffix = key
            .strip_prefix(&prefix)
            .ok_or(RepositoryError::CorruptState)?;
        if ids.len() == 16
            || !id.starts_with("hypothesis-")
            || id.len() > 80
            || project.as_deref() != Some(task.project_id.as_str())
            || branch.as_deref() != Some("main")
            || suffix.len() != 64
            || !suffix
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
            || digest != crate::hypotheses::request_digest(&snapshot, &intent, &assumptions)
        {
            return Err(RepositoryError::CorruptState);
        }
        ids.push(id);
    }
    Ok(ids)
}
fn record_prefix(task: &TaskRecord) -> String {
    format!(
        "task-hypothesis-{}-",
        hash(&json!([task.project_id.as_str(), task.task_id]))
    )
}

fn hash(value: &Value) -> String {
    format!("{:x}", Sha256::digest(value.to_string().as_bytes()))
}
fn token(
    task: &TaskRecord,
    snapshot: &str,
    entity: &Option<String>,
    offset: usize,
    count: usize,
    intent_revision: i64,
) -> String {
    json!({"intentRevision":intent_revision,"snapshot":snapshot,"entityId":entity,"offset":offset,"count":count,
        "signature":hash(&json!([task.capability_hash,task.project_id.as_str(),task.task_id,snapshot,entity,offset,count,intent_revision]))}).to_string()
}
fn decode(
    text: &str,
    task: &TaskRecord,
    snapshot: &str,
    intent_revision: i64,
) -> Result<Page, RepositoryError> {
    if text.len() > 1_024 {
        return Err(invalid("invalid baseline token"));
    }
    let page: Page = serde_json::from_str(text).map_err(|_| invalid("invalid baseline token"))?;
    let expected: Value = serde_json::from_str(&token(
        task,
        snapshot,
        &page.entity_id,
        page.offset,
        page.count,
        intent_revision,
    ))
    .map_err(|_| RepositoryError::CorruptState)?;
    if page.intent_revision != intent_revision
        || page.snapshot != snapshot
        || page.signature != expected["signature"]
    {
        return Err(invalid(
            "baseline token belongs to another task, snapshot or query",
        ));
    }
    Ok(page)
}
fn invalid(message: &str) -> RepositoryError {
    RepositoryError::InvalidContent(message.into())
}
fn storage(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}

/// Public task hypothesis capability; SQLite retains transaction ownership.
pub trait TaskHypothesisRepository {
    /// Creates only immutable analysis records under a live advisory task's pinned inputs.
    /// Tokens bind exact delivered baseline pages; no caller snapshot or story write is accepted.
    fn task_hypothesis(
        &self,
        task: &TaskRecord,
        input: &Value,
        now: i64,
    ) -> Result<Value, RepositoryError>;
}

impl TaskHypothesisRepository for SqliteStoryRepository {
    /// Creates only immutable analysis records under a live advisory task's pinned inputs.
    /// Tokens bind exact delivered baseline pages; no caller snapshot or story write is accepted.
    fn task_hypothesis(
        &self,
        task: &TaskRecord,
        input: &Value,
        now: i64,
    ) -> Result<Value, RepositoryError> {
        if input.to_string().len() > 8_192 {
            return Err(invalid("hypothesis input exceeds 8KiB"));
        }
        let action: Action = serde_json::from_value(input.clone())
            .map_err(|_| invalid("invalid hypothesis action"))?;
        let tx =
            rusqlite::Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
                .map_err(storage)?;
        let current = self.authorize_task_capability(
            &task.project_id,
            &task.task_id,
            &task.thread_id,
            task.input_revision,
            &task.capability_hash,
            now,
        )?;
        if current.operation != "advisory"
            || serde_json::from_str::<Value>(&current.target_json)
                .map_err(|_| RepositoryError::CorruptState)?["type"]
                != "project"
        {
            return Err(RepositoryError::TaskOutputForbidden);
        }
        if matches!(action, Action::Simulate { .. })
            && serde_json::from_str::<Value>(&current.target_json)
                .map_err(|_| RepositoryError::CorruptState)?
                .get("workDecision")
                .is_some()
        {
            return Err(RepositoryError::TaskOutputForbidden);
        }
        self.assert_task_inputs_current(&current)?;
        let intent_revision: i64 = tx.query_row("SELECT COALESCE(MAX(intent_revision),0) FROM story_task_intents_v1 WHERE task_id=?1",[&task.task_id],|row|row.get(/*idx*/0)).map_err(storage)?;
        crate::task_intents::assert_action_intent(&tx, &task.task_id, intent_revision)?;
        let snapshot: String = tx.query_row("SELECT head_snapshot_id FROM story_branches_v2 WHERE project_id=?1 AND branch_id='main'",
            [task.project_id.as_str()],|row| row.get(/*idx*/0)).map_err(storage)?;
        let (entity, page) = match &action {
            Action::ReadBaseline {
                entity_id,
                cursor,
                limit,
            } => {
                if limit.is_some_and(|n| !(1..=16).contains(&n))
                    || entity_id
                        .as_ref()
                        .is_some_and(|id| id.is_empty() || id.len() > 160)
                {
                    return Err(invalid("invalid baseline limits"));
                }
                let page = cursor
                    .as_deref()
                    .map(|token| decode(token, &current, &snapshot, intent_revision))
                    .transpose()?;
                if page
                    .as_ref()
                    .is_some_and(|page| page.entity_id != *entity_id || page.count != 0)
                {
                    return Err(invalid("baseline cursor scope changed"));
                }
                (entity_id.clone(), page)
            }
            Action::Simulate { baseline_token, .. } => {
                let page = decode(baseline_token, &current, &snapshot, intent_revision)?;
                if !(1..=16).contains(&page.count) {
                    return Err(invalid("baseline page contains no usable references"));
                }
                (page.entity_id.clone(), Some(page))
            }
        };
        let snapshot_id =
            SnapshotId::parse(snapshot.clone()).map_err(|_| RepositoryError::CorruptState)?;
        let baseline = self.knowledge_state_in_snapshot(KnowledgeStateQuery {
            project_id: &task.project_id,
            snapshot_id: Some(&snapshot_id),
            perspective: KnowledgePerspective::Author,
            world_time: None,
            epistemic_time: None,
            reading_chapter: None,
            owner_id: None,
            entity_id: entity.as_deref(),
            predicate: None,
        })?;
        let entries = baseline["entries"]
            .as_array()
            .ok_or(RepositoryError::CorruptState)?;
        let offset = page.as_ref().map_or(/*default*/ 0, |page| page.offset);
        if offset > entries.len() {
            return Err(invalid("baseline cursor is outside the projection"));
        }
        let result = match action {
            Action::ReadBaseline { limit, .. } => {
                let end = (offset + limit.unwrap_or(/*default*/ 8)).min(entries.len());
                json!({"action":"readBaseline","baseSnapshotId":snapshot,"taskId":task.task_id,"intentRevision":intent_revision,
                    "entries":entries[offset..end],"identityRevisionId":baseline["identityRevisionId"],
                    "coverage":baseline["coverage"],"projectedEntryCount":entries.len(),
                    "baselineToken":token(&current,&snapshot,&entity,offset,end-offset,intent_revision),
                    "nextCursor":(end<entries.len()).then(||token(&current,&snapshot,&entity,end,/*count*/0,intent_revision)),
                    "truncated":end<entries.len() || baseline["coverage"]["complete"]!=true,
                    "authority":"fixedSnapshotEvidence","scopeNotice":"Only正文 and knowledge captured by this saved snapshot are covered. Chapters not captured here, later edits and later knowledge are outside its scope; missing evidence does not prove no impact."})
            }
            Action::Simulate {
                intent,
                assumptions,
                logical_key,
                limit,
                ..
            } => {
                let limit = limit.unwrap_or(/*default*/ 16);
                if !(1..=16).contains(&limit) {
                    return Err(invalid("invalid findings preview limit"));
                }
                if logical_key.is_empty() || logical_key.len() > 80 {
                    return Err(invalid("invalid hypothesis logicalKey"));
                }
                let end = offset + page.as_ref().ok_or(RepositoryError::CorruptState)?.count;
                let delivered = entries
                    .get(offset..end)
                    .ok_or_else(|| invalid("baseline references changed"))?;
                for assumption in &assumptions {
                    let visible = delivered.iter().any(|entry| match assumption {
                        HypothesisAssumption::SuppressClaim {
                            modeling_batch_id,
                            source_revision_id,
                            assertion_index,
                        } => {
                            entry["modelingBatchId"] == *modeling_batch_id
                                && entry["sourceRevisionId"] == *source_revision_id
                                && entry["assertionIndex"] == *assertion_index
                        }
                        HypothesisAssumption::AssumeClaim {
                            subject_id,
                            predicate,
                            ..
                        } => {
                            entry["assertion"]["subjectId"] == *subject_id
                                && entry["assertion"]["predicate"] == *predicate
                        }
                    });
                    if !visible {
                        return Err(invalid(
                            "assumption reference is not in the delivered baseline page; read its exact subject/property first",
                        ));
                    }
                }
                let prefix = record_prefix(&current);
                let key = format!("{prefix}{}", hash(&json!([intent_revision, logical_key])));
                let existing = tx.query_row("SELECT 1 FROM story_hypotheses_v1 WHERE project_id=?1 AND idempotency_key=?2",
                    params![task.project_id.as_str(),key], |_|Ok(())).optional().map_err(storage)?.is_some();
                let count: usize = tx.query_row("SELECT COUNT(*) FROM story_hypotheses_v1 WHERE project_id=?1 AND substr(idempotency_key,1,?2)=?3",
                    params![task.project_id.as_str(),prefix.len(),prefix],|row|row.get(/*idx*/0)).map_err(storage)?;
                if !existing && count >= 16 {
                    return Err(invalid(
                        "hypothesis task record limit reached (16); no new analysis was created",
                    ));
                }
                let receipt = crate::hypotheses::create_in_transaction(
                    &tx,
                    CreateHypothesis {
                        project_id: &task.project_id,
                        base_snapshot_id: &snapshot_id,
                        intent: &intent,
                        assumptions: &assumptions,
                        idempotency_key: &key,
                        now,
                    },
                )?;
                let full = self.read_hypothesis_in_snapshot(
                    &task.project_id,
                    receipt["id"]
                        .as_str()
                        .ok_or(RepositoryError::CorruptState)?,
                )?;
                let findings = full["simulation"]["findings"]
                    .as_array()
                    .ok_or(RepositoryError::CorruptState)?;
                let suppressed: Vec<_> = entries.iter().filter(|entry| assumptions.iter().any(|a|
                    matches!(a,HypothesisAssumption::SuppressClaim {modeling_batch_id,source_revision_id,assertion_index}
                        if entry["modelingBatchId"]==*modeling_batch_id && entry["sourceRevisionId"]==*source_revision_id && entry["assertionIndex"]==*assertion_index))).collect();
                let assumed: Vec<_> = full["simulation"]["entries"]
                    .as_array()
                    .ok_or(RepositoryError::CorruptState)?
                    .iter()
                    .filter(|entry| entry["confirmationState"] == "assumed")
                    .collect();
                let baseline_findings = full["baseline"]["findings"]
                    .as_array()
                    .ok_or(RepositoryError::CorruptState)?;
                json!({"action":"simulate","receipt":receipt,"baseSnapshotId":snapshot,"intent":intent,"intentRevision":intent_revision,"assumptions":assumptions,
                    "baseline":{"entryCount":full["baseline"]["entryCount"],"coverage":full["baseline"]["coverage"],
                        "findings":baseline_findings.iter().take(limit).collect::<Vec<_>>(),"findingCount":baseline_findings.len(),"findingsTruncated":baseline_findings.len()>limit},"suppressedEvidence":suppressed,
                    "simulation":{"assumedEntries":assumed,"retainedEntryCount":full["simulation"]["entries"].as_array().ok_or(RepositoryError::CorruptState)?.len()-assumed.len(),
                        "findings":findings.iter().take(limit).collect::<Vec<_>>(),"findingCount":findings.len(),"findingsTruncated":findings.len()>limit,
                        "coverage":full["simulation"]["coverage"]},"authority":"hypothesisOnly",
                    "scopeNotice":"Immutable local analysis record only; no正文, formal facts, identity decision or publication changes. Unshown/unknown consequences are not absence."})
            }
        };
        tx.commit().map_err(storage)?;
        Ok(result)
    }
}

use crate::{TaskInputRepository, TasksRepository};
