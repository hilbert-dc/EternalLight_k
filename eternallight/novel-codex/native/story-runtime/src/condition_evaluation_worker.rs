use novel_codex_story_core::{RepositoryError, StoryDatabase};

/// Advance one persisted condition job without creating a model task or service.
pub fn condition_evaluation_tick(database: &StoryDatabase) -> Result<bool, RepositoryError> {
    database.repository()?.condition_evaluation_tick()
}

use novel_codex_story_core::{ConditionEvaluationWorkerRepository};
