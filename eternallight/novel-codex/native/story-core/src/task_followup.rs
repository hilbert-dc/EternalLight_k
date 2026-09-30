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
    pub fn task_followup(
        &self,
        project_id: &ProjectId,
        task_id: &str,
    ) -> Result<Option<TaskFollowup>, RepositoryError> {
        let task = self
            .task(project_id, task_id)?
            .ok_or(RepositoryError::TaskNotFound)?;
        if task.status != "completed" {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        if task.operation != "generateChapter" {
            return Ok(None);
        }
        let target: Value =
            serde_json::from_str(&task.target_json).map_err(|_| RepositoryError::CorruptState)?;
        if target.get("runId").is_some() {
            // The persistent generation run schedules its own extract step.
            return Ok(None);
        }
        let chapter_id = target
            .get("chapterId")
            .and_then(Value::as_str)
            .ok_or(RepositoryError::CorruptState)?;
        let knowledge = self.chapter_knowledge(project_id, chapter_id)?;
        let source_revision_id = knowledge
            .get("sourceRevisionId")
            .and_then(Value::as_str)
            .ok_or(RepositoryError::TaskInputStale)?;
        let outputs: Vec<Value> = serde_json::from_str(&task.output_refs_json)
            .map_err(|_| RepositoryError::CorruptState)?;
        let output_revision = outputs
            .last()
            .and_then(|output| output.get("revisionId"))
            .and_then(Value::as_str)
            .ok_or(RepositoryError::CorruptState)?;
        if source_revision_id != output_revision {
            return Err(RepositoryError::TaskInputStale);
        }
        Ok(self
            .chapter_followup(project_id, chapter_id)?
            .map(|action| match action {
                TaskFollowup::ModelKnowledge {
                    chapter_id,
                    source_revision_id,
                    ..
                } => TaskFollowup::ModelKnowledge {
                    chapter_id,
                    source_revision_id,
                    source_task_id: Some(task_id.into()),
                },
            }))
    }

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
            "SELECT t.task_id FROM story_task_outputs_v2 o JOIN story_tasks_v2 t ON t.task_id = o.task_id WHERE t.project_id = ?1 AND t.operation = 'generateChapter' AND t.status = 'completed' AND o.revision_id = ?2 ORDER BY t.rowid DESC LIMIT 1",
            rusqlite::params![project_id.as_str(), source_revision_id], |row| row.get(0),
        ).optional().map_err(|error| RepositoryError::Storage(error.to_string()))?;
        Ok(Some(TaskFollowup::ModelKnowledge {
            chapter_id: chapter_id.to_owned(),
            source_revision_id: source_revision_id.to_owned(),
            source_task_id,
        }))
    }
}
