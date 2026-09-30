use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::authoring_runs::storage_error;
use crate::{CreateTaskRequest, ProjectId, RepositoryError, TaskRecord};

pub(crate) const MAX_PACKET_BYTES: usize = 1_048_576;

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Binding {
    run_id: String,
    author_message_id: String,
    role: String,
    step_id: String,
    source_packet_hash: String,
}

pub(crate) struct Source {
    pub author_message_id: String,
    pub text: String,
    packet: Option<String>,
    packet_hash: String,
    kind: String,
    revision: String,
    node: Option<String>,
    input_revision: i64,
    created_at: i64,
    goal: String,
    recorded_text: Option<String>,
    recorded_at: Option<i64>,
}

pub(crate) fn root(
    db: &Connection,
    project: &ProjectId,
    run: &str,
) -> Result<Source, RepositoryError> {
    let source: Source = db.query_row(
        "SELECT r.author_message_id,r.author_intent,
         CASE WHEN length(CAST(r.source_packet_json AS BLOB))<=?3 THEN r.source_packet_json ELSE NULL END,
         r.source_packet_hash,r.artifact_kind,r.revision_id,r.target_node_id,r.input_revision,r.created_at,w.goal,m.author_text,m.created_at
         FROM story_planning_review_runs_v2 r JOIN story_work_tasks_v1 w ON w.id='work-'||r.run_id AND w.project_id=r.project_id AND w.kind='direct' AND w.input_revision=r.input_revision
         LEFT JOIN story_author_messages_v1 m ON m.message_id=r.author_message_id AND m.review_run_id=r.run_id
          AND m.work_task_id=w.id AND m.project_id=r.project_id AND m.intent_revision=0
          AND m.task_id IS NULL AND m.generation_run_id IS NULL AND m.thread_id IS NULL
         WHERE r.run_id=?1 AND r.project_id=?2",
        params![run,project.as_str(),MAX_PACKET_BYTES], |row| Ok(Source {
            author_message_id:row.get(0)?,text:row.get(1)?,packet:row.get(2)?,packet_hash:row.get(3)?,kind:row.get(4)?,
            revision:row.get(5)?,node:row.get(6)?,input_revision:row.get(7)?,created_at:row.get(8)?,goal:row.get(9)?,
            recorded_text:row.get(10)?,recorded_at:row.get(11)?,
        }),
    ).optional().map_err(storage_error)?.ok_or(RepositoryError::TaskBindingInvalid)?;
    let packet = source
        .packet
        .as_deref()
        .ok_or(RepositoryError::TaskBindingInvalid)?;
    let parsed: Value =
        serde_json::from_str(packet).map_err(|_| RepositoryError::TaskBindingInvalid)?;
    if !crate::author_message::valid_id(&source.author_message_id)
        || source.text.trim().is_empty()
        || source.text.chars().count() > 4_000
        || source.created_at < 0
        || source.recorded_text.as_deref() != Some(source.text.as_str())
        || source.recorded_at != Some(source.created_at)
        || source.goal != source.text
        || source.packet_hash != format!("{:x}", Sha256::digest(packet.as_bytes()))
        || parsed["authorMessageId"] != source.author_message_id
        || parsed["authorIntent"] != source.text
        || parsed["artifactKind"] != source.kind
        || parsed["revisionId"] != source.revision
        || parsed["scope"]["targetNodeId"] != json!(source.node)
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    Ok(source)
}

pub(crate) fn owns_task(db: &Connection, task: &str) -> Result<bool, RepositoryError> {
    db.query_row("SELECT EXISTS(SELECT 1 FROM story_work_attempts_v1 a
        JOIN story_work_steps_v1 s ON s.id=a.step_id
        JOIN story_planning_review_runs_v2 r ON s.work_task_id='work-'||r.run_id WHERE a.attempt_id=?1)",
        [task], |row|row.get(0)).map_err(storage_error)
}

pub(crate) fn validate_work(
    db: &Connection,
    project: &ProjectId,
    work: &str,
) -> Result<(), RepositoryError> {
    let run: Option<String> = db.query_row("SELECT run_id FROM story_planning_review_runs_v2 WHERE 'work-'||run_id=?1 AND project_id=?2",
        params![work,project.as_str()],|row|row.get(0)).optional().map_err(storage_error)?;
    if let Some(run) = run {
        root(db,project,&run)?;
    } else if db.query_row("SELECT EXISTS(SELECT 1 FROM story_work_steps_v1 WHERE work_task_id=?1 AND operation='reviewPlanningSeat')",
        [work],|row|row.get::<_,bool>(0)).map_err(storage_error)? {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    Ok(())
}

pub(crate) fn freeze(
    db: &Connection,
    request: &CreateTaskRequest,
    target: &Value,
    manifest: &mut Value,
) -> Result<(), RepositoryError> {
    let run = target["reviewRunId"]
        .as_str()
        .ok_or(RepositoryError::TaskBindingInvalid)?;
    let role = target["role"]
        .as_str()
        .ok_or(RepositoryError::TaskBindingInvalid)?;
    let step = target["resumeStepId"]
        .as_str()
        .ok_or(RepositoryError::TaskBindingInvalid)?;
    let source = root(db, &request.project_id, run)?;
    let mut expected =
        crate::planning_review_work::target(run, role, &source.kind, &source.revision);
    expected["resumeStepId"] = json!(step);
    let valid: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM story_work_steps_v1 s
        JOIN story_planning_review_seats_v2 seat ON seat.run_id=?2 AND seat.role=?3
        WHERE s.id=?1 AND s.work_task_id='work-'||?2 AND s.operation='reviewPlanningSeat'
        AND s.input_revision=?4 AND s.target_json=?5)",
            params![
                step,
                run,
                role,
                source.input_revision,
                crate::planning_review_work::target(run, role, &source.kind, &source.revision)
                    .to_string()
            ],
            |row| row.get(0),
        )
        .map_err(storage_error)?;
    if !valid
        || request.operation != "reviewPlanningSeat"
        || target != &expected
        || step != format!("review-seat-{run}-{role}")
        || request.input_revision != source.input_revision
        || !matches!(role, "structure" | "character" | "experience")
        || manifest.get("authorMessageId").is_some()
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let prior: Option<String> = db.query_row("SELECT attempt_id FROM story_work_attempts_v1 WHERE step_id=?1 ORDER BY attempt_number DESC LIMIT 1",
        [step],|row|row.get(0)).optional().map_err(storage_error)?;
    if let Some(prior) = prior
        && crate::task_author_intent::source_intent(db, &prior, /*revision*/ 0)? != source.text
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    manifest["authorIntent"] = json!(source.text);
    manifest["authorMessageId"] = json!(source.author_message_id);
    manifest["sourcePacketHash"] = json!(source.packet_hash);
    manifest["planningReviewAuthorIntentBinding"] = serde_json::to_value(Binding {
        run_id: run.into(),
        author_message_id: source.author_message_id,
        role: role.into(),
        step_id: step.into(),
        source_packet_hash: source.packet_hash,
    })
    .map_err(|_| RepositoryError::CorruptState)?;
    Ok(())
}

pub(crate) fn validate_task(
    db: &Connection,
    task: &TaskRecord,
    manifest: &Value,
    target: &Value,
) -> Result<(), RepositoryError> {
    let binding: Binding =
        serde_json::from_value(manifest["planningReviewAuthorIntentBinding"].clone())
            .map_err(|_| RepositoryError::TaskBindingInvalid)?;
    let source = root(db, &task.project_id, &binding.run_id)?;
    let actual_step: Option<(String,String,i64)> = db.query_row("SELECT s.id,s.target_json,s.input_revision FROM story_work_attempts_v1 a
        JOIN story_work_steps_v1 s ON s.id=a.step_id AND s.work_task_id='work-'||?2 AND s.operation='reviewPlanningSeat'
        JOIN story_work_tasks_v1 w ON w.id=s.work_task_id AND w.project_id=?3
        JOIN story_planning_review_seats_v2 seat ON seat.run_id=?2 AND seat.role=?4
        WHERE a.attempt_id=?1",
        params![task.task_id,binding.run_id,task.project_id.as_str(),binding.role],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?)),
    ).optional().map_err(storage_error)?;
    let (step, stored_target, revision) = actual_step.ok_or(RepositoryError::TaskBindingInvalid)?;
    let expected = crate::planning_review_work::target(
        &binding.run_id,
        &binding.role,
        &source.kind,
        &source.revision,
    );
    let mut task_target = expected.clone();
    task_target["resumeStepId"] = json!(step);
    if task.operation != "reviewPlanningSeat"
        || !matches!(
            binding.role.as_str(),
            "structure" | "character" | "experience"
        )
        || step != format!("review-seat-{}-{}", binding.run_id, binding.role)
        || binding.step_id != step
        || binding.author_message_id != source.author_message_id
        || binding.source_packet_hash != source.packet_hash
        || task.input_revision != source.input_revision
        || revision != source.input_revision
        || serde_json::from_str::<Value>(&stored_target)
            .map_err(|_| RepositoryError::TaskBindingInvalid)?
            != expected
        || target != &task_target
        || manifest["authorMessageId"] != source.author_message_id
        || manifest["authorIntent"] != source.text
        || manifest["sourcePacketHash"] != source.packet_hash
        || [
            "authorIntentBinding",
            "candidateInstruction",
            "generationAuthorIntentBinding",
            "domainTaskSource",
            "domainInstruction",
        ]
        .into_iter()
        .any(|field| manifest.get(field).is_some())
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    Ok(())
}
