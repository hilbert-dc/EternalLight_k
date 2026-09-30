use std::collections::HashSet;

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::authoring_runs::storage_error;
use crate::{ProjectId, RepositoryError, SqliteStoryRepository, TaskRecord};

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
/// An author-approved criterion restricted to the writer's selected scene groups.
pub struct DesiredOutcome {
    pub id: String,
    pub criterion: String,
    pub group_ids: Vec<String>,
}

/// A source-bound candidate interpretation, never a semantic completion certificate.
pub struct StoredRevisionOutcome {
    pub revision_id: String,
    pub content: Value,
    pub current: bool,
    pub decisions: Vec<crate::RevisionOutcomeDecision>,
}

pub(crate) fn desired_outcomes(target: &Value) -> Result<Vec<DesiredOutcome>, RepositoryError> {
    let Some(value) = target.get("desiredOutcomes") else {
        return Ok(Vec::new());
    };
    let invalid = || RepositoryError::TaskBindingInvalid;
    crate::scene_revision::selection(target)?;
    let outcomes: Vec<DesiredOutcome> =
        serde_json::from_value(value.clone()).map_err(|_| invalid())?;
    let selected = target["scope"]["groupIds"].as_array().ok_or_else(invalid)?;
    if outcomes.is_empty()
        || outcomes.len() > 4
        || outcomes
            .iter()
            .map(|item| &item.id)
            .collect::<HashSet<_>>()
            .len()
            != outcomes.len()
        || outcomes.iter().any(|item| {
            item.id.is_empty()
                || item.id.len() > 64
                || !item
                    .id
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
                || item.criterion.trim().is_empty()
                || item.criterion.chars().count() > 500
                || item.group_ids.is_empty()
                || item.group_ids.len() > 16
                || item.group_ids.iter().collect::<HashSet<_>>().len() != item.group_ids.len()
                || item
                    .group_ids
                    .iter()
                    .any(|id| !selected.contains(&json!(id)))
        })
    {
        return Err(invalid());
    }
    Ok(outcomes)
}

pub(crate) fn writer(
    db: &Connection,
    project: &ProjectId,
    id: &str,
) -> Result<(TaskRecord, Value), RepositoryError> {
    let task = crate::tasks::load_task(db, id)?;
    let target: Value =
        serde_json::from_str(&task.target_json).map_err(|_| RepositoryError::CorruptState)?;
    if task.project_id != *project
        || task.operation != "generateChapter"
        || task.status != "completed"
        || target.get("runId").is_some()
        || desired_outcomes(&target)?.is_empty()
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let (source, action_id): (String,String) = db.query_row(
        "SELECT a.source_task_id,a.action_id FROM story_task_action_requests_v1 a JOIN story_work_attempts_v1 w ON w.step_id=a.action_id
         WHERE w.attempt_id=?1 AND a.project_id=?2 AND a.operation='generateChapter'",
        params![id,project.as_str()], |row|Ok((row.get(0)?,row.get(1)?)),
    ).optional().map_err(storage_error)?.ok_or(RepositoryError::TaskBindingInvalid)?;
    let action = crate::task_actions::load_action(db, project, &source, &action_id)?;
    let mut proposed = target.clone();
    let fields = proposed
        .as_object_mut()
        .ok_or(RepositoryError::TaskBindingInvalid)?;
    fields.remove("requestedByTaskId");
    fields.remove("requestedActionId");
    fields.remove("resumeStepId");
    if action.target != proposed
        || action.operation != "generateChapter"
        || !crate::task_action_permit::granted(db, project, &action)?
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    crate::author_message::validate_execution(db, &task)?;
    Ok((task, target))
}

/// Validate the immutable predecessor chain; current draft and batch are separate checks.
pub(crate) fn validate_target(
    db: &Connection,
    project: &ProjectId,
    target: &Value,
) -> Result<(), RepositoryError> {
    let id = target["writerTaskId"]
        .as_str()
        .ok_or(RepositoryError::TaskBindingInvalid)?;
    let (task, original) = writer(db, project, id)?;
    let output: Option<String> = db.query_row(
        "SELECT revision_id FROM story_task_outputs_v2 WHERE task_id=?1 AND output_kind='draft' AND completeness='chapterComplete' ORDER BY ordinal DESC LIMIT 1",
        [id], |row| row.get(0),
    ).optional().map_err(storage_error)?;
    let step = target["resumeStepId"]
        .as_str()
        .ok_or(RepositoryError::TaskBindingInvalid)?;
    let mut actual = target.clone();
    actual
        .as_object_mut()
        .ok_or(RepositoryError::TaskBindingInvalid)?
        .remove("resumeStepId");
    let registered: Option<String> = db.query_row(
        "SELECT s.target_json FROM story_work_steps_v1 s JOIN story_work_steps_v1 prior ON prior.id=s.depends_on_step_id
         JOIN story_work_attempts_v1 w ON w.step_id IN (SELECT id FROM story_work_steps_v1 WHERE work_task_id=s.work_task_id)
         WHERE s.id=?1 AND s.operation='assessRevisionOutcome' AND prior.operation='modelKnowledge' AND prior.status='completed' AND w.attempt_id=?2",
        params![step,id], |row| row.get(0),
    ).optional().map_err(storage_error)?;
    if registered
        .as_deref()
        .map(serde_json::from_str::<Value>)
        .transpose()
        .map_err(|_| RepositoryError::CorruptState)?
        .as_ref()
        != Some(&actual)
        || target["chapterId"] != original["chapterId"]
        || target["chapterNumber"] != original["chapterNumber"]
        || target["originalRevisionId"] != original["scope"]["sourceRevisionId"]
        || output.as_deref() != target["draftRevisionId"].as_str()
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let batch: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM story_modeling_batches_v2 b JOIN story_work_attempts_v1 a ON a.attempt_id=b.publisher_task_id
         JOIN story_work_steps_v1 s ON s.id=a.step_id JOIN story_work_steps_v1 next ON next.depends_on_step_id=s.id
         WHERE next.id=?1 AND s.current_attempt_id=a.attempt_id AND b.batch_id=?2 AND b.project_id=?3
         AND b.object_id='draft-'||?4 AND b.source_revision_id=?5 AND b.published_at IS NOT NULL AND b.status IN ('ready','stale'))",
        params![step,target["knowledgeBatchId"].as_str(),project.as_str(),original["chapterId"].as_str(),output], |row|row.get(0),
    ).map_err(storage_error)?;
    if !batch || task.project_id != *project {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    Ok(())
}

pub(crate) fn inputs_current(
    db: &Connection,
    project: &ProjectId,
    target: &Value,
) -> Result<bool, RepositoryError> {
    db.query_row(
        "SELECT EXISTS(SELECT 1 FROM story_object_heads_v2 h JOIN story_knowledge_heads_v2 k ON k.project_id=h.project_id AND k.object_id=h.object_id
         JOIN story_modeling_batches_v2 b ON b.batch_id=k.batch_id AND b.project_id=k.project_id
         WHERE h.project_id=?1 AND h.object_id='draft-'||?2 AND COALESCE(h.candidate_revision_id,h.accepted_revision_id)=?3
         AND k.batch_id=?4 AND b.status='ready' AND b.source_revision_id=?3)",
        params![project.as_str(),target["chapterId"].as_str(),target["draftRevisionId"].as_str(),target["knowledgeBatchId"].as_str()], |row|row.get(0),
    ).map_err(storage_error)
}

pub(crate) fn queue_after_model(
    db: &Connection,
    attempt: &TaskRecord,
    parent: &str,
    step: &str,
) -> Result<(), RepositoryError> {
    let model_target: Value =
        serde_json::from_str(&attempt.target_json).map_err(|_| RepositoryError::CorruptState)?;
    let Some(id) = model_target["sourceTaskId"].as_str() else {
        return Ok(());
    };
    let original_task = crate::tasks::load_task(db, id)?;
    let original: Value = serde_json::from_str(&original_task.target_json)
        .map_err(|_| RepositoryError::CorruptState)?;
    if desired_outcomes(&original)?.is_empty() {
        return Ok(());
    }
    let (_, original) = writer(db, &attempt.project_id, id)?;
    let output: String = db.query_row("SELECT revision_id FROM story_task_outputs_v2 WHERE task_id=?1 AND output_kind='draft' AND completeness='chapterComplete' ORDER BY ordinal DESC LIMIT 1", [id], |row|row.get(0)).map_err(storage_error)?;
    let target = json!({"writerTaskId":id,"chapterId":original["chapterId"],"chapterNumber":original["chapterNumber"],
        "originalRevisionId":original["scope"]["sourceRevisionId"],"draftRevisionId":output,"knowledgeBatchId":model_target["batchId"]});
    if !inputs_current(db, &attempt.project_id, &target)? {
        return Err(RepositoryError::TaskInputStale);
    }
    db.execute(
        "INSERT OR IGNORE INTO story_work_steps_v1(id,work_task_id,ordinal,operation,target_json,input_revision,depends_on_step_id,status)
         VALUES(?1,?2,(SELECT COALESCE(MAX(ordinal),0)+1 FROM story_work_steps_v1 WHERE work_task_id=?2),'assessRevisionOutcome',?3,
         (SELECT revision FROM story_workspaces_v2 WHERE project_id=?4),?5,'queued')",
        params![format!("outcome-{id}"),parent,target.to_string(),attempt.project_id.as_str(),step],
    ).map_err(storage_error)?;
    Ok(())
}

impl SqliteStoryRepository {
    pub fn revision_outcome(
        &self,
        project: &ProjectId,
        writer_id: &str,
    ) -> Result<Option<StoredRevisionOutcome>, RepositoryError> {
        let read = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let task = crate::tasks::load_task(&read, writer_id)?;
        if task.project_id != *project || task.operation != "generateChapter" {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        if task.status != "completed" {
            return Ok(None);
        }
        let _ = writer(&read, project, writer_id)?;
        let row: Option<(String,String,String)> = read.query_row(
            "SELECT r.revision_id,r.content_json,t.target_json FROM story_object_heads_v2 h
             JOIN story_revisions_v2 r ON r.revision_id=h.candidate_revision_id AND r.project_id=h.project_id AND r.object_id=h.object_id
             JOIN story_task_outputs_v2 o ON o.revision_id=r.revision_id AND o.output_kind='revisionOutcome'
             JOIN story_tasks_v2 t ON t.task_id=o.task_id AND t.operation='assessRevisionOutcome' AND t.project_id=h.project_id
             WHERE h.project_id=?1 AND h.object_id=?2 AND r.object_kind='revisionOutcome'",
            params![project.as_str(),format!("revision-outcome-{writer_id}")], |row|Ok((row.get(0)?,row.get(1)?,row.get(2)?)),
        ).optional().map_err(storage_error)?;
        row.map(|(revision_id, encoded, target)| {
            let target: Value =
                serde_json::from_str(&target).map_err(|_| RepositoryError::CorruptState)?;
            validate_target(&read, project, &target)?;
            crate::revision_outcome_validation::validate_report(&read, project, &target, &encoded)?;
            Ok(StoredRevisionOutcome {
                revision_id,
                content: serde_json::from_str(&encoded)
                    .map_err(|_| RepositoryError::CorruptState)?,
                current: inputs_current(&read, project, &target)?,
                decisions: crate::revision_outcome_decision::history(&read, project, writer_id)?,
            })
        })
        .transpose()
    }
}
