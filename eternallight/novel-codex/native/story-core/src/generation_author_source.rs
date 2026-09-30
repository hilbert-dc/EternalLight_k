use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::authoring_runs::storage_error;
use crate::{
    CreateTaskRequest, CreativeIntentRevision, CreativeIntentSource, GenerationFeedbackDecision,
    ProjectId, RepositoryError, TaskRecord, generation_author_intent,
};

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Binding {
    run_id: String,
    author_message_id: String,
    intent_generation: usize,
    chapter_number: i64,
}

struct SourceMessage<'a> {
    id: &'a str,
    text: &'a str,
    generation: i64,
    created_at: i64,
}

fn history(state: &Value) -> Result<Vec<CreativeIntentRevision>, RepositoryError> {
    let history: Vec<CreativeIntentRevision> =
        serde_json::from_value(state["intentRevisions"].clone())
            .map_err(|_| RepositoryError::CorruptState)?;
    let start = state["startChapterNumber"]
        .as_i64()
        .ok_or(RepositoryError::CorruptState)?;
    let end = state["targetChapters"]
        .as_i64()
        .ok_or(RepositoryError::CorruptState)?;
    if start < 1
        || end < start
        || end - start > 2
        || history.iter().any(|intent| {
            intent.effective_from_chapter < start || intent.effective_from_chapter > end
        })
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    // A future instruction must fit before it is persisted, not first fail when that chapter starts.
    for chapter in start..=end {
        generation_author_intent(&history, chapter)?;
    }
    Ok(history)
}

fn decisions(state: &Value) -> Result<Vec<GenerationFeedbackDecision>, RepositoryError> {
    serde_json::from_value(state["feedbackDecisions"].clone())
        .map_err(|_| RepositoryError::CorruptState)
}

fn source_messages<'a>(
    db: &Connection,
    project: &ProjectId,
    history: &'a [CreativeIntentRevision],
    decisions: &'a [GenerationFeedbackDecision],
) -> Result<Vec<SourceMessage<'a>>, RepositoryError> {
    if decisions.len() > 9 {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    for (index, decision) in decisions.iter().enumerate() {
        let has_author_note = !decision.instruction.trim().is_empty();
        if !crate::author_message::valid_id(&decision.id)
            || decisions[..index]
                .iter()
                .any(|prior| prior.id == decision.id)
            || decision.decided_at < 0
            || decision.instruction.chars().count() > 1_000
            || decision.source_digest.len() != 64
            || !decision
                .source_digest
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            || !matches!(decision.choice.as_str(), "continue" | "repairDraft")
            || (decision.choice == "continue" && !decision.instruction.trim().is_empty())
            || decision.author_message_id.is_some() != has_author_note
            || decision
                .author_message_id
                .as_deref()
                .is_some_and(|id| !crate::author_message::valid_id(id))
        {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        let references = history.iter().filter(|intent| matches!(&intent.source, CreativeIntentSource::FeedbackDecision { decision_id } if decision_id == &decision.id)).count();
        if references != usize::from(decision.choice == "repairDraft") {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        // Bind the historical decision to its immutable assessed draft, not today's chapter numbering.
        let assessed_chapter: Option<i64> = db.query_row("SELECT json_extract(d.content_json,'$.chapterNumber') FROM story_revisions_v2 r JOIN story_revisions_v2 d ON d.revision_id=json_extract(r.content_json,'$.draftRevisionId') AND d.project_id=r.project_id AND d.object_id='draft-'||?3 AND d.object_kind='draft' WHERE r.revision_id=?1 AND r.project_id=?2 AND r.object_id='scene-assessment-'||?3 AND r.object_kind='sceneAssessment' AND json_extract(r.content_json,'$.chapterId')=?3", params![decision.assessment_revision_id,project.as_str(),decision.chapter_id], |row|row.get(0)).optional().map_err(storage_error)?;
        if assessed_chapter != Some(decision.chapter_number) {
            return Err(RepositoryError::TaskBindingInvalid);
        }
    }
    let mut messages = Vec::new();
    for intent in history {
        match &intent.source {
            CreativeIntentSource::AuthorMessage {
                author_message_id,
                created_at,
            } => messages.push(SourceMessage {
                id: author_message_id,
                text: &intent.instruction,
                generation: intent.generation,
                created_at: *created_at,
            }),
            CreativeIntentSource::FeedbackDecision { decision_id } => {
                let decision = decisions
                    .iter()
                    .find(|decision| &decision.id == decision_id)
                    .ok_or(RepositoryError::TaskBindingInvalid)?;
                if decision.choice != "repairDraft"
                    || decision.instruction != intent.instruction
                    || decision.chapter_number != intent.effective_from_chapter
                {
                    return Err(RepositoryError::TaskBindingInvalid);
                }
                if let Some(id) = decision.author_message_id.as_deref() {
                    messages.push(SourceMessage {
                        id,
                        text: &decision.instruction,
                        generation: intent.generation,
                        created_at: decision.decided_at,
                    });
                }
            }
        }
    }
    for (index, message) in messages.iter().enumerate() {
        if messages[..index].iter().any(|prior| prior.id == message.id) {
            return Err(RepositoryError::TaskBindingInvalid);
        }
    }
    Ok(messages)
}

fn insert_message(
    db: &Connection,
    project: &ProjectId,
    run: &str,
    message: &SourceMessage<'_>,
) -> Result<(), RepositoryError> {
    db.execute("INSERT INTO story_author_messages_v1(message_id,project_id,work_task_id,generation_run_id,author_text,created_at,intent_revision) VALUES(?1,?2,?3,?4,?5,?6,?7)", params![message.id,project.as_str(),format!("work-{run}"),run,message.text,message.created_at,message.generation]).map_err(storage_error)?;
    Ok(())
}

pub(crate) fn owns_task(db: &Connection, task: &str) -> Result<bool, RepositoryError> {
    db.query_row("SELECT EXISTS(SELECT 1 FROM story_work_attempts_v1 a JOIN story_work_steps_v1 s ON s.id=a.step_id JOIN story_work_tasks_v1 w ON w.id=s.work_task_id WHERE a.attempt_id=?1 AND w.kind='generation')",
        [task],|row|row.get(0)).map_err(storage_error)
}

pub(crate) fn record(
    db: &Connection,
    project: &ProjectId,
    run: &str,
    state: &Value,
) -> Result<(), RepositoryError> {
    let history = history(state)?;
    let decisions = decisions(state)?;
    let message = state["authorMessageId"]
        .as_str()
        .ok_or(RepositoryError::TaskBindingInvalid)?;
    if history.len() != 1
        || state["intentGeneration"] != 0
        || !crate::author_message::valid_id(message)
        || !decisions.is_empty()
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let sources = source_messages(db, project, &history, &decisions)?;
    if sources.len() != 1 || sources[0].id != message {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    insert_message(db, project, run, &sources[0])?;
    crate::generation_execution_scope::record(db, project, run, state, sources[0].created_at)?;
    validate_run(db, project, run, state)?;
    Ok(())
}

pub(crate) fn validate_run(
    db: &Connection,
    project: &ProjectId,
    run: &str,
    state: &Value,
) -> Result<Vec<CreativeIntentRevision>, RepositoryError> {
    crate::generation_execution_scope::validate(db, project, run, state)?;
    crate::revision_continuation::validate_history(db, project, run, state)?;
    crate::generation_delivery::validate_history(db, project, run, state)?;
    let (message,text): (String,String) = db.query_row("SELECT message_id,author_text FROM story_author_messages_v1 WHERE generation_run_id=?1 AND project_id=?2 AND work_task_id=?3 AND task_id IS NULL AND thread_id IS NULL AND intent_revision=0",
        params![run,project.as_str(),format!("work-{run}")],|row|Ok((row.get(0)?,row.get(1)?))).optional().map_err(storage_error)?.ok_or(RepositoryError::TaskBindingInvalid)?;
    let history = history(state)?;
    let decisions = decisions(state)?;
    let sources = source_messages(db, project, &history, &decisions)?;
    let chapter = state["completedChapters"]
        .as_i64()
        .and_then(|chapter| chapter.checked_add(1))
        .ok_or(RepositoryError::CorruptState)?;
    generation_author_intent(&history, chapter)?;
    if state["id"] != run
        || state["projectId"] != project.as_str()
        || state["authorMessageId"] != message
        || history[0].instruction != text
        || sources
            .first()
            .is_none_or(|source| source.id != message || source.generation != 0)
        || state["intentGeneration"].as_u64() != Some((history.len() - 1) as u64)
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let count: usize = db
        .query_row(
            "SELECT COUNT(*) FROM story_author_messages_v1 WHERE generation_run_id=?1",
            [run],
            |row| row.get(0),
        )
        .map_err(storage_error)?;
    if count != sources.len() {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    for source in sources {
        let valid: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM story_author_messages_v1 WHERE message_id=?1 AND project_id=?2 AND work_task_id=?3 AND generation_run_id=?4 AND task_id IS NULL AND thread_id IS NULL AND intent_revision=?5 AND author_text=?6 AND created_at=?7)", params![source.id,project.as_str(),format!("work-{run}"),run,source.generation,source.text,source.created_at], |row|row.get(0)).map_err(storage_error)?;
        if !valid {
            return Err(RepositoryError::TaskBindingInvalid);
        }
    }
    Ok(history)
}

pub(crate) fn validate_update(
    db: &Connection,
    project: &ProjectId,
    run: &str,
    prior: &Value,
    next: &Value,
) -> Result<(), RepositoryError> {
    let old = validate_run(db, project, run, prior)?;
    let new = history(next)?;
    let old_decisions = decisions(prior)?;
    let new_decisions = decisions(next)?;
    if !new.starts_with(&old) || !new_decisions.starts_with(&old_decisions) {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let old_sources = source_messages(db, project, &old, &old_decisions)?;
    let new_sources = source_messages(db, project, &new, &new_decisions)?;
    for source in new_sources.iter().skip(old_sources.len()) {
        insert_message(db, project, run, source)?;
    }
    validate_run(db, project, run, next)?;
    Ok(())
}

pub(crate) fn freeze(
    db: &Connection,
    request: &CreateTaskRequest,
    target: &Value,
    manifest: &mut Value,
) -> Result<(), RepositoryError> {
    let run = target["runId"]
        .as_str()
        .ok_or(RepositoryError::TaskBindingInvalid)?;
    let payload: String = db
        .query_row(
            "SELECT state_json FROM story_generation_runs_v1 WHERE project_id=?1 AND run_id=?2",
            params![request.project_id.as_str(), run],
            |row| row.get(0),
        )
        .optional()
        .map_err(storage_error)?
        .ok_or(RepositoryError::AuthoringRunNotFound)?;
    let state: Value = serde_json::from_str(&payload).map_err(|_| RepositoryError::CorruptState)?;
    let history = validate_run(db, &request.project_id, run, &state)?;
    let binding = Binding {
        run_id: run.into(),
        author_message_id: state["authorMessageId"]
            .as_str()
            .ok_or(RepositoryError::CorruptState)?
            .into(),
        intent_generation: history.len() - 1,
        chapter_number: state["completedChapters"]
            .as_i64()
            .and_then(|chapter| chapter.checked_add(1))
            .ok_or(RepositoryError::CorruptState)?,
    };
    manifest["authorIntent"] = json!(generation_author_intent(&history, binding.chapter_number)?);
    manifest["authorMessageId"] = json!(binding.author_message_id);
    manifest["generationAuthorIntentBinding"] = json!(binding);
    if request.operation == "generateChapter"
        && let Some(prompt) = state["feedbackRepairPrompt"].as_str()
    {
        manifest["candidateInstruction"] = json!(prompt);
    }
    Ok(())
}

pub(crate) fn validate_task(
    db: &Connection,
    task: &TaskRecord,
    manifest: &Value,
) -> Result<(), RepositoryError> {
    let binding: Binding =
        serde_json::from_value(manifest["generationAuthorIntentBinding"].clone())
            .map_err(|_| RepositoryError::TaskBindingInvalid)?;
    let target: Value =
        serde_json::from_str(&task.target_json).map_err(|_| RepositoryError::CorruptState)?;
    if target["runId"] != binding.run_id
        || target["intentGeneration"].as_u64() != Some(binding.intent_generation as u64)
        || manifest["authorMessageId"] != binding.author_message_id
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let owned: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM story_work_attempts_v1 a JOIN story_work_steps_v1 s ON s.id=a.step_id JOIN story_work_tasks_v1 w ON w.id=s.work_task_id WHERE a.attempt_id=?1 AND w.id=?2 AND w.project_id=?3 AND w.kind='generation')",
        params![task.task_id,format!("work-{}",binding.run_id),task.project_id.as_str()],|row|row.get(0)).map_err(storage_error)?;
    if !owned {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let payload: String = db
        .query_row(
            "SELECT state_json FROM story_generation_runs_v1 WHERE project_id=?1 AND run_id=?2",
            params![task.project_id.as_str(), binding.run_id],
            |row| row.get(0),
        )
        .map_err(storage_error)?;
    let state: Value = serde_json::from_str(&payload).map_err(|_| RepositoryError::CorruptState)?;
    let history = validate_run(db, &task.project_id, &binding.run_id, &state)?;
    let chapter = match task.operation.as_str() {
        "generateBrief" | "generateOutline" => state["startChapterNumber"]
            .as_i64()
            .ok_or(RepositoryError::CorruptState)?,
        "generatePlan" => target["planFromChapter"]
            .as_i64()
            .ok_or(RepositoryError::TaskBindingInvalid)?,
        "generateChapter" | "modelKnowledge" | "extractCommitments" | "assessSceneOutcome" => {
            let id = target["chapterId"]
                .as_str()
                .ok_or(RepositoryError::TaskBindingInvalid)?;
            db.query_row(
                "SELECT number FROM story_chapters_v2 WHERE project_id=?1 AND chapter_id=?2",
                params![task.project_id.as_str(), id],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(storage_error)?
            .ok_or(RepositoryError::TaskBindingInvalid)?
        }
        _ => return Err(RepositoryError::TaskBindingInvalid),
    };
    if chapter != binding.chapter_number {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let frozen = history
        .get(..=binding.intent_generation)
        .ok_or(RepositoryError::TaskBindingInvalid)?;
    if state["authorMessageId"] != binding.author_message_id
        || manifest["authorIntent"] != generation_author_intent(frozen, binding.chapter_number)?
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    Ok(())
}
