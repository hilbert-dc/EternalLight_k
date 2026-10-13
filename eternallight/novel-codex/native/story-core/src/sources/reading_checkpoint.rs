use rusqlite::{OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::query_scope::TaskQueryScope;
use crate::{
    ProjectId, QueryOperation, QueryRanking, RepositoryError, SqliteStoryRepository,
    StoryQueryPlan, TaskRecord,
};

pub(crate) const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS story_reading_checkpoints_v1 (
 step_id TEXT PRIMARY KEY REFERENCES story_work_steps_v1(id) ON DELETE CASCADE,
 version INTEGER NOT NULL CHECK(version BETWEEN 1 AND 10000),
 state_json TEXT NOT NULL, state_hash TEXT NOT NULL
);
"#;

#[derive(Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CheckpointReadingRequest {
    pub plan: StoryQueryPlan,
    pub report_hash: String,
    pub expected_version: i64,
    pub summary: String,
}

#[derive(Deserialize, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Stored {
    version: i64,
    task_id: String,
    scope_hash: String,
    plan: StoryQueryPlan,
    next_cursor: Option<String>,
    ended: bool,
    summary: String,
    pages: i64,
    characters: usize,
    request_hash: String,
    report_hash: String,
    receipt: String,
}

/// Author-visible source traversal, not model understanding or permission to resume.
#[derive(Clone, Debug, PartialEq, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(
    tag = "status",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum AuthorReadingProgress {
    Current {
        version: i64,
        pages: i64,
        acknowledged_characters: usize,
        traversal: ReadingTraversal,
    },
    Stale,
    Unavailable,
}

#[derive(Clone, Debug, PartialEq, Serialize, ts_rs::TS, serde::Deserialize)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub enum ReadingTraversal {
    Reading,
    AcknowledgedEnd,
    BudgetExhausted,
}

impl SqliteStoryRepository {
    /// Called only within the existing WorkTask read/control transaction.
    pub(crate) fn author_reading_progress(
        &self,
        project: &ProjectId,
        step: &str,
    ) -> Result<Option<AuthorReadingProgress>, RepositoryError> {
        let result = (|| {
            let Some(saved) = load(&self.connection, step)? else {
                return Ok(None);
            };
            let bound: bool = self.connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM story_work_attempts_v1 a JOIN story_work_steps_v1 s ON s.id=a.step_id JOIN story_work_tasks_v1 w ON w.id=s.work_task_id WHERE a.attempt_id=?1 AND s.id=?2 AND w.project_id=?3)",
                params![saved.task_id,step,project.as_str()], |row| row.get(0),
            ).map_err(storage)?;
            if !bound {
                return Err(RepositoryError::CorruptState);
            }
            let task = self
                .task(project, &saved.task_id)?
                .ok_or(RepositoryError::CorruptState)?;
            self.assert_task_inputs_current(&task)?;
            let scope = TaskQueryScope::from_task(self, &task)?;
            if scope_digest(self, &task, &scope)? != saved.scope_hash {
                return Err(RepositoryError::TaskInputStale);
            }
            Ok(Some(AuthorReadingProgress::Current {
                version: saved.version,
                pages: saved.pages,
                acknowledged_characters: saved.characters,
                traversal: if saved.ended {
                    ReadingTraversal::AcknowledgedEnd
                } else if saved.version == 10000 {
                    ReadingTraversal::BudgetExhausted
                } else {
                    ReadingTraversal::Reading
                },
            }))
        })();
        match result {
            Err(RepositoryError::TaskInputStale) => Ok(Some(AuthorReadingProgress::Stale)),
            Err(
                RepositoryError::CorruptState
                | RepositoryError::TaskBindingInvalid
                | RepositoryError::TaskNotFound
                | RepositoryError::TaskOutputForbidden,
            ) => Ok(Some(AuthorReadingProgress::Unavailable)),
            result => result,
        }
    }

    

    

    fn reading_binding(
        &self,
        task: &TaskRecord,
        now: i64,
    ) -> Result<(TaskRecord, String, TaskQueryScope, String), RepositoryError> {
        if task.task_id.is_empty()
            || task.task_id.len() > 160
            || task.task_id.chars().any(char::is_control)
        {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        let task = self.authorize_task_capability(
            &task.project_id,
            &task.task_id,
            &task.thread_id,
            task.input_revision,
            &task.capability_hash,
            now,
        )?;
        self.assert_task_inputs_current(&task)?;
        let step: String = self.connection.query_row("SELECT s.id FROM story_work_attempts_v1 a
            JOIN story_work_steps_v1 s ON s.id=a.step_id JOIN story_work_tasks_v1 w ON w.id=s.work_task_id
            WHERE a.attempt_id=?1 AND s.current_attempt_id=?1 AND s.status='running' AND w.status='running' AND w.project_id=?2",
            params![task.task_id,task.project_id.as_str()],|row|row.get(0)).optional().map_err(storage)?.ok_or(RepositoryError::TaskNotWritable)?;
        let scope = TaskQueryScope::from_task(self, &task)?;
        let scope_hash = scope_digest(self, &task, &scope)?;
        Ok((task, step, scope, scope_hash))
    }
}

fn scope_digest(
    repo: &SqliteStoryRepository,
    task: &TaskRecord,
    scope: &TaskQueryScope,
) -> Result<String, RepositoryError> {
    Ok(digest(
        &json!({"policy":scope.policy(),"sources":scope.watermarks(repo,task)?}).to_string(),
    ))
}

fn passage_cursor(plan: &StoryQueryPlan) -> Result<Option<&str>, RepositoryError> {
    plan.validate()?;
    if plan.question.len() > 512
        || plan.steps.len() != 1
        || !matches!(plan.ranking, QueryRanking::Literal)
    {
        return Err(invalid());
    }
    match &plan.steps[0].operation {
        QueryOperation::Passages { cursor, .. } => Ok(cursor.as_deref()),
        _ => Err(invalid()),
    }
}

fn projection(
    repo: &SqliteStoryRepository,
    task: &TaskRecord,
    scope: &TaskQueryScope,
    saved: &Stored,
) -> Result<Value, RepositoryError> {
    let mut plan = saved.plan.clone();
    let QueryOperation::Passages { cursor, .. } = &mut plan.steps[0].operation else {
        return Err(RepositoryError::CorruptState);
    };
    *cursor = saved
        .next_cursor
        .as_deref()
        .map(|value| {
            crate::query_passages::rebind_cursor(
                repo,
                task,
                scope,
                &saved.plan.question,
                &saved.plan.steps[0].id,
                value,
            )
        })
        .transpose()?;
    let exhausted = saved.version == 10000 && !saved.ended;
    Ok(
        json!({"checkpoint":{"status":if saved.ended {"acknowledgedEnd"} else if exhausted {"budgetExhausted"} else {"reading"},"version":saved.version,
        "pages":saved.pages,"acknowledgedCharacters":saved.characters,"summary":saved.summary,"summaryAuthority":"modelCandidate",
        "acknowledgedTraversalComplete":saved.ended,"budgetExhausted":exhausted,"modelJudgmentVerified":false,"receiptRef":saved.receipt,
        "resumePlan":if saved.ended || exhausted {None} else {Some(plan)}},"semanticCompleteness":"unknown"}),
    )
}

fn load(db: &rusqlite::Connection, step: &str) -> Result<Option<Stored>, RepositoryError> {
    let row: Option<(i64,String,String)> = db.query_row("SELECT version,state_json,state_hash FROM story_reading_checkpoints_v1 WHERE step_id=?1",
        [step],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).optional().map_err(storage)?;
    row.map(|(version, encoded, hash)| {
        if encoded.len() > 12000 || digest(&encoded) != hash {
            return Err(RepositoryError::CorruptState);
        }
        let saved: Stored =
            serde_json::from_str(&encoded).map_err(|_| RepositoryError::CorruptState)?;
        passage_cursor(&saved.plan).map_err(|_| RepositoryError::CorruptState)?;
        if saved.version != version
            || saved.pages != version
            || !(1..=10000).contains(&version)
            || saved.summary.len() > 512
            || saved.ended != saved.next_cursor.is_none()
            || saved
                .next_cursor
                .as_ref()
                .is_some_and(|cursor| cursor.is_empty() || cursor.len() > 768)
            || saved.task_id.is_empty()
            || saved.task_id.len() > 160
            || [&saved.scope_hash, &saved.request_hash, &saved.report_hash]
                .into_iter()
                .any(|hash| hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()))
            || saved.receipt != format!("reading:{step}:{version}:{}", saved.request_hash)
        {
            return Err(RepositoryError::CorruptState);
        }
        Ok(saved)
    })
    .transpose()
}

fn digest(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
fn storage(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}
fn invalid() -> RepositoryError {
    RepositoryError::InvalidContent(
        "reading checkpoint requires an intact contiguous literal page and current version".into(),
    )
}

/// Public reading checkpoint capability; SQLite retains transaction ownership.
pub trait ReadingCheckpointRepository {
    /// Persist a contiguous, acknowledged source page, never a semantic truth certificate.
    fn checkpoint_reading(
        &mut self,
        task: &TaskRecord,
        request: &CheckpointReadingRequest,
        now: i64,
    ) -> Result<Value, RepositoryError>;
    /// Read one bounded step checkpoint; a new attempt gets a newly scoped cursor, not old credentials.
    fn reading_progress(&self, task: &TaskRecord, now: i64) -> Result<Value, RepositoryError>;
}

impl ReadingCheckpointRepository for SqliteStoryRepository {
    /// Persist a contiguous, acknowledged source page, never a semantic truth certificate.
    fn checkpoint_reading(
        &mut self,
        task: &TaskRecord,
        request: &CheckpointReadingRequest,
        now: i64,
    ) -> Result<Value, RepositoryError> {
        if task.operation == "assessChapterStructure" {
            return Err(RepositoryError::TaskOutputForbidden);
        }
        let cursor = passage_cursor(&request.plan)?;
        if request.summary.len() > 512
            || !(0..10000).contains(&request.expected_version)
            || request.report_hash.len() != 64
            || !request
                .report_hash
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(invalid());
        }
        let tx =
            rusqlite::Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
                .map_err(storage)?;
        let (task, step, scope, scope_hash) = self.reading_binding(task, now)?;
        let request_hash = digest(&json!({"taskId":task.task_id,"request":request}).to_string());
        let saved = load(&self.connection, &step)?;
        if let Some(saved) = &saved {
            if saved.scope_hash != scope_hash {
                return Err(RepositoryError::TaskInputStale);
            }
            if saved.version == request.expected_version + 1 && saved.request_hash == request_hash {
                let response = projection(self, &task, &scope, saved)?;
                crate::recovery_operations::commit_operation(
                    &tx,
                    self.recovery_operation.as_ref(),
                    &task.project_id,
                    &task.task_id,
                    &["checkpoint_reading"],
                    &saved.receipt,
                )?;
                tx.commit().map_err(storage)?;
                return Ok(response);
            }
            if saved.version != request.expected_version
                || saved.ended
                || saved.plan.question != request.plan.question
                || saved.plan.steps[0].id != request.plan.steps[0].id
            {
                return Err(invalid());
            }
            let expected = saved
                .next_cursor
                .as_deref()
                .map(|cursor| {
                    crate::query_passages::rebind_cursor(
                        self,
                        &task,
                        &scope,
                        &saved.plan.question,
                        &saved.plan.steps[0].id,
                        cursor,
                    )
                })
                .transpose()?;
            if cursor != expected.as_deref() {
                return Err(invalid());
            }
        } else if request.expected_version != 0 || cursor.is_some() {
            return Err(invalid());
        }
        let report = self.query_task_plan_in_snapshot(&task, &request.plan, now)?;
        let page = &report["steps"][0];
        if report["contentHash"].as_str() != Some(&request.report_hash)
            || page["coverage"]["pageComplete"] != true
            || report["coverage"]["responseTruncated"] != false
        {
            return Err(invalid());
        }
        let mut characters = saved.as_ref().map_or(0, |saved| saved.characters);
        for row in page["results"]
            .as_array()
            .ok_or(RepositoryError::CorruptState)?
        {
            let evidence = &report["evidence"][row["evidenceRef"]
                .as_str()
                .ok_or(RepositoryError::CorruptState)?];
            let start = evidence["start"]
                .as_u64()
                .ok_or(RepositoryError::CorruptState)? as usize;
            let end = evidence["end"]
                .as_u64()
                .ok_or(RepositoryError::CorruptState)? as usize;
            characters = characters
                .checked_add(
                    end.checked_sub(
                        start
                            + if start > 0 {
                                crate::query_passages::OVERLAP_CHARS
                            } else {
                                0
                            },
                    )
                    .ok_or(RepositoryError::CorruptState)?,
                )
                .ok_or(RepositoryError::CorruptState)?;
        }
        let version = request.expected_version + 1;
        let receipt = format!("reading:{step}:{version}:{request_hash}");
        if receipt.len() > 500 {
            return Err(invalid());
        }
        let state = Stored {
            version,
            task_id: task.task_id.clone(),
            scope_hash,
            plan: request.plan.clone(),
            next_cursor: page["nextCursor"].as_str().map(str::to_owned),
            ended: page["coverage"]["endOfScopeFromCursor"] == true,
            summary: request.summary.clone(),
            pages: version,
            characters,
            request_hash,
            report_hash: request.report_hash.clone(),
            receipt,
        };
        let encoded = serde_json::to_string(&state).map_err(|_| RepositoryError::CorruptState)?;
        if encoded.len() > 12000 {
            return Err(invalid());
        }
        self.connection.execute("INSERT INTO story_reading_checkpoints_v1 VALUES (?1,?2,?3,?4)
            ON CONFLICT(step_id) DO UPDATE SET version=excluded.version,state_json=excluded.state_json,state_hash=excluded.state_hash",
            params![step,version,encoded,digest(&encoded)]).map_err(storage)?;
        crate::recovery_operations::commit_operation(
            &tx,
            self.recovery_operation.as_ref(),
            &task.project_id,
            &task.task_id,
            &["checkpoint_reading"],
            &state.receipt,
        )?;
        let response = projection(self, &task, &scope, &state)?;
        tx.commit().map_err(storage)?;
        Ok(response)
    }

    /// Read one bounded step checkpoint; a new attempt gets a newly scoped cursor, not old credentials.
    fn reading_progress(&self, task: &TaskRecord, now: i64) -> Result<Value, RepositoryError> {
        self.with_read_snapshot(|| {
            let (task,step,scope,scope_hash) = self.reading_binding(task,now)?;
            match load(&self.connection,&step)? {
                None => Ok(json!({"checkpoint":null,"semanticCompleteness":"unknown"})),
                Some(saved) if saved.scope_hash != scope_hash => Ok(json!({"checkpoint":{"status":"stale",
                    "version":saved.version,"resumePlan":null,"summary":null,"acknowledgedTraversalComplete":false},"semanticCompleteness":"unknown"})),
                Some(saved) => projection(self,&task,&scope,&saved),
            }
        })?
    }
}

use crate::{ReadSnapshotRepository, TaskInputRepository, TasksRepository};
