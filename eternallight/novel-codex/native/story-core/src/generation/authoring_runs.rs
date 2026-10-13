use rusqlite::{OptionalExtension, params};
use serde_json::Value;

use crate::{ProjectId, RepositoryError, SqliteStoryRepository};

enum GenerationRunSourceGuard<'a> {
    Unchecked,
    AuthorControl,
    Matches(&'a str),
    ModelPause(&'a crate::TaskRecord, &'a crate::ModelRunPauseRequest),
}

pub(crate) fn claim_generation_run_for_task(
    db: &rusqlite::Connection,
    project_id: &ProjectId,
    run_id: &str,
    task_id: &str,
    operation: &str,
    target: &Value,
) -> Result<(), RepositoryError> {
    let (version, state_json): (i64, String) = db
        .query_row(
            "SELECT version, state_json FROM story_generation_runs_v1 WHERE project_id = ?1 AND run_id = ?2",
            params![project_id.as_str(), run_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(storage_error)?
        .ok_or(RepositoryError::AuthoringRunNotFound)?;
    let mut state: Value =
        serde_json::from_str(&state_json).map_err(|_| RepositoryError::CorruptState)?;
    if state.get("providerBudget").is_some() {
        return Err(RepositoryError::CorruptState);
    }
    crate::generation_author_source::validate_run(db, project_id, run_id, &state)?;
    if state["usedTaskAttempts"] == 0 {
        crate::revision_continuation::current(db, project_id, &state)?;
    }
    crate::execution_scope_storage::assert_attempt_capacity(
        db,
        project_id,
        &format!("work-{run_id}"),
    )?;
    let provider_budget =
        crate::work_budget_storage::load(db, project_id, &format!("work-{run_id}"))?;
    let expected_operation =
        if target.get("workDecision").is_some() || target.get("workDecisionAction").is_some() {
            operation
        } else {
            return Err(RepositoryError::TaskBindingInvalid);
        };
    let now: i64 = db
        .query_row("SELECT unixepoch()", [], |row| row.get(0))
        .map_err(storage_error)?;
    if state["version"].as_i64() != Some(version)
        || state["id"].as_str() != Some(run_id)
        || state["projectId"].as_str() != Some(project_id.as_str())
    {
        return Err(RepositoryError::CorruptState);
    }
    if state["status"] != "running"
        || !state["activeTaskId"].is_null()
        || state["stage"] != "coordinate"
        || expected_operation != operation
        || target["runId"] != run_id
        || target["runVersion"].as_i64() != Some(version)
        || target["stepToken"] != state["stepToken"]
        || target["intentGeneration"] != state["intentGeneration"]
        || serde_json::from_value::<Option<i64>>(state["deadlineAt"].clone())
            .map_err(|_| RepositoryError::CorruptState)?
            .is_some_and(|deadline| now >= deadline)
        || state["usedTaskAttempts"].as_i64().is_none()
        || serde_json::from_value::<Option<i64>>(state["maxTaskAttempts"].clone())
            .map_err(|_| RepositoryError::CorruptState)?
            .is_some_and(|max| {
                state["usedTaskAttempts"]
                    .as_i64()
                    .is_some_and(|used| used >= max)
            })
        || provider_budget.exhausted()
        || Some(provider_budget.charges.len())
            != state["usedTaskAttempts"].as_u64().map(|used| used as usize)
        || crate::task_input::project_source_digest(db, project_id)? != state["sourceDigest"]
        || (operation == "generateChapter"
            && (target["chapterId"] != state["currentChapterId"]
                || target["chapterNumber"] != state["currentChapterNumber"]))
        || (matches!(
            operation,
            "modelKnowledge" | "extractCommitments" | "assessSceneOutcome"
        ) && target["chapterId"] != state["currentChapterId"])
        || (matches!(operation, "extractCommitments" | "assessSceneOutcome")
            && target["chapterNumber"] != state["currentChapterNumber"])
        || (operation == "generatePlan"
            && target["planFromChapter"].as_i64()
                != state["completedChapters"]
                    .as_i64()
                    .map(|chapter| chapter + 1))
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    if operation == "generateChapter" {
        crate::bound_writer_plan::validate_generation_writer_scope(db, project_id, target)?;
    }
    crate::task_provider_budget::reserve(
        db,
        project_id,
        &format!("work-{run_id}"),
        task_id,
        serde_json::from_value::<Option<i64>>(state["deadlineAt"].clone())
            .map_err(|_| RepositoryError::CorruptState)?,
    )?;
    state["activeTaskId"] = Value::String(task_id.to_owned());
    state["usedTaskAttempts"] = Value::from(
        state["usedTaskAttempts"]
            .as_i64()
            .ok_or(RepositoryError::CorruptState)?
            + 1,
    );
    state["version"] = Value::from(version + 1);
    let updated = db
        .execute(
            "UPDATE story_generation_runs_v1 SET version = version + 1, state_json = ?1 WHERE project_id = ?2 AND run_id = ?3 AND version = ?4",
            params![state.to_string(), project_id.as_str(), run_id, version],
        )
        .map_err(storage_error)?;
    if updated != 1 {
        return Err(RepositoryError::AuthoringRunStale);
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredGenerationRun {
    pub run_id: String,
    pub project_id: ProjectId,
    pub version: i64,
    pub state_json: String,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct GenerationStartPosition {
    pub completed_chapters: i64,
    pub first_stage: String,
}

fn generation_start_position(
    db: &rusqlite::Connection,
    project_id: &ProjectId,
) -> Result<GenerationStartPosition, RepositoryError> {
    let (count, first, last, distinct): (i64, i64, i64, i64) = db
        .query_row(
            "SELECT COUNT(*), COALESCE(MIN(number), 0), COALESCE(MAX(number), 0), COUNT(DISTINCT number) FROM story_chapters_v2 WHERE project_id = ?1",
            [project_id.as_str()],
            |row| Ok((row.get(/*idx*/ 0)?, row.get(/*idx*/ 1)?, row.get(/*idx*/ 2)?, row.get(/*idx*/ 3)?)),
        )
        .map_err(storage_error)?;
    if count > 0 && (first < 1 || distinct != count) {
        return Err(RepositoryError::InvalidContent(
            "automatic continuation requires positive unique chapter numbers".into(),
        ));
    }
    let unready: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM story_chapters_v2 c
             LEFT JOIN story_object_heads_v2 h ON h.project_id = c.project_id AND h.object_id = 'draft-' || c.chapter_id AND h.object_kind = 'draft'
             WHERE c.project_id = ?1 AND COALESCE(h.candidate_revision_id, h.accepted_revision_id) IS NULL",
            [project_id.as_str()],
            |row| row.get(0),
        )
        .map_err(storage_error)?;
    if unready != 0 {
        return Err(RepositoryError::InvalidContent(
            "automatic continuation requires a saved draft for every existing chapter".into(),
        ));
    }
    Ok(GenerationStartPosition {
        // GenerationRun uses this as the numbering frontier, not the count of retained chapters.
        // Deleting a chapter leaves a gap; the next chapter still follows MAX(number) + 1.
        completed_chapters: last,
        first_stage: "coordinate".into(),
    })
}

impl SqliteStoryRepository {
    

    

    

    

    

    

    
    

    

    

    

    pub(crate) fn update_generation_run_for_model_pause(
        &mut self,
        task: &crate::TaskRecord,
        request: &crate::ModelRunPauseRequest,
        state: &str,
    ) -> Result<StoredGenerationRun, RepositoryError> {
        self.update_generation_run_with_guard(
            &task.project_id,
            &request.run_id,
            request.expected_version,
            state,
            GenerationRunSourceGuard::ModelPause(task, request),
            None,
        )
    }

    fn update_generation_run_with_guard(
        &mut self,
        project_id: &ProjectId,
        run_id: &str,
        expected_version: i64,
        state_json: &str,
        source_guard: GenerationRunSourceGuard<'_>,
        handoff_source: Option<&str>,
    ) -> Result<StoredGenerationRun, RepositoryError> {
        // Shared repository reads reuse this write snapshot without opening nested transactions.
        let transaction = crate::work_control_commands::ControlTransaction::new(&self.connection)?;
        if let Some((work, version)) = &self.generation_work_control_guard {
            if work != &format!("work-{run_id}") {
                return Err(RepositoryError::TaskBindingInvalid);
            }
            let actual:Option<i64>=transaction.query_row("SELECT control_revision FROM story_work_tasks_v1 WHERE id=?1 AND project_id=?2 AND kind='generation'",params![work,project_id.as_str()],|r|r.get(0)).optional().map_err(storage_error)?;
            if actual != Some(*version) {
                return Err(RepositoryError::WorkTaskStale);
            }
        }
        if let GenerationRunSourceGuard::Matches(expected) = source_guard {
            let actual = crate::task_input::project_source_digest(&transaction, project_id)?;
            if actual != expected {
                return Err(RepositoryError::AuthoringRunStale);
            }
        }
        if let GenerationRunSourceGuard::ModelPause(task, request) = source_guard {
            crate::generation_model_control::validate(&transaction, task, request)?;
            // An author-owned drain cannot produce a model pause receipt while
            // its frozen attempts are still running.
            crate::work_pause_barrier::assert_can_schedule(&transaction, &format!("work-{run_id}"))?;
        }
        let previous_state: Option<String> = transaction.query_row(
            "SELECT state_json FROM story_generation_runs_v1 WHERE project_id = ?1 AND run_id = ?2 AND version = ?3",
            params![project_id.as_str(), run_id, expected_version], |row| row.get(0),
        ).optional().map_err(storage_error)?;
        let mut stored_state: Value =
            serde_json::from_str(state_json).map_err(|_| RepositoryError::CorruptState)?;
        if let Some(prior) = &previous_state {
            let prior: Value =
                serde_json::from_str(prior).map_err(|_| RepositoryError::CorruptState)?;
            if prior["status"] == "cancelled" || stored_state["status"] == "cancelled" {
                // Cancellation has its own transaction. A terminal run may only
                // reconcile its already-authorized usage, never resume or edit.
                let mut unchanged = stored_state.clone();
                unchanged
                    .as_object_mut()
                    .ok_or(RepositoryError::CorruptState)?
                    .remove("providerBudget");
                unchanged["version"] = prior["version"].clone();
                if prior["status"] != "cancelled" || unchanged != prior {
                    return Err(RepositoryError::TaskBindingInvalid);
                }
            }
            let appended_replan = stored_state["replans"]
                .as_array()
                .zip(prior["replans"].as_array())
                .is_some_and(|(next, previous)| {
                    next.len() == previous.len() + 1
                        && next.last().is_some_and(|record| {
                            record["previousVersion"] == prior["version"]
                                && record["previousTaskId"] == prior["activeTaskId"]
                        })
                });
            if (prior["intentGeneration"] != stored_state["intentGeneration"] || appended_replan)
                && stored_state["activeTaskId"].is_null()
            {
                // A checkpoint refresh invalidates unclaimed choices, not immutable
                // receipts or granted budget. Only author replan changes intent.
                transaction.execute("UPDATE story_work_steps_v1 SET status='superseded' WHERE work_task_id=?1 AND status='queued' AND current_attempt_id IS NULL",[format!("work-{run_id}")]).map_err(storage_error)?;
                if appended_replan {
                    let unresolved: bool = transaction.query_row(
                        "SELECT EXISTS(SELECT 1 FROM story_work_steps_v1 s JOIN story_work_attempts_v1 a ON a.step_id=s.id JOIN story_tool_operations_v1 o ON o.task_id=a.attempt_id WHERE s.work_task_id=?1 AND o.state='accepted') OR EXISTS(SELECT 1 FROM story_work_steps_v1 s JOIN story_work_attempts_v1 a ON a.step_id=s.id JOIN story_task_provider_budgets_v1 b ON b.task_id=a.attempt_id WHERE s.work_task_id=?1 AND b.settled=0)",
                        [format!("work-{run_id}")], |row| row.get(0),
                    ).map_err(storage_error)?;
                    if unresolved {
                        return Err(RepositoryError::TaskNotWritable);
                    }
                    if let Some(id) = prior["activeTaskId"].as_str() {
                        let task = crate::tasks::load_task(&transaction, id)?;
                        let unresolved: bool = transaction.query_row(
                            "SELECT EXISTS(SELECT 1 FROM story_tool_operations_v1 WHERE task_id=?1 AND state='accepted') OR EXISTS(SELECT 1 FROM story_task_provider_budgets_v1 WHERE task_id=?1 AND settled=0)",
                            [id], |row| row.get(0),
                        ).map_err(storage_error)?;
                        if unresolved
                            || !matches!(task.status.as_str(), "completed" | "failed" | "cancelled")
                        {
                            return Err(RepositoryError::TaskNotWritable);
                        }
                        // A context refresh is not an instruction to discard a saved
                        // chapter. Reconcile its receipt before retiring the attempt.
                        // A new author intent or stale/partial output still replans.
                        if prior["intentGeneration"] == stored_state["intentGeneration"]
                            && task.operation == "generateChapter"
                            && (task.status == "failed" || task.was_author_paused())
                            && crate::tasks::recoverable_output(&transaction, &task)?
                        {
                            transaction.execute(
                                "UPDATE story_tasks_v2 SET status='completed',stage='ready_for_review',delivery_status='ready_for_review',result_summary='检查点恢复：复用完整保存的章节，等待核验',capability_revoked_at=COALESCE(capability_revoked_at,unixepoch()) WHERE task_id=?1",
                                [id],
                            ).map_err(storage_error)?;
                            crate::work_tasks::finish_attempt(
                                &transaction, &crate::tasks::load_task(&transaction, id)?,
                            )?;
                        } else {
                            transaction.execute("UPDATE story_work_steps_v1 SET status='superseded' WHERE work_task_id=?1 AND current_attempt_id=?2 AND status IN ('blocked','cancelled')",
                                params![format!("work-{run_id}"),id]).map_err(storage_error)?;
                        }
                    }
                }
                transaction
                    .execute(
                        "UPDATE story_work_decisions_v1 SET finished=0 WHERE work_task_id=?1",
                        [format!("work-{run_id}")],
                    )
                    .map_err(storage_error)?;
            }
            crate::generation_delivery::prepare_transition(
                self,
                project_id,
                run_id,
                &prior,
                &mut stored_state,
            )?;
            crate::generation_author_source::validate_update(
                &transaction,
                project_id,
                run_id,
                &prior,
                &stored_state,
            )?;
        }
        if matches!(source_guard, GenerationRunSourceGuard::AuthorControl) {
            crate::work_interludes::invalidate(&transaction,&format!("work-{run_id}"))?;
            crate::work_pause_barrier::clear(&transaction, &format!("work-{run_id}"))?;
        } else if let Some(barrier) = crate::work_pause_barrier::read(&transaction, &format!("work-{run_id}"))? {
            stored_state["status"] = serde_json::json!(if barrier.remaining_attempts == 0 { "paused" } else { "running" });
        }
        let result_state_json = stored_state.to_string();
        if let Some(source) = handoff_source {
            crate::generation_handoff_execution::record_start(
                &transaction,
                project_id,
                source,
                run_id,
                expected_version,
                &stored_state,
            )?;
        }
        let budget = crate::work_budget_storage::take(&mut stored_state)?;
        let updated = transaction
            .execute(
                "UPDATE story_generation_runs_v1 SET version = version + 1, state_json = ?1 WHERE project_id = ?2 AND run_id = ?3 AND version = ?4",
                params![stored_state.to_string(), project_id.as_str(), run_id, expected_version],
            )
            .map_err(storage_error)?;
        if updated == 0 {
            let exists = transaction
                .query_row(
                    "SELECT 1 FROM story_generation_runs_v1 WHERE project_id = ?1 AND run_id = ?2",
                    params![project_id.as_str(), run_id],
                    |_| Ok(()),
                )
                .optional()
                .map_err(storage_error)?
                .is_some();
            return Err(if exists {
                RepositoryError::AuthoringRunStale
            } else {
                RepositoryError::AuthoringRunNotFound
            });
        }
        crate::work_budget_storage::update(
            &transaction,
            project_id,
            &format!("work-{run_id}"),
            &budget,
        )?;
        let state = stored_state;
        if let Some(prior) = previous_state.as_deref() {
            let prior: Value =
                serde_json::from_str(prior).map_err(|_| RepositoryError::CorruptState)?;
            if prior["status"] == "running" && state["status"] == "paused" {
                if let Some(id) = prior["activeTaskId"].as_str() {
                    let changed = transaction.execute(
                        "UPDATE story_tasks_v2 SET status='cancelled',stage='cancelled',delivery_status=CASE WHEN output_refs_json='[]' THEN 'pending' ELSE 'incomplete' END,error='author_paused',capability_revoked_at=unixepoch() WHERE task_id=?1 AND status IN ('queued','running','waiting')",
                        [id],
                    ).map_err(storage_error)?;
                    if changed == 1 {
                        crate::work_tasks::finish_attempt(
                            &transaction,
                            &crate::tasks::load_task(&transaction, id)?,
                        )?;
                    }
                }
            }
            if prior["status"] == "paused"
                && state["status"] == "running"
                && state["activeTaskId"].is_null()
            {
                if let Some(id) = prior["activeTaskId"].as_str() {
                    let task = crate::tasks::load_task(&transaction, id)?;
                    if task.was_author_paused() {
                        let unknown: bool = transaction.query_row(
                            "SELECT EXISTS(SELECT 1 FROM story_tool_operations_v1 WHERE task_id=?1 AND state='accepted') OR EXISTS(SELECT 1 FROM story_task_provider_budgets_v1 WHERE task_id=?1 AND settled=0)",
                            [id], |row| row.get(0),
                        ).map_err(storage_error)?;
                        if unknown {
                            return Err(RepositoryError::TaskNotWritable);
                        }
                        if task.operation == "generateChapter"
                            && crate::tasks::recoverable_output(&transaction, &task)?
                        {
                            transaction.execute("UPDATE story_tasks_v2 SET status='completed',stage='ready_for_review',delivery_status='ready_for_review',result_summary='作者恢复：复用暂停前完整保存的章节' WHERE task_id=?1", [id]).map_err(storage_error)?;
                            crate::work_tasks::finish_attempt(
                                &transaction,
                                &crate::tasks::load_task(&transaction, id)?,
                            )?;
                        } else {
                            transaction.execute("UPDATE story_work_steps_v1 SET status='queued',current_attempt_id=NULL WHERE current_attempt_id=?1 AND status='cancelled'", [id]).map_err(storage_error)?;
                            transaction.execute("UPDATE story_task_action_requests_v1 SET status='queued',result_task_id=NULL,error=NULL WHERE result_task_id=?1 AND status='failed'", [id]).map_err(storage_error)?;
                        }
                    }
                }
            }
        }
        if state["status"] == "running"
            && state["retryAttempts"]
                .as_i64()
                .is_some_and(|attempts| attempts > 0)
        {
            let old: serde_json::Value = serde_json::from_str(
                previous_state
                    .as_deref()
                    .ok_or(RepositoryError::CorruptState)?,
            )
            .map_err(|_| RepositoryError::CorruptState)?;
            if state["retryAttempts"].as_i64() > old["retryAttempts"].as_i64() {
                let previous = old["activeTaskId"]
                    .as_str()
                    .ok_or(RepositoryError::CorruptState)?;
                let task = crate::tasks::load_task(&transaction, previous)?;
                let unresolved: bool = transaction.query_row(
                    "SELECT EXISTS(SELECT 1 FROM story_tool_operations_v1 WHERE task_id=?1 AND state='accepted')",
                    [previous], |row| row.get(0),
                ).map_err(storage_error)?;
                if unresolved {
                    return Err(RepositoryError::TaskNotWritable);
                }
                if task.status == "failed"
                    && task.operation == "generateChapter"
                    && crate::tasks::recoverable_output(&transaction, &task)?
                {
                    // Author continuation recovers a verified stored product, not
                    // a model completion call or a final literary self-review.
                    transaction.execute(
                        "UPDATE story_tasks_v2 SET status='completed',stage='ready_for_review',delivery_status='ready_for_review',result_summary='作者继续：已核验完整章节保存回执，未重复生成正文',capability_revoked_at=COALESCE(capability_revoked_at,unixepoch()) WHERE task_id=?1 AND status='failed'",
                        [previous],
                    ).map_err(storage_error)?;
                    let recovered = crate::tasks::load_task(&transaction, previous)?;
                    crate::work_tasks::finish_attempt(&transaction, &recovered)?;
                } else {
                    transaction.execute(
                    "UPDATE story_work_steps_v1 SET status = 'queued', current_attempt_id = NULL WHERE id = (SELECT step_id FROM story_work_attempts_v1 WHERE attempt_id = ?1) AND status = 'blocked'",
                    [previous],
                ).map_err(storage_error)?;
                }
            }
        }
        if matches!(source_guard, GenerationRunSourceGuard::AuthorControl) { crate::work_interludes::invalidate(&transaction,&format!("work-{run_id}"))?; }
        let status = state["status"]
            .as_str()
            .ok_or(RepositoryError::CorruptState)?;
        let status = match status {
            "running" => "running",
            "paused" => "paused",
            "blocked" => "blocked",
            "completed" => "completed",
            "cancelled" => "cancelled",
            _ => return Err(RepositoryError::CorruptState),
        };
        let status=if crate::work_interludes::top(&transaction,&format!("work-{run_id}"))?.is_some(){"running"}else{status};
        transaction.execute(
            "UPDATE story_work_tasks_v1 SET status = ?1, version = version + 1 WHERE id = ?2 AND project_id = ?3",
            params![status, format!("work-{run_id}"), project_id.as_str()],
        ).map_err(storage_error)?;
        if matches!(source_guard, GenerationRunSourceGuard::AuthorControl) {
            transaction.execute("UPDATE story_work_tasks_v1 SET control_revision=control_revision+1 WHERE id=?1 AND project_id=?2", params![format!("work-{run_id}"), project_id.as_str()]).map_err(storage_error)?;
        }
        if let GenerationRunSourceGuard::ModelPause(task, request) = source_guard {
            crate::generation_model_control::finish(
                &transaction,
                task,
                request,
                expected_version + 1,
            )?;
            crate::recovery_operations::commit_operation(
                &transaction,
                self.recovery_operation.as_ref(),
                project_id,
                &task.task_id,
                &["pause_generation_run"],
                &format!(
                    "runtime:model-run-pause:{}:{}",
                    task.task_id, request.idempotency_key
                ),
            )?;
        }
        transaction.commit()?;
        Ok(StoredGenerationRun {
            run_id: run_id.to_owned(),
            project_id: project_id.clone(),
            version: expected_version + 1,
            state_json: result_state_json,
        })
    }
}

pub(crate) fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}

#[cfg(test)]
#[path = "authoring_runs_tests.rs"]
mod tests;

/// Public authoring runs capability; SQLite retains transaction ownership.
pub trait AuthoringRunsRepository {
fn generation_start_position(
        &self,
        project_id: &ProjectId,
    ) -> Result<GenerationStartPosition, RepositoryError>;
fn has_open_generation_run(&self, project_id: &ProjectId) -> Result<bool, RepositoryError>;
fn create_generation_run(
        &mut self,
        project_id: &ProjectId,
        run_id: &str,
        state_json: &str,
    ) -> Result<StoredGenerationRun, RepositoryError>;
fn generation_run(
        &self,
        project_id: &ProjectId,
        run_id: &str,
    ) -> Result<StoredGenerationRun, RepositoryError>;
fn generation_runs(
        &self,
        project_id: &ProjectId,
    ) -> Result<Vec<StoredGenerationRun>, RepositoryError>;
fn cancel_generation_run(
        &mut self,
        project_id: &ProjectId,
        run_id: &str,
        expected_version: i64,
        state_json: &str,
        now: i64,
    ) -> Result<StoredGenerationRun, RepositoryError>;
/// The selected Work control revision is checked inside the generation write transaction.
    /// This request-only guard is never persisted and never substitutes a Run version.
    fn with_generation_work_control<T>(
        &mut self,
        work_id: &str,
        expected_control_revision: i64,
        control: impl FnOnce(&mut Self) -> Result<T, RepositoryError>,
    ) -> Result<T, RepositoryError>;
/// Persist an explicit author control and its Work control revision atomically.
    fn update_generation_run_for_author_control(
        &mut self,
        project_id: &ProjectId,
        run_id: &str,
        expected_version: i64,
        state_json: &str,
    ) -> Result<StoredGenerationRun, RepositoryError>;
fn update_generation_run(
        &mut self,
        project_id: &ProjectId,
        run_id: &str,
        expected_version: i64,
        state_json: &str,
    ) -> Result<StoredGenerationRun, RepositoryError>;
/// Advances a generation decision only while its evaluated story inputs are unchanged.
    fn update_generation_run_if_source_matches(
        &mut self,
        project_id: &ProjectId,
        run_id: &str,
        expected_version: i64,
        state_json: &str,
        expected_source_digest: &str,
    ) -> Result<StoredGenerationRun, RepositoryError>;
/// The source proposal is consumed atomically with the first resumed run
    /// version, including its checkpoint and Work transition.
    fn update_generation_run_for_handoff(
        &mut self,
        project_id: &ProjectId,
        run_id: &str,
        expected_version: i64,
        state_json: &str,
        expected_source_digest: &str,
        source_task_id: &str,
    ) -> Result<StoredGenerationRun, RepositoryError>;
}

impl AuthoringRunsRepository for SqliteStoryRepository {
fn generation_start_position(
        &self,
        project_id: &ProjectId,
    ) -> Result<GenerationStartPosition, RepositoryError> {
        generation_start_position(&self.connection, project_id)
    }

fn has_open_generation_run(&self, project_id: &ProjectId) -> Result<bool, RepositoryError> {
        self.connection
            .query_row(
                "SELECT 1 FROM story_generation_runs_v1 WHERE project_id = ?1 AND json_extract(state_json, '$.status') IN ('running','paused','blocked') LIMIT 1",
                [project_id.as_str()],
                |_| Ok(()),
            )
            .optional()
            .map_err(storage_error)
            .map(|row| row.is_some())
    }

fn create_generation_run(
        &mut self,
        project_id: &ProjectId,
        run_id: &str,
        state_json: &str,
    ) -> Result<StoredGenerationRun, RepositoryError> {
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let exists = transaction
            .query_row(
                "SELECT 1 FROM story_projects_v2 WHERE project_id = ?1",
                [project_id.as_str()],
                |_| Ok(()),
            )
            .optional()
            .map_err(storage_error)?
            .is_some();
        if !exists {
            return Err(RepositoryError::ProjectNotFound);
        }
        let open: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM story_generation_runs_v1 WHERE project_id = ?1 AND json_extract(state_json, '$.status') IN ('running','paused','blocked'))",
            [project_id.as_str()], |row| row.get(0),
        ).map_err(storage_error)?;
        if open {
            return Err(RepositoryError::InvalidContent(
                "an unfinished generation run already owns this creative workspace".into(),
            ));
        }
        let mut state: serde_json::Value =
            serde_json::from_str(state_json).map_err(|_| RepositoryError::CorruptState)?;
        let budget = crate::work_budget_storage::take(&mut state)?;
        if !budget.charges.is_empty() {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        if state["id"] != run_id
            || state["projectId"] != project_id.as_str()
            || state["version"] != 0
            || state["status"] != "running"
        {
            return Err(RepositoryError::CorruptState);
        }
        let source_digest = crate::task_input::project_source_digest(&transaction, project_id)?;
        let position = generation_start_position(&transaction, project_id)?;
        let branch_head: String = transaction
            .query_row(
                "SELECT head_snapshot_id FROM story_branches_v2 WHERE project_id = ?1 AND branch_id = 'main'",
                [project_id.as_str()],
                |row| row.get(0),
            )
            .map_err(storage_error)?;
        if state["sourceDigest"] != source_digest || state["baseSnapshotId"] != branch_head {
            return Err(RepositoryError::AuthoringRunStale);
        }
        if state["completedChapters"] != position.completed_chapters
            || state["startChapterNumber"] != position.completed_chapters + 1
            || state["stage"] != "coordinate"
            || state["targetChapters"].as_i64().is_none_or(|target| {
                target <= position.completed_chapters
                    || target - position.completed_chapters > crate::MAX_GENERATION_CHAPTERS
            })
        {
            return Err(RepositoryError::AuthoringRunStale);
        }
        crate::generation_handoff::consume(&transaction, project_id, run_id, &mut state)?;
        transaction
            .execute(
                "INSERT INTO story_generation_runs_v1(run_id, project_id, version, state_json) VALUES (?1, ?2, 0, ?3)",
                params![run_id, project_id.as_str(), state.to_string()],
            )
            .map_err(storage_error)?;
        let intent = state["intentRevisions"][0]["instruction"]
            .as_str()
            .ok_or(RepositoryError::TaskBindingInvalid)?;
        let revision: i64 = transaction
            .query_row(
                "SELECT revision FROM story_workspaces_v2 WHERE project_id = ?1",
                [project_id.as_str()],
                |row| row.get(0),
            )
            .map_err(storage_error)?;
        transaction.execute(
            "INSERT INTO story_work_tasks_v1(id, project_id, thread_id, goal, kind, status, input_revision) VALUES (?1, ?2, ?5, ?3, 'generation', 'running', ?4)",
            params![format!("work-{run_id}"), project_id.as_str(), intent, revision, state["rootThreadId"].as_str().unwrap_or_default()],
        ).map_err(storage_error)?;
        if let Some(origin) = state["originTaskId"].as_str() {
            transaction.execute("INSERT INTO story_work_decisions_v1(work_task_id,origin_task_id) VALUES(?1,?2)",
                params![format!("work-{run_id}"),origin]).map_err(storage_error)?;
        }
        transaction
            .execute(
                "INSERT INTO story_work_budgets_v1(work_task_id,ledger_json) VALUES (?1,?2)",
                params![
                    format!("work-{run_id}"),
                    serde_json::to_string(&budget).map_err(|_| RepositoryError::CorruptState)?
                ],
            )
            .map_err(storage_error)?;
        crate::revision_continuation::record(&transaction, project_id, run_id, &state)?;
        crate::generation_author_source::record(&transaction, project_id, run_id, &state)?;
        transaction.commit().map_err(storage_error)?;
        Ok(StoredGenerationRun {
            run_id: run_id.to_owned(),
            project_id: project_id.clone(),
            version: 0,
            state_json: state.to_string(),
        })
    }

fn generation_run(
        &self,
        project_id: &ProjectId,
        run_id: &str,
    ) -> Result<StoredGenerationRun, RepositoryError> {
        let snapshot = self.connection.is_autocommit().then(|| self.connection.unchecked_transaction()).transpose().map_err(storage_error)?;
        let read = snapshot.as_ref().map(|tx| &**tx).unwrap_or(&self.connection);
        read
            .query_row(
                "SELECT version, state_json FROM story_generation_runs_v1 WHERE project_id = ?1 AND run_id = ?2",
                params![project_id.as_str(), run_id],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()
            .map_err(storage_error)?
            .ok_or(RepositoryError::AuthoringRunNotFound)
            .and_then(|(version, state_json)| {
                let state_json = crate::work_budget_storage::hydrate(&read, project_id, run_id, &state_json)?;
                let state: Value = serde_json::from_str(&state_json).map_err(|_| RepositoryError::CorruptState)?;
                crate::generation_author_source::validate_run(&read, project_id, run_id, &state)?;
                Ok(StoredGenerationRun {run_id:run_id.to_owned(),project_id:project_id.clone(),version,state_json})
            })
    }

fn generation_runs(
        &self,
        project_id: &ProjectId,
    ) -> Result<Vec<StoredGenerationRun>, RepositoryError> {
        let read = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let mut statement = read.prepare(
            "SELECT run_id, version, state_json FROM story_generation_runs_v1 WHERE project_id = ?1 ORDER BY rowid DESC LIMIT 20",
        ).map_err(storage_error)?;
        statement
            .query_map([project_id.as_str()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(storage_error)?
            .map(|row| {
                let (run_id, version, state_json) = row.map_err(storage_error)?;
                let state_json =
                    crate::work_budget_storage::hydrate(&read, project_id, &run_id, &state_json)?;
                let state: Value =
                    serde_json::from_str(&state_json).map_err(|_| RepositoryError::CorruptState)?;
                crate::generation_author_source::validate_run(&read, project_id, &run_id, &state)?;
                Ok(StoredGenerationRun {
                    run_id,
                    project_id: project_id.clone(),
                    version,
                    state_json,
                })
            })
            .collect()
    }

fn cancel_generation_run(
        &mut self,
        project_id: &ProjectId,
        run_id: &str,
        expected_version: i64,
        state_json: &str,
        now: i64,
    ) -> Result<StoredGenerationRun, RepositoryError> {
        let transaction = crate::work_control_commands::ControlTransaction::new(&self.connection)?;
        if let Some((work, version)) = &self.generation_work_control_guard {
            if work != &format!("work-{run_id}") {
                return Err(RepositoryError::TaskBindingInvalid);
            }
            let actual:Option<i64>=transaction.query_row("SELECT control_revision FROM story_work_tasks_v1 WHERE id=?1 AND project_id=?2 AND kind='generation'",params![work,project_id.as_str()],|r|r.get(0)).optional().map_err(storage_error)?;
            if actual != Some(*version) {
                return Err(RepositoryError::WorkTaskStale);
            }
        }
        crate::work_interludes::invalidate(&transaction,&format!("work-{run_id}"))?;
        crate::work_pause_barrier::clear(&transaction, &format!("work-{run_id}"))?;
        let previous: Option<(i64, String)> = transaction
            .query_row(
                "SELECT version, state_json FROM story_generation_runs_v1 WHERE project_id = ?1 AND run_id = ?2",
                params![project_id.as_str(), run_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(storage_error)?;
        let (version, previous_json) = previous.ok_or(RepositoryError::AuthoringRunNotFound)?;
        if version != expected_version {
            return Err(RepositoryError::AuthoringRunStale);
        }
        let previous_json =
            crate::work_budget_storage::hydrate(&transaction, project_id, run_id, &previous_json)?;
        let old: Value =
            serde_json::from_str(&previous_json).map_err(|_| RepositoryError::CorruptState)?;
        let next: Value =
            serde_json::from_str(state_json).map_err(|_| RepositoryError::CorruptState)?;
        let mut expected = old.clone();
        expected["version"] = Value::from(version + 1);
        expected["status"] = Value::String("cancelled".into());
        if old["id"] != run_id
            || old["projectId"] != project_id.as_str()
            || old["version"] != version
            || !matches!(
                old["status"].as_str(),
                Some("running" | "paused" | "blocked")
            )
            || next != expected
        {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        let mut stored_next = next.clone();
        crate::work_budget_storage::take(&mut stored_next)?;
        let updated = transaction
            .execute(
                "UPDATE story_generation_runs_v1 SET version = version + 1, state_json = ?1 WHERE project_id = ?2 AND run_id = ?3 AND version = ?4",
                params![stored_next.to_string(), project_id.as_str(), run_id, version],
            )
            .map_err(storage_error)?;
        if updated != 1 {
            return Err(RepositoryError::AuthoringRunStale);
        }
        let updated = transaction
            .execute(
                "UPDATE story_work_tasks_v1 SET status = 'cancelled', version = version + 1, control_revision = control_revision + 1 WHERE id = ?1 AND project_id = ?2",
                params![format!("work-{run_id}"), project_id.as_str()],
            )
            .map_err(storage_error)?;
        if updated != 1 {
            return Err(RepositoryError::CorruptState);
        }
        // Persist author cancellation before observing child termination. Both
        // remain in this transaction; the observer must not advance the Run CAS.
        if let Some(task_id) = old["activeTaskId"].as_str() {
            let task = crate::tasks::load_task(&transaction, task_id)?;
            let target: Value = serde_json::from_str(&task.target_json)
                .map_err(|_| RepositoryError::CorruptState)?;
            if task.project_id != *project_id || target["runId"] != run_id {
                return Err(RepositoryError::TaskBindingInvalid);
            }
            if matches!(task.status.as_str(), "queued" | "running" | "waiting") {
                transaction
                    .execute(
                        "UPDATE story_tasks_v2 SET status = 'cancelled', stage = 'cancelled', delivery_status = CASE WHEN output_refs_json = '[]' THEN 'pending' ELSE 'incomplete' END, capability_revoked_at = ?1 WHERE task_id = ?2",
                        params![now, task_id],
                    )
                    .map_err(storage_error)?;
                let cancelled = crate::tasks::load_task(&transaction, task_id)?;
                crate::work_tasks::finish_attempt(&transaction, &cancelled)?;
            }
        }
        transaction.commit()?;
        Ok(StoredGenerationRun {
            run_id: run_id.to_owned(),
            project_id: project_id.clone(),
            version: version + 1,
            state_json: state_json.to_owned(),
        })
    }

/// The selected Work control revision is checked inside the generation write transaction.
    /// This request-only guard is never persisted and never substitutes a Run version.
    fn with_generation_work_control<T>(
        &mut self,
        work_id: &str,
        expected_control_revision: i64,
        control: impl FnOnce(&mut Self) -> Result<T, RepositoryError>,
    ) -> Result<T, RepositoryError> {
        if self.generation_work_control_guard.is_some() {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        self.generation_work_control_guard = Some((work_id.to_owned(), expected_control_revision));
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| control(self)));
        self.generation_work_control_guard = None;
        match result { Ok(result) => result, Err(panic) => std::panic::resume_unwind(panic) }
    }

/// Persist an explicit author control and its Work control revision atomically.
    fn update_generation_run_for_author_control(
        &mut self,
        project_id: &ProjectId,
        run_id: &str,
        expected_version: i64,
        state_json: &str,
    ) -> Result<StoredGenerationRun, RepositoryError> {
        self.update_generation_run_with_guard(
            project_id, run_id, expected_version, state_json,
            GenerationRunSourceGuard::AuthorControl, None,
        )
    }

fn update_generation_run(
        &mut self,
        project_id: &ProjectId,
        run_id: &str,
        expected_version: i64,
        state_json: &str,
    ) -> Result<StoredGenerationRun, RepositoryError> {
        self.update_generation_run_with_guard(
            project_id,
            run_id,
            expected_version,
            state_json,
            GenerationRunSourceGuard::Unchecked,
            None,
        )
    }

/// Advances a generation decision only while its evaluated story inputs are unchanged.
    fn update_generation_run_if_source_matches(
        &mut self,
        project_id: &ProjectId,
        run_id: &str,
        expected_version: i64,
        state_json: &str,
        expected_source_digest: &str,
    ) -> Result<StoredGenerationRun, RepositoryError> {
        self.update_generation_run_with_guard(
            project_id,
            run_id,
            expected_version,
            state_json,
            GenerationRunSourceGuard::Matches(expected_source_digest),
            None,
        )
    }

/// The source proposal is consumed atomically with the first resumed run
    /// version, including its checkpoint and Work transition.
    fn update_generation_run_for_handoff(
        &mut self,
        project_id: &ProjectId,
        run_id: &str,
        expected_version: i64,
        state_json: &str,
        expected_source_digest: &str,
        source_task_id: &str,
    ) -> Result<StoredGenerationRun, RepositoryError> {
        self.update_generation_run_with_guard(
            project_id,
            run_id,
            expected_version,
            state_json,
            GenerationRunSourceGuard::Matches(expected_source_digest),
            Some(source_task_id),
        )
    }
}
