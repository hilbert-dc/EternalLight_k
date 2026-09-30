use rusqlite::{OptionalExtension, TransactionBehavior, params};
use serde_json::Value;
use uuid::Uuid;

use crate::authoring_runs::storage_error;
use crate::{ProjectId, RepositoryError, SqliteStoryRepository, StoredGenerationRun};

impl SqliteStoryRepository {
    pub fn materialize_generation_chapter(
        &mut self,
        project_id: &ProjectId,
        run_id: &str,
        expected_version: i64,
        proposed_state_json: &str,
        now: i64,
    ) -> Result<StoredGenerationRun, RepositoryError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let stored: Option<(i64, String)> = transaction
            .query_row(
                "SELECT version, state_json FROM story_generation_runs_v1 WHERE project_id = ?1 AND run_id = ?2",
                params![project_id.as_str(), run_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(storage_error)?;
        let (version, old_json) = stored.ok_or(RepositoryError::AuthoringRunNotFound)?;
        if version != expected_version {
            return Err(RepositoryError::AuthoringRunStale);
        }
        let old_json =
            crate::work_budget_storage::hydrate(&transaction, project_id, run_id, &old_json)?;
        let old: Value =
            serde_json::from_str(&old_json).map_err(|_| RepositoryError::CorruptState)?;
        let mut next: Value =
            serde_json::from_str(proposed_state_json).map_err(|_| RepositoryError::CorruptState)?;
        let number = old["completedChapters"]
            .as_i64()
            .and_then(|value| value.checked_add(1))
            .ok_or(RepositoryError::CorruptState)?;
        let chapter_id = next["currentChapterId"]
            .as_str()
            .ok_or(RepositoryError::TaskBindingInvalid)?
            .to_owned();
        let step_token = next["stepToken"]
            .as_str()
            .ok_or(RepositoryError::TaskBindingInvalid)?
            .to_owned();
        let mut expected = old.clone();
        expected["version"] = Value::from(version + 1);
        expected["stage"] = Value::String("draft".into());
        expected["currentChapterId"] = Value::String(chapter_id.clone());
        expected["currentChapterNumber"] = Value::from(number);
        expected["stepToken"] = Value::String(step_token.clone());
        if old["id"] != run_id
            || old["projectId"] != project_id.as_str()
            || old["version"] != version
            || old["status"] != "running"
            || old["stage"] != "materializeChapter"
            || !old["activeTaskId"].is_null()
            || !old["currentChapterId"].is_null()
            || !old["currentChapterNumber"].is_null()
            || old["targetChapters"]
                .as_i64()
                .is_none_or(|target| number > target)
            || old["deadlineAt"]
                .as_i64()
                .is_none_or(|deadline| now >= deadline)
            || chapter_id
                .strip_prefix("chapter-")
                .is_none_or(|id| Uuid::parse_str(id).is_err())
            || step_token.is_empty()
            || step_token == old["stepToken"]
            || next != expected
        {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        if crate::task_input::project_source_digest(&transaction, project_id)?
            != old["sourceDigest"]
        {
            return Err(RepositoryError::AuthoringRunStale);
        }
        let (revision, next_number): (i64, i64) = transaction
            .query_row(
                "SELECT w.revision, (SELECT COALESCE(MAX(number), 0) + 1 FROM story_chapters_v2 WHERE project_id = ?1) FROM story_workspaces_v2 w WHERE w.project_id = ?1",
                [project_id.as_str()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(storage_error)?;
        if next_number != number {
            return Err(RepositoryError::AuthoringRunStale);
        }
        let (plan_revision_id, plan_content): (String, String) = transaction
            .query_row(
                "SELECT h.candidate_revision_id, r.content_json FROM story_object_heads_v2 h JOIN story_revisions_v2 r ON r.revision_id = h.candidate_revision_id AND r.project_id = h.project_id AND r.object_id = h.object_id WHERE h.project_id = ?1 AND h.object_id = 'artifact-plan' AND h.object_kind = 'plan'",
                [project_id.as_str()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(storage_error)?
            .ok_or(RepositoryError::TaskBindingInvalid)?;
        if let Some(plan_task_id) = old["lastTaskId"].as_str() {
            let plan_belongs_to_run: bool = transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM story_tasks_v2 t JOIN story_task_outputs_v2 o ON o.task_id = t.task_id WHERE t.task_id = ?1 AND t.project_id = ?2 AND t.operation = 'generatePlan' AND t.status = 'completed' AND json_extract(t.target_json, '$.runId') = ?3 AND json_extract(t.target_json, '$.planFromChapter') = ?4 AND o.output_kind = 'plan' AND o.revision_id = ?5)",
                params![plan_task_id, project_id.as_str(), run_id, number, plan_revision_id],
                |row| row.get(0),
            ).map_err(storage_error)?;
            if !plan_belongs_to_run {
                return Err(RepositoryError::TaskBindingInvalid);
            }
        }
        let plan: Value =
            serde_json::from_str(&plan_content).map_err(|_| RepositoryError::CorruptState)?;
        let chapters = plan
            .get("payload")
            .unwrap_or(&plan)
            .get("chapters")
            .and_then(Value::as_array)
            .ok_or(RepositoryError::TaskBindingInvalid)?;
        let matching = chapters
            .iter()
            .filter(|chapter| chapter["chapterNumber"].as_i64() == Some(number))
            .collect::<Vec<_>>();
        let [chapter_plan] = matching.as_slice() else {
            return Err(RepositoryError::TaskBindingInvalid);
        };
        crate::bound_writer_plan::validate_plan_position(chapter_plan)
            .map_err(|_| RepositoryError::InvalidChapter)?;
        let chapter_plan_id = chapter_plan["chapterPlanId"]
            .as_str()
            .filter(|id| !id.trim().is_empty())
            .ok_or(RepositoryError::TaskBindingInvalid)?;
        let chapter_title = chapter_plan["publicTitle"]
            .as_str()
            .or_else(|| chapter_plan["title"].as_str())
            .filter(|title| !title.trim().is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| format!("第 {number} 章"));
        let next_revision = revision
            .checked_add(1)
            .ok_or(RepositoryError::CorruptState)?;
        transaction
            .execute(
                "INSERT INTO story_chapters_v2(chapter_id, project_id, number, title, status, summary) VALUES (?1, ?2, ?3, ?4, 'draft', '')",
                params![chapter_id, project_id.as_str(), number, chapter_title],
            )
            .map_err(storage_error)?;
        transaction
            .execute(
                "INSERT INTO story_chapter_plan_bindings_v2(project_id, plan_revision_id, chapter_plan_id, chapter_id, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![project_id.as_str(), plan_revision_id, chapter_plan_id, chapter_id, now],
            )
            .map_err(storage_error)?;
        transaction
            .execute(
                "UPDATE story_workspaces_v2 SET revision = ?1 WHERE project_id = ?2 AND revision = ?3",
                params![next_revision, project_id.as_str(), revision],
            )
            .map_err(storage_error)?;
        crate::chapters::cancel_active_tasks_for_chapter_structure(&transaction, project_id, now)?;
        next["sourceDigest"] = Value::String(crate::task_input::project_source_digest(
            &transaction,
            project_id,
        )?);
        let state_json = next.to_string();
        crate::work_budget_storage::take(&mut next)?;
        let updated = transaction
            .execute(
                "UPDATE story_generation_runs_v1 SET version = version + 1, state_json = ?1 WHERE project_id = ?2 AND run_id = ?3 AND version = ?4",
                params![next.to_string(), project_id.as_str(), run_id, version],
            )
            .map_err(storage_error)?;
        if updated != 1 {
            return Err(RepositoryError::AuthoringRunStale);
        }
        let updated = transaction
            .execute(
                "UPDATE story_work_tasks_v1 SET version = version + 1 WHERE id = ?1 AND project_id = ?2 AND status = 'running'",
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
            state_json,
        })
    }
}
