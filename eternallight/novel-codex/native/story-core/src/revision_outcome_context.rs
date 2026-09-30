use serde_json::{Value, json};

use crate::{RepositoryError, SqliteStoryRepository, TaskRecord};

impl SqliteStoryRepository {
    /// Bounded original/new scene excerpts from the persisted approved writer scope.
    /// Truncation is explicit; this projection never expands an Agent's query authority.
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
            let mut complete = true;
            let mut before = Vec::new();
            let mut after = Vec::new();
            for (field, scenes) in [
                ("originalRevisionId", &mut before),
                ("draftRevisionId", &mut after),
            ] {
                let mut remaining = 6_000;
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
                    let mut bounded = String::new();
                    for character in body.chars() {
                        if character.len_utf8() > remaining {
                            break;
                        }
                        remaining -= character.len_utf8();
                        bounded.push(character);
                    }
                    let truncated = bounded.len() != body.len();
                    complete &= !truncated;
                    scenes
                        .push(json!({"groupId":group["id"],"body":bounded,"truncated":truncated}));
                }
            }
            Ok(
                json!({"desiredOutcomes":desired,"beforeScenes":before,"afterScenes":after,"coverage":{"complete":complete,"textByteLimit":12_000}}),
            )
        };
        if self.connection.is_autocommit() {
            self.with_read_snapshot(read_input)?
        } else {
            read_input()
        }
    }
}
