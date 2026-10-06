use serde_json::{Value, json};

use crate::{RepositoryError, SqliteStoryRepository, TaskRecord};

impl SqliteStoryRepository {
    /// Complete approved scenes; the task-source transport pages this exact projection.
    /// This does not expand the Agent's ordinary evidence-query authority.
    pub fn revision_outcome_input(&self, task: &TaskRecord) -> Result<Value, RepositoryError> {
        let read_input = || {
            let read = &self.connection;
            if task.operation != "assessRevisionOutcome" {
                return Err(RepositoryError::TaskBindingInvalid);
            }
            let target: Value = serde_json::from_str(&task.target_json)
                .map_err(|_| RepositoryError::CorruptState)?;
            crate::revision_outcome::validate_target(read, &task.project_id, &target)?;
            if !crate::revision_outcome::inputs_current(read, &task.project_id, &target)? {
                return Err(RepositoryError::TaskInputStale);
            }
            let id = target["writerTaskId"]
                .as_str()
                .ok_or(RepositoryError::TaskBindingInvalid)?;
            let (_, writer) = crate::revision_outcome::writer(read, &task.project_id, id)?;
            let desired = crate::revision_outcome::desired_outcomes(&writer)?;
            let chapter = target["chapterId"]
                .as_str()
                .ok_or(RepositoryError::TaskBindingInvalid)?;
            let mut before = Vec::new();
            let mut after = Vec::new();
            for (field, scenes) in [
                ("originalRevisionId", &mut before),
                ("draftRevisionId", &mut after),
            ] {
                let revision = target[field]
                    .as_str()
                    .ok_or(RepositoryError::TaskBindingInvalid)?;
                let draft = crate::revision_outcome_validation::draft(
                    read,
                    &task.project_id,
                    chapter,
                    revision,
                )?;
                let groups = draft["sceneGroups"]
                    .as_array()
                    .ok_or(RepositoryError::CorruptState)?;
                for group in groups.iter().filter(|group| {
                    desired
                        .iter()
                        .any(|item| item.group_ids.iter().any(|id| group["id"] == *id))
                }) {
                    let body = group["body"]
                        .as_str()
                        .ok_or(RepositoryError::CorruptState)?;
                    scenes.push(json!({"groupId":group["id"],"body":body,"truncated":false}));
                }
            }
            Ok(
                json!({"desiredOutcomes":desired,"beforeScenes":before,"afterScenes":after,"coverage":{"complete":true,"delivery":"taskSourcePages"}}),
            )
        };
        if self.connection.is_autocommit() {
            self.with_read_snapshot(read_input)?
        } else {
            read_input()
        }
    }
}
