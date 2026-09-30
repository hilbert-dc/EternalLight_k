use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};

use crate::{CreateTaskRequest, RepositoryError};

pub(crate) const MAX_AUTHOR_HISTORY_BYTES: usize = 24_000;

pub(crate) fn source_intent(
    db: &Connection,
    source_id: &str,
    revision: i64,
) -> Result<String, RepositoryError> {
    source_intent_at_depth(db, source_id, revision, /*remaining_depth*/ 32)
}

fn source_intent_at_depth(
    db: &Connection,
    source_id: &str,
    revision: i64,
    remaining_depth: u8,
) -> Result<String, RepositoryError> {
    if remaining_depth == 0 {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    if !(0..=16).contains(&revision) {
        return Err(RepositoryError::TaskInputStale);
    }
    let source = crate::tasks::load_task(db, source_id)?;
    let manifest: Value = serde_json::from_str(&source.input_manifest_json)
        .map_err(|_| RepositoryError::CorruptState)?;
    let target: Value =
        serde_json::from_str(&source.target_json).map_err(|_| RepositoryError::CorruptState)?;
    let review_source = crate::planning_review_author_source::owns_task(db, source_id)?
        || source.operation == "reviewPlanningSeat"
        || target.get("reviewRunId").is_some()
        || manifest.get("planningReviewAuthorIntentBinding").is_some();
    if review_source {
        crate::planning_review_author_source::validate_task(db, &source, &manifest, &target)?;
    }
    if crate::generation_author_source::owns_task(db, source_id)? || target.get("runId").is_some() {
        if manifest.get("authorIntentBinding").is_some() {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        crate::generation_author_source::validate_task(db, &source, &manifest)?;
    } else if manifest.get("generationAuthorIntentBinding").is_some() {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let mut text = manifest["authorIntent"]
        .as_str()
        .filter(|text| !text.trim().is_empty())
        .ok_or(RepositoryError::TaskBindingInvalid)?
        .to_owned();
    if let Some(binding) = manifest.get("authorIntentBinding") {
        let prior_id = binding["sourceTaskId"]
            .as_str()
            .ok_or(RepositoryError::TaskBindingInvalid)?;
        let prior_revision = binding["intentRevision"]
            .as_i64()
            .ok_or(RepositoryError::TaskBindingInvalid)?;
        let valid: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM story_work_attempts_v1 current
             JOIN story_work_steps_v1 cs ON cs.id=current.step_id
             JOIN story_work_attempts_v1 prior ON prior.attempt_id=?2 AND prior.sequence<current.sequence
             JOIN story_work_steps_v1 ps ON ps.id=prior.step_id AND ps.work_task_id=cs.work_task_id
             WHERE current.attempt_id=?1)",
            params![source_id,prior_id], |row|row.get(0),
        ).map_err(storage_error)?;
        if !valid
            || text != source_intent_at_depth(db, prior_id, prior_revision, remaining_depth - 1)?
        {
            return Err(RepositoryError::TaskBindingInvalid);
        }
    } else if target.get("runId").is_none() && !review_source {
        crate::author_message::validate_root(db, &source, &manifest)?;
    }
    let mut statement = db.prepare(
        "SELECT intent_revision FROM story_task_intents_v1 WHERE task_id=?1 AND intent_revision<=?2 ORDER BY intent_revision LIMIT 16",
    ).map_err(storage_error)?;
    let rows = statement
        .query_map(params![source_id, revision], |row| row.get::<_, i64>(0))
        .map_err(storage_error)?;
    let mut count = 0;
    for row in rows {
        let generation = row.map_err(storage_error)?;
        let record = crate::task_intents::load_intent(db, source_id, generation)?;
        count += 1;
        if generation != count || record.status != "delivered" {
            return Err(RepositoryError::TaskInputStale);
        }
        text.push_str("\n\n作者追加要求：\n");
        text.push_str(&record.author_text);
    }
    if count != revision {
        return Err(RepositoryError::TaskInputStale);
    }
    if text.len() > MAX_AUTHOR_HISTORY_BYTES {
        return Err(RepositoryError::InvalidContent(
            "author instruction history exceeds the task budget".into(),
        ));
    }
    Ok(text)
}

pub(crate) fn freeze(
    db: &Connection,
    request: &CreateTaskRequest,
    target: &Value,
    manifest: &mut Value,
) -> Result<(), RepositoryError> {
    if manifest.get("authorIntentBinding").is_some()
        || manifest.get("candidateInstruction").is_some()
        || manifest.get("generationAuthorIntentBinding").is_some()
        || manifest.get("domainTaskSource").is_some()
        || manifest.get("domainInstruction").is_some()
        || manifest.get("planningReviewAuthorIntentBinding").is_some()
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    if target.get("runId").is_some() {
        return crate::generation_author_source::freeze(db, request, target, manifest);
    }
    if request.operation == "reviewPlanningSeat" || target.get("reviewRunId").is_some() {
        return crate::planning_review_author_source::freeze(db, request, target, manifest);
    }
    if request.operation == "assessRevisionOutcome" {
        crate::revision_outcome::validate_target(db, &request.project_id, target)?;
        let writer = target["writerTaskId"]
            .as_str()
            .ok_or(RepositoryError::TaskBindingInvalid)?;
        manifest["authorIntent"] = json!(source_intent(db, writer, /*revision*/ 0)?);
        manifest["authorIntentBinding"] =
            json!({"sourceTaskId":writer,"intentRevision":0,"actionId":null});
        return Ok(());
    }
    if let Some(action_id) = target["requestedActionId"].as_str() {
        let (source_id, revision, instruction): (String,i64,String) = db.query_row(
            "SELECT source_task_id,intent_revision,instruction FROM story_task_action_requests_v1 WHERE project_id=?1 AND action_id=?2",
            params![request.project_id.as_str(),action_id],
            |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?)),
        ).optional().map_err(storage_error)?.ok_or(RepositoryError::TaskBindingInvalid)?;
        if target["requestedByTaskId"].as_str() != Some(source_id.as_str()) {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        crate::task_intents::assert_action_intent(db, &source_id, revision)?;
        manifest["authorIntent"] = json!(source_intent(db, &source_id, revision)?);
        manifest["candidateInstruction"] = json!(instruction);
        manifest["authorIntentBinding"] =
            json!({"sourceTaskId":source_id,"intentRevision":revision,"actionId":action_id});
    } else if let Some(source_id) = target["sourceTaskId"].as_str() {
        manifest["authorIntent"] = json!(source_intent(db, source_id, /*revision*/ 0)?);
        manifest["authorIntentBinding"] =
            json!({"sourceTaskId":source_id,"intentRevision":0,"actionId":null});
    } else if let Some(step_id) = target["resumeStepId"].as_str() {
        if crate::task_domain_source::freeze(db, request, target, manifest)? {
            return Ok(());
        }
        let continuation_root = if target.get("scopeContinuation").is_some() {
            let mut frozen_target = target.clone();
            frozen_target
                .as_object_mut()
                .ok_or(RepositoryError::TaskBindingInvalid)?
                .remove("resumeStepId");
            let (source, revision) = crate::scope_continuation::validate_step(
                db,
                &request.project_id,
                step_id,
                &frozen_target,
            )?;
            if request.operation != "advisory" {
                return Err(RepositoryError::TaskBindingInvalid);
            }
            Some((source, revision))
        } else {
            None
        };
        let source_id: Option<String> = db.query_row(
            "SELECT a.attempt_id FROM story_work_attempts_v1 a JOIN story_work_steps_v1 s ON s.id=a.step_id JOIN story_work_tasks_v1 w ON w.id=s.work_task_id WHERE s.id=?1 AND w.project_id=?2 AND s.operation=?3 ORDER BY a.attempt_number DESC LIMIT 1",
            params![step_id,request.project_id.as_str(),request.operation], |row| row.get(0),
        ).optional().map_err(storage_error)?;
        let Some(source_id) = source_id else {
            if let Some((source, revision)) = continuation_root {
                manifest["authorIntent"] = json!(source_intent(db, &source, revision)?);
                manifest["authorIntentBinding"] =
                    json!({"sourceTaskId":source,"intentRevision":revision,"actionId":null});
                return Ok(());
            }
            // Prequeued domain steps (e.g. review seats) have no earlier Attempt.
            // Their domain claim validates the source; they are not author messages.
            if request.operation == "advisory" {
                return Err(RepositoryError::TaskBindingInvalid);
            }
            return Ok(());
        };
        let revision: i64 = db.query_row(
            "SELECT COALESCE(MAX(intent_revision),0) FROM story_task_intents_v1 WHERE task_id=?1",
            [&source_id], |row| row.get(0),
        ).map_err(storage_error)?;
        crate::task_intents::assert_action_intent(db, &source_id, revision)?;
        manifest["authorIntent"] = json!(source_intent(db, &source_id, revision)?);
        manifest["authorIntentBinding"] =
            json!({"sourceTaskId":source_id,"intentRevision":revision,"actionId":null});
        let prior = crate::tasks::load_task(db, &source_id)?;
        let prior_manifest: Value = serde_json::from_str(&prior.input_manifest_json)
            .map_err(|_| RepositoryError::CorruptState)?;
        if let Some(instruction) = prior_manifest.get("candidateInstruction") {
            manifest["candidateInstruction"] = instruction.clone();
        }
        if request.operation == "advisory" {
            crate::task_intents::supersede_queued_actions(db, &source_id)?;
        }
        if continuation_root.is_some() {
            return Ok(());
        }
    }
    if target.get("scopeContinuation").is_some() {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    Ok(())
}

fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}
