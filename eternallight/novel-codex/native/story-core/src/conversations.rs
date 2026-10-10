//! Conversation ownership is independent of WorkTasks and content revision authority.
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

use crate::{ProjectId, RepositoryError, SqliteStoryRepository};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationRecord {
    pub id: String,
    pub project_id: String,
    pub scope_id: Option<String>,
    pub title: String,
    pub is_pinned: bool,
    pub is_archived: bool,
    pub created_at: i64,
    pub updated_at: i64,
    pub forked_from_id: Option<String>,
    /// The source turn selected when branching, not an inferred latest engine turn.
    pub last_turn_id: Option<String>,
    pub version: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateConversationRequest {
    pub scope_id: Option<String>,
    pub thread_id: String,
    pub title: String,
    pub forked_from_id: Option<String>,
    pub last_turn_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateConversationRequest {
    pub expected_version: i64,
    pub title: Option<String>,
    pub is_pinned: Option<bool>,
    pub is_archived: Option<bool>,
}

fn invalid(message: &str) -> RepositoryError {
    RepositoryError::InvalidContent(message.into())
}
fn storage(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}
fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 160
        && value.trim() == value
        && !value.chars().any(char::is_control)
}
fn valid_title(value: &str) -> bool {
    !value.trim().is_empty() && value.chars().count() <= 120 && !value.chars().any(char::is_control)
}
fn require_project(db: &Connection, project: &ProjectId) -> Result<(), RepositoryError> {
    if !db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM story_projects_v2 WHERE project_id=?1)",
            [project.as_str()],
            |row| row.get::<_, bool>(0),
        )
        .map_err(storage)?
    {
        return Err(RepositoryError::ProjectNotFound);
    }
    Ok(())
}
fn load(
    db: &Connection,
    project: &ProjectId,
    id: &str,
) -> Result<ConversationRecord, RepositoryError> {
    if !valid_id(id) {
        return Err(invalid("Conversation thread ID is invalid"));
    }
    db.query_row("SELECT thread_id,project_id,title,is_pinned,is_archived,created_at,updated_at,forked_from_id,last_turn_id,version,scope_id FROM story_conversations_v1 WHERE project_id=?1 AND thread_id=?2",
        params![project.as_str(), id], record_from_row).optional().map_err(storage)?.ok_or(RepositoryError::ConversationNotFound)
}
fn record_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ConversationRecord> {
    Ok(ConversationRecord {
        id: row.get(0)?,
        project_id: row.get(1)?,
        title: row.get(2)?,
        is_pinned: row.get(3)?,
        is_archived: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
        forked_from_id: row.get(7)?,
        last_turn_id: row.get(8)?,
        version: row.get(9)?,
        scope_id: row.get(10)?,
    })
}

impl SqliteStoryRepository {
    pub fn conversations(
        &self,
        project: &ProjectId,
        archived: bool,
    ) -> Result<Vec<ConversationRecord>, RepositoryError> {
        require_project(&self.connection, project)?;
        let mut statement = self.connection.prepare("SELECT thread_id,project_id,title,is_pinned,is_archived,created_at,updated_at,forked_from_id,last_turn_id,version,scope_id FROM story_conversations_v1 WHERE project_id=?1 AND is_archived=?2 ORDER BY is_pinned DESC,updated_at DESC,thread_id").map_err(storage)?;
        statement
            .query_map(params![project.as_str(), archived], record_from_row)
            .map_err(storage)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage)
    }

    pub fn scoped_conversations(&self, project: &ProjectId, archived: bool, scope_id: &str) -> Result<Vec<ConversationRecord>, RepositoryError> {
        let valid: bool = self.connection.query_row("SELECT EXISTS(SELECT 1 FROM story_creative_scopes_v1 WHERE project_id=?1 AND scope_id=?2)",params![project.as_str(),scope_id],|row|row.get(0)).map_err(storage)?;
        if !valid { return Err(RepositoryError::CrossProjectReference); }
        Ok(self.conversations(project,archived)?.into_iter().filter(|c|c.scope_id.as_deref()==Some(scope_id)).collect())
    }

    pub fn conversation(
        &self,
        project: &ProjectId,
        thread_id: &str,
    ) -> Result<ConversationRecord, RepositoryError> {
        require_project(&self.connection, project)?;
        load(&self.connection, project, thread_id)
    }

    pub fn create_conversation(
        &mut self,
        project: &ProjectId,
        request: CreateConversationRequest,
        now: i64,
    ) -> Result<ConversationRecord, RepositoryError> {
        if !valid_id(&request.thread_id)
            || !valid_title(&request.title)
            || request
                .forked_from_id
                .as_deref()
                .is_some_and(|id| !valid_id(id))
            || request
                .last_turn_id
                .as_deref()
                .is_some_and(|id| !valid_id(id))
            || request.last_turn_id.is_some() != request.forked_from_id.is_some()
            || request.forked_from_id.as_deref() == Some(request.thread_id.as_str())
            || now < 0
        {
            return Err(invalid("Conversation registration is invalid"));
        }
        let registration =
            serde_json::to_string(&request).map_err(|_| RepositoryError::CorruptState)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        require_project(&transaction, project)?;
        if let Some(scope)=request.scope_id.as_deref(){
            let valid:bool=transaction.query_row("SELECT EXISTS(SELECT 1 FROM story_creative_scopes_v1 WHERE project_id=?1 AND scope_id=?2)",params![project.as_str(),scope],|row|row.get(0)).map_err(storage)?;
            if !valid{return Err(RepositoryError::CrossProjectReference);}
        }
        let existing: Option<(String, String)> = transaction.query_row(
            "SELECT project_id,registration_json FROM story_conversations_v1 WHERE thread_id=?1",
            [&request.thread_id], |row| Ok((row.get(0)?, row.get(1)?))).optional().map_err(storage)?;
        if let Some((owner, original_registration)) = existing {
            if owner != project.as_str() {
                return Err(RepositoryError::CrossProjectReference);
            }
            if original_registration != registration {
                return Err(RepositoryError::IdempotencyConflict);
            }
            let record = load(&transaction, project, &request.thread_id)?;
            transaction.commit().map_err(storage)?;
            return Ok(record);
        }
        if let Some(source) = &request.forked_from_id {
            let source=load(&transaction, project, source)?;
            if source.scope_id!=request.scope_id{return Err(RepositoryError::CrossProjectReference);}
        }
        transaction.execute("INSERT INTO story_conversations_v1(thread_id,project_id,title,created_at,updated_at,forked_from_id,last_turn_id,registration_json,scope_id) VALUES(?1,?2,?3,?4,?4,?5,?6,?7,?8)",
            params![request.thread_id, project.as_str(), request.title, now, request.forked_from_id, request.last_turn_id, registration,request.scope_id]).map_err(storage)?;
        let record = load(&transaction, project, &request.thread_id)?;
        transaction.commit().map_err(storage)?;
        Ok(record)
    }

    pub fn update_conversation(
        &mut self,
        project: &ProjectId,
        thread_id: &str,
        request: UpdateConversationRequest,
        now: i64,
    ) -> Result<ConversationRecord, RepositoryError> {
        if !valid_id(thread_id)
            || request.expected_version < 0
            || request.expected_version == i64::MAX
            || request
                .title
                .as_deref()
                .is_some_and(|title| !valid_title(title))
            || now < 0
            || request.title.is_none()
                && request.is_pinned.is_none()
                && request.is_archived.is_none()
        {
            return Err(invalid("Conversation metadata update is invalid"));
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        require_project(&transaction, project)?;
        let current = load(&transaction, project, thread_id)?;
        if current.version != request.expected_version {
            return Err(RepositoryError::ConversationStale {
                expected: request.expected_version,
                actual: current.version,
            });
        }
        if request.is_archived == Some(true) {
            let active:bool = transaction.query_row("SELECT EXISTS(SELECT 1 FROM story_work_tasks_v1 WHERE project_id=?1 AND thread_id=?2 AND status='running') OR EXISTS(SELECT 1 FROM story_conversation_messages_v1 WHERE project_id=?1 AND thread_id=?2 AND status IN ('queued','claimed','sending'))",params![project.as_str(),thread_id],|row|row.get(0)).map_err(storage)?;
            if active { return Err(invalid("Stop active work and resolve queued messages before archiving")); }
        }
        transaction.execute("UPDATE story_conversations_v1 SET title=?3,is_pinned=?4,is_archived=?5,updated_at=?6,version=version+1 WHERE project_id=?1 AND thread_id=?2 AND version=?7",
            params![project.as_str(), thread_id, request.title.unwrap_or(current.title),
                request.is_pinned.unwrap_or(current.is_pinned), request.is_archived.unwrap_or(current.is_archived),
                now.max(current.updated_at), request.expected_version]).map_err(storage)?;
        let record = load(&transaction, project, thread_id)?;
        transaction.commit().map_err(storage)?;
        Ok(record)
    }
}
