use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{ProjectId, RepositoryError, SqliteStoryRepository};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum TaskFollowup {
    ModelKnowledge {
        chapter_id: String,
        source_revision_id: String,
        source_task_id: Option<String>,
    },
}

impl SqliteStoryRepository {
    pub fn chapter_followup(
        &self,
        project_id: &ProjectId,
        chapter_id: &str,
    ) -> Result<Option<TaskFollowup>, RepositoryError> {
        if !self
            .chapters(project_id)?
            .iter()
            .any(|chapter| chapter.chapter_id == chapter_id)
        {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        let knowledge = self.chapter_knowledge(project_id, chapter_id)?;
        let source_revision_id = knowledge
            .get("sourceRevisionId")
            .and_then(Value::as_str)
            .ok_or(RepositoryError::TaskInputStale)?;
        if knowledge.get("status").and_then(Value::as_str) == Some("ready") {
            return Ok(None);
        }
        let source_task_id: Option<String> = self.connection.query_row(
            "SELECT t.task_id FROM story_task_outputs_v2 o JOIN story_tasks_v2 t ON t.task_id = o.task_id WHERE t.project_id = ?1 AND t.operation = 'generateChapter' AND o.revision_id = ?2 ORDER BY t.rowid DESC LIMIT 1",
            rusqlite::params![project_id.as_str(), source_revision_id], |row| row.get(0),
        ).optional().map_err(|error| RepositoryError::Storage(error.to_string()))?;
        // A writer output is provenance, not perpetual authorization to reuse
        // its WorkTask. Preserve actionable blockers on the original workflow;
        // a terminal or stale workflow cannot own a new analysis attempt.
        let source_task_id = if let Some(source) = source_task_id {
            let work = self.work_task_for_attempt(project_id, &source)?;
            let decision_owned:bool=self.connection.query_row("SELECT EXISTS(SELECT 1 FROM story_work_decisions_v1 WHERE work_task_id=?1 AND finished=0)",[&work.id],|r|r.get(/*idx*/0)).map_err(|e|RepositoryError::Storage(e.to_string()))?;
            let owns_execution = decision_owned
                || matches!(work.status.as_str(), "running" | "paused" | "blocked")
                || (work.status == "completed" && work.kind != "generation");
            owns_execution.then_some(source)
        } else {
            None
        };
        Ok(Some(TaskFollowup::ModelKnowledge {
            chapter_id: chapter_id.to_owned(),
            source_revision_id: source_revision_id.to_owned(),
            source_task_id,
        }))
    }
}

#[cfg(test)]
#[path = "task_followup_tests.rs"]
mod tests;
