use super::ConversationStore;
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

use crate::{ProjectId, RepositoryError};

impl ConversationStore<'_> {
    /// Reads persisted author intent for specific engine turns, not composed model prompts.
    pub fn thread_author_messages(
        &self,
        project: &ProjectId,
        thread_id: &str,
        turn_ids: &[String],
    ) -> Result<Value, RepositoryError> {
        if thread_id.is_empty()
            || thread_id.len() > 160
            || thread_id.trim() != thread_id
            || thread_id.chars().any(char::is_control)
            || turn_ids.len() > 12
            || turn_ids.iter().any(|id| {
                id.is_empty()
                    || id.len() > 160
                    || id.trim() != id
                    || id.chars().any(char::is_control)
            })
        {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        let read = self
            .connection
            .unchecked_transaction()
            .map_err(|error| RepositoryError::Storage(error.to_string()))?;
        let mut messages = Vec::new();
        let author_tasks = inherited_author_tasks(&read, project, thread_id, turn_ids)?;
        for turn_id in turn_ids {
            let Some((_, task_id)) = author_tasks.get(turn_id) else {
                continue;
            };
            let task = crate::tasks::load_task(&read, &task_id)?;
            // A task's inherited execution intent is not a new author message.
            // Only the persisted messages attached to this exact task may be shown.
            let mut statement = read
                .prepare(
                    "SELECT m.author_text FROM story_author_messages_v1 m
                 WHERE m.project_id=?1 AND m.thread_id=?2 AND m.task_id=?3
                   AND m.generation_run_id IS NULL AND m.review_run_id IS NULL
                   AND (m.intent_revision=0 OR EXISTS (
                     SELECT 1 FROM story_task_intents_v1 i WHERE i.task_id=m.task_id
                     AND i.author_message_id=m.message_id AND i.status='delivered'))
                 ORDER BY m.intent_revision,m.rowid",
                )
                .map_err(|error| RepositoryError::Storage(error.to_string()))?;
            let texts = statement
                .query_map(params![project.as_str(), task.thread_id, task_id], |row| {
                    row.get::<_, String>(0)
                })
                .map_err(|error| RepositoryError::Storage(error.to_string()))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| RepositoryError::Storage(error.to_string()))?;
            if texts.is_empty() {
                continue;
            }
            crate::author_message::validate_execution(&read, &task)?;
            let text = texts
                .iter()
                .map(|text| crate::task_intents::display_author_text(text, &task_id))
                .collect::<Vec<_>>()
                .join("\n\n作者追加要求：\n");
            let mut visible: String = text.chars().take(8_000).collect();
            if visible.len() < text.len() {
                visible.push('…');
            }
            let text = visible;
            let target: Value = serde_json::from_str(&task.target_json)
                .map_err(|_| RepositoryError::CorruptState)?;
            let mut images = Vec::<String>::new();
            if let Some(ids) = target
                .get("imageAssetRevisionIds")
                .and_then(Value::as_array)
            {
                if ids.len() > 8 {
                    return Err(RepositoryError::CorruptState);
                }
                for id in ids {
                    let id = id.as_str().ok_or(RepositoryError::CorruptState)?;
                    let belongs:bool=read.query_row("SELECT EXISTS(SELECT 1 FROM story_media_assets_v1 a JOIN story_conversations_v1 c ON c.project_id=a.project_id AND c.scope_id=a.scope_id WHERE a.project_id=?1 AND a.asset_revision_id=?2 AND c.thread_id=?3 AND json_extract(a.metadata_json,'$.kind')='image')",params![project.as_str(),id,thread_id],|r|r.get(0)).map_err(|e|RepositoryError::Storage(e.to_string()))?;
                    if !belongs {
                        return Err(RepositoryError::CrossProjectReference);
                    }
                    images.push(id.to_owned());
                }
            }
            messages.push(json!({"turnId":turn_id,"text":text,"imageAssetRevisionIds":images}));
        }
        read.commit()
            .map_err(|error| RepositoryError::Storage(error.to_string()))?;
        Ok(json!({"threadId":thread_id,"messages":messages}))
    }
}

// Engine turns are bound once to a persisted task. Foreground task creation and
// turn execution are serialized; rowid therefore supplies an authoritative
// same-thread author-turn order. Unknown/unbound branch boundaries inherit no
// author messages, rather than substituting an engine-composed prompt.
fn inherited_author_tasks(
    db: &Connection,
    project: &ProjectId,
    thread_id: &str,
    requested: &[String],
) -> Result<BTreeMap<String, (i64, String)>, RepositoryError> {
    let mut ancestry = Vec::<(String, Option<String>)>::new();
    let mut seen = BTreeSet::new();
    let mut current = thread_id.to_owned();
    let mut relevant = requested.iter().cloned().collect::<BTreeSet<_>>();
    loop {
        if !seen.insert(current.clone()) {
            return Err(RepositoryError::CorruptState);
        }
        let source: Option<(Option<String>, Option<String>)> = db.query_row(
            "SELECT forked_from_id,last_turn_id FROM story_conversations_v1 WHERE project_id=?1 AND thread_id=?2",
            params![project.as_str(), current], |row| Ok((row.get(0)?, row.get(1)?)))
            .optional().map_err(|error| RepositoryError::Storage(error.to_string()))?;
        let (parent, boundary) = source.unwrap_or((None, None));
        if let Some(boundary) = &boundary {
            relevant.insert(boundary.clone());
        }
        ancestry.push((current, boundary));
        let Some(parent) = parent else { break };
        let belongs: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM story_conversations_v1 WHERE project_id=?1 AND thread_id=?2)",
            params![project.as_str(), parent], |row| row.get(0)).map_err(|error| RepositoryError::Storage(error.to_string()))?;
        if !belongs {
            return Err(RepositoryError::CrossProjectReference);
        }
        current = parent;
    }
    let relevant_json =
        serde_json::to_string(&relevant).map_err(|_| RepositoryError::CorruptState)?;
    let mut allowed = BTreeMap::<String, (i64, String)>::new();
    for (thread, boundary) in ancestry.into_iter().rev() {
        if let Some(boundary) = boundary {
            if let Some((ceiling, _)) = allowed.get(&boundary) {
                let ceiling = *ceiling;
                allowed.retain(|_, (ordinal, _)| *ordinal <= ceiling);
            } else {
                allowed.clear();
            }
        } else if !allowed.is_empty() {
            allowed.clear();
        }
        let mut statement = db.prepare("SELECT turn_id,rowid,task_id FROM story_tasks_v2 WHERE project_id=?1 AND thread_id=?2 AND turn_id IN (SELECT value FROM json_each(?3)) ORDER BY rowid")
            .map_err(|error| RepositoryError::Storage(error.to_string()))?;
        let rows = statement
            .query_map(params![project.as_str(), thread, relevant_json], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    (row.get::<_, i64>(1)?, row.get::<_, String>(2)?),
                ))
            })
            .map_err(|error| RepositoryError::Storage(error.to_string()))?;
        for row in rows {
            let (turn_id, task) =
                row.map_err(|error| RepositoryError::Storage(error.to_string()))?;
            allowed.insert(turn_id, task);
        }
    }
    Ok(allowed)
}
