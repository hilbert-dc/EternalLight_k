//! Author-bounded workflow decisions. Readiness is evidence, never an execution order.
use crate::authoring_runs::storage_error;
use crate::{ProjectId, RepositoryError, SqliteStoryRepository, TaskRecord, WorkStep};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub(crate) const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS story_work_execution_conditions (
 work_task_id TEXT PRIMARY KEY REFERENCES story_work_tasks_v1(id), digest TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS story_work_decisions_v1 (
 work_task_id TEXT PRIMARY KEY REFERENCES story_work_tasks_v1(id),
 origin_task_id TEXT, finished INTEGER NOT NULL DEFAULT 0,
 decision_count INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS story_work_delivery_reviews_v1 (
 decision_task_id TEXT PRIMARY KEY REFERENCES story_tasks_v2(task_id),
 work_task_id TEXT NOT NULL REFERENCES story_work_tasks_v1(id),
 content_json TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS story_work_decision_actions_v1 (
 step_id TEXT PRIMARY KEY REFERENCES story_work_steps_v1(id),
 work_task_id TEXT NOT NULL REFERENCES story_work_tasks_v1(id),
 decision_task_id TEXT NOT NULL REFERENCES story_tasks_v2(task_id),
 action_key TEXT NOT NULL, idempotency_key TEXT NOT NULL,
 operation TEXT NOT NULL, target_json TEXT NOT NULL, instruction TEXT NOT NULL,
 source_task_id TEXT, source_revision_id TEXT,
 UNIQUE(decision_task_id,idempotency_key)
);
CREATE TABLE IF NOT EXISTS story_generation_handoffs_v1 (
 proposal_id TEXT PRIMARY KEY, source_task_id TEXT NOT NULL REFERENCES story_tasks_v2(task_id),
 idempotency_key TEXT NOT NULL, content_json TEXT NOT NULL, consumed_run_id TEXT,
 UNIQUE(source_task_id,idempotency_key)
);";

/// Register the need for author-goal delivery as soon as a model records work.
/// Worker completion alone must not close the owning conversation Work.
pub(crate) fn ensure_author_delivery_pending(
    db: &Connection,
    task: &TaskRecord,
) -> Result<(), RepositoryError> {
    if task.operation != "advisory" {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    if binding(db, task)?.is_some() {
        return Ok(());
    }
    let work = crate::model_work_plan::work_for(db, task)?;
    db.execute("INSERT INTO story_work_decisions_v1(work_task_id,origin_task_id) VALUES(?1,?2) ON CONFLICT(work_task_id) DO UPDATE SET finished=0 WHERE story_work_decisions_v1.origin_task_id=excluded.origin_task_id",params![work,task.task_id]).map_err(storage_error)?;
    Ok(())
}

#[derive(Deserialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkDecisionRequest {
    pub expected_work_version: i64,
    pub expected_revision: i64,
    pub expected_source_digest: String,
}

#[derive(Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct WorkDecisionContinuation {
    pub work_task_id: String,
    pub thread_id: String,
    pub origin_task_id: Option<String>,
    pub decision_step_id: Option<String>,
    pub decision_task_id: Option<String>,
    pub step: Option<WorkStep>,
    pub stop_reason: Option<&'static str>,
}

pub(crate) fn binding(
    db: &Connection,
    task: &TaskRecord,
) -> Result<Option<Value>, RepositoryError> {
    let target: Value =
        serde_json::from_str(&task.target_json).map_err(|_| RepositoryError::CorruptState)?;
    let Some(requested) = target.get("workDecision").cloned() else {
        return Ok(None);
    };
    if task.operation != "advisory" {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let (work, step, original, origin, thread): (String,String,String,Option<String>,String) = db.query_row(
        "SELECT w.id,s.id,s.target_json,d.origin_task_id,w.thread_id FROM story_work_attempts_v1 a JOIN story_work_steps_v1 s ON s.id=a.step_id AND s.current_attempt_id=a.attempt_id JOIN story_work_tasks_v1 w ON w.id=s.work_task_id JOIN story_work_decisions_v1 d ON d.work_task_id=w.id WHERE a.attempt_id=?1 AND w.project_id=?2",
        params![task.task_id,task.project_id.as_str()], |r|Ok((r.get(/*idx*/0)?,r.get(/*idx*/1)?,r.get(/*idx*/2)?,r.get(/*idx*/3)?,r.get(/*idx*/4)?)),
    ).optional().map_err(storage_error)?.ok_or(RepositoryError::TaskBindingInvalid)?;
    let original: Value =
        serde_json::from_str(&original).map_err(|_| RepositoryError::CorruptState)?;
    let mut actual = target;
    actual
        .as_object_mut()
        .ok_or(RepositoryError::TaskBindingInvalid)?
        .remove("resumeStepId");
    if actual != original
        || requested["workTaskId"] != work
        || requested["originTaskId"] != json!(origin)
        || task.thread_id != thread
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    Ok(Some(
        json!({"workTaskId":work,"originTaskId":origin,"sourceTaskId":requested["sourceTaskId"],"rootThreadId":thread,"decisionStepId":step,"decisionTaskId":task.task_id}),
    ))
}

pub(crate) fn freeze(
    db: &Connection,
    request: &crate::CreateTaskRequest,
    target: &Value,
    manifest: &mut Value,
) -> Result<bool, RepositoryError> {
    if target.get("workDecision").is_none() && target.get("workDecisionAction").is_none() {
        return Ok(false);
    }
    let step = target["resumeStepId"]
        .as_str()
        .ok_or(RepositoryError::TaskBindingInvalid)?;
    let (work, original, origin, thread, kind): (String,String,Option<String>,String,String)=db.query_row(
        "SELECT w.id,s.target_json,d.origin_task_id,w.thread_id,w.kind FROM story_work_steps_v1 s JOIN story_work_tasks_v1 w ON w.id=s.work_task_id JOIN story_work_decisions_v1 d ON d.work_task_id=w.id WHERE s.id=?1 AND w.project_id=?2 AND s.operation=?3 AND s.status='queued'",
        params![step,request.project_id.as_str(),request.operation],|r|Ok((r.get(/*idx*/0)?,r.get(/*idx*/1)?,r.get(/*idx*/2)?,r.get(/*idx*/3)?,r.get(/*idx*/4)?)),
    ).optional().map_err(storage_error)?.ok_or(RepositoryError::TaskBindingInvalid)?;
    let original: Value =
        serde_json::from_str(&original).map_err(|_| RepositoryError::CorruptState)?;
    let mut actual = target.clone();
    actual
        .as_object_mut()
        .ok_or(RepositoryError::TaskBindingInvalid)?
        .remove("resumeStepId");
    if actual != original
        || (!thread.is_empty()
            && target.get("workDecision").is_some()
            && request.thread_id != thread)
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let root_thread = if thread.is_empty() {
        request.thread_id.as_str()
    } else {
        thread.as_str()
    };
    let mut value = json!({"workTaskId":work,"originTaskId":origin,"sourceTaskId":target["workDecision"]["sourceTaskId"],"rootThreadId":root_thread,"decisionStepId":step,"decisionTaskId":request.task_id});
    if target.get("workDecision").is_some() {
        if request.operation != "advisory" {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        manifest["workDecision"] = value;
        manifest["candidateInstruction"] = json!(
            "Read the current Work status, persisted model plan, bounded receipts and the complete author goal. Choose the necessary authorized action; canFinish only describes persisted dependencies and does not replace the author's prerequisites or continuation goal. Record a future continuation in the work plan before its prerequisites, then register its real handoff after they settle. Complete only after all substantive goals and handoffs are evidenced, or explicitly record a blocked goal and honestly report partial delivery. Optional checks are choices; do not claim an unperformed check."
        );
    } else {
        let (parent,key,instruction,source,revision):(String,String,String,Option<String>,Option<String>)=db.query_row("SELECT decision_task_id,action_key,instruction,source_task_id,source_revision_id FROM story_work_decision_actions_v1 WHERE step_id=?1 AND work_task_id=?2",params![step,work],|r|Ok((r.get(/*idx*/0)?,r.get(/*idx*/1)?,r.get(/*idx*/2)?,r.get(/*idx*/3)?,r.get(/*idx*/4)?))).optional().map_err(storage_error)?.ok_or(RepositoryError::TaskBindingInvalid)?;
        let parent_task = crate::tasks::load_task(db, &parent)?;
        let parent_binding =
            binding(db, &parent_task)?.ok_or(RepositoryError::TaskBindingInvalid)?;
        value["decisionStepId"] = parent_binding["decisionStepId"].clone();
        value["decisionTaskId"] = json!(parent);
        value["actionKey"] = json!(key);
        value["operation"] = json!(request.operation);
        value["sourceTaskId"] = json!(source);
        value["sourceRevisionId"] = json!(revision);
        if target["workDecisionAction"]
            != json!({"workTaskId":work,"decisionStepId":value["decisionStepId"],"decisionTaskId":value["decisionTaskId"],"actionKey":key})
        {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        manifest["workDecisionAction"] = value;
        manifest["candidateInstruction"] = json!(instruction);
        if let Some(plan) = target.get("recoveryPlan") {
            manifest["recoveryPlan"] = plan.clone();
        }
    }
    if kind == "generation" {
        crate::generation_author_source::freeze(db, request, target, manifest)?;
    } else if let Some(origin) = origin {
        let revision:i64=db.query_row("SELECT COALESCE(MAX(intent_revision),0) FROM story_task_intents_v1 WHERE task_id=?1",[&origin],|r|r.get(/*idx*/0)).map_err(storage_error)?;
        crate::task_intents::assert_action_intent(db, &origin, revision)?;
        manifest["authorIntent"] = json!(crate::task_author_intent::source_intent(
            db, &origin, revision
        )?);
        manifest["authorIntentBinding"] =
            json!({"sourceTaskId":origin,"intentRevision":revision,"actionId":null});
    } else {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    Ok(true)
}

pub(crate) fn require_cas(
    db: &Connection,
    project: &ProjectId,
    work: &str,
    request: &WorkDecisionRequest,
) -> Result<(), RepositoryError> {
    let (version,revision):(i64,i64)=db.query_row("SELECT w.version,p.revision FROM story_work_tasks_v1 w JOIN story_workspaces_v2 p ON p.project_id=w.project_id WHERE w.id=?1 AND w.project_id=?2",params![work,project.as_str()],|r|Ok((r.get(/*idx*/0)?,r.get(/*idx*/1)?))).optional().map_err(storage_error)?.ok_or(RepositoryError::TaskNotFound)?;
    if version != request.expected_work_version {
        return Err(RepositoryError::WorkTaskStale);
    }
    if revision != request.expected_revision
        || crate::task_input::project_source_digest(db, project)? != request.expected_source_digest
    {
        return Err(RepositoryError::TaskInputStale);
    }
    Ok(())
}

fn completion_ready(
    db: &Connection,
    work: &str,
    kind: &str,
    steps: &[WorkStep],
) -> Result<bool, RepositoryError> {
    let closed:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM story_work_decisions_v1 WHERE work_task_id=?1 AND finished=1)",[work],|r|r.get(0)).map_err(storage_error)?;
    let meaningful:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM story_task_outputs_v2 o JOIN story_work_attempts_v1 a ON a.attempt_id=o.task_id JOIN story_work_steps_v1 s ON s.id=a.step_id WHERE s.work_task_id=?1) OR EXISTS(SELECT 1 FROM story_task_action_requests_v1 a JOIN story_work_attempts_v1 wa ON wa.attempt_id=a.source_task_id JOIN story_work_steps_v1 s ON s.id=wa.step_id WHERE s.work_task_id=?1)",[work],|r|r.get(0)).map_err(storage_error)?;
    let quality_work:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM story_quality_packets_v1 p JOIN story_work_attempts_v1 a ON a.attempt_id=p.created_by_task_id JOIN story_work_steps_v1 s ON s.id=a.step_id WHERE s.work_task_id=?1)",[work],|r|r.get(0)).map_err(storage_error)?;
    let active_plan =
        crate::model_work_plan::load(db, work)?.is_some_and(|(_, p)| p.active_goal_id.is_some());
    let delivery_pending:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM story_work_decisions_v1 WHERE work_task_id=?1 AND finished=0)",[work],|r|r.get(0)).map_err(storage_error)?;
    Ok(closed
        || (!(meaningful || quality_work || active_plan || delivery_pending)
            && kind != "generation"
            && steps.iter().all(|s| s.status == "completed")))
}



/// Shared by explicit reads and action receipts while their source/write snapshot is held.
pub(crate) fn status_in_snapshot(
    db: &Connection, task: &TaskRecord, cursor: Option<&str>,
) -> Result<Value, RepositoryError> {
    crate::task_input::assert_task_inputs_current(db, task)?;
    let Some(bound) = binding(db, task)? else {
        if task.operation != "advisory" || cursor.is_some() { return Err(RepositoryError::TaskBindingInvalid); }
        return conversation_status(db, task);
    };
    let work = bound["workTaskId"].as_str().ok_or(RepositoryError::CorruptState)?;
    let selected: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM story_work_decision_actions_v1 WHERE decision_task_id=?1)", [&task.task_id], |row| row.get(0)).map_err(storage_error)?;
    let mut envelope = bound.clone();
    envelope["workPlan"] = crate::model_work_plan::summary(db, work)?;
    envelope["deliveryReviewRequired"] = json!(crate::work_decision_generation::final_delivery_review_required(db, task)? || !selected);
    crate::work_decision_status::status_page(db, &task.project_id, work, &task.task_id, cursor, envelope)
}

fn conversation_status(db: &Connection, task: &TaskRecord) -> Result<Value, RepositoryError> {
    let work = crate::model_work_plan::work_for(db, task)?;
    let resume_review = crate::work_decision_status::root_review_required(db, task)?;
    let review_status = if resume_review {
        Some(crate::work_decision_status::status(
            db,
            &task.project_id,
            &work,
        )?)
    } else {
        None
    };
    let mut query = db.prepare("SELECT r.state_json,w.goal,w.status FROM story_generation_runs_v1 r JOIN story_work_tasks_v1 w ON w.id='work-'||r.run_id AND w.project_id=r.project_id WHERE r.project_id=?1 AND w.thread_id=?2 AND w.status IN ('paused','blocked','running') ORDER BY w.rowid DESC LIMIT 9").map_err(storage_error)?;
    let rows = query
        .query_map(params![task.project_id.as_str(), task.thread_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?))
        })
        .map_err(storage_error)?;
    let mut runs = Vec::new();
    for row in rows {
        let (state, goal, work_status) = row.map_err(storage_error)?;
        let run: Value = serde_json::from_str(&state).map_err(|_| RepositoryError::CorruptState)?;
        runs.push(json!({"id":run["id"],"expectedVersion":run["version"],"goal":goal.chars().take(1000).collect::<String>(),
                        "status":run["status"],"workStatus":work_status,"resumeSelection":{"status":crate::generation_handoff::resume_selection_status(&run, &work_status)},"completedChapters":run["completedChapters"],"targetChapters":run["targetChapters"],
                        "blockedReason":run["blockedReason"],"activeTaskId":run["activeTaskId"],"currentChapterId":run["currentChapterId"]}));
    }
    let mut query = db.prepare("SELECT action_id,operation,status,result_task_id FROM story_task_action_requests_v1 WHERE project_id=?1 AND source_task_id=?2 ORDER BY rowid LIMIT 3").map_err(storage_error)?;
    let registered_actions = query.query_map(params![task.project_id.as_str(),task.task_id], |r| Ok(json!({"id":r.get::<_,String>(0)?,"operation":r.get::<_,String>(1)?,"status":r.get::<_,String>(2)?,"resultTaskId":r.get::<_,Option<String>>(3)?}))).map_err(storage_error)?.collect::<Result<Vec<_>,_>>().map_err(storage_error)?;
    let handoff: Option<(String, String, Option<String>)> = db.query_row(
        "SELECT content_json,idempotency_key,consumed_run_id FROM story_generation_handoffs_v1 WHERE source_task_id=?1",
        [&task.task_id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?)),
    ).optional().map_err(storage_error)?;
    let handoff = handoff.map(|(encoded,key,run)| {
        let proposal: Value = serde_json::from_str(&encoded).map_err(|_| RepositoryError::CorruptState)?;
        Ok::<_,RepositoryError>(json!({"proposal":proposal,"idempotencyKey":key,"consumedRunId":run}))
    }).transpose()?;
    let truncated = runs.len() > 8;
    runs.truncate(8);
    let mut response = json!({"mode":"conversation","registeredActions":registered_actions,"generationHandoff":handoff,"workPlan":crate::model_work_plan::summary(db,&work)?,"conversationRuns":runs,"truncated":truncated,"deliveryReviewRequired":resume_review,
                    "instruction":"Use registeredActions and generationHandoff to verify this task's prior registrations after compaction; generationHandoff is a saved scope proposal, not executed chapters. A saved proposal requires no new registration or guessed idempotency key; complete the current task and report its actual approval state; a later rejected retry does not undo a saved action. Interpret the author's intent in context. Select resumeRun only for an explicitly intended candidate in resumeSelection; candidate is run-state eligibility, not execution admission. notNeeded means both run and Work are running; no handoff is needed. unavailable requires inspecting workStatus, not retrying handoff. Retain its original targetChapters. Discussion, negation, planning edits or prerequisite checks do not authorize immediate continuation. New scope requires a new proposal. Never guess a missing run or reset its allowance. After registering resumeRun, re-read this state and submit the required deliveryReview before completing; fulfilled covers the prerequisites and registered transfer, not all future chapters."});
    if let Some(status) = review_status {
        for key in [
            "workTaskId",
            "version",
            "sourceDigest",
            "canFinish",
            "canReportPartial",
        ] {
            response[key] = status[key].clone();
        }
    }
    bounded(response)
}

pub(crate) const BOUNDED_JSON_BYTES: usize = 6500;

pub(crate) fn bounded(mut value: Value) -> Result<Value, RepositoryError> {
    if serde_json::to_vec(&value)
        .map_err(|_| RepositoryError::CorruptState)?
        .len()
        > BOUNDED_JSON_BYTES
        && value["issues"]["entries"].is_array()
    {
        value["issues"]["entries"] = json!([]);
        value["issues"]["hasMore"] = json!(value["issues"]["totalCount"].as_u64().unwrap_or(0) > 0);
    }
    let bytes = serde_json::to_vec(&value)
        .map_err(|_| RepositoryError::CorruptState)?
        .len();
    if bytes > BOUNDED_JSON_BYTES {
        return Err(RepositoryError::InvalidContent(format!(
            "workflow decision metadata exceeds 6500 bytes ({bytes} bytes)"
        )));
    }
    Ok(value)
}

// Stable execution conditions and real saved progress delimit a recovery episode.
// Error prose, failed attempts, timestamps and fresh IDs never reset it.
pub(crate) fn recovery_fingerprint(db: &Connection, work: &str) -> Result<String, RepositoryError> {
    let condition: Option<String> = db.query_row("SELECT digest FROM story_work_execution_conditions WHERE work_task_id=?1", [work], |r|r.get(0)).optional().map_err(storage_error)?;
    let intent: i64 = db.query_row("SELECT COALESCE(MAX(intent_revision),0) FROM story_author_messages_v1 WHERE work_task_id=?1", [work], |r|r.get(0)).map_err(storage_error)?;
    Ok(format!("{:x}", Sha256::digest(json!([condition,intent]).to_string())))
}
pub(crate) fn unchanged_failures(db: &Connection, work: &str, fingerprint: &str) -> Result<i64, RepositoryError> {
    // A committed domain output is progress; a coordinator's text report is not.
    db.query_row("SELECT COUNT(*) FROM story_work_steps_v1 s JOIN story_work_attempts_v1 a ON a.attempt_id=s.current_attempt_id JOIN story_tasks_v2 t ON t.task_id=a.attempt_id WHERE s.work_task_id=?1 AND s.operation='advisory' AND json_extract(s.target_json,'$.workDecision.fingerprint')=?2 AND t.status='failed' AND a.sequence>COALESCE((SELECT MAX(p.sequence) FROM story_work_attempts_v1 p JOIN story_work_steps_v1 ps ON ps.id=p.step_id JOIN story_task_outputs_v2 o ON o.task_id=p.attempt_id WHERE ps.work_task_id=?1 AND o.output_kind!='completionReport'),0)", params![work,fingerprint], |r|r.get(0)).map_err(storage_error)
}


#[cfg(test)]
mod episode_tests {
    use super::*;
    #[test]
    fn unchanged_failures_are_scoped_to_saved_progress_and_stable_conditions() {
        let db=Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE story_work_execution_conditions(work_task_id TEXT,digest TEXT); CREATE TABLE story_author_messages_v1(work_task_id TEXT,intent_revision INTEGER); CREATE TABLE story_work_steps_v1(id TEXT,work_task_id TEXT,current_attempt_id TEXT,operation TEXT,target_json TEXT); CREATE TABLE story_work_attempts_v1(attempt_id TEXT,step_id TEXT,sequence INTEGER); CREATE TABLE story_tasks_v2(task_id TEXT,status TEXT); CREATE TABLE story_task_outputs_v2(task_id TEXT,output_kind TEXT);").unwrap();
        let set=|digest:&str|{db.execute("DELETE FROM story_work_execution_conditions",[]).unwrap();db.execute("INSERT INTO story_work_execution_conditions VALUES('w',?1)",[digest]).unwrap();};
        set("A");let a=recovery_fingerprint(&db,"w").unwrap();
        for i in 1..=2 {
            let id=format!("attempt-{i}");
            db.execute("INSERT INTO story_work_steps_v1 VALUES(?1,'w',?1,'advisory',?2)",params![id,json!({"workDecision":{"fingerprint":a}}).to_string()]).unwrap();
            db.execute("INSERT INTO story_work_attempts_v1 VALUES(?1,?1,?2)",params![id,i]).unwrap();
            db.execute("INSERT INTO story_tasks_v2 VALUES(?1,'failed')",[id]).unwrap();
        }
        assert_eq!(unchanged_failures(&db,"w",&a).unwrap(),2);
        set("B");assert_eq!(unchanged_failures(&db,"w",&recovery_fingerprint(&db,"w").unwrap()).unwrap(),0);
        set("A");assert_eq!(unchanged_failures(&db,"w",&recovery_fingerprint(&db,"w").unwrap()).unwrap(),2,"A/B oscillation cannot erase attempts");
        db.execute("INSERT INTO story_work_steps_v1 VALUES('saved','w','saved','generateChapter','{}')",[]).unwrap();
        db.execute("INSERT INTO story_work_attempts_v1 VALUES('saved','saved',3)",[]).unwrap();
        db.execute("INSERT INTO story_task_outputs_v2 VALUES('saved','completionReport')",[]).unwrap();
        assert_eq!(unchanged_failures(&db,"w",&a).unwrap(),2,"a coordinator report is not domain progress");
        db.execute("INSERT INTO story_task_outputs_v2 VALUES('saved','draft')",[]).unwrap();
        assert_eq!(unchanged_failures(&db,"w",&a).unwrap(),0);
    }
}

/// Public work decision capability; SQLite retains transaction ownership.
pub trait WorkDecisionRepository {
    /// Prepares a decision under existing Work authority after all dispatched children have settled.
    fn prepare_work_decision(
        &mut self,
        project: &ProjectId,
        work: &str,
        request: &WorkDecisionRequest,
    ) -> Result<WorkDecisionContinuation, RepositoryError>;
    fn task_conversation_runs(&self, task: &TaskRecord) -> Result<Value, RepositoryError>;
    fn task_work_decision(
        &self,
        task: &TaskRecord,
        cursor: Option<&str>,
    ) -> Result<Value, RepositoryError>;
    /// Author transport supplies a verified Engine configuration digest, never model text.
    fn record_work_execution_condition(
        &mut self,
        project: &ProjectId,
        work: &str,
        digest: &str,
    ) -> Result<(), RepositoryError>;
}

impl WorkDecisionRepository for SqliteStoryRepository {
    /// Prepares a decision under existing Work authority after all dispatched children have settled.
    fn prepare_work_decision(
        &mut self,
        project: &ProjectId,
        work: &str,
        request: &WorkDecisionRequest,
    ) -> Result<WorkDecisionContinuation, RepositoryError> {
        let tx =
            rusqlite::Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
                .map_err(storage_error)?;
        require_cas(&tx, project, work, request)?;
        crate::work_pause_barrier::assert_can_schedule(&tx, work)?;
        let current = crate::work_tasks::load_work_task(self, &tx, project, work)?;
        let origin:Option<String>=tx.query_row("SELECT a.attempt_id FROM story_work_attempts_v1 a JOIN story_work_steps_v1 s ON s.id=a.step_id WHERE s.work_task_id=?1 ORDER BY a.sequence LIMIT 1",[work],|r|r.get(/*idx*/0)).optional().map_err(storage_error)?;
        let stored_origin: Option<Option<String>> = tx
            .query_row(
                "SELECT origin_task_id FROM story_work_decisions_v1 WHERE work_task_id=?1",
                [work],
                |r| r.get(/*idx*/ 0),
            )
            .optional()
            .map_err(storage_error)?;
        let origin = stored_origin.unwrap_or(if current.kind == "generation" {
            None
        } else {
            origin
        });
        let mut response = WorkDecisionContinuation {
            work_task_id: work.into(),
            thread_id: current.thread_id.clone(),
            origin_task_id: origin.clone(),
            decision_step_id: None,
            decision_task_id: None,
            step: None,
            stop_reason: None,
        };
        // Replaying a durable final delivery neither starts a model nor grants
        // more allowance. Outstanding writes, charges and handoffs stay fenced.
        let unsettled:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM story_tool_operations_v1 o JOIN story_work_attempts_v1 a ON a.attempt_id=o.task_id JOIN story_work_steps_v1 s ON s.id=a.step_id WHERE s.work_task_id=?1 AND o.state='accepted') OR EXISTS(SELECT 1 FROM story_task_provider_budgets_v1 WHERE work_task_id=?1 AND settled=0) OR EXISTS(SELECT 1 FROM story_generation_handoffs_v1 h JOIN story_work_attempts_v1 a ON a.attempt_id=h.source_task_id JOIN story_work_steps_v1 s ON s.id=a.step_id WHERE s.work_task_id=?1 AND h.consumed_run_id IS NULL)",[work],|r|r.get(0)).map_err(storage_error)?;
        if current.status == "completed"
            && !unsettled
            && !current
                .steps
                .iter()
                .any(|s| matches!(s.status.as_str(), "queued" | "running"))
            && completion_ready(&tx, work, &current.kind, &current.steps)?
        {
            response.stop_reason = Some("completed");
            tx.commit().map_err(storage_error)?;
            return Ok(response);
        }
        let budget = crate::work_budget_storage::load(&tx, project, work)?;
        if budget.continuation_block_reason()
            == Some(crate::provider_budget::ProviderContinuationBlock::UnknownUsage)
        {
            response.stop_reason = Some("unknownUsage");
            tx.commit().map_err(storage_error)?;
            return Ok(response);
        }
        crate::task_actions::reconsider_unstarted_planning(&tx, work)?;
        let current = crate::work_tasks::load_work_task(self, &tx, project, work)?;
        if let Some(step) = current
            .steps
            .iter()
            .find(|s| s.status == "queued" && s.target.get("workDecision").is_some())
        {
            response.decision_step_id = Some(step.id.clone());
            response.decision_task_id = step.current_attempt_id.clone();
            response.step = Some(step.clone());
            tx.commit().map_err(storage_error)?;
            return Ok(response);
        }
        if matches!(current.status.as_str(), "paused" | "cancelled")
            || current
                .steps
                .iter()
                .any(|s| matches!(s.status.as_str(), "queued" | "running"))
        {
            response.stop_reason = Some("workNotReady");
        }
        // A persisted completion is the idempotent result of complete_task.
        // Reconcile only that operation; unknown candidate writes still block.
        tx.execute("UPDATE story_tool_operations_v1 SET state='committed',receipt_ref='runtime:task:'||task_id||':completed',updated_at=unixepoch() WHERE project_id=?1 AND tool_name='complete_task' AND state='accepted' AND task_id IN (SELECT t.task_id FROM story_tasks_v2 t JOIN story_work_attempts_v1 a ON a.attempt_id=t.task_id JOIN story_work_steps_v1 s ON s.id=a.step_id WHERE s.work_task_id=?2 AND t.project_id=?1 AND t.status='completed')", params![project.as_str(),work]).map_err(storage_error)?;
        if response.stop_reason.is_none() {
            response.stop_reason =
                crate::work_modeling_recovery::pending_reason(&tx, project, work)?;
        }
        for step in current.steps.iter().filter(|s| s.status == "blocked") {
            if response.stop_reason.is_none()
                && !step
                    .current_attempt_id
                    .as_deref()
                    .map(|id| crate::work_decision_status::retryable(&tx, id))
                    .transpose()?
                    .unwrap_or(false)
            {
                response.stop_reason = Some(
                    budget
                        .continuation_block_reason()
                        .map(|reason| reason.as_str())
                        .unwrap_or("executionBlocked"),
                )
            }
        }
        if response.stop_reason.is_none()
            && completion_ready(&tx, work, &current.kind, &current.steps)?
        {
            response.stop_reason = Some("completed")
        }
        let fingerprint = recovery_fingerprint(&tx, work)?;
        let unchanged_failures = unchanged_failures(&tx, work, &fingerprint)?;
        if response.stop_reason.is_none() && unchanged_failures >= 2 {
            response.stop_reason = Some("recoveryStalled")
        }
        if response.stop_reason.is_none() {
            let (_, ready) = crate::work_decision_status::actions(&tx, project, work)?;
            let decision_status = crate::work_decision_status::status(&tx, project, work)?;
            let plan_needs_review = crate::model_work_plan::summary(&tx, work)?["pendingGoals"]
                .as_u64()
                .unwrap_or(0)
                > 0;
            if ready.is_empty()
                && decision_status["canFinish"] != true
                && decision_status["canReportPartial"] != true
                && !plan_needs_review
            {
                response.stop_reason = Some("noReadyActions")
            }
            if let Some(reason) = budget.continuation_block_reason() {
                response.stop_reason = Some(reason.as_str())
            }

            crate::execution_scope_storage::assert_attempt_capacity(&tx, project, work)?;
        }
        if response.stop_reason.is_some() {
            tx.commit().map_err(storage_error)?;
            return Ok(response);
        }
        tx.execute("INSERT OR IGNORE INTO story_work_decisions_v1(work_task_id,origin_task_id) VALUES(?1,?2)",params![work,origin]).map_err(storage_error)?;
        tx.execute("UPDATE story_work_steps_v1 SET status='superseded' WHERE work_task_id=?1 AND status='blocked'",[work]).map_err(storage_error)?;
        let step = format!("decision-{}", uuid::Uuid::new_v4().simple());
        let source = current
            .steps
            .iter()
            .rev()
            .filter(|s| s.target.get("interludeId").is_none())
            .filter_map(|s| s.current_attempt_id.as_ref())
            .next()
            .cloned();
        let mut target = json!({"workDecision":{"workTaskId":work,"originTaskId":origin,"sourceTaskId":source,"fingerprint":fingerprint}});
        if current.kind == "generation" {
            crate::work_decision_generation::bind_target(&tx, project, work, &mut target)?;
        }
        tx.execute("INSERT INTO story_work_steps_v1(id,work_task_id,ordinal,operation,target_json,input_revision,status) VALUES(?1,?2,(SELECT COALESCE(MAX(ordinal),0)+1 FROM story_work_steps_v1 WHERE work_task_id=?2),'advisory',?3,?4,'queued')",params![step,work,target.to_string(),request.expected_revision]).map_err(storage_error)?;
        tx.execute("UPDATE story_work_decisions_v1 SET decision_count=decision_count+1 WHERE work_task_id=?1",[work]).map_err(storage_error)?;
        tx.execute("UPDATE story_work_tasks_v1 SET status='running',version=version+1,input_revision=?2 WHERE id=?1",params![work,request.expected_revision]).map_err(storage_error)?;
        response.step = crate::work_tasks::load_work_task(self, &tx, project, work)?
            .steps
            .into_iter()
            .find(|s| s.id == step);
        response.decision_step_id = Some(step);
        tx.commit().map_err(storage_error)?;
        Ok(response)
    }

    fn task_conversation_runs(&self, task: &TaskRecord) -> Result<Value, RepositoryError> {
        self.with_read_snapshot(|| {
            crate::task_input::assert_task_inputs_current(&self.connection, task)?;
            crate::model_work_plan::work_for(&self.connection, task)?;
            conversation_status(&self.connection, task)
        })?
    }

    fn task_work_decision(
        &self,
        task: &TaskRecord,
        cursor: Option<&str>,
    ) -> Result<Value, RepositoryError> {
        self.with_source_return_snapshot(|| status_in_snapshot(&self.connection, task, cursor))?
    }

    /// Author transport supplies a verified Engine configuration digest, never model text.
    fn record_work_execution_condition(
        &mut self,
        project: &ProjectId,
        work: &str,
        digest: &str,
    ) -> Result<(), RepositoryError> {
        if digest.len() != 64 || !digest.bytes().all(|c| c.is_ascii_hexdigit()) {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let owned: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM story_work_tasks_v1 WHERE id=?1 AND project_id=?2)",
                params![work, project.as_str()],
                |r| r.get(0),
            )
            .map_err(storage_error)?;
        if !owned {
            return Err(RepositoryError::TaskNotFound);
        }
        tx.execute("INSERT INTO story_work_execution_conditions(work_task_id,digest) VALUES(?1,?2) ON CONFLICT(work_task_id) DO UPDATE SET digest=excluded.digest WHERE digest!=excluded.digest", params![work,digest]).map_err(storage_error)?;
        tx.commit().map_err(storage_error)
    }
}

use crate::ReadSnapshotRepository;
