//! Advisory quality records. Never adopts prose, facts, or aesthetic judgments.
use super::QualityStore;
#[cfg(test)]
use crate::SqliteStoryRepository;
use crate::{ProjectId, RepositoryError, TaskRecord};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub(crate) const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS story_quality_state_v1 (
 project_id TEXT PRIMARY KEY REFERENCES story_projects_v2(project_id) ON DELETE CASCADE,
 version INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS story_quality_notes_v1 (
 id TEXT PRIMARY KEY, project_id TEXT NOT NULL REFERENCES story_projects_v2(project_id) ON DELETE CASCADE,
 ordinal INTEGER NOT NULL, body_json TEXT NOT NULL, status TEXT NOT NULL,
 author_kind TEXT NOT NULL, task_id TEXT, decisions_json TEXT NOT NULL DEFAULT '[]'
);
CREATE TABLE IF NOT EXISTS story_quality_packets_v1 (
 id TEXT PRIMARY KEY, project_id TEXT NOT NULL REFERENCES story_projects_v2(project_id) ON DELETE CASCADE,
 ordinal INTEGER NOT NULL, packet_json TEXT NOT NULL, sources_json TEXT NOT NULL,
 created_by_task_id TEXT, task_id TEXT UNIQUE, delivered_pages INTEGER NOT NULL DEFAULT 0, report_json TEXT,
 feedback_json TEXT NOT NULL DEFAULT '[]'
);
CREATE TABLE IF NOT EXISTS story_quality_requests_v1 (
 project_id TEXT NOT NULL REFERENCES story_projects_v2(project_id) ON DELETE CASCADE, request_key TEXT NOT NULL, digest TEXT NOT NULL, receipt_json TEXT NOT NULL,
 PRIMARY KEY(project_id, request_key)
);
"#;

#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct QualityEvidence {
    pub revision_id: String,
    pub quote: String,
    pub role: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct QualityNote {
    pub kind: String,
    pub scope: String,
    pub text: String,
    pub evidence: Vec<QualityEvidence>,
    pub question: Option<String>,
    pub body: Option<String>,
    pub relates_to: Option<String>,
    pub verdict: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct QualityPacketSource {
    pub revision_id: Option<String>,
    pub quote: Option<String>,
    pub note_id: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(tag = "action", rename_all = "camelCase", deny_unknown_fields)]
pub enum QualityAction {
    Note {
        note: QualityNote,
    },
    Decide {
        id: String,
        status: String,
        reason: String,
    },
    Packet {
        kind: String,
        sources: Vec<QualityPacketSource>,
    },
    Feedback {
        id: String,
        preference: String,
        reason: String,
    },
}
#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct QualityMutation {
    pub expected_version: i64,
    pub idempotency_key: String,
    pub change: QualityAction,
}

fn invalid(message: &str) -> RepositoryError {
    RepositoryError::InvalidContent(message.into())
}
fn storage(e: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(e.to_string())
}
fn text(value: &str, max: usize) -> bool {
    !value.trim().is_empty()
        && value.len() <= max
        && !value
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\t' | '\r'))
}
fn hash(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
// Bound escaped records as they will travel inside an MCP text result.
fn model_record_fits(value: &Value) -> bool {
    json!(value.to_string()).to_string().len() <= 6000
}
fn parse(value: &str) -> Result<Value, RepositoryError> {
    serde_json::from_str(value).map_err(|_| RepositoryError::CorruptState)
}
fn version(db: &Connection, project: &ProjectId) -> Result<i64, RepositoryError> {
    db.query_row("SELECT COALESCE((SELECT version FROM story_quality_state_v1 WHERE project_id=?1),0) FROM story_projects_v2 WHERE project_id=?1",[project.as_str()],|r|r.get(0)).map_err(storage)
}
fn source(
    db: &Connection,
    project: &ProjectId,
    id: &str,
) -> Result<(String, bool), RepositoryError> {
    let (raw,current):(String,bool)=db.query_row("SELECT r.content_json,EXISTS(SELECT 1 FROM story_object_heads_v2 h LEFT JOIN story_working_baselines_v2 b ON b.project_id=h.project_id AND b.object_kind='draft' AND b.object_id=substr(h.object_id,7) WHERE h.project_id=r.project_id AND h.object_id=r.object_id AND COALESCE(b.revision_id,h.candidate_revision_id,h.accepted_revision_id)=r.revision_id) FROM story_revisions_v2 r WHERE r.project_id=?1 AND r.revision_id=?2 AND r.object_kind='draft'",params![project.as_str(),id],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(storage)?.ok_or_else(||invalid("Quality evidence must reference a draft in this project"))?;
    let value = parse(&raw)?;
    let payload = value.get("payload").unwrap_or(&value);
    let body = payload["body"]
        .as_str()
        .map(str::to_owned)
        .or_else(|| {
            payload["sceneGroups"]
                .as_array()
                .and_then(|groups| {
                    groups
                        .iter()
                        .map(|g| g["body"].as_str().map(str::trim))
                        .collect::<Option<Vec<_>>>()
                })
                .map(|groups| groups.join("\n\n---\n\n"))
        })
        .ok_or_else(|| invalid("Draft has no canonical text"))?;
    Ok((body, current))
}
fn evidence(
    db: &Connection,
    project: &ProjectId,
    items: &[QualityEvidence],
) -> Result<bool, RepositoryError> {
    if items.len() > 8 || (!items.is_empty() && !items.iter().any(|e| e.role == "current")) {
        return Err(invalid(
            "Use at most eight quotes with at least one current anchor; historical comparisons use reference",
        ));
    }
    let mut current = true;
    for item in items {
        if !matches!(item.role.as_str(), "current" | "reference") {
            return Err(invalid("Evidence role must be current or reference"));
        }
        let (body, live) = source(db, project, &item.revision_id)?;
        if !text(&item.quote, 2000) || body.matches(&item.quote).count() != 1 {
            return Err(invalid(
                "Evidence must uniquely match the frozen draft verbatim",
            ));
        }
        if item.role == "current" {
            current &= live;
        }
    }
    Ok(current)
}
fn authorize(task: &TaskRecord, project: &ProjectId) -> Result<(), RepositoryError> {
    if task.project_id != *project
        || task.operation != "advisory"
        || !matches!(task.status.as_str(), "queued" | "running")
    {
        return Err(RepositoryError::TaskOutputForbidden);
    }
    Ok(())
}
fn note(
    db: &Connection,
    project: &ProjectId,
    id: &str,
) -> Result<(QualityNote, String), RepositoryError> {
    let (raw, status): (String, String) = db
        .query_row(
            "SELECT body_json,status FROM story_quality_notes_v1 WHERE project_id=?1 AND id=?2",
            params![project.as_str(), id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(storage)?
        .ok_or_else(|| invalid("Unknown quality note"))?;
    Ok((
        serde_json::from_str(&raw).map_err(|_| RepositoryError::CorruptState)?,
        status,
    ))
}

// Work membership is storage authority, never a model-supplied target or thread.
// Methods and observations of them may be consulted across Attempts in that Work;
// other task-local notes retain their original isolation and every scope is intact.
const VISIBLE_NOTES: &str = r#"
WITH visible_notes AS (
 SELECT n.* FROM story_quality_notes_v1 n WHERE n.project_id=?1 AND (
  ?2 IS NULL OR json_extract(n.body_json,'$.scope')='work' OR n.task_id=?2 OR (
   ?3 IS NOT NULL AND (
    json_extract(n.body_json,'$.kind')='method' OR (
     json_extract(n.body_json,'$.kind')='observation' AND EXISTS (
      SELECT 1 FROM story_quality_notes_v1 m WHERE m.project_id=n.project_id
      AND m.id=json_extract(n.body_json,'$.relatesTo') AND json_extract(m.body_json,'$.kind')='method'
     )
    )
   ) AND EXISTS (
    SELECT 1 FROM story_work_attempts_v1 a JOIN story_work_steps_v1 s ON s.id=a.step_id
    JOIN story_work_tasks_v1 w ON w.id=s.work_task_id
    WHERE a.attempt_id=n.task_id AND w.id=?3 AND w.project_id=n.project_id
   )
  )
 )
)
"#;

fn task_work(
    db: &Connection,
    project: &ProjectId,
    task: &str,
) -> Result<Option<String>, RepositoryError> {
    db.query_row(
        "SELECT w.id FROM story_work_attempts_v1 a JOIN story_work_steps_v1 s ON s.id=a.step_id JOIN story_work_tasks_v1 w ON w.id=s.work_task_id WHERE a.attempt_id=?1 AND w.project_id=?2",
        params![task, project.as_str()],
        |row| row.get(0),
    ).optional().map_err(storage)
}

fn note_visible(
    db: &Connection,
    project: &ProjectId,
    task: &TaskRecord,
    id: &str,
) -> Result<bool, RepositoryError> {
    let work = task_work(db, project, &task.task_id)?;
    db.query_row(
        &format!("{VISIBLE_NOTES} SELECT EXISTS(SELECT 1 FROM visible_notes WHERE id=?4)"),
        params![project.as_str(), task.task_id, work, id],
        |row| row.get(0),
    )
    .map_err(storage)
}

impl QualityStore<'_> {
    pub fn quality_notebook(
        &self,
        project: &ProjectId,
        before: Option<i64>,
        task: Option<&TaskRecord>,
    ) -> Result<Value, RepositoryError> {
        if let Some(task) = task {
            if task.project_id != *project
                || !matches!(
                    task.operation.as_str(),
                    "advisory"
                        | "generateChapter"
                        | "generateBrief"
                        | "generateOutline"
                        | "generatePlan"
                )
                || !matches!(task.status.as_str(), "queued" | "running")
            {
                return Err(RepositoryError::TaskOutputForbidden);
            }
            crate::task_input::assert_task_inputs_current(&self.connection, task)?;
        }
        crate::read_snapshot::with_connection_snapshot(&self.connection, || {
            let db = &self.connection;
            let version = version(db, project)?;
            let limit = if task.is_some() { 1 } else { 20 };
            let upper = before.unwrap_or(i64::MAX);
            let bound_task = task.map(|t| t.task_id.as_str());
            let bound_work = bound_task
                .map(|id| task_work(db, project, id))
                .transpose()?
                .flatten();
            let mut cursor_query=db.prepare(&format!("{VISIBLE_NOTES} SELECT ordinal FROM (SELECT ordinal FROM visible_notes UNION ALL SELECT ordinal FROM story_quality_packets_v1 WHERE project_id=?1) WHERE ordinal<?4 ORDER BY ordinal DESC LIMIT 21")).map_err(storage)?;
            let ordinals = cursor_query
                .query_map(
                    params![project.as_str(), bound_task, bound_work, upper],
                    |r| r.get::<_, i64>(0),
                )
                .map_err(storage)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(storage)?;
            let next = if ordinals.len() > limit {
                Some(ordinals[limit - 1])
            } else {
                None
            };
            let lower = ordinals
                .get(limit - 1)
                .or(ordinals.last())
                .copied()
                .unwrap_or(upper);
            let mut query=db.prepare(&format!("{VISIBLE_NOTES} SELECT id,ordinal,body_json,status,author_kind,task_id,decisions_json FROM visible_notes WHERE ordinal<?4 AND ordinal>=?5 ORDER BY ordinal DESC")).map_err(storage)?;
            let rows = query
                .query_map(
                    params![project.as_str(), bound_task, bound_work, upper, lower],
                    |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, i64>(1)?,
                            r.get::<_, String>(2)?,
                            r.get::<_, String>(3)?,
                            r.get::<_, String>(4)?,
                            r.get::<_, Option<String>>(5)?,
                            r.get::<_, String>(6)?,
                        ))
                    },
                )
                .map_err(storage)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(storage)?;
            let mut notes = Vec::new();
            for (id, ordinal, raw, status, author, task_id, decisions) in
                rows.into_iter().take(limit)
            {
                let item: QualityNote =
                    serde_json::from_str(&raw).map_err(|_| RepositoryError::CorruptState)?;
                let current = evidence(db, project, &item.evidence)?;
                let decisions = parse(&decisions)?;
                let history = decisions.as_array().ok_or(RepositoryError::CorruptState)?;
                let decision_limit = if task.is_some() { 0 } else { 3 };
                notes.push(json!({"id":id,"ordinal":ordinal,"note":item,"status":status,"authorKind":author,"taskId":task_id,"sourceCurrent":current,"decisions":history.iter().rev().take(decision_limit).collect::<Vec<_>>(),"decisionCount":history.len(),"decisionsTruncated":history.len()>decision_limit}));
            }
            let mut stmt=db.prepare("SELECT id,packet_json,task_id,report_json,feedback_json,sources_json FROM story_quality_packets_v1 WHERE project_id=?1 AND ordinal<?2 AND ordinal>=?3 ORDER BY ordinal DESC").map_err(storage)?;
            let packets = stmt
                .query_map(params![project.as_str(), upper, lower], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, Option<String>>(2)?,
                        r.get::<_, Option<String>>(3)?,
                        r.get::<_, String>(4)?,
                        r.get::<_, String>(5)?,
                    ))
                })
                .map_err(storage)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(storage)?;
            let mut summaries = Vec::new();
            for (id, raw, task_id, report, feedback, sources) in packets {
                let packet = parse(&raw)?;
                let sources: Vec<QualityPacketSource> =
                    serde_json::from_str(&sources).map_err(|_| RepositoryError::CorruptState)?;
                let current = sources.iter().all(|s| {
                    s.revision_id
                        .as_ref()
                        .is_none_or(|id| source(db, project, id).is_ok_and(|(_, live)| live))
                        && s.note_id.as_ref().is_none_or(|id| {
                            note(db, project, id).is_ok_and(|(n, status)| {
                                status != "withdrawn"
                                    && evidence(db, project, &n.evidence).unwrap_or(false)
                            })
                        })
                });
                let status = task_id
                    .as_ref()
                    .map(|id| {
                        db.query_row(
                            "SELECT status FROM story_tasks_v2 WHERE task_id=?1",
                            [id],
                            |r| r.get::<_, String>(0),
                        )
                        .map_err(storage)
                    })
                    .transpose()?;
                summaries.push(json!({"id":id,"kind":packet["kind"],"labels":packet["items"].as_array().unwrap().iter().map(|i|i["label"].clone()).collect::<Vec<_>>(),"taskId":task_id,"status":status.unwrap_or("prepared".into()),"sourceCurrent":current,"hasReport":report.is_some(),"feedbackCount":parse(&feedback)?.as_array().unwrap().len(),"cost":"seeTaskUsage"}));
            }
            Ok(
                json!({"version":version,"notes":notes,"nextCursor":next,"packets":summaries,"authority":"advisoryOnly","qualityGain":"unvalidated"}),
            )
        })?
    }

    pub fn change_quality_notebook(
        &mut self,
        project: &ProjectId,
        request: QualityMutation,
        task: Option<&TaskRecord>,
    ) -> Result<Value, RepositoryError> {
        if !text(&request.idempotency_key, 160)
            || serde_json::to_string(&request)
                .map_err(|_| invalid("Invalid request"))?
                .len()
                > 28000
        {
            return Err(invalid("Quality request exceeds budget"));
        }
        if let Some(task) = task {
            authorize(task, project)?;
            crate::task_input::assert_task_inputs_current(&self.connection, task)?;
        }
        let tx = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage)?;
        if let Some(task) = task {
            let fresh = crate::tasks::load_task(&tx, &task.task_id)?;
            authorize(&fresh, project)?;
            crate::task_input::assert_task_inputs_current(&tx, &fresh)?;
        }
        let digest=hash(&json!({"request":request,"author":if task.is_some(){"model"}else{"author"},"task":task.map(|t|&t.task_id)}).to_string());
        let replay:Option<(String,String)>=tx.query_row("SELECT digest,receipt_json FROM story_quality_requests_v1 WHERE project_id=?1 AND request_key=?2",params![project.as_str(),request.idempotency_key],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(storage)?;
        if let Some((prior, receipt)) = replay {
            if prior != digest {
                return Err(invalid("Idempotency key belongs to a different request"));
            }
            let receipt = parse(&receipt)?;
            if let Some(task) = task {
                crate::recovery_operations::commit_operation(
                    &tx,
                    self.recovery_operation.as_ref(),
                    project,
                    &task.task_id,
                    &["record_quality_note"],
                    &format!(
                        "runtime:quality:{}",
                        receipt["result"]["id"]
                            .as_str()
                            .ok_or(RepositoryError::CorruptState)?
                    ),
                )?;
            }
            tx.commit().map_err(storage)?;
            return Ok(receipt);
        }
        let prior = version(&tx, project)?;
        if prior != request.expected_version {
            return Err(invalid("Quality notebook changed; reread before editing"));
        }
        let next = prior + 1;
        let id = format!("quality-{}", Uuid::new_v4().simple());
        let result = match &request.change {
            QualityAction::Note { note: item } => {
                if !model_record_fits(&json!(item)) {
                    return Err(invalid(
                        "Quality note exceeds the 6KB escaped record budget; keep evidence and trial prose focused",
                    ));
                }
                if !matches!(
                    item.kind.as_str(),
                    "preference" | "goal" | "strength" | "observation" | "trial" | "method"
                ) || !matches!(item.scope.as_str(), "work" | "task")
                    || !text(&item.text, 1600)
                {
                    return Err(invalid("Invalid note kind, scope or text"));
                }
                if !matches!(item.kind.as_str(), "preference" | "method")
                    && item.evidence.is_empty()
                {
                    return Err(invalid("This quality note needs actual draft evidence"));
                }
                evidence(&tx, project, &item.evidence)?;
                if matches!(item.kind.as_str(), "trial" | "method") {
                    if !item.question.as_ref().is_some_and(|v| text(v, 800))
                        || !item.body.as_ref().is_some_and(|v| text(v, 8000))
                    {
                        return Err(invalid(
                            "A trial needs its reading question and prose; a method draft needs its applicability question and bounded method text",
                        ));
                    }
                } else if item.body.is_some() || item.question.is_some() {
                    return Err(invalid(
                        "Only trials and method drafts contain text and questions",
                    ));
                }
                if item.kind == "method" && task.is_some() && item.scope != "task" {
                    return Err(invalid(
                        "Model-created methods remain drafts for this Native Work, stored with task scope; only the author can choose persistent work scope",
                    ));
                }
                if let Some(target) = &item.relates_to {
                    let (related, status) = note(&tx, project, target)?;
                    if !matches!(related.kind.as_str(), "goal" | "strength" | "method")
                        || status == "withdrawn"
                        || item.kind != "observation"
                        || !matches!(
                            item.verdict.as_deref(),
                            Some("supported" | "unmet" | "insufficientEvidence")
                        )
                    {
                        return Err(invalid(
                            "Assessment must reference an active goal, strength or method and declare its verdict",
                        ));
                    }
                    if related.kind == "method" {
                        if let Some(task) = task {
                            if item.scope != "task" || !note_visible(&tx, project, task, target)? {
                                return Err(invalid(
                                    "Model method observations must stay task-scoped and reference a method visible through the same Native Work",
                                ));
                            }
                        }
                    }
                } else if item.verdict.is_some() {
                    return Err(invalid("A verdict requires a goal, strength or method"));
                }
                let count: i64 = tx
                    .query_row(
                        "SELECT COUNT(*) FROM story_quality_notes_v1 WHERE project_id=?1",
                        [project.as_str()],
                        |r| r.get(0),
                    )
                    .map_err(storage)?;
                if count >= 1000 {
                    return Err(invalid("Quality notebook is full"));
                }
                tx.execute("INSERT INTO story_quality_notes_v1(id,project_id,ordinal,body_json,status,author_kind,task_id) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![id,project.as_str(),next,serde_json::to_string(item).unwrap(),if task.is_some(){"proposed"}else{"confirmed"},if task.is_some(){"model"}else{"author"},task.map(|t|&t.task_id)]).map_err(storage)?;
                json!({"id":id})
            }
            QualityAction::Decide { id, status, reason } => {
                if task.is_some()
                    || !matches!(status.as_str(), "confirmed" | "withdrawn")
                    || !text(reason, 1000)
                {
                    return Err(RepositoryError::TaskOutputForbidden);
                }
                let (item, _) = note(&tx, project, id)?;
                if status == "confirmed" && !evidence(&tx, project, &item.evidence)? {
                    return Err(invalid(
                        "Source changed; review current prose before confirming",
                    ));
                }
                tx.execute("UPDATE story_quality_notes_v1 SET status=?1,decisions_json=json_insert(decisions_json,'$[#]',json(?2)) WHERE project_id=?3 AND id=?4",params![status,json!({"version":next,"status":status,"reason":reason}).to_string(),project.as_str(),id]).map_err(storage)?;
                json!({"id":id})
            }
            QualityAction::Packet { kind, sources } => {
                if !matches!(kind.as_str(), "reading" | "comparison")
                    || sources.is_empty()
                    || sources.len() > 6
                    || (kind == "comparison" && sources.len() < 2)
                {
                    return Err(invalid(
                        "Use one to six reading sources, or two to six comparison candidates",
                    ));
                }
                let count: i64 = tx
                    .query_row(
                        "SELECT COUNT(*) FROM story_quality_packets_v1 WHERE project_id=?1",
                        [project.as_str()],
                        |r| r.get(0),
                    )
                    .map_err(storage)?;
                if count >= 200 {
                    return Err(invalid("Reading packet limit reached"));
                }
                let mut rows = Vec::new();
                let mut seen = std::collections::HashSet::new();
                let mut question = None;
                for entry in sources {
                    let body = match (&entry.revision_id, &entry.note_id) {
                        (Some(revision), None) => {
                            let (body, current) = source(&tx, project, revision)?;
                            if !current {
                                return Err(invalid(
                                    "Reader packets require current source revisions",
                                ));
                            }
                            if let Some(quote) = &entry.quote {
                                if !text(quote, 20000) || body.matches(quote).count() != 1 {
                                    return Err(invalid(
                                        "Reader excerpt must uniquely match its source",
                                    ));
                                }
                                quote.clone()
                            } else {
                                body
                            }
                        }
                        (None, Some(id)) if entry.quote.is_none() => {
                            let (item, status) = note(&tx, project, id)?;
                            if item.kind != "trial"
                                || status == "withdrawn"
                                || !evidence(&tx, project, &item.evidence)?
                            {
                                return Err(invalid("Comparison source must be a current trial"));
                            }
                            if kind == "comparison" {
                                if question
                                    .as_ref()
                                    .is_some_and(|q| Some(q) != item.question.as_ref())
                                {
                                    return Err(invalid(
                                        "Compare trials of the same creative question",
                                    ));
                                }
                                question = item.question;
                            }
                            item.body.ok_or(RepositoryError::CorruptState)?
                        }
                        _ => return Err(invalid("Select exactly one draft revision or trial")),
                    };
                    if !text(&body, 24000) || !seen.insert(hash(&body)) {
                        return Err(invalid(
                            "Reading sources must be distinct, nonempty and bounded; select a precise excerpt for long drafts",
                        ));
                    }
                    rows.push((hash(&format!("{}:{}", id, rows.len())), body, entry.clone()));
                }
                if kind == "comparison" {
                    rows.sort_by(|a, b| a.0.cmp(&b.0));
                }
                let items:Vec<_>=rows.iter().enumerate().map(|(index,(_,body,_))|json!({"label":((b'A'+index as u8)as char).to_string(),"text":body})).collect();
                let packet = json!({"kind":kind,"items":items,"coverage":"onlySuppliedText","authority":"readerHypotheses"});
                if packet.to_string().len() > 30000 {
                    return Err(invalid(
                        "Reading packet exceeds 30KB; choose a smaller explicit scope",
                    ));
                }
                let ordered: Vec<_> = rows.into_iter().map(|(_, _, source)| source).collect();
                tx.execute("INSERT INTO story_quality_packets_v1(id,project_id,ordinal,packet_json,sources_json,created_by_task_id) VALUES(?1,?2,?3,?4,?5,?6)",params![id,project.as_str(),next,packet.to_string(),serde_json::to_string(&ordered).unwrap(),task.map(|t|&t.task_id)]).map_err(storage)?;
                json!({"id":id,"status":"prepared"})
            }
            QualityAction::Feedback {
                id,
                preference,
                reason,
            } => {
                if task.is_some() || !text(reason, 1600) {
                    return Err(RepositoryError::TaskOutputForbidden);
                }
                let raw:String=tx.query_row("SELECT packet_json FROM story_quality_packets_v1 WHERE project_id=?1 AND id=?2",params![project.as_str(),id],|r|r.get(0)).optional().map_err(storage)?.ok_or_else(||invalid("Unknown reading packet"))?;
                let packet = parse(&raw)?;
                let count:i64=tx.query_row("SELECT json_array_length(feedback_json) FROM story_quality_packets_v1 WHERE project_id=?1 AND id=?2",params![project.as_str(),id],|r|r.get(0)).map_err(storage)?;
                if count >= 100 {
                    return Err(invalid("Feedback history is full"));
                }
                if !valid_preference(&packet, preference) {
                    return Err(invalid("Choose a packet label, tie or abstain"));
                }
                tx.execute("UPDATE story_quality_packets_v1 SET feedback_json=json_insert(feedback_json,'$[#]',json(?1)) WHERE project_id=?2 AND id=?3",params![json!({"reviewerKind":"author","isolation":"labelsOnly","preference":preference,"reason":reason,"version":next}).to_string(),project.as_str(),id]).map_err(storage)?;
                json!({"id":id})
            }
        };
        tx.execute("INSERT INTO story_quality_state_v1 VALUES(?1,?2) ON CONFLICT(project_id) DO UPDATE SET version=excluded.version",params![project.as_str(),next]).map_err(storage)?;
        let receipt = json!({"version":next,"result":result,"authority":"advisoryOnly"});
        if let Some(task) = task {
            crate::recovery_operations::commit_operation(
                &tx,
                self.recovery_operation.as_ref(),
                project,
                &task.task_id,
                &["record_quality_note"],
                &format!(
                    "runtime:quality:{}",
                    receipt["result"]["id"]
                        .as_str()
                        .ok_or(RepositoryError::CorruptState)?
                ),
            )?;
        }
        tx.execute(
            "INSERT INTO story_quality_requests_v1 VALUES(?1,?2,?3,?4)",
            params![
                project.as_str(),
                request.idempotency_key,
                digest,
                receipt.to_string()
            ],
        )
        .map_err(storage)?;
        tx.commit().map_err(storage)?;
        Ok(receipt)
    }
    pub fn quality_packet(&self, project: &ProjectId, id: &str) -> Result<Value, RepositoryError> {
        let raw: Option<String> = self
            .connection
            .query_row(
                "SELECT packet_json FROM story_quality_packets_v1 WHERE project_id=?1 AND id=?2",
                params![project.as_str(), id],
                |r| r.get(0),
            )
            .optional()
            .map_err(storage)?;
        parse(&raw.ok_or_else(|| invalid("Unknown reading packet"))?)
    }
    pub fn quality_review(&self, project: &ProjectId, id: &str) -> Result<Value, RepositoryError> {
        let (report,feedback,sources,task):(Option<String>,String,String,Option<String>)=self.connection.query_row("SELECT report_json,feedback_json,sources_json,task_id FROM story_quality_packets_v1 WHERE project_id=?1 AND id=?2",params![project.as_str(),id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional().map_err(storage)?.ok_or_else(||invalid("Unknown quality packet"))?;
        let all = parse(&feedback)?;
        let count = all.as_array().unwrap().len();
        let source_items: Vec<QualityPacketSource> =
            serde_json::from_str(&sources).map_err(|_| RepositoryError::CorruptState)?;
        let current = source_items.iter().all(|s| {
            s.revision_id
                .as_ref()
                .is_none_or(|id| source(&self.connection, project, id).is_ok_and(|(_, live)| live))
                && s.note_id.as_ref().is_none_or(|id| {
                    note(&self.connection, project, id).is_ok_and(|(n, status)| {
                        status != "withdrawn"
                            && evidence(&self.connection, project, &n.evidence).unwrap_or(false)
                    })
                })
        });
        let report = report.map(|r| parse(&r)).transpose()?;
        let model_preference = report
            .as_ref()
            .and_then(|r| r["report"]["preference"].as_str());
        let author_preference = all
            .as_array()
            .unwrap()
            .last()
            .and_then(|f| f["preference"].as_str());
        let disagreement = match (model_preference, author_preference) {
            (Some(m), Some(a)) if m != "abstain" && a != "abstain" => Some(m != a),
            _ => None,
        };
        let mapping:Vec<_>=parse(&sources)?.as_array().ok_or(RepositoryError::CorruptState)?.iter().enumerate().map(|(i,s)|json!({"label":((b'A'+i as u8)as char).to_string(),"revisionId":s["revisionId"],"noteId":s["noteId"]})).collect();
        Ok(
            json!({"id":id,"report":report,"latestAuthorFeedback":all.as_array().unwrap().iter().rev().take(3).collect::<Vec<_>>(),"feedbackCount":count,"feedbackTruncated":count>3,"sources":mapping,"sourceCurrent":current,"calibration":{"preferenceDisagreement":disagreement,"policy":"One author's choice is a preference signal, not evidence that every quality dimension improved. Do not erase disagreement or count repeated feedback as independent samples."},"usageTaskId":task,"qualityGain":"unvalidated","independentReviewers":"unknown"}),
        )
    }
}
fn valid_preference(packet: &Value, preference: &str) -> bool {
    matches!(preference, "tie" | "abstain")
        || packet["items"]
            .as_array()
            .is_some_and(|items| items.iter().any(|i| i["label"] == preference))
}

#[path = "quality_reader.rs"]
mod reader;
pub(crate) use reader::{bind_reader, complete_reader, reader_actions};

#[cfg(test)]
#[path = "quality_notebook_tests.rs"]
mod tests;
