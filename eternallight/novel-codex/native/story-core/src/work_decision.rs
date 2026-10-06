//! Author-bounded workflow decisions. Readiness is evidence, never an execution order.
use crate::authoring_runs::storage_error;
use crate::{ProjectId, RepositoryError, SqliteStoryRepository, TaskRecord, WorkStep};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub(crate) const SCHEMA: &str = "
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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkDecisionRequest {
    pub expected_work_version: i64,
    pub expected_revision: i64,
    pub expected_source_digest: String,
}

#[derive(Serialize)]
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
            "Read the current Work status and its bounded receipts. Decide which authorized ready action is necessary for the author goal, or finish when all mandatory requirements are satisfied. Optional checks are choices; do not claim an unperformed check."
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
        if let Some(plan) = target.get("recoveryPlan") { manifest["recoveryPlan"] = plan.clone(); }
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

impl SqliteStoryRepository {
    /// Prepares a decision under existing Work authority after all dispatched children have settled.
    pub fn prepare_work_decision(
        &mut self,
        project: &ProjectId,
        work: &str,
        request: &WorkDecisionRequest,
    ) -> Result<WorkDecisionContinuation, RepositoryError> {
        let tx =
            rusqlite::Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
                .map_err(storage_error)?;
        require_cas(&tx, project, work, request)?;
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
        let unknown:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM story_tool_operations_v1 o JOIN story_work_attempts_v1 a ON a.attempt_id=o.task_id JOIN story_work_steps_v1 s ON s.id=a.step_id WHERE s.work_task_id=?1 AND o.state='accepted') OR EXISTS(SELECT 1 FROM story_task_provider_budgets_v1 WHERE work_task_id=?1 AND settled=0)",[work],|r|r.get(/*idx*/0)).map_err(storage_error)?;
        if response.stop_reason.is_none() && unknown {
            response.stop_reason = Some("pendingRecovery")
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
                response.stop_reason = Some("executionBlocked")
            }
        }
        let closed:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM story_work_decisions_v1 WHERE work_task_id=?1 AND finished=1)",[work],|r|r.get(/*idx*/0)).map_err(storage_error)?;
        let meaningful:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM story_task_outputs_v2 o JOIN story_work_attempts_v1 a ON a.attempt_id=o.task_id JOIN story_work_steps_v1 s ON s.id=a.step_id WHERE s.work_task_id=?1) OR EXISTS(SELECT 1 FROM story_task_action_requests_v1 a JOIN story_work_attempts_v1 wa ON wa.attempt_id=a.source_task_id JOIN story_work_steps_v1 s ON s.id=wa.step_id WHERE s.work_task_id=?1)",[work],|r|r.get(/*idx*/0)).map_err(storage_error)?;
        let quality_work: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM story_quality_packets_v1 p JOIN story_work_attempts_v1 a ON a.attempt_id=p.created_by_task_id JOIN story_work_steps_v1 s ON s.id=a.step_id WHERE s.work_task_id=?1)", [work], |r| r.get(0)).map_err(storage_error)?;
        let meaningful = meaningful || quality_work;
        if response.stop_reason.is_none()
            && (closed
                || (!meaningful
                    && current.kind != "generation"
                    && current.steps.iter().all(|s| s.status == "completed")))
        {
            response.stop_reason = Some("completed")
        }
        let fingerprint = format!(
            "{:x}",
            Sha256::digest(format!(
                "{}:{}",
                request.expected_revision, request.expected_source_digest
            ))
        );
        let unchanged_failures:i64=tx.query_row("SELECT COUNT(*) FROM story_work_steps_v1 s JOIN story_tasks_v2 t ON t.task_id=s.current_attempt_id WHERE s.work_task_id=?1 AND s.operation='advisory' AND json_extract(s.target_json,'$.workDecision.fingerprint')=?2 AND t.status='failed'",params![work,fingerprint],|r|r.get(/*idx*/0)).map_err(storage_error)?;
        if response.stop_reason.is_none() && unchanged_failures >= 2 {
            response.stop_reason = Some("recoveryStalled")
        }
        if response.stop_reason.is_none() {
            let (_, ready) = crate::work_decision_status::actions(&tx, project, work)?;
            if ready.is_empty()
                && crate::work_decision_status::status(&tx, project, work)?["canFinish"] != true
            {
                response.stop_reason = Some("noReadyActions")
            }
            let budget = crate::work_budget_storage::load(&tx, project, work)?;
            if budget.exhausted() {
                response.stop_reason = Some("budgetExhausted")
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

    pub fn task_work_decision(
        &self,
        task: &TaskRecord,
        cursor: Option<&str>,
    ) -> Result<Value, RepositoryError> {
        self.with_read_snapshot(|| {
            crate::task_input::assert_task_inputs_current(&self.connection, task)?;
            let bound =
                binding(&self.connection, task)?.ok_or(RepositoryError::TaskBindingInvalid)?;
            let mut status = crate::work_decision_status::status_page(
                &self.connection,
                &task.project_id,
                bound["workTaskId"]
                    .as_str()
                    .ok_or(RepositoryError::CorruptState)?,
                &task.task_id,
                cursor,
            )?;
            let selected: bool = self.connection.query_row("SELECT EXISTS(SELECT 1 FROM story_work_decision_actions_v1 WHERE decision_task_id=?1)", [&task.task_id], |row| row.get(0)).map_err(storage_error)?;
            status["deliveryReviewRequired"] = json!(crate::work_decision_generation::final_delivery_review_required(&self.connection, task)? || (!selected && (status["canFinish"] == true || status["canReportPartial"] == true)));
            for (key, value) in bound.as_object().ok_or(RepositoryError::CorruptState)? {
                status[key] = value.clone()
            }
            bounded(status)
        })?
    }
}

pub(crate) fn bounded(value: Value) -> Result<Value, RepositoryError> {
    if serde_json::to_vec(&value)
        .map_err(|_| RepositoryError::CorruptState)?
        .len()
        > 6500
    {
        return Err(RepositoryError::InvalidContent(
            "workflow decision metadata exceeds 6500 bytes".into(),
        ));
    }
    Ok(value)
}
