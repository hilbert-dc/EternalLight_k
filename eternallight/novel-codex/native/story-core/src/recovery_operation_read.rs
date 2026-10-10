use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};

use crate::{RecoveryOperationBinding, RepositoryError, SqliteStoryRepository};

impl SqliteStoryRepository {
    /// Exact, read-only receipt lookup. A terminal task may read its own receipt;
    /// this deliberately grants no authority to run another operation.
    pub fn read_recovery_operation(
        &self,
        binding: &RecoveryOperationBinding,
        capability_hash: Option<&str>,
    ) -> Result<Option<Value>, RepositoryError> {
        read_operation(&self.connection, binding, capability_hash)
    }
}

pub(crate) fn read_operation(
    connection: &rusqlite::Connection,
    binding: &RecoveryOperationBinding,
    capability_hash: Option<&str>,
) -> Result<Option<Value>, RepositoryError> {
        for value in [&binding.task_id, &binding.thread_id, &binding.turn_id,
            &binding.call_id, &binding.tool_name] {
            if value.is_empty() || value.len() > 160 || value.chars().any(char::is_control) {
                return Err(RepositoryError::TaskBindingInvalid);
            }
        }
        if binding.request_hash.len() != 64 || !binding.request_hash.bytes()
            .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v)) {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        let task = crate::tasks::load_task(connection, &binding.task_id)?;
        if task.project_id != binding.project_id || task.thread_id != binding.thread_id {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        // Completion revokes write authority. Until its original expiry, the same
        // credential may only query metadata belonging to this exact root call.
        if capability_hash.is_some_and(|hash|
            hash != task.capability_hash || task.capability_expires_at <= binding.now) {
            return Err(RepositoryError::TaskCapabilityInvalid);
        }
        connection.query_row(
            "SELECT operation_id,task_id,thread_id,turn_id,call_id,tool_name,request_hash,idempotency_ref,state,receipt_ref,updated_at FROM story_tool_operations_v1 WHERE project_id=?1 AND task_id=?2 AND thread_id=?3 AND turn_id=?4 AND call_id=?5 AND tool_name=?6 AND request_hash=?7",
            params![binding.project_id.as_str(),binding.task_id,binding.thread_id,binding.turn_id,binding.call_id,binding.tool_name,binding.request_hash],
            |row| Ok(json!({
                "operation_id":row.get::<_,String>(0)?,"run_id":row.get::<_,String>(1)?,
                "thread_id":row.get::<_,String>(2)?,"turn_id":row.get::<_,String>(3)?,
                "tool_call_id":row.get::<_,String>(4)?,"tool_name":row.get::<_,String>(5)?,
                "request_hash":row.get::<_,String>(6)?,"idempotency_ref":row.get::<_,Option<String>>(7)?,
                "state":row.get::<_,String>(8)?,"receipt_ref":row.get::<_,Option<String>>(9)?,
                "updated_at":row.get::<_,i64>(10)?
            })),
        ).optional().map_err(|e| RepositoryError::Storage(e.to_string()))
}
