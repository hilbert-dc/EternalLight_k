use std::collections::HashSet;

use rusqlite::{Connection, params};
use serde_json::Value;

use crate::{ProjectId, RepositoryError, SqliteStoryRepository, TaskRecord};

impl SqliteStoryRepository {
    /// Host-side source for merging a Writer prefix; never expose its hidden suffix to the model.
    pub fn task_draft_base(
        &self,
        project: &ProjectId,
        task_id: &str,
    ) -> Result<Option<Value>, RepositoryError> {
        let task = crate::tasks::load_task(&self.connection, task_id)?;
        if task.project_id != *project || task.operation != "generateChapter" {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        crate::task_input::assert_task_inputs_current(&self.connection, &task)?;
        draft_base(&self.connection, &task)
    }
}

pub(crate) fn draft_base(
    db: &Connection,
    task: &TaskRecord,
) -> Result<Option<Value>, RepositoryError> {
    let (object, candidate) = crate::task_input::expected_task_target(db, task)?
        .ok_or(RepositoryError::TaskBindingInvalid)?;
    let source = crate::workspace_selection::task_output_working_base(
        db,
        &task.task_id,
        object.as_str(),
        candidate.as_ref(),
    )?;
    if let Some(source) = source {
        let content: String = db.query_row(
            "SELECT content_json FROM story_revisions_v2 WHERE project_id=?1 AND object_id=?2 AND revision_id=?3",
            params![task.project_id.as_str(),object.as_str(),source.as_str()], |row| row.get(0),
        ).map_err(|error| RepositoryError::Storage(error.to_string()))?;
        serde_json::from_str(&content)
            .map(Some)
            .map_err(|_| RepositoryError::CorruptState)
    } else {
        Ok(None)
    }
}

pub(crate) fn validate_write(
    db: &Connection,
    task: &TaskRecord,
    content: &str,
) -> Result<(), RepositoryError> {
    let before = draft_base(db, task)?.unwrap_or(Value::Null);
    let output: Value =
        serde_json::from_str(content).map_err(|_| RepositoryError::TaskOutputForbidden)?;
    let prior = before["sceneGroups"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let after = output["sceneGroups"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let mut protected = HashSet::new();
    for (index, group) in prior.iter().enumerate() {
        if group["locked"] == true {
            let id = group["id"]
                .as_str()
                .filter(|id| !id.is_empty())
                .ok_or(RepositoryError::CorruptState)?;
            if !protected.insert(id) || after.get(index) != Some(group) {
                return Err(RepositoryError::TaskOutputForbidden);
            }
        }
    }
    let mut ids = HashSet::new();
    for group in after {
        let id = group["id"]
            .as_str()
            .filter(|id| !id.is_empty())
            .ok_or(RepositoryError::TaskOutputForbidden)?;
        if !ids.insert(id) || (group["locked"] != false && !protected.contains(id)) {
            return Err(RepositoryError::TaskOutputForbidden);
        }
    }
    if !protected.is_empty() {
        let body = after
            .iter()
            .map(|group| group["body"].as_str().map(str::trim))
            .collect::<Option<Vec<_>>>()
            .ok_or(RepositoryError::TaskOutputForbidden)?
            .join("\n\n---\n\n");
        if output["body"].as_str() != Some(body.as_str()) {
            return Err(RepositoryError::TaskOutputForbidden);
        }
    }
    Ok(())
}
