//! Proposed new-chapter scope, separate from the author's execution permission.
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::authoring_runs::storage_error;
use crate::{ProjectId, RepositoryError, SqliteStoryRepository, TaskRecord};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GenerationHandoffRequest {
    pub target_chapters: i64,
    pub resume_run: Option<Value>,
    pub instruction: String,
    pub expected_intent_revision: i64,
    pub idempotency_key: String,
}

impl SqliteStoryRepository {
    /// Records a proposal only. No chapter, execution allowance or model task is created.
    pub fn propose_generation_handoff(
        &mut self,
        task: &TaskRecord,
        request: &GenerationHandoffRequest,
    ) -> Result<Value, RepositoryError> {
        let outcome = self.propose_generation_handoff_transaction(task, request);
        crate::recovery_operations::settle_prewrite_rejection(
            self,
            task,
            "request_generation_handoff",
            &outcome,
        )?;
        outcome
    }

    fn propose_generation_handoff_transaction(
        &mut self,
        task: &TaskRecord,
        request: &GenerationHandoffRequest,
    ) -> Result<Value, RepositoryError> {
        if !(1..=crate::MAX_GENERATION_CHAPTERS).contains(&request.target_chapters)
            || request.instruction.trim().is_empty()
            || request.instruction.chars().count() > 1_000
            || request.idempotency_key.is_empty()
            || request.idempotency_key.len() > 160
            || request.idempotency_key.chars().any(char::is_control)
            || request.expected_intent_revision < 0
        {
            return Err(RepositoryError::InvalidContent(
                "invalid generation proposal".into(),
            ));
        }
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let current = crate::tasks::load_task(&tx, &task.task_id)?;
        let target: Value = serde_json::from_str(&current.target_json)
            .map_err(|_| RepositoryError::CorruptState)?;
        if target["conversationFocus"]["readOnly"] == true || target["readOnly"] == true {
            return Err(RepositoryError::TaskOutputForbidden);
        }
        if current.project_id != task.project_id
            || current.thread_id != task.thread_id
            || current.operation != "advisory"
            || target.get("runId").is_some()
            || target.get("revisionRepairSource").is_some()
        {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        crate::tasks::validate_active_task(
            &tx,
            &current,
            &task.project_id,
            &task.thread_id,
            task.input_revision,
        )?;
        crate::task_input::assert_task_inputs_current(&tx, &current)?;
        crate::task_intents::assert_action_intent(
            &tx,
            &task.task_id,
            request.expected_intent_revision,
        )?;
        if let Some(bound) = crate::work_decision::binding(&tx, &current)? {
            let work = bound["workTaskId"]
                .as_str()
                .ok_or(RepositoryError::TaskBindingInvalid)?;
            let (requirements, _) =
                crate::work_decision_status::actions(&tx, &task.project_id, work)?;
            if requirements
                .iter()
                .any(|r| r["kind"] == "revisionOutcome" && r["status"] == "pending")
            {
                return Err(RepositoryError::InvalidContent("generationHandoff: every current saved chapter must pass independent assessment before handoff".into()));
            }
        }
        if let Some(resume) = &request.resume_run {
            let object = resume
                .as_object()
                .ok_or(RepositoryError::TaskBindingInvalid)?;
            if object.len() != 2
                || resume["id"].as_str().is_none()
                || resume["expectedVersion"].as_i64().is_none()
            {
                return Err(RepositoryError::TaskBindingInvalid);
            }
            let saved: Option<String> = tx.query_row(
                "SELECT r.state_json FROM story_generation_runs_v1 r JOIN story_work_tasks_v1 w ON w.id='work-'||r.run_id AND w.project_id=r.project_id WHERE r.project_id=?1 AND r.run_id=?2 AND w.thread_id=?3",
                params![task.project_id.as_str(), resume["id"].as_str(), task.thread_id], |row| row.get(0),
            ).optional().map_err(storage_error)?;
            let state: Value =
                serde_json::from_str(&saved.ok_or(RepositoryError::TaskBindingInvalid)?)
                    .map_err(|_| RepositoryError::CorruptState)?;
            if state["version"] != resume["expectedVersion"]
                || state["targetChapters"] != request.target_chapters
                || !matches!(state["status"].as_str(), Some("paused" | "blocked"))
            {
                return Err(RepositoryError::TaskBindingInvalid);
            }
            let has_actions: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM story_task_action_requests_v1 WHERE source_task_id=?1) OR EXISTS(SELECT 1 FROM story_planning_review_runs_v2 WHERE source_task_id=?1)",
                [&task.task_id], |row| row.get(0)).map_err(storage_error)?;
            if has_actions {
                return Err(RepositoryError::TaskOutputForbidden);
            }
            if let Some(bound) = crate::work_decision::binding(&tx, &current)? {
                let work = bound["workTaskId"]
                    .as_str()
                    .ok_or(RepositoryError::TaskBindingInvalid)?;
                let (requirements, _) =
                    crate::work_decision_status::actions(&tx, &task.project_id, work)?;
                let pending:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM story_work_steps_v1 WHERE work_task_id=?1 AND current_attempt_id IS NOT ?2 AND status IN ('queued','running','waiting')) OR EXISTS(SELECT 1 FROM story_tool_operations_v1 o JOIN story_work_attempts_v1 a ON a.attempt_id=o.task_id JOIN story_work_steps_v1 s ON s.id=a.step_id WHERE s.work_task_id=?1 AND o.task_id<>?2 AND o.state='accepted') OR EXISTS(SELECT 1 FROM story_task_provider_budgets_v1 WHERE work_task_id=?1 AND task_id<>?2 AND settled=0)",params![work,task.task_id],|r|r.get(0)).map_err(storage_error)?;
                let selected:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM story_work_decision_actions_v1 WHERE decision_task_id=?1)",[&task.task_id],|r|r.get(0)).map_err(storage_error)?;
                if pending || selected || requirements.iter().any(|r| r["status"] == "pending") {
                    return Err(RepositoryError::InvalidContent("generationHandoff: prerequisite actions or writes are not settled; complete and verify them before selecting continuation".into()));
                }
            }
        }
        let prior: Option<(String, String)> = tx.query_row(
            "SELECT idempotency_key,content_json FROM story_generation_handoffs_v1 WHERE source_task_id=?1",
            [&task.task_id], |row| Ok((row.get(/*idx*/0)?, row.get(/*idx*/1)?)),
        ).optional().map_err(storage_error)?;
        if let Some((key, encoded)) = prior {
            let value: Value =
                serde_json::from_str(&encoded).map_err(|_| RepositoryError::CorruptState)?;
            if value["resumeRun"] != json!(request.resume_run)
                || key != request.idempotency_key
                || value["targetChapters"] != request.target_chapters
                || value["instruction"] != request.instruction
                || value["intentRevision"] != request.expected_intent_revision
            {
                return Err(RepositoryError::TaskBindingInvalid);
            }
            crate::recovery_operations::commit_operation(
                &tx,
                self.recovery_operation.as_ref(),
                &task.project_id,
                &task.task_id,
                &["request_generation_handoff"],
                &format!(
                    "runtime:generation-proposal:{}",
                    value["proposalId"]
                        .as_str()
                        .ok_or(RepositoryError::CorruptState)?
                ),
            )?;
            tx.commit().map_err(storage_error)?;
            return Ok(value);
        }
        crate::model_work_plan::action_ready(
            &tx,
            &crate::model_work_plan::work_for(&tx, &current)?,
        )?;
        let author_intent = crate::task_author_intent::source_intent(
            &tx,
            &task.task_id,
            request.expected_intent_revision,
        )?;
        let snapshot: String = tx.query_row("SELECT head_snapshot_id FROM story_branches_v2 WHERE project_id=?1 AND branch_id='main'",
            [task.project_id.as_str()], |row| row.get(/*idx*/0)).map_err(storage_error)?;
        let proposal = json!({"proposalId":format!("generation-proposal-{}",uuid::Uuid::new_v4().simple()),
            "projectId":task.project_id.as_str(),"sourceTaskId":task.task_id,"threadId":task.thread_id,
            "authorIntent":author_intent,"resumeRun":request.resume_run,"targetChapters":request.target_chapters,"instruction":request.instruction,
            "intentRevision":request.expected_intent_revision,"expectedRevision":current.input_revision,
            "sourceDigest":crate::task_input::project_source_digest(&tx,&task.project_id)?,
            "baseSnapshotId":snapshot,"state":"proposed"});
        tx.execute("INSERT INTO story_generation_handoffs_v1(proposal_id,source_task_id,idempotency_key,content_json) VALUES(?1,?2,?3,?4)",
            params![proposal["proposalId"].as_str(),task.task_id,request.idempotency_key,proposal.to_string()]).map_err(storage_error)?;
        crate::recovery_operations::commit_operation(
            &tx,
            self.recovery_operation.as_ref(),
            &task.project_id,
            &task.task_id,
            &["request_generation_handoff"],
            &format!(
                "runtime:generation-proposal:{}",
                proposal["proposalId"]
                    .as_str()
                    .ok_or(RepositoryError::CorruptState)?
            ),
        )?;
        tx.commit().map_err(storage_error)?;
        Ok(proposal)
    }

    /// Reads the exact proposal for the author UI, without granting execution.
    pub fn generation_handoff(
        &self,
        project: &ProjectId,
        task: &str,
    ) -> Result<Option<Value>, RepositoryError> {
        let encoded: Option<String> = self.connection.query_row(
            "SELECT h.content_json FROM story_generation_handoffs_v1 h JOIN story_tasks_v2 t ON t.task_id=h.source_task_id WHERE t.project_id=?1 AND t.task_id=?2 AND h.consumed_run_id IS NULL",
            params![project.as_str(),task], |row| row.get(/*idx*/0),
        ).optional().map_err(storage_error)?;
        encoded
            .map(|text| serde_json::from_str(&text).map_err(|_| RepositoryError::CorruptState))
            .transpose()
    }
}

/// Called within create_generation_run's author-only transaction, after source CAS.
/// A stale proposal, unsettled parent or different requested range cannot authorize a run.
pub(crate) fn consume(
    db: &Connection,
    project: &ProjectId,
    run: &str,
    state: &mut Value,
) -> Result<(), RepositoryError> {
    let Some(id) = state["generationProposalId"].as_str() else {
        return Ok(());
    };
    if id.len() > 160 || !state["revisionPredecessor"].is_null() {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let encoded: String = db.query_row(
        "SELECT h.content_json FROM story_generation_handoffs_v1 h JOIN story_tasks_v2 t ON t.task_id=h.source_task_id WHERE h.proposal_id=?1 AND t.project_id=?2 AND t.status='completed' AND h.consumed_run_id IS NULL",
        params![id,project.as_str()], |row| row.get(/*idx*/0),
    ).optional().map_err(storage_error)?.ok_or(RepositoryError::TaskBindingInvalid)?;
    let proposal: Value =
        serde_json::from_str(&encoded).map_err(|_| RepositoryError::CorruptState)?;
    if !proposal["resumeRun"].is_null() {
        return Err(RepositoryError::TaskOutputForbidden);
    }
    let source = proposal["sourceTaskId"]
        .as_str()
        .ok_or(RepositoryError::CorruptState)?;
    let (work, status): (String, String) = db.query_row(
        "SELECT w.id,w.status FROM story_work_attempts_v1 a JOIN story_work_steps_v1 s ON s.id=a.step_id JOIN story_work_tasks_v1 w ON w.id=s.work_task_id WHERE a.attempt_id=?1 AND w.project_id=?2",
        params![source,project.as_str()], |row| Ok((row.get(/*idx*/0)?,row.get(/*idx*/1)?)),
    ).map_err(storage_error)?;
    crate::work_pause_barrier::assert_can_schedule(db, &work)?;
    let pending: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM story_task_provider_budgets_v1 WHERE work_task_id=?1 AND settled=0) OR EXISTS(SELECT 1 FROM story_tool_operations_v1 o JOIN story_work_attempts_v1 a ON a.attempt_id=o.task_id JOIN story_work_steps_v1 s ON s.id=a.step_id WHERE s.work_task_id=?1 AND o.state='accepted')",
        [&work], |row| row.get(/*idx*/0),
    ).map_err(storage_error)?;
    let revision: i64 = db
        .query_row(
            "SELECT revision FROM story_workspaces_v2 WHERE project_id=?1",
            [project.as_str()],
            |row| row.get(/*idx*/ 0),
        )
        .map_err(storage_error)?;
    let count = state["targetChapters"]
        .as_i64()
        .zip(state["startChapterNumber"].as_i64())
        .map(|(end, start)| end - start + 1);
    if status != "completed"
        || pending
        || proposal["expectedRevision"] != revision
        || proposal["sourceDigest"] != state["sourceDigest"]
        || proposal["baseSnapshotId"] != state["baseSnapshotId"]
        || proposal["authorIntent"] != state["intentRevisions"][0]["instruction"]
        || proposal["targetChapters"].as_i64() != count
    {
        return Err(RepositoryError::TaskInputStale);
    }
    let changed = db.execute("UPDATE story_generation_handoffs_v1 SET consumed_run_id=?2 WHERE proposal_id=?1 AND consumed_run_id IS NULL",params![id,run]).map_err(storage_error)?;
    if changed != 1 {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    state["originTaskId"] = proposal["sourceTaskId"].clone();
    state["rootThreadId"] = proposal["threadId"].clone();
    Ok(())
}
