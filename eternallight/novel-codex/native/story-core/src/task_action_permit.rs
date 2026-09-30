use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{ProjectId, RepositoryError, SqliteStoryRepository, TaskActionRequest};

pub(crate) enum PermissionBasis<'a> {
    ExactAction,
    FiniteScope(&'a str),
}

impl SqliteStoryRepository {
    /// Reads historical author evidence, not a fresh authorization to execute.
    pub fn task_action_permission(
        &self,
        project: &ProjectId,
        source_task_id: &str,
        action_id: &str,
    ) -> Result<Option<Value>, RepositoryError> {
        let read = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let action = crate::task_actions::load_action(&read, project, source_task_id, action_id)?;
        let result = if granted(&read, project, &action)? {
            let (parent, payload, at, scope): (String, String, i64, Option<String>) = read.query_row(
                "SELECT work_task_id,evidence_json,granted_at,scope_permit_id FROM story_action_permits_v1 WHERE action_id=?1",
                [action_id], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?)),
            ).map_err(storage_error)?;
            let evidence: Value =
                serde_json::from_str(&payload).map_err(|_| RepositoryError::CorruptState)?;
            Some(
                json!({"kind":if scope.is_some() { "finiteScope" } else { "exactAction" }, "actionId":action_id,"workTaskId":parent,
                "scopePermitId":scope,"grantedAt":at,"evidence":evidence,"actionStatus":action.status}),
            )
        } else {
            None
        };
        read.commit().map_err(storage_error)?;
        Ok(result)
    }
}

/// Frozen evidence for an author's exact-action permission. This is not a
/// project-wide grant and cannot authorize another action or formal acceptance.
#[derive(Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ActionPermission {
    author_message_id: Option<String>,
    project_id: String,
    source_task_id: String,
    intent_revision: i64,
    input_revision: i64,
    operation: String,
    target: Value,
    instruction: String,
    author_intent: String,
}

fn evidence(
    db: &Connection,
    project: &ProjectId,
    action: &TaskActionRequest,
) -> Result<ActionPermission, RepositoryError> {
    Ok(ActionPermission {
        author_message_id: crate::author_message::identity(db, &action.source_task_id)?,
        project_id: project.as_str().to_owned(),
        source_task_id: action.source_task_id.clone(),
        intent_revision: action.intent_revision,
        input_revision: action.input_revision,
        operation: action.operation.clone(),
        target: action.target.clone(),
        instruction: action.instruction.clone(),
        author_intent: crate::task_author_intent::source_intent(
            db,
            &action.source_task_id,
            action.intent_revision,
        )?,
    })
}

pub(crate) fn granted(
    db: &Connection,
    project: &ProjectId,
    action: &TaskActionRequest,
) -> Result<bool, RepositoryError> {
    let stored: Option<(String, String, Option<String>)> = db
        .query_row(
            "SELECT work_task_id,evidence_json,scope_permit_id FROM story_action_permits_v1 WHERE action_id=?1",
            [&action.id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(storage_error)?;
    let Some((parent, stored, scope)) = stored else {
        return Ok(false);
    };
    let stored: ActionPermission =
        serde_json::from_str(&stored).map_err(|_| RepositoryError::CorruptState)?;
    let belongs: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM story_work_attempts_v1 a JOIN story_work_steps_v1 s ON s.id=a.step_id JOIN story_work_tasks_v1 w ON w.id=s.work_task_id WHERE a.attempt_id=?1 AND w.id=?2 AND w.project_id=?3)",
        params![action.source_task_id,parent,project.as_str()], |row| row.get(0),
    ).map_err(storage_error)?;
    if !belongs || stored != evidence(db, project, action)? {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    if let Some(id) = scope {
        crate::execution_scope_storage::validate(db, project, action, &id)?;
    }
    Ok(true)
}

pub(crate) fn grant(
    db: &Connection,
    project: &ProjectId,
    parent: &str,
    action: &TaskActionRequest,
    basis: PermissionBasis<'_>,
    now: i64,
) -> Result<(), RepositoryError> {
    let payload = serde_json::to_string(&evidence(db, project, action)?)
        .map_err(|_| RepositoryError::CorruptState)?;
    let scope = match basis {
        PermissionBasis::ExactAction => None,
        PermissionBasis::FiniteScope(id) => Some(id),
    };
    db.execute(
        "INSERT INTO story_action_permits_v1(action_id,work_task_id,evidence_json,granted_at,scope_permit_id) VALUES(?1,?2,?3,?4,?5)",
        params![action.id,parent,payload,now,scope],
    ).map_err(storage_error)?;
    Ok(())
}

fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}
