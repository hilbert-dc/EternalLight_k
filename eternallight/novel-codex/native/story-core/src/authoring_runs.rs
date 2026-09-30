use rusqlite::{OptionalExtension, params};
use serde_json::Value;

use crate::{ProjectId, RepositoryError, SqliteStoryRepository};

enum GenerationRunSourceGuard<'a> {
    Unchecked,
    Matches(&'a str),
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
    let expected_operation = match state["stage"].as_str() {
        Some("brief") => "generateBrief",
        Some("outline") => "generateOutline",
        Some("rollingPlan") => "generatePlan",
        Some("draft") => "generateChapter",
        Some("extract") => "modelKnowledge",
        Some("commitments") => "extractCommitments",
        Some("assess") => "assessSceneOutcome",
        _ => return Err(RepositoryError::TaskBindingInvalid),
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
        || expected_operation != operation
        || target["runId"] != run_id
        || target["runVersion"].as_i64() != Some(version)
        || target["stepToken"] != state["stepToken"]
        || target["intentGeneration"] != state["intentGeneration"]
        || now
            >= state["deadlineAt"]
                .as_i64()
                .ok_or(RepositoryError::CorruptState)?
        || state["usedTaskAttempts"]
            .as_i64()
            .zip(state["maxTaskAttempts"].as_i64())
            .is_none_or(|(used, max)| used >= max)
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
        state["deadlineAt"]
            .as_i64()
            .ok_or(RepositoryError::CorruptState)?,
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

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationStartPosition {
    pub completed_chapters: i64,
    pub first_stage: String,
}

fn generation_start_position(
    db: &rusqlite::Connection,
    project_id: &ProjectId,
) -> Result<GenerationStartPosition, RepositoryError> {
    let (count, first, last): (i64, i64, i64) = db
        .query_row(
            "SELECT COUNT(*), COALESCE(MIN(number), 0), COALESCE(MAX(number), 0) FROM story_chapters_v2 WHERE project_id = ?1",
            [project_id.as_str()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(storage_error)?;
    if count > 0 && (first != 1 || last != count) {
        return Err(RepositoryError::InvalidContent(
            "automatic continuation requires chapters numbered consecutively from one".into(),
        ));
    }
    let unready: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM story_chapters_v2 c
             LEFT JOIN story_object_heads_v2 h ON h.project_id = c.project_id AND h.object_id = 'draft-' || c.chapter_id AND h.object_kind = 'draft'
             LEFT JOIN story_knowledge_heads_v2 k ON k.project_id = c.project_id AND k.object_id = h.object_id
             LEFT JOIN story_modeling_batches_v2 b ON b.project_id = c.project_id AND b.batch_id = k.batch_id
             WHERE c.project_id = ?1 AND (COALESCE(h.candidate_revision_id, h.accepted_revision_id) IS NULL OR b.status IS NOT 'ready' OR b.source_revision_id IS NOT COALESCE(h.candidate_revision_id, h.accepted_revision_id))",
            [project_id.as_str()],
            |row| row.get(0),
        )
        .map_err(storage_error)?;
    if unready != 0 {
        return Err(RepositoryError::InvalidContent(
            "automatic continuation requires current draft and knowledge for every existing chapter".into(),
        ));
    }
    let (brief, outline, plan, plan_locked): (i64, i64, i64, i64) = db
        .query_row(
            "SELECT
                COALESCE(MAX(CASE WHEN object_id = 'artifact-brief' AND COALESCE(candidate_revision_id, accepted_revision_id) IS NOT NULL THEN 1 ELSE 0 END), 0),
                COALESCE(MAX(CASE WHEN object_id = 'artifact-outline' AND COALESCE(candidate_revision_id, accepted_revision_id) IS NOT NULL THEN 1 ELSE 0 END), 0),
                COALESCE(MAX(CASE WHEN object_id = 'artifact-plan' AND COALESCE(candidate_revision_id, accepted_revision_id) IS NOT NULL THEN 1 ELSE 0 END), 0),
                COALESCE(MAX(CASE WHEN object_id = 'artifact-plan' AND locked = 1 THEN 1 ELSE 0 END), 0)
             FROM story_object_heads_v2 WHERE project_id = ?1 AND object_id IN ('artifact-brief', 'artifact-outline', 'artifact-plan')",
            [project_id.as_str()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .map_err(storage_error)?;
    if plan_locked != 0 {
        return Err(RepositoryError::InvalidContent(
            "automatic continuation cannot rewrite a locked plan".into(),
        ));
    }
    let first_stage = if count > 0 || outline != 0 || plan != 0 {
        "rollingPlan"
    } else if brief == 0 {
        "brief"
    } else if outline == 0 {
        "outline"
    } else {
        "rollingPlan"
    };
    Ok(GenerationStartPosition {
        completed_chapters: count,
        first_stage: first_stage.into(),
    })
}

impl SqliteStoryRepository {
    pub fn generation_start_position(
        &self,
        project_id: &ProjectId,
    ) -> Result<GenerationStartPosition, RepositoryError> {
        generation_start_position(&self.connection, project_id)
    }

    pub fn has_open_generation_run(&self, project_id: &ProjectId) -> Result<bool, RepositoryError> {
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

    pub fn create_generation_run(
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
            || state["stage"] != position.first_stage
            || state["targetChapters"].as_i64().is_none_or(|target| {
                target <= position.completed_chapters || target > position.completed_chapters + 3
            })
        {
            return Err(RepositoryError::AuthoringRunStale);
        }
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
            "INSERT INTO story_work_tasks_v1(id, project_id, thread_id, goal, kind, status, input_revision) VALUES (?1, ?2, '', ?3, 'generation', 'running', ?4)",
            params![format!("work-{run_id}"), project_id.as_str(), intent, revision],
        ).map_err(storage_error)?;
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
            state_json: state_json.to_owned(),
        })
    }

    pub fn generation_run(
        &self,
        project_id: &ProjectId,
        run_id: &str,
    ) -> Result<StoredGenerationRun, RepositoryError> {
        let read = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
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

    pub fn generation_runs(
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

    pub fn cancel_generation_run(
        &mut self,
        project_id: &ProjectId,
        run_id: &str,
        expected_version: i64,
        state_json: &str,
        now: i64,
    ) -> Result<StoredGenerationRun, RepositoryError> {
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage_error)?;
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
                "UPDATE story_work_tasks_v1 SET status = 'cancelled', version = version + 1 WHERE id = ?1 AND project_id = ?2",
                params![format!("work-{run_id}"), project_id.as_str()],
            )
            .map_err(storage_error)?;
        if updated != 1 {
            return Err(RepositoryError::CorruptState);
        }
        transaction.commit().map_err(storage_error)?;
        Ok(StoredGenerationRun {
            run_id: run_id.to_owned(),
            project_id: project_id.clone(),
            version: version + 1,
            state_json: state_json.to_owned(),
        })
    }

    pub fn update_generation_run(
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
        )
    }

    /// Advances a generation decision only while its evaluated story inputs are unchanged.
    pub fn update_generation_run_if_source_matches(
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
        )
    }

    fn update_generation_run_with_guard(
        &mut self,
        project_id: &ProjectId,
        run_id: &str,
        expected_version: i64,
        state_json: &str,
        source_guard: GenerationRunSourceGuard<'_>,
    ) -> Result<StoredGenerationRun, RepositoryError> {
        // Shared repository reads reuse this write snapshot without opening nested transactions.
        let transaction = rusqlite::Transaction::new_unchecked(
            &self.connection,
            rusqlite::TransactionBehavior::Immediate,
        )
        .map_err(storage_error)?;
        if let GenerationRunSourceGuard::Matches(expected) = source_guard {
            let actual = crate::task_input::project_source_digest(&transaction, project_id)?;
            if actual != expected {
                return Err(RepositoryError::AuthoringRunStale);
            }
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
        let result_state_json = stored_state.to_string();
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
        if state["status"] == "cancelled" {
            return Err(RepositoryError::TaskBindingInvalid);
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
                transaction.execute(
                    "UPDATE story_work_steps_v1 SET status = 'queued', current_attempt_id = NULL WHERE id = (SELECT step_id FROM story_work_attempts_v1 WHERE attempt_id = ?1) AND status = 'blocked'",
                    [previous],
                ).map_err(storage_error)?;
            }
        }
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
        transaction.execute(
            "UPDATE story_work_tasks_v1 SET status = ?1, version = version + 1 WHERE id = ?2 AND project_id = ?3",
            params![status, format!("work-{run_id}"), project_id.as_str()],
        ).map_err(storage_error)?;
        transaction.commit().map_err(storage_error)?;
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
