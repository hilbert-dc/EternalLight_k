use crate::{ProjectId, RepositoryError, SqliteStoryRepository, WorkTask};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::ops::Deref;

pub(crate) const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS story_work_control_commands_v1 (
  author_identity TEXT NOT NULL CHECK(author_identity='local-author'),
  command_id TEXT NOT NULL,
  project_id TEXT NOT NULL REFERENCES story_projects_v2(project_id),
  work_task_id TEXT NOT NULL REFERENCES story_work_tasks_v1(id),
  request_json TEXT NOT NULL,
  receipt_json TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  PRIMARY KEY(author_identity,command_id)
);
"#;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkControlCommand {
    pub command_id: String,
    pub thread_id: String,
    pub expected_control_revision: i64,
    pub command: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct WorkControlReceipt {
    pub command_id: String,
    pub project_id: String,
    pub work_task_id: String,
    pub thread_id: String,
    pub command: String,
    pub expected_control_revision: i64,
    pub status: String,
    pub applied_control_revision: Option<i64>,
    pub work_version: i64,
    pub reason: Option<String>,
    pub created_at: i64,
}

#[derive(Clone, Debug, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct WorkControlOutcome {
    pub receipt: WorkControlReceipt,
    pub replayed: bool,
    pub work: WorkTask,
}

fn storage(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}

/// A domain control owns a transaction when called directly, and a savepoint
/// inside the command transaction. Releasing it cannot commit the outer command.
pub(crate) struct ControlTransaction<'a> {
    db: &'a Connection,
    nested: bool,
    finished: bool,
}
impl<'a> ControlTransaction<'a> {
    pub(crate) fn new(db: &'a Connection) -> Result<Self, RepositoryError> {
        let nested = !db.is_autocommit();
        db.execute_batch(if nested {
            "SAVEPOINT work_control_step"
        } else {
            "BEGIN IMMEDIATE"
        })
        .map_err(storage)?;
        Ok(Self {
            db,
            nested,
            finished: false,
        })
    }
    pub(crate) fn commit(mut self) -> Result<(), RepositoryError> {
        self.db
            .execute_batch(if self.nested {
                "RELEASE work_control_step"
            } else {
                "COMMIT"
            })
            .map_err(storage)?;
        self.finished = true;
        Ok(())
    }
}
impl Deref for ControlTransaction<'_> {
    type Target = Connection;
    fn deref(&self) -> &Connection {
        self.db
    }
}
impl Drop for ControlTransaction<'_> {
    fn drop(&mut self) {
        if !self.finished {
            let _ = self.db.execute_batch(if self.nested {
                "ROLLBACK TO work_control_step; RELEASE work_control_step"
            } else {
                "ROLLBACK"
            });
        }
    }
}

/// Public work control commands capability; SQLite retains transaction ownership.
pub trait WorkControlCommandsRepository {
    fn work_control_receipt(
        &self,
        project: &ProjectId,
        work_id: &str,
        command_id: &str,
    ) -> Result<Option<WorkControlReceipt>, RepositoryError>;
    /// Runtime's authenticated author role is the local database's single author
    /// principal. Neither a model nor the request body supplies that identity.
    fn apply_work_control_command(
        &mut self,
        project: &ProjectId,
        work_id: &str,
        request: &WorkControlCommand,
        now: i64,
        apply: impl FnOnce(&mut Self) -> Result<(), RepositoryError>,
    ) -> Result<WorkControlOutcome, RepositoryError>;
}

impl WorkControlCommandsRepository for SqliteStoryRepository {
    fn work_control_receipt(
        &self,
        project: &ProjectId,
        work_id: &str,
        command_id: &str,
    ) -> Result<Option<WorkControlReceipt>, RepositoryError> {
        let row: Option<(String, String, String)> = self.connection.query_row(
            "SELECT project_id,work_task_id,receipt_json FROM story_work_control_commands_v1 WHERE author_identity='local-author' AND command_id=?1",
            [command_id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?)),
        ).optional().map_err(storage)?;
        row.map(|(owner, work, json)| {
            if owner != project.as_str() || work != work_id {
                return Err(RepositoryError::TaskBindingInvalid);
            }
            serde_json::from_str(&json).map_err(|_| RepositoryError::CorruptState)
        })
        .transpose()
    }

    /// Runtime's authenticated author role is the local database's single author
    /// principal. Neither a model nor the request body supplies that identity.
    fn apply_work_control_command(
        &mut self,
        project: &ProjectId,
        work_id: &str,
        request: &WorkControlCommand,
        now: i64,
        apply: impl FnOnce(&mut Self) -> Result<(), RepositoryError>,
    ) -> Result<WorkControlOutcome, RepositoryError> {
        if uuid::Uuid::parse_str(&request.command_id).is_err()
            || request.expected_control_revision < 0
            || request.thread_id.len() > 160
            || request.thread_id.chars().any(char::is_control)
            || !matches!(
                request.command.as_str(),
                "pause" | "pauseAfterStep" | "resume" | "cancel" | "recoverSavedResult"
            )
        {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        if !self.connection.is_autocommit() {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        self.connection
            .execute_batch("BEGIN IMMEDIATE")
            .map_err(storage)?;
        // This closure accepts &mut Repository, so a Connection-borrowing RAII
        // transaction cannot surround it. Roll back both errors and unwinding.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let encoded =
                serde_json::to_string(request).map_err(|_| RepositoryError::CorruptState)?;
            if let Some(receipt) =
                self.work_control_receipt(project, work_id, &request.command_id)?
            {
                let prior: String = self.connection.query_row(
                    "SELECT request_json FROM story_work_control_commands_v1 WHERE author_identity='local-author' AND command_id=?1",
                    [&request.command_id], |r| r.get(0),
                ).map_err(storage)?;
                if prior != encoded {
                    return Err(RepositoryError::TaskBindingInvalid);
                }
                let work =
                    crate::work_tasks::load_work_task(self, &self.connection, project, work_id)?;
                return Ok(WorkControlOutcome {
                    receipt,
                    replayed: true,
                    work,
                });
            }
            let before =
                crate::work_tasks::load_work_task(self, &self.connection, project, work_id)?;
            self.connection
                .execute_batch("SAVEPOINT work_command_effect")
                .map_err(storage)?;
            let effect = if before.thread_id != request.thread_id {
                Err(RepositoryError::TaskBindingInvalid)
            } else if before.control_revision != request.expected_control_revision {
                Err(RepositoryError::WorkTaskStale)
            } else {
                apply(self)
            };
            let reason = match effect {
                Ok(()) => {
                    self.connection
                        .execute_batch("RELEASE work_command_effect")
                        .map_err(storage)?;
                    None
                }
                Err(error) => {
                    self.connection
                        .execute_batch(
                            "ROLLBACK TO work_command_effect; RELEASE work_command_effect",
                        )
                        .map_err(storage)?;
                    let reason = match error {
                        RepositoryError::TaskBindingInvalid => "TaskBindingInvalid",
                        RepositoryError::WorkTaskStale => "WorkTaskStale",
                        RepositoryError::TaskNotWritable => "TaskNotWritable",
                        RepositoryError::TaskInputStale => "TaskInputStale",
                        RepositoryError::WorkUsageUnknown => "WorkUsageUnknown",
                        RepositoryError::WorkBudgetExhausted => "WorkBudgetExhausted",
                        RepositoryError::AuthoringRunStale => "AuthoringRunStale",
                        other => return Err(other),
                    };
                    Some(reason.to_owned())
                }
            };
            let work = crate::work_tasks::load_work_task(self, &self.connection, project, work_id)?;
            if reason.is_none() && work.control_revision != request.expected_control_revision + 1 {
                return Err(RepositoryError::CorruptState);
            }
            let receipt = WorkControlReceipt {
                command_id: request.command_id.clone(),
                project_id: project.to_string(),
                work_task_id: work_id.into(),
                thread_id: request.thread_id.clone(),
                command: request.command.clone(),
                expected_control_revision: request.expected_control_revision,
                status: if reason.is_none() {
                    "applied"
                } else {
                    "rejected"
                }
                .into(),
                applied_control_revision: reason.is_none().then_some(work.control_revision),
                work_version: work.version,
                reason,
                created_at: now,
            };
            self.connection.execute(
                "INSERT INTO story_work_control_commands_v1(author_identity,command_id,project_id,work_task_id,request_json,receipt_json,created_at) VALUES('local-author',?1,?2,?3,?4,?5,?6)",
                params![request.command_id,project.as_str(),work_id,encoded,serde_json::to_string(&receipt).map_err(|_|RepositoryError::CorruptState)?,now],
            ).map_err(storage)?;
            Ok(WorkControlOutcome {
                receipt,
                replayed: false,
                work,
            })
        }));
        match result {
            Ok(Ok(value)) => {
                if let Err(error) = self.connection.execute_batch("COMMIT") {
                    let _ = self.connection.execute_batch("ROLLBACK");
                    return Err(storage(error));
                }
                Ok(value)
            }
            Ok(Err(error)) => {
                self.connection.execute_batch("ROLLBACK").map_err(storage)?;
                Err(error)
            }
            Err(panic) => {
                let _ = self.connection.execute_batch("ROLLBACK");
                std::panic::resume_unwind(panic)
            }
        }
    }
}
