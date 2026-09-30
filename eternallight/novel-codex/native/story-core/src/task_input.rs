use std::collections::{BTreeMap, HashSet};

use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::{ObjectId, ProjectId, RepositoryError, RevisionId, SqliteStoryRepository, TaskRecord};

type ObjectWatermark = (Option<String>, Option<String>, Option<String>, Option<i64>);

const CURRENT_HEADS_SQL: &str =
    "SELECT h.object_id,h.candidate_revision_id,h.accepted_revision_id,b.revision_id,b.selection_revision
     FROM story_object_heads_v2 h LEFT JOIN story_working_baselines_v2 b
       ON b.project_id=h.project_id AND b.object_kind=h.object_kind
       AND b.object_id=CASE WHEN h.object_kind='draft' THEN substr(h.object_id,7) ELSE h.object_kind END
     WHERE h.project_id=?1";

pub(crate) fn capture_task_inputs(
    connection: &rusqlite::Connection,
    task_id: &str,
    project_id: &ProjectId,
) -> Result<(), RepositoryError> {
    connection.execute(
        "INSERT INTO story_task_input_heads_v2(task_id,object_id,candidate_revision_id,accepted_revision_id,working_revision_id,working_selection_revision)
         SELECT ?1,h.object_id,h.candidate_revision_id,h.accepted_revision_id,b.revision_id,b.selection_revision
         FROM story_object_heads_v2 h LEFT JOIN story_working_baselines_v2 b
           ON b.project_id=h.project_id AND b.object_kind=h.object_kind
           AND b.object_id=CASE WHEN h.object_kind='draft' THEN substr(h.object_id,7) ELSE h.object_kind END
         WHERE h.project_id=?2",
        params![task_id, project_id.as_str()],
    ).map_err(storage_error)?;
    connection
        .execute(
            "INSERT INTO story_task_input_knowledge_v2(task_id, object_id, batch_id)
         SELECT ?1, object_id, batch_id FROM story_knowledge_heads_v2 WHERE project_id = ?2",
            params![task_id, project_id.as_str()],
        )
        .map_err(storage_error)?;
    Ok(())
}

pub(crate) fn expected_task_target(
    connection: &rusqlite::Connection,
    task: &TaskRecord,
) -> Result<Option<(ObjectId, Option<RevisionId>)>, RepositoryError> {
    let manifest: serde_json::Value = serde_json::from_str(&task.input_manifest_json)
        .map_err(|_| RepositoryError::CorruptState)?;
    let Some(object_id) = manifest["outputObjectId"].as_str() else {
        return Ok(None);
    };
    let object_id =
        ObjectId::parse(object_id.to_owned()).map_err(|_| RepositoryError::CorruptState)?;
    let previous_output = connection.query_row(
        "SELECT r.object_id, o.revision_id FROM story_task_outputs_v2 o JOIN story_revisions_v2 r ON r.revision_id = o.revision_id WHERE o.task_id = ?1 ORDER BY o.ordinal DESC LIMIT 1",
        [&task.task_id],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
    ).optional().map_err(storage_error)?;
    let revision = match previous_output {
        Some((previous_object_id, revision_id)) => {
            if previous_object_id != object_id.as_str() {
                return Err(RepositoryError::CorruptState);
            }
            Some(RevisionId::parse(revision_id).map_err(|_| RepositoryError::CorruptState)?)
        }
        None => manifest["expectedCandidateRevisionId"]
            .as_str()
            .map(|id| RevisionId::parse(id.to_owned()).map_err(|_| RepositoryError::CorruptState))
            .transpose()?,
    };
    Ok(Some((object_id, revision)))
}

pub(crate) fn assert_task_inputs_current(
    connection: &rusqlite::Connection,
    task: &TaskRecord,
) -> Result<(), RepositoryError> {
    crate::author_message::validate_execution(connection, task)?;
    crate::revision_repair::assert_discussion_current(connection, task)?;
    let target: Value =
        serde_json::from_str(&task.target_json).map_err(|_| RepositoryError::CorruptState)?;
    crate::scope_continuation::assert_current(connection, task, &target)?;
    if task.operation == "assessRevisionOutcome" {
        crate::revision_outcome::validate_target(connection, &task.project_id, &target)?;
        if !crate::revision_outcome::inputs_current(connection, &task.project_id, &target)? {
            return Err(RepositoryError::TaskInputStale);
        }
    }
    if let (Some(source), Some(action)) = (
        target["requestedByTaskId"].as_str(),
        target["requestedActionId"].as_str(),
    ) {
        let action =
            crate::task_actions::load_action(connection, &task.project_id, source, action)?;
        crate::execution_scope_storage::assert_current(connection, &task.project_id, &action)?;
    }
    let revision: i64 = connection
        .query_row(
            "SELECT revision FROM story_workspaces_v2 WHERE project_id = ?1",
            [task.project_id.as_str()],
            |row| row.get(0),
        )
        .map_err(storage_error)?;
    if revision != task.input_revision {
        return Err(RepositoryError::TaskInputStale);
    }
    let mut captured = object_heads(
        connection,
        "SELECT object_id,candidate_revision_id,accepted_revision_id,working_revision_id,working_selection_revision FROM story_task_input_heads_v2 WHERE task_id = ?1",
        &task.task_id,
    )?;
    let current = object_heads(connection, CURRENT_HEADS_SQL, task.project_id.as_str())?;
    if let Some((target_id, expected_candidate)) = expected_task_target(connection, task)? {
        let key = target_id.as_str();
        if let Some((candidate, _, _, _)) = captured.get_mut(key) {
            *candidate = expected_candidate.map(|value| value.to_string());
        } else if let Some(expected_candidate) = expected_candidate {
            captured.insert(
                key.to_owned(),
                (Some(expected_candidate.to_string()), None, None, None),
            );
        }
    }
    if captured != current {
        return Err(RepositoryError::TaskInputStale);
    }
    let mut captured_knowledge = knowledge_heads(
        connection,
        "SELECT object_id, batch_id FROM story_task_input_knowledge_v2 WHERE task_id = ?1",
        &task.task_id,
    )?;
    let current_knowledge = knowledge_heads(
        connection,
        "SELECT object_id, batch_id FROM story_knowledge_heads_v2 WHERE project_id = ?1",
        task.project_id.as_str(),
    )?;
    if task.output_refs_json != "[]"
        && let Some((target_id, _)) = expected_task_target(connection, task)?
        && !current_knowledge.contains_key(target_id.as_str())
    {
        captured_knowledge.remove(target_id.as_str());
    }
    if task.operation == "modelKnowledge" {
        let target: serde_json::Value =
            serde_json::from_str(&task.target_json).map_err(|_| RepositoryError::CorruptState)?;
        if let Some(batch_id) = target["batchId"].as_str() {
            let published_object: Option<String> = connection.query_row(
                "SELECT object_id FROM story_modeling_batches_v2 WHERE project_id = ?1 AND batch_id = ?2 AND publisher_task_id = ?3 AND status = 'ready'",
                params![task.project_id.as_str(), batch_id, task.task_id],
                |row| row.get(0),
            ).optional().map_err(storage_error)?;
            if let Some(object_id) = published_object
                && current_knowledge
                    .get(&object_id)
                    .is_some_and(|current| current == batch_id)
            {
                captured_knowledge.insert(object_id, batch_id.to_owned());
            }
        }
    }
    if captured_knowledge != current_knowledge {
        return Err(RepositoryError::TaskInputStale);
    }
    Ok(())
}

fn object_heads(
    connection: &rusqlite::Connection,
    sql: &str,
    key: &str,
) -> Result<BTreeMap<String, ObjectWatermark>, RepositoryError> {
    let mut statement = connection.prepare(sql).map_err(storage_error)?;
    statement
        .query_map([key], |row| {
            Ok((
                row.get::<_, String>(0)?,
                (
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<i64>>(4)?,
                ),
            ))
        })
        .map_err(storage_error)?
        .map(|row| row.map_err(storage_error))
        .collect()
}

fn knowledge_heads(
    connection: &rusqlite::Connection,
    sql: &str,
    key: &str,
) -> Result<BTreeMap<String, String>, RepositoryError> {
    let mut statement = connection.prepare(sql).map_err(storage_error)?;
    statement
        .query_map([key], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(storage_error)?
        .map(|row| row.map_err(storage_error))
        .collect()
}

impl SqliteStoryRepository {
    pub fn assert_task_inputs_current(&self, task: &TaskRecord) -> Result<(), RepositoryError> {
        assert_task_inputs_current(&self.connection, task)
    }

    pub fn project_source_watermarks(
        &self,
        project_id: &ProjectId,
    ) -> Result<Value, RepositoryError> {
        self.source_watermarks_for_scope(project_id, None)
    }

    pub fn writer_source_watermarks(
        &self,
        project_id: &ProjectId,
        allowed_objects: &HashSet<String>,
    ) -> Result<Value, RepositoryError> {
        self.source_watermarks_for_scope(project_id, Some(allowed_objects))
    }

    fn source_watermarks_for_scope(
        &self,
        project_id: &ProjectId,
        allowed_objects: Option<&HashSet<String>>,
    ) -> Result<Value, RepositoryError> {
        source_watermarks_for_scope(&self.connection, project_id, allowed_objects)
    }
}

pub(crate) fn project_source_digest(
    connection: &rusqlite::Connection,
    project_id: &ProjectId,
) -> Result<String, RepositoryError> {
    source_watermarks_for_scope(connection, project_id, None)?["digest"]
        .as_str()
        .map(str::to_owned)
        .ok_or(RepositoryError::CorruptState)
}

fn source_watermarks_for_scope(
    connection: &rusqlite::Connection,
    project_id: &ProjectId,
    allowed_objects: Option<&HashSet<String>>,
) -> Result<Value, RepositoryError> {
    let project_revision: i64 = connection
        .query_row(
            "SELECT revision FROM story_workspaces_v2 WHERE project_id = ?1",
            [project_id.as_str()],
            |row| row.get(0),
        )
        .map_err(storage_error)?;
    let mut objects = object_heads(connection, CURRENT_HEADS_SQL, project_id.as_str())?;
    let mut knowledge = knowledge_heads(
        connection,
        "SELECT object_id, batch_id FROM story_knowledge_heads_v2 WHERE project_id = ?1",
        project_id.as_str(),
    )?;
    if let Some(allowed_objects) = allowed_objects {
        objects.retain(|object_id, _| allowed_objects.contains(object_id));
        knowledge.retain(|object_id, _| allowed_objects.contains(object_id));
    }
    let object_count = objects.len();
    let knowledge_count = knowledge.len();
    let encoded = serde_json::to_vec(&(project_revision, objects, knowledge))
        .map_err(|_| RepositoryError::CorruptState)?;
    Ok(json!({
        "algorithm": "sha256-project-revision-object-working-and-knowledge-heads-v3",
        "digest": format!("{:x}", Sha256::digest(encoded)),
        "projectRevision": project_revision,
        "objectCount": object_count, "knowledgeBatchCount": knowledge_count
    }))
}

fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}
