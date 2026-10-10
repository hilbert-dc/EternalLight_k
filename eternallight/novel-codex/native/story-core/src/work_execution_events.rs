//! Durable result notification. This owns delivery identity, never creative decisions or grants.
use crate::authoring_runs::storage_error;
use crate::{ProjectId, RepositoryError, SqliteStoryRepository, TaskRecord};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub(crate) const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS story_work_execution_owners_v1 (
 work_task_id TEXT PRIMARY KEY REFERENCES story_work_tasks_v1(id) ON DELETE CASCADE,
 host_session_id TEXT NOT NULL, lease_id TEXT NOT NULL, expires_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS story_work_execution_events_v1 (
 sequence INTEGER PRIMARY KEY AUTOINCREMENT, event_id TEXT NOT NULL UNIQUE,
 work_task_id TEXT NOT NULL REFERENCES story_work_tasks_v1(id) ON DELETE CASCADE,
 source_step_id TEXT NOT NULL REFERENCES story_work_steps_v1(id) ON DELETE CASCADE,
 source_attempt_id TEXT NOT NULL REFERENCES story_tasks_v2(task_id) ON DELETE CASCADE,
 kind TEXT NOT NULL CHECK(kind='attemptSettled'), author_source_digest TEXT NOT NULL,
 created_at INTEGER NOT NULL,
 state TEXT NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','claimed','consumed')),
 claim_id TEXT, host_session_id TEXT, lease_id TEXT,
 consumer_step_id TEXT REFERENCES story_work_steps_v1(id),
 UNIQUE(source_attempt_id,kind)
);
CREATE INDEX IF NOT EXISTS story_work_execution_events_pending_v1
 ON story_work_execution_events_v1(state,sequence);
";
const LEASE_SECONDS: i64 = 30;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkExecutionOwnerRequest {
    pub host_session_id: String,
    pub expected_work_version: i64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkExecutionLeaseRequest {
    pub host_session_id: String,
    pub lease_id: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkExecutionClaimRequest {
    pub host_session_id: String,
    pub expected_work_version: i64,
    pub expected_project_revision: i64,
    pub expected_source_digest: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkExecutionReleaseRequest {
    pub host_session_id: String,
    pub claim_id: String,
    pub lease_id: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkExecutionAckRequest {
    pub host_session_id: String,
    pub claim_id: String,
    pub lease_id: String,
    #[serde(deserialize_with = "crate::deserialize_execution_limit")]
    pub consumer_step_id: Option<String>,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Cursor {
    project_id: String,
    sequence: i64,
}
struct Work {
    id: String,
    project: String,
    thread: String,
    kind: String,
    status: String,
    version: i64,
}
struct Event {
    id: String,
    work: String,
    step: String,
    attempt: String,
    author: String,
    created: i64,
    state: String,
    claim: Option<String>,
    session: Option<String>,
    lease: Option<String>,
}

fn invalid(message: &str) -> RepositoryError {
    RepositoryError::InvalidContent(format!("work execution event: {message}"))
}
fn identity(value: &str) -> Result<(), RepositoryError> {
    if value.is_empty()
        || value.len() > 160
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        return Err(invalid("invalid identity"));
    }
    Ok(())
}
fn author_digest(db: &Connection, work: &str) -> Result<String, RepositoryError> {
    let mut hash = Sha256::new();
    let mut stmt = db.prepare("SELECT message_id,intent_revision,author_text FROM story_author_messages_v1 WHERE work_task_id=?1 ORDER BY intent_revision,message_id").map_err(storage_error)?;
    for row in stmt
        .query_map([work], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .map_err(storage_error)?
    {
        let (id, revision, text) = row.map_err(storage_error)?;
        for value in [id, revision.to_string(), text] {
            hash.update((value.len() as u64).to_be_bytes());
            hash.update(value.as_bytes());
        }
    }
    Ok(format!("{:x}", hash.finalize()))
}

/// Called in the owning finish transaction; no backfill and no notification before commit.
pub(crate) fn record_terminal(db: &Connection, task: &TaskRecord) -> Result<(), RepositoryError> {
    if !matches!(task.status.as_str(), "completed" | "failed" | "cancelled") {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let (step, work): (String, String) = db.query_row("SELECT s.id,s.work_task_id FROM story_work_attempts_v1 a JOIN story_work_steps_v1 s ON s.id=a.step_id JOIN story_tasks_v2 t ON t.task_id=a.attempt_id WHERE a.attempt_id=?1 AND t.project_id=?2 AND t.status=?3", params![task.task_id, task.project_id.as_str(), task.status], |r| Ok((r.get(0)?, r.get(1)?))).map_err(storage_error)?;
    db.execute("INSERT INTO story_work_execution_events_v1(event_id,work_task_id,source_step_id,source_attempt_id,kind,author_source_digest,created_at) VALUES(?1,?2,?3,?4,'attemptSettled',?5,unixepoch()) ON CONFLICT(source_attempt_id,kind) DO NOTHING", params![format!("work-event-{}", uuid::Uuid::new_v4().simple()), work, step, task.task_id, author_digest(db,&work)?]).map_err(storage_error)?;
    Ok(())
}
fn work(db: &Connection, project: &ProjectId, id: &str) -> Result<Work, RepositoryError> {
    db.query_row("SELECT id,project_id,thread_id,kind,status,version FROM story_work_tasks_v1 WHERE project_id=?1 AND id=?2", params![project.as_str(), id], |r| Ok(Work {id:r.get(0)?,project:r.get(1)?,thread:r.get(2)?,kind:r.get(3)?,status:r.get(4)?,version:r.get(5)?})).optional().map_err(storage_error)?.ok_or(RepositoryError::TaskNotFound)
}
fn event(db: &Connection, project: &ProjectId, id: &str) -> Result<Event, RepositoryError> {
    db.query_row("SELECT e.event_id,e.work_task_id,e.source_step_id,e.source_attempt_id,e.author_source_digest,e.created_at,e.state,e.claim_id,e.host_session_id,e.lease_id FROM story_work_execution_events_v1 e JOIN story_work_tasks_v1 w ON w.id=e.work_task_id WHERE w.project_id=?1 AND e.event_id=?2", params![project.as_str(),id], |r| Ok(Event{id:r.get(0)?,work:r.get(1)?,step:r.get(2)?,attempt:r.get(3)?,author:r.get(4)?,created:r.get(5)?,state:r.get(6)?,claim:r.get(7)?,session:r.get(8)?,lease:r.get(9)?})).optional().map_err(storage_error)?.ok_or(RepositoryError::TaskNotFound)
}
fn work_json(w: &Work) -> Value {
    json!({"id":w.id,"projectId":w.project,"threadId":w.thread,"kind":w.kind,"status":w.status,"version":w.version})
}
fn owner(db: &Connection, id: &str) -> Result<Option<(String, String, i64)>, RepositoryError> {
    db.query_row("SELECT host_session_id,lease_id,expires_at FROM story_work_execution_owners_v1 WHERE work_task_id=?1",[id],|r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(storage_error)
}
fn metadata(db: &Connection, e: &Event, w: &Work) -> Result<Value, RepositoryError> {
    let expires = owner(db, &w.id)?
        .filter(|(s, l, _)| e.session.as_deref() == Some(s) && e.lease.as_deref() == Some(l))
        .map(|(_, _, expires)| expires);
    Ok(
        json!({"eventId":e.id,"workTaskId":w.id,"sourceStepId":e.step,"sourceAttemptId":e.attempt,"rootThreadId":w.thread,"kind":"attemptSettled","createdAt":e.created,"workVersion":w.version,"state":e.state,"claimId":e.claim,"hostSessionId":e.session,"leaseExpiresAt":expires}),
    )
}
fn lease_json(w: &Work, session: &str, lease: &str, expires: i64) -> Value {
    json!({"leaseId":lease,"hostSessionId":session,"workTaskId":w.id,"rootThreadId":w.thread,"expiresAt":expires})
}
fn acquire(db: &Connection, w: &Work, session: &str, now: i64) -> Result<Value, RepositoryError> {
    identity(&w.thread)?;
    let other_thread_owner: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM story_work_execution_owners_v1 o JOIN story_work_tasks_v1 other ON other.id=o.work_task_id WHERE other.project_id=?1 AND other.thread_id=?2 AND other.id<>?3 AND o.expires_at>?4)",params![w.project,w.thread,w.id,now],|row|row.get(0)).map_err(storage_error)?;
    if other_thread_owner { return Err(invalid("another Work owns this conversation")); }
    let lease = match owner(db, &w.id)? {
        Some((other, lease, expires)) if expires > now => {
            if other != session {
                return Err(invalid("active owner"));
            }
            lease
        }
        _ => format!("work-lease-{}", uuid::Uuid::new_v4().simple()),
    };
    db.execute("INSERT INTO story_work_execution_owners_v1(work_task_id,host_session_id,lease_id,expires_at) VALUES(?1,?2,?3,?4) ON CONFLICT(work_task_id) DO UPDATE SET host_session_id=excluded.host_session_id,lease_id=excluded.lease_id,expires_at=excluded.expires_at",params![w.id,session,lease,now+LEASE_SECONDS]).map_err(storage_error)?;
    Ok(lease_json(w, session, &lease, now + LEASE_SECONDS))
}
fn require_owner(
    db: &Connection,
    w: &Work,
    session: &str,
    lease: &str,
    now: i64,
) -> Result<(), RepositoryError> {
    identity(session)?;
    identity(lease)?;
    if !owner(db, &w.id)?.is_some_and(|(s, l, e)| s == session && l == lease && e > now) {
        return Err(invalid("stale execution lease"));
    }
    Ok(())
}
fn finished(db: &Connection, w: &Work) -> Result<bool, RepositoryError> {
    if w.status != "completed" {
        return Ok(false);
    }
    let pending:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM story_work_decisions_v1 WHERE work_task_id=?1 AND finished=0) OR EXISTS(SELECT 1 FROM story_generation_handoffs_v1 h JOIN story_work_attempts_v1 a ON a.attempt_id=h.source_task_id JOIN story_work_steps_v1 s ON s.id=a.step_id WHERE s.work_task_id=?1 AND h.consumed_run_id IS NULL)",[&w.id],|r|r.get(0)).map_err(storage_error)?;
    if pending {
        return Ok(false);
    }
    let closed:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM story_work_decisions_v1 WHERE work_task_id=?1 AND finished=1)",[&w.id],|r|r.get(0)).map_err(storage_error)?;
    let project = ProjectId::parse(w.project.clone()).map_err(|_| RepositoryError::CorruptState)?;
    Ok(closed || crate::work_decision_status::status(db, &project, &w.id)?["canFinish"] == true)
}
fn successor(db: &Connection, e: &Event, w: &Work) -> Result<Option<String>, RepositoryError> {
    db.query_row("SELECT s.id FROM story_work_steps_v1 s JOIN story_work_attempts_v1 a ON a.attempt_id=s.current_attempt_id JOIN story_work_attempts_v1 origin ON origin.attempt_id=?2 WHERE s.work_task_id=?1 AND a.sequence>origin.sequence AND (s.operation='advisory' OR s.depends_on_step_id=?3) ORDER BY a.sequence LIMIT 1",params![w.id,e.attempt,e.step],|r|r.get(0)).optional().map_err(storage_error)
}
fn safety_blocked(
    db: &Connection,
    e: &Event,
    w: &Work,
) -> Result<Option<&'static str>, RepositoryError> {
    if crate::work_interludes::top(db,&w.id)?.is_some() {return Ok(Some("interlude"));}
    if crate::work_pause_barrier::read(db, &w.id)?.is_some() || matches!(w.status.as_str(), "paused" | "cancelled") {
        return Ok(Some("authorStopped"));
    }
    if author_digest(db, &w.id)? != e.author {
        return Ok(Some("authorIntentChanged"));
    }
    for (sql, reason) in [
        (
            "SELECT EXISTS(SELECT 1 FROM story_task_intents_v1 i JOIN story_work_attempts_v1 a ON a.attempt_id=i.task_id JOIN story_work_steps_v1 s ON s.id=a.step_id WHERE s.work_task_id=?1 AND i.status='pending')",
            "authorIntentPending",
        ),
        (
            "SELECT EXISTS(SELECT 1 FROM story_tasks_v2 t JOIN story_work_attempts_v1 a ON a.attempt_id=t.task_id JOIN story_work_steps_v1 s ON s.id=a.step_id WHERE s.work_task_id=?1 AND t.status IN ('queued','running','waiting'))",
            "activeAttempt",
        ),
        (
            "SELECT EXISTS(SELECT 1 FROM story_tool_operations_v1 o JOIN story_work_attempts_v1 a ON a.attempt_id=o.task_id JOIN story_work_steps_v1 s ON s.id=a.step_id WHERE s.work_task_id=?1 AND o.state='accepted')",
            "unknownWrite",
        ),
        (
            "SELECT EXISTS(SELECT 1 FROM story_task_provider_budgets_v1 WHERE work_task_id=?1 AND settled=0)",
            "unsettledUsage",
        ),
    ] {
        if db
            .query_row(sql, [&w.id], |r| r.get::<_, bool>(0))
            .map_err(storage_error)?
        {
            return Ok(Some(reason));
        }
    }
    Ok(None)
}
fn blocked(db: &Connection, w: &Work, now: i64) -> Result<Option<&'static str>, RepositoryError> {
    // Closing a proven final result is deterministic and never requires a new model grant.
    if finished(db, w)? {
        return Ok(None);
    }
    // A proposal is not an executable continuation. Keep its durable event
    // pending until the current authorization contract admits the scope.
    let awaiting_scope: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM story_generation_handoffs_v1 h JOIN story_work_attempts_v1 a ON a.attempt_id=h.source_task_id JOIN story_work_steps_v1 s ON s.id=a.step_id WHERE s.work_task_id=?1 AND h.consumed_run_id IS NULL AND json_extract(h.content_json,'$.state')='proposed' AND json_type(h.content_json,'$.resumeRun')='null')",
        [&w.id], |r| r.get(0),
    ).map_err(storage_error)?;
    if awaiting_scope {
        return Ok(Some("generationScopeApprovalRequired"));
    }
    if !db.query_row("SELECT EXISTS(SELECT 1 FROM story_author_messages_v1 WHERE work_task_id=?1 AND intent_revision=0)",[&w.id],|r|r.get::<_,bool>(0)).map_err(storage_error)? {return Ok(Some("authorAuthorizationMissing"));}
    let ledger: Option<String> = db
        .query_row(
            "SELECT ledger_json FROM story_work_budgets_v1 WHERE work_task_id=?1",
            [&w.id],
            |r| r.get(0),
        )
        .optional()
        .map_err(storage_error)?;
    let Some(ledger) = ledger else {
        return Ok(Some("budgetAuthorizationMissing"));
    };
    let budget: crate::ProviderBudget =
        serde_json::from_str(&ledger).map_err(|_| RepositoryError::CorruptState)?;
    if let Some(reason) = budget.continuation_block_reason() {
        return Ok(Some(reason.as_str()));
    }
    let deadline = if w.kind == "generation" {
        let state:Option<String>=db.query_row("SELECT state_json FROM story_generation_runs_v1 WHERE project_id=?1 AND 'work-'||run_id=?2",params![w.project,w.id],|r|r.get(0)).optional().map_err(storage_error)?;
        let state: Value = serde_json::from_str(&state.ok_or(RepositoryError::CorruptState)?)
            .map_err(|_| RepositoryError::CorruptState)?;
        crate::generation_author_source::validate_run(
            db,
            &ProjectId::parse(w.project.clone()).map_err(|_| RepositoryError::CorruptState)?,
            w.id.strip_prefix("work-")
                .ok_or(RepositoryError::CorruptState)?,
            &state,
        )?;
        if state["status"] != "running" && !crate::work_decision_generation::recoverable_stopped_run(db, &state)? {
            return Ok(Some("generationNotRunning"));
        }
        state["deadlineAt"].as_i64()
    } else {
        let authorization:Option<String>=db.query_row("SELECT authorization_json FROM story_work_budget_authorizations_v1 WHERE work_task_id=?1",[&w.id],|r|r.get(0)).optional().map_err(storage_error)?;
        let Some(authorization) = authorization else {
            return Ok(Some("budgetAuthorizationMissing"));
        };
        serde_json::from_str::<crate::WorkBudgetAuthorization>(&authorization)
            .map_err(|_| RepositoryError::CorruptState)?
            .deadline_at
    };
    if deadline.is_some_and(|deadline| deadline <= now) {
        return Ok(Some("budgetExpired"));
    }
    if !blocked_steps_retryable(db, w)? {
        return Ok(Some("executionBlocked"));
    }
    Ok(None)
}
fn blocked_steps_retryable(db: &Connection, w: &Work) -> Result<bool, RepositoryError> {
    let mut stmt=db.prepare("SELECT current_attempt_id FROM story_work_steps_v1 WHERE work_task_id=?1 AND status='blocked'").map_err(storage_error)?;
    for row in stmt
        .query_map([&w.id], |r| r.get::<_, Option<String>>(0))
        .map_err(storage_error)?
    {
        let Some(task) = row.map_err(storage_error)? else {
            return Ok(false);
        };
        if !crate::work_decision_status::retryable(db, &task)?
            && !crate::conversation_receipts::recoverable_saved_chapter(db, &task)? {
            return Ok(false);
        }
    }
    Ok(true)
}
fn assert_generation_owner_current(db: &Connection, w: &Work) -> Result<(), RepositoryError> {
    if w.kind != "generation" || w.status != "blocked" {
        return Ok(());
    }
    let project = ProjectId::parse(w.project.clone()).map_err(|_| RepositoryError::CorruptState)?;
    let stored: String = db.query_row(
        "SELECT state_json FROM story_generation_runs_v1 WHERE project_id=?1 AND 'work-'||run_id=?2",
        params![w.project,w.id],
        |row| row.get(0),
    ).map_err(storage_error)?;
    let state: Value = serde_json::from_str(&stored).map_err(|_| RepositoryError::CorruptState)?;
    crate::generation_author_source::validate_run(
        db,
        &project,
        w.id.strip_prefix("work-")
            .ok_or(RepositoryError::CorruptState)?,
        &state,
    )?;
    if (state["status"] != "running" && !crate::work_decision_generation::recoverable_stopped_run(db, &state)?) || !blocked_steps_retryable(db, w)? {
        return Err(invalid("generation blocked"));
    }
    Ok(())
}

impl SqliteStoryRepository {
    pub fn work_execution_events(
        &self,
        project: &ProjectId,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<Value, RepositoryError> {
        if !(1..=20).contains(&limit) {
            return Err(invalid("page limit must be 1..20"));
        }
        let after = match cursor {
            Some(value) => {
                if value.len() > 512 {
                    return Err(invalid("invalid cursor"));
                }
                let c: Cursor =
                    serde_json::from_str(value).map_err(|_| invalid("invalid cursor"))?;
                if c.project_id != project.as_str() || c.sequence < 1 {
                    return Err(invalid("foreign cursor"));
                }
                c.sequence
            }
            None => 0,
        };
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let ids: Vec<(i64, String)> = {
            let mut stmt=tx.prepare("SELECT e.sequence,e.event_id FROM story_work_execution_events_v1 e JOIN story_work_tasks_v1 w ON w.id=e.work_task_id WHERE w.project_id=?1 AND e.state IN ('pending','claimed') AND e.sequence>?2 ORDER BY e.sequence LIMIT ?3").map_err(storage_error)?;
            stmt.query_map(params![project.as_str(), after, (limit + 1) as i64], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .map_err(storage_error)?
            .collect::<Result<_, _>>()
            .map_err(storage_error)?
        };
        let next = if ids.len() > limit {
            Some(
                serde_json::to_string(&Cursor {
                    project_id: project.as_str().into(),
                    sequence: ids[limit - 1].0,
                })
                .map_err(|_| RepositoryError::CorruptState)?,
            )
        } else {
            None
        };
        let mut items = Vec::new();
        for (_, id) in ids.iter().take(limit) {
            let e = event(&tx, project, id)?;
            let w = work(&tx, project, &e.work)?;
            items.push(metadata(&tx, &e, &w)?);
        }
        tx.commit().map_err(storage_error)?;
        Ok(json!({"projectId":project.as_str(),"items":items,"nextCursor":next}))
    }
    pub fn acquire_work_execution_owner(
        &mut self,
        project: &ProjectId,
        id: &str,
        request: WorkExecutionOwnerRequest,
        now: i64,
    ) -> Result<Value, RepositoryError> {
        identity(&request.host_session_id)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let w = work(&tx, project, id)?;
        if w.version != request.expected_work_version {
            return Err(RepositoryError::WorkTaskStale);
        }
        if matches!(w.status.as_str(), "paused" | "cancelled") {
            return Err(invalid("author stopped"));
        }
        assert_generation_owner_current(&tx, &w)?;
        let value = acquire(&tx, &w, &request.host_session_id, now)?;
        tx.commit().map_err(storage_error)?;
        Ok(value)
    }
    pub fn renew_work_execution_owner(
        &mut self,
        project: &ProjectId,
        id: &str,
        request: WorkExecutionLeaseRequest,
        now: i64,
    ) -> Result<Value, RepositoryError> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let w = work(&tx, project, id)?;
        require_owner(&tx, &w, &request.host_session_id, &request.lease_id, now)?;
        if matches!(w.status.as_str(), "paused" | "cancelled") {
            return Err(invalid("author stopped"));
        }
        assert_generation_owner_current(&tx, &w)?;
        tx.execute(
            "UPDATE story_work_execution_owners_v1 SET expires_at=?2 WHERE work_task_id=?1",
            params![id, now + LEASE_SECONDS],
        )
        .map_err(storage_error)?;
        let value = lease_json(
            &w,
            &request.host_session_id,
            &request.lease_id,
            now + LEASE_SECONDS,
        );
        tx.commit().map_err(storage_error)?;
        Ok(value)
    }
    pub fn release_work_execution_owner(
        &mut self,
        project: &ProjectId,
        id: &str,
        request: WorkExecutionLeaseRequest,
    ) -> Result<Value, RepositoryError> {
        identity(&request.host_session_id)?;
        identity(&request.lease_id)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        work(&tx, project, id)?;
        let changed=tx.execute("DELETE FROM story_work_execution_owners_v1 WHERE work_task_id=?1 AND host_session_id=?2 AND lease_id=?3",params![id,request.host_session_id,request.lease_id]).map_err(storage_error)?;
        tx.commit().map_err(storage_error)?;
        Ok(json!({"workTaskId":id,"released":changed==1}))
    }
    pub fn claim_work_execution_event(
        &mut self,
        project: &ProjectId,
        id: &str,
        request: WorkExecutionClaimRequest,
        now: i64,
    ) -> Result<Value, RepositoryError> {
        identity(&request.host_session_id)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let e = event(&tx, project, id)?;
        let w = work(&tx, project, &e.work)?;
        if w.version != request.expected_work_version {
            return Err(RepositoryError::WorkTaskStale);
        }
        let revision: i64 = tx
            .query_row(
                "SELECT revision FROM story_workspaces_v2 WHERE project_id=?1",
                [project.as_str()],
                |r| r.get(0),
            )
            .map_err(storage_error)?;
        if revision != request.expected_project_revision
            || crate::task_input::project_source_digest(&tx, project)?
                != request.expected_source_digest
        {
            return Err(invalid("source changed"));
        }
        if e.state == "consumed" {
            return Ok(
                json!({"event":metadata(&tx,&e,&w)?,"work":work_json(&w),"lease":null,"admission":"settled","blockedReason":null}),
            );
        }
        if let Some(reason) = safety_blocked(&tx, &e, &w)? {
            return Ok(
                json!({"event":metadata(&tx,&e,&w)?,"work":work_json(&w),"lease":null,"admission":"blocked","blockedReason":reason}),
            );
        }
        if owner(&tx, &w.id)?.is_some_and(|(session, _, expires)| {
            expires > now && session != request.host_session_id
        }) {
            return Ok(
                json!({"event":metadata(&tx,&e,&w)?,"work":work_json(&w),"lease":null,"admission":"blocked","blockedReason":"activeOwner"}),
            );
        }
        let consumed_by = successor(&tx, &e, &w)?;
        if consumed_by.is_some() || finished(&tx, &w)? {
            tx.execute("UPDATE story_work_execution_events_v1 SET state='consumed',consumer_step_id=?2 WHERE event_id=?1",params![id,consumed_by]).map_err(storage_error)?;
            let value = json!({"event":metadata(&tx,&event(&tx,project,id)?,&w)?,"work":work_json(&w),"lease":null,"admission":"settled","blockedReason":null});
            tx.commit().map_err(storage_error)?;
            return Ok(value);
        }
        if let Some(reason) = blocked(&tx, &w, now)? {
            return Ok(
                json!({"event":metadata(&tx,&e,&w)?,"work":work_json(&w),"lease":null,"admission":"blocked","blockedReason":reason}),
            );
        }
        let lease = acquire(&tx, &w, &request.host_session_id, now)?;
        let lease_id = lease["leaseId"]
            .as_str()
            .ok_or(RepositoryError::CorruptState)?;
        let claim = if e.state == "claimed"
            && e.session.as_deref() == Some(request.host_session_id.as_str())
            && e.lease.as_deref() == Some(lease_id)
        {
            e.claim.ok_or(RepositoryError::CorruptState)?
        } else {
            format!("work-claim-{}", uuid::Uuid::new_v4().simple())
        };
        tx.execute("UPDATE story_work_execution_events_v1 SET state='claimed',claim_id=?2,host_session_id=?3,lease_id=?4 WHERE event_id=?1",params![id,claim,request.host_session_id,lease_id]).map_err(storage_error)?;
        let e = event(&tx, project, id)?;
        let value = json!({"event":metadata(&tx,&e,&w)?,"work":work_json(&w),"lease":lease,"admission":"claimed","blockedReason":null});
        tx.commit().map_err(storage_error)?;
        Ok(value)
    }
    pub fn ack_work_execution_event(
        &mut self,
        project: &ProjectId,
        id: &str,
        request: WorkExecutionAckRequest,
        now: i64,
    ) -> Result<Value, RepositoryError> {
        identity(&request.claim_id)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let e = event(&tx, project, id)?;
        let w = work(&tx, project, &e.work)?;
        require_owner(&tx, &w, &request.host_session_id, &request.lease_id, now)?;
        if e.session.as_deref() != Some(&request.host_session_id)
            || e.lease.as_deref() != Some(&request.lease_id)
            || e.claim.as_deref() != Some(&request.claim_id)
        {
            return Err(invalid("stale event claim"));
        }
        if e.state == "consumed" {
            return Ok(metadata(&tx, &e, &w)?);
        }
        if e.state != "claimed"
            || matches!(w.status.as_str(), "paused" | "cancelled")
            || author_digest(&tx, &w.id)? != e.author
        {
            return Err(invalid("event consumption no longer authorized"));
        }
        let consumed = match request.consumer_step_id.as_deref() {
            Some(step) => {
                identity(step)?;
                tx.query_row("SELECT EXISTS(SELECT 1 FROM story_work_steps_v1 s JOIN story_work_attempts_v1 a ON a.attempt_id=s.current_attempt_id JOIN story_work_attempts_v1 origin ON origin.attempt_id=?3 WHERE s.id=?1 AND s.work_task_id=?2 AND a.sequence>origin.sequence AND (s.operation='advisory' OR s.depends_on_step_id=?4))",params![step,w.id,e.attempt,e.step],|r|r.get::<_,bool>(0)).map_err(storage_error)?
            }
            None => finished(&tx, &w)?,
        };
        if !consumed {
            return Err(invalid("no actual successor Attempt or final delivery"));
        }
        tx.execute("UPDATE story_work_execution_events_v1 SET state='consumed',consumer_step_id=?2 WHERE event_id=?1",params![id,request.consumer_step_id]).map_err(storage_error)?;
        let value = metadata(&tx, &event(&tx, project, id)?, &w)?;
        tx.commit().map_err(storage_error)?;
        Ok(value)
    }
    pub fn release_work_execution_event(
        &mut self,
        project: &ProjectId,
        id: &str,
        request: WorkExecutionReleaseRequest,
    ) -> Result<Value, RepositoryError> {
        identity(&request.host_session_id)?;
        identity(&request.claim_id)?;
        identity(&request.lease_id)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let e = event(&tx, project, id)?;
        let w = work(&tx, project, &e.work)?;
        if e.session.as_deref() != Some(&request.host_session_id)
            || e.lease.as_deref() != Some(&request.lease_id)
            || e.claim.as_deref() != Some(&request.claim_id)
        {
            return Err(invalid("stale event claim"));
        }
        if e.state == "consumed" {
            return Ok(metadata(&tx, &e, &w)?);
        }
        if e.state != "claimed" {
            return Err(invalid("event not claimed"));
        }
        tx.execute("UPDATE story_work_execution_events_v1 SET state='pending',claim_id=NULL,host_session_id=NULL,lease_id=NULL WHERE event_id=?1",[id]).map_err(storage_error)?;
        tx.execute("DELETE FROM story_work_execution_owners_v1 WHERE work_task_id=?1 AND host_session_id=?2 AND lease_id=?3",params![w.id,request.host_session_id,request.lease_id]).map_err(storage_error)?;
        let value = metadata(&tx, &event(&tx, project, id)?, &w)?;
        tx.commit().map_err(storage_error)?;
        Ok(value)
    }
}
