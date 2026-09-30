use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};

use crate::{ProjectId, RepositoryError, SqliteStoryRepository};

impl SqliteStoryRepository {
    /// Freeze bounded recovery metadata in one database snapshot. Pagination never observes later writes.
    pub fn recovery_snapshot(
        &mut self,
        project: &ProjectId,
        task_ids: &[String],
        fence: Option<&str>,
        cursor: usize,
        limit: usize,
        now: i64,
    ) -> Result<Value, RepositoryError> {
        if !(1..=100).contains(&task_ids.len())
            || !(1..=500).contains(&limit)
            || task_ids.iter().any(|id| id.is_empty() || id.len() > 160)
        {
            return Err(invalid());
        }
        let mut ids = task_ids.to_vec();
        ids.sort();
        ids.dedup();
        let ids_json = serde_json::to_string(&ids).map_err(|_| RepositoryError::CorruptState)?;
        let tx = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let (fence, tasks) = if let Some(fence) = fence {
            if fence.len() > 160 {
                return Err(invalid());
            }
            let stored: Option<(String,String,String,usize,i64)> = tx.query_row(
                "SELECT project_id,task_ids_json,tasks_json,page_limit,expires_at FROM story_recovery_fences_v1 WHERE fence=?1", [fence],
                |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?))
            ).optional().map_err(storage_error)?;
            let Some((stored_project, stored_ids, tasks, stored_limit, expires)) = stored else {
                return Err(invalid());
            };
            if stored_project != project.as_str()
                || stored_ids != ids_json
                || stored_limit != limit
                || expires <= now
                || !cursor.is_multiple_of(limit)
            {
                return Err(invalid());
            }
            (
                fence.to_owned(),
                serde_json::from_str::<Value>(&tasks).map_err(|_| RepositoryError::CorruptState)?,
            )
        } else {
            if cursor != 0 {
                return Err(invalid());
            }
            tx.execute(
                "DELETE FROM story_recovery_fences_v1 WHERE expires_at<=?1",
                [now],
            )
            .map_err(storage_error)?;
            let count: i64 = tx
                .query_row(
                    "SELECT COUNT(*) FROM story_recovery_fences_v1 WHERE project_id=?1",
                    [project.as_str()],
                    |row| row.get(0),
                )
                .map_err(storage_error)?;
            if count >= 100 {
                return Err(RepositoryError::InvalidContent(
                    "recovery snapshot budget exhausted".into(),
                ));
            }
            let mut tasks = Vec::new();
            for id in &ids {
                match crate::tasks::load_task(&tx,id) {
                    Ok(task) if task.project_id==*project => tasks.push(json!({"id":task.task_id,"status":task.status,"stage":task.stage,"deliveryStatus":task.delivery_status})),
                    Ok(_) => return Err(RepositoryError::TaskBindingInvalid),
                    Err(RepositoryError::TaskNotFound) => {},
                    Err(error) => return Err(error),
                }
            }
            let fence = format!("recovery-{}", uuid::Uuid::new_v4().simple());
            let tasks = Value::Array(tasks);
            tx.execute(
                "INSERT INTO story_recovery_fences_v1 VALUES (?1,?2,?3,?4,?5,?6)",
                params![
                    fence,
                    project.as_str(),
                    ids_json,
                    tasks.to_string(),
                    limit,
                    now + 3600
                ],
            )
            .map_err(storage_error)?;
            let mut ordinal = 0usize;
            for id in &ids {
                let mut statement=tx.prepare("SELECT operation_id, task_id, thread_id, turn_id, call_id, tool_name, request_hash, idempotency_ref, state, receipt_ref, updated_at FROM story_tool_operations_v1 WHERE project_id=?1 AND task_id=?2 ORDER BY rowid").map_err(storage_error)?;
                let rows=statement.query_map(params![project.as_str(),id], |row| Ok(json!({
                    "operation_id":row.get::<_,String>(0)?,"run_id":row.get::<_,String>(1)?,"thread_id":row.get::<_,String>(2)?,"turn_id":row.get::<_,String>(3)?,"tool_call_id":row.get::<_,String>(4)?,"tool_name":row.get::<_,String>(5)?,"request_hash":row.get::<_,String>(6)?,"idempotency_ref":row.get::<_,Option<String>>(7)?,"state":row.get::<_,String>(8)?,"receipt_ref":row.get::<_,Option<String>>(9)?,"updated_at":row.get::<_,i64>(10)?
                }))).map_err(storage_error)?;
                for row in rows {
                    if ordinal >= 20_000 {
                        return Err(RepositoryError::InvalidContent(
                            "recovery snapshot operation budget exhausted".into(),
                        ));
                    }
                    tx.execute(
                        "INSERT INTO story_recovery_members_v1 VALUES (?1,?2,?3)",
                        params![fence, ordinal, row.map_err(storage_error)?.to_string()],
                    )
                    .map_err(storage_error)?;
                    ordinal += 1;
                }
            }
            (fence, tasks)
        };
        let count: usize = tx
            .query_row(
                "SELECT COUNT(*) FROM story_recovery_members_v1 WHERE fence=?1",
                [&fence],
                |row| row.get(0),
            )
            .map_err(storage_error)?;
        if cursor > count {
            return Err(invalid());
        }
        let mut statement=tx.prepare("SELECT operation_json FROM story_recovery_members_v1 WHERE fence=?1 AND ordinal>=?2 ORDER BY ordinal LIMIT ?3").map_err(storage_error)?;
        let operations = statement
            .query_map(params![fence, cursor, limit], |row| row.get::<_, String>(0))
            .map_err(storage_error)?
            .map(|row| {
                serde_json::from_str::<Value>(&row.map_err(storage_error)?)
                    .map_err(|_| RepositoryError::CorruptState)
            })
            .collect::<Result<Vec<_>, _>>()?;
        drop(statement);
        let next = cursor + operations.len();
        let result = json!({"fence":fence,"tasks":tasks,"operations":operations,"cursor":cursor,"nextCursor":if next<count {Some(next)} else {None},"complete":next>=count});
        tx.commit().map_err(storage_error)?;
        Ok(result)
    }
}

fn invalid() -> RepositoryError {
    RepositoryError::InvalidContent("invalid or expired recovery fence, scope or cursor".into())
}
fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}
