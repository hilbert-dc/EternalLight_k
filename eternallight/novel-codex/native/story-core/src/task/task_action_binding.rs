use rusqlite::Connection;
use serde_json::Value;

use crate::{ProjectId, RepositoryError};

pub(crate) fn validate_action_binding(
    db: &Connection,
    project: &ProjectId,
    source_id: &str,
    action_id: &str,
    operation: &str,
    input_revision: i64,
    target: &Value,
) -> Result<(), RepositoryError> {
    let stored = crate::task_actions::load_action(db, project, source_id, action_id)?;
    let mut actual = target.clone();
    let fields = actual
        .as_object_mut()
        .ok_or(RepositoryError::TaskBindingInvalid)?;
    let source = fields.remove("requestedByTaskId");
    let action = fields.remove("requestedActionId");
    if operation == "modelKnowledge" {
        crate::modeling_action::normalize_execution(db, project, input_revision, &mut actual)?;
    }
    if source.as_ref().and_then(Value::as_str) != Some(source_id)
        || action.as_ref().and_then(Value::as_str) != Some(action_id)
        || stored.operation != operation
        || stored.input_revision != input_revision
        || actual != stored.target
        || !crate::task_action_permit::granted(db, project, &stored)?
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    crate::execution_scope_storage::assert_current(db, project, &stored)?;
    Ok(())
}
