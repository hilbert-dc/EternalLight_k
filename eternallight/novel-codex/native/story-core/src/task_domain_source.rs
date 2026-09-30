use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::authoring_runs::storage_error;
use crate::{CreateTaskRequest, RepositoryError, TaskRecord};

pub(crate) const MODELING_INSTRUCTION: &str =
    "从绑定章节工作稿提取有证据的候选知识；不得改写正文或确认 Canon。";

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum DomainSourceKind {
    ModelingOutbox,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DomainTaskSource {
    #[serde(rename = "type")]
    pub source_type: DomainSourceKind,
    pub work_task_id: String,
    pub step_id: String,
    pub batch_id: String,
    pub object_id: String,
    pub source_revision_id: String,
    pub chapter_id: String,
    pub intent_generation: i64,
}

struct RegisteredSourceRow {
    encoded: String,
    work: String,
    owner: String,
    encoded_target: String,
    object: String,
    revision: String,
    current_generation: Option<i64>,
}

fn registered_source(
    db: &Connection,
    project: &str,
    step: &str,
) -> Result<Option<DomainTaskSource>, RepositoryError> {
    let record: Option<RegisteredSourceRow> = db.query_row(
        "SELECT d.source_json,w.id,w.project_id,s.target_json,b.object_id,b.source_revision_id,current.intent_generation
         FROM story_domain_task_sources_v1 d JOIN story_work_steps_v1 s ON s.id=d.step_id
         JOIN story_work_tasks_v1 w ON w.id=s.work_task_id
         JOIN story_modeling_batches_v2 b ON b.batch_id=json_extract(s.target_json,'$.batchId') AND b.project_id=w.project_id
         LEFT JOIN story_modeling_dispatch_v1 current ON current.step_id=s.id AND current.project_id=w.project_id
         WHERE d.step_id=?1 AND s.operation='modelKnowledge' AND w.kind='direct'",
        [step], |row| Ok(RegisteredSourceRow {
            encoded:row.get(0)?,work:row.get(1)?,owner:row.get(2)?,encoded_target:row.get(3)?,
            object:row.get(4)?,revision:row.get(5)?,current_generation:row.get(6)?,
        }),
    ).optional().map_err(storage_error)?;
    let Some(RegisteredSourceRow {
        encoded,
        work,
        owner,
        encoded_target,
        object,
        revision,
        current_generation,
    }) = record
    else {
        return Ok(None);
    };
    let source: DomainTaskSource =
        serde_json::from_str(&encoded).map_err(|_| RepositoryError::TaskBindingInvalid)?;
    let target: Value =
        serde_json::from_str(&encoded_target).map_err(|_| RepositoryError::TaskBindingInvalid)?;
    if owner != project
        || source.work_task_id != work
        || source.step_id != step
        || source.object_id != object
        || source.source_revision_id != revision
        || source.object_id != format!("draft-{}", source.chapter_id)
        || source.intent_generation <= 0
        || current_generation.is_some_and(|generation| generation != source.intent_generation)
        || [
            &source.work_task_id,
            &source.step_id,
            &source.batch_id,
            &source.object_id,
            &source.source_revision_id,
            &source.chapter_id,
        ]
        .into_iter()
        .any(|id| crate::ObjectId::parse(id.as_str()).is_err())
        || target
            != json!({"batchId":source.batch_id,"chapterId":source.chapter_id,"modelingIntent":true})
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    Ok(Some(source))
}

pub(crate) fn freeze(
    db: &Connection,
    request: &CreateTaskRequest,
    target: &Value,
    manifest: &mut Value,
) -> Result<bool, RepositoryError> {
    let Some(step) = target["resumeStepId"].as_str() else {
        return Ok(false);
    };
    let Some(source) = registered_source(db, request.project_id.as_str(), step)? else {
        if target.get("modelingIntent").is_some() {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        return Ok(false);
    };
    let mut normalized = target.clone();
    normalized
        .as_object_mut()
        .ok_or(RepositoryError::TaskBindingInvalid)?
        .remove("resumeStepId");
    if request.operation != "modelKnowledge"
        || manifest.get("authorMessageId").is_some()
        || normalized
            != json!({"batchId":source.batch_id,"chapterId":source.chapter_id,"modelingIntent":true})
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let prior: Option<String> = db.query_row(
        "SELECT attempt_id FROM story_work_attempts_v1 WHERE step_id=?1 ORDER BY attempt_number DESC LIMIT 1",
        [step], |row| row.get(0),
    ).optional().map_err(storage_error)?;
    if let Some(prior) = prior {
        let task = crate::tasks::load_task(db, &prior)?;
        validate_execution(db, &task)?;
    }
    manifest
        .as_object_mut()
        .ok_or(RepositoryError::TaskBindingInvalid)?
        .remove("authorIntent");
    manifest["domainTaskSource"] =
        serde_json::to_value(source).map_err(|_| RepositoryError::CorruptState)?;
    manifest["domainInstruction"] = json!(MODELING_INSTRUCTION);
    Ok(true)
}

pub(crate) fn validate_execution(
    db: &Connection,
    task: &TaskRecord,
) -> Result<bool, RepositoryError> {
    let manifest: Value = serde_json::from_str(&task.input_manifest_json)
        .map_err(|_| RepositoryError::CorruptState)?;
    let mut target: Value =
        serde_json::from_str(&task.target_json).map_err(|_| RepositoryError::CorruptState)?;
    let step: Option<String> = db.query_row(
        "SELECT a.step_id FROM story_work_attempts_v1 a JOIN story_domain_task_sources_v1 d ON d.step_id=a.step_id WHERE a.attempt_id=?1",
        [&task.task_id], |row| row.get(0),
    ).optional().map_err(storage_error)?;
    if step.is_none()
        && manifest.get("domainTaskSource").is_none()
        && manifest.get("domainInstruction").is_none()
        && target.get("modelingIntent").is_none()
    {
        return Ok(false);
    }
    let step = step.ok_or(RepositoryError::TaskBindingInvalid)?;
    let source = registered_source(db, task.project_id.as_str(), &step)?
        .ok_or(RepositoryError::TaskBindingInvalid)?;
    if task.operation != "modelKnowledge"
        || target["resumeStepId"].as_str() != Some(step.as_str())
        || manifest["domainTaskSource"]
            != serde_json::to_value(&source).map_err(|_| RepositoryError::CorruptState)?
        || manifest["domainInstruction"].as_str() != Some(MODELING_INSTRUCTION)
        || [
            "authorIntent",
            "authorMessageId",
            "authorIntentBinding",
            "candidateInstruction",
            "generationAuthorIntentBinding",
            "planningReviewAuthorIntentBinding",
        ]
        .into_iter()
        .any(|key| manifest.get(key).is_some())
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    target
        .as_object_mut()
        .ok_or(RepositoryError::TaskBindingInvalid)?
        .remove("resumeStepId");
    if target
        != json!({"batchId":source.batch_id,"chapterId":source.chapter_id,"modelingIntent":true})
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    Ok(true)
}
