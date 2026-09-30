use novel_codex_story_core::{ProjectId, RepositoryError, SqliteStoryRepository};
use serde_json::{Value, json};

use crate::{GenerationRun, effective_intent};

pub(crate) fn action(
    repository: &SqliteStoryRepository,
    project: &ProjectId,
    run: &GenerationRun,
) -> Result<Value, RepositoryError> {
    let chapter = run
        .current_chapter_id
        .as_deref()
        .ok_or(RepositoryError::CorruptState)?;
    let knowledge = repository.chapter_knowledge(project, chapter)?;
    if knowledge["status"] != "ready" || !knowledge["sourceRevisionId"].is_string() {
        return Err(RepositoryError::TaskInputStale);
    }
    Ok(json!({"kind":"agentTask","operation":"extractCommitments",
        "target":{"type":"commitmentExtraction","runId":run.id,"stepToken":run.step_token,
            "runVersion":run.version,"intentGeneration":run.intent_generation,
            "chapterId":chapter,"chapterNumber":run.current_chapter_number,
            "sourceRevisionId":knowledge["sourceRevisionId"]},
        "authorIntent":effective_intent(run,run.completed_chapters+1)?}))
}
