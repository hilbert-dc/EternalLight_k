use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

use crate::authoring_runs::storage_error;
use crate::execution_scope::ExecutionScopePreview;
use crate::task_action_permit::PermissionBasis;
use crate::{ProjectId, RepositoryError, TaskActionRequest};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ScopeRecord {
    pub(crate) id: String,
    pub(crate) scope: ExecutionScopePreview,
    pub(crate) status: String,
    #[serde(skip)]
    pub(crate) superseded_by_author_message_id: Option<String>,
    confirmed_at: i64,
}

type StoredScope = (String, String, String, i64, String, Option<String>, String);
type StoredSource = (String, String, Option<String>, Option<String>);

pub(crate) fn load(
    db: &Connection,
    project: &ProjectId,
    source: &str,
    id: &str,
) -> Result<Option<ScopeRecord>, RepositoryError> {
    let row: Option<StoredScope> = db.query_row(
        "SELECT e.scope_json,e.preview_digest,e.status,e.confirmed_at,e.work_task_id,e.superseded_by_author_message_id,e.author_message_id FROM story_execution_scopes_v1 e JOIN story_work_tasks_v1 w ON w.id=e.work_task_id WHERE e.permit_id=?1 AND e.source_task_id=?2 AND w.project_id=?3 AND e.kind='advisory' AND e.generation_run_id IS NULL",
        params![id,source,project.as_str()],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?,row.get(6)?))).optional().map_err(storage_error)?;
    row.map(
        |(
            payload,
            digest,
            status,
            confirmed_at,
            work,
            superseded_by_author_message_id,
            message,
        )| {
            let scope: ExecutionScopePreview =
                serde_json::from_str(&payload).map_err(|_| RepositoryError::CorruptState)?;
            if scope.digest()? != digest
                || scope.preview_digest != digest
                || id != format!("scope-{digest}")
                || scope.source_task_id != source
                || scope.project_id != project.as_str()
                || scope.work_task_id != work
                || scope.author_message_id != message
                || !matches!(
                    (status.as_str(), superseded_by_author_message_id.as_ref()),
                    ("active", None) | ("superseded", Some(_))
                )
            {
                return Err(RepositoryError::CorruptState);
            }
            Ok(ScopeRecord {
                id: id.into(),
                scope,
                status,
                confirmed_at,
                superseded_by_author_message_id,
            })
        },
    )
    .transpose()
}

pub(crate) fn admit(
    db: &Connection,
    project: &ProjectId,
    parent: &str,
    action: &TaskActionRequest,
) -> Result<(), RepositoryError> {
    let id: Option<String> = db.query_row("SELECT permit_id FROM story_execution_scopes_v1 WHERE work_task_id=?1 AND status='active' AND kind='advisory'",
        [parent],|row|row.get(0)).optional().map_err(storage_error)?;
    let Some(id) = id else {
        return Ok(());
    };
    let scope = by_id(db, project, &id)?;
    if !scope.scope.covers(action) {
        return Ok(());
    }
    if action.source_task_id != scope.scope.source_task_id {
        return Ok(());
    }
    let count: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM story_action_permits_v1 WHERE scope_permit_id=?1",
            [&id],
            |row| row.get(0),
        )
        .map_err(storage_error)?;
    if count >= i64::from(scope.scope.request.max_actions) {
        return Ok(());
    }
    let now: i64 = db
        .query_row("SELECT unixepoch()", [], |row| row.get(0))
        .map_err(storage_error)?;
    let state: String = db
        .query_row(
            "SELECT status FROM story_work_tasks_v1 WHERE id=?1",
            [parent],
            |row| row.get(0),
        )
        .map_err(storage_error)?;
    if !matches!(state.as_str(), "running" | "completed")
        || scope
            .scope
            .deadline_at
            .is_some_and(|deadline| deadline <= now)
        || crate::work_budget_storage::load(db, project, parent)?.exhausted()
    {
        return Err(RepositoryError::TaskNotWritable);
    }
    validate(db, project, action, &id)?;
    crate::task_action_queue::queue(
        db,
        project,
        parent,
        action,
        PermissionBasis::FiniteScope(&id),
        now,
    )
}

pub(crate) fn validate(
    db: &Connection,
    project: &ProjectId,
    action: &TaskActionRequest,
    id: &str,
) -> Result<(), RepositoryError> {
    let scope = by_id(db, project, id)?;
    if action.source_task_id != scope.scope.source_task_id
        || action.intent_revision != scope.scope.request.expected_intent_revision
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    if !scope.scope.covers(action)
        || crate::author_message::identity(db, &action.source_task_id)?.as_deref()
            != Some(scope.scope.author_message_id.as_str())
        || crate::task_author_intent::source_intent(
            db,
            &action.source_task_id,
            action.intent_revision,
        )? != scope.scope.author_intent
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    Ok(())
}

pub(crate) fn assert_current(
    db: &Connection,
    project: &ProjectId,
    action: &TaskActionRequest,
) -> Result<(), RepositoryError> {
    let id: Option<String> = db
        .query_row(
            "SELECT scope_permit_id FROM story_action_permits_v1 WHERE action_id=?1",
            [&action.id],
            |row| row.get(0),
        )
        .map_err(storage_error)?;
    if let Some(id) = id {
        let scope = by_id(db, project, &id)?;
        assert_live(db, &scope)?;
        crate::task_intents::assert_action_intent(
            db,
            &action.source_task_id,
            action.intent_revision,
        )?;
        validate(db, project, action, &id)?;
    }
    Ok(())
}

pub(crate) fn by_id(
    db: &Connection,
    project: &ProjectId,
    id: &str,
) -> Result<ScopeRecord, RepositoryError> {
    let source: String = db
        .query_row(
            "SELECT source_task_id FROM story_execution_scopes_v1 WHERE permit_id=?1 AND kind='advisory'",
            [id],
            |row| row.get(0),
        )
        .optional()
        .map_err(storage_error)?
        .ok_or(RepositoryError::TaskBindingInvalid)?;
    load(db, project, &source, id)?.ok_or(RepositoryError::TaskBindingInvalid)
}

pub(crate) fn assert_live(db: &Connection, scope: &ScopeRecord) -> Result<(), RepositoryError> {
    let now: i64 = db
        .query_row("SELECT unixepoch()", [], |row| row.get(0))
        .map_err(storage_error)?;
    if scope.status != "active"
        || scope
            .scope
            .deadline_at
            .is_some_and(|deadline| deadline <= now)
    {
        return Err(RepositoryError::TaskInputStale);
    }
    crate::task_intents::assert_action_intent(
        db,
        &scope.scope.source_task_id,
        scope.scope.request.expected_intent_revision,
    )
}

pub(crate) fn assert_attempt_capacity(
    db: &Connection,
    project: &ProjectId,
    work: &str,
) -> Result<(), RepositoryError> {
    let entry: Option<StoredSource> = db.query_row("SELECT permit_id,kind,source_task_id,generation_run_id FROM story_execution_scopes_v1 WHERE work_task_id=?1 ORDER BY status='active' DESC,rowid DESC LIMIT 1",[work],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).optional().map_err(storage_error)?;
    if let Some((id, kind, source, run)) = entry {
        let maximum = match kind.as_str() {
            "advisory" => {
                let source = source.ok_or(RepositoryError::CorruptState)?;
                let scope =
                    load(db, project, &source, &id)?.ok_or(RepositoryError::CorruptState)?;
                Some(i64::from(scope.scope.request.max_task_attempts))
            }
            "generation" => {
                let run = run.ok_or(RepositoryError::CorruptState)?;
                let state: String = db.query_row("SELECT state_json FROM story_generation_runs_v1 WHERE run_id=?1 AND project_id=?2",params![run,project.as_str()],|row|row.get(0)).map_err(storage_error)?;
                crate::generation_execution_scope::validate(
                    db,
                    project,
                    &run,
                    &serde_json::from_str(&state).map_err(|_| RepositoryError::CorruptState)?,
                )?
            }
            _ => return Err(RepositoryError::CorruptState),
        };
        let count: i64 = db.query_row("SELECT COUNT(*) FROM story_work_attempts_v1 a JOIN story_work_steps_v1 s ON s.id=a.step_id WHERE s.work_task_id=?1",[work],|row|row.get(0)).map_err(storage_error)?;
        if maximum.is_some_and(|maximum| count >= maximum) {
            return Err(RepositoryError::InvalidContent(
                "author-confirmed work attempt limit reached".into(),
            ));
        }
    }
    Ok(())
}
