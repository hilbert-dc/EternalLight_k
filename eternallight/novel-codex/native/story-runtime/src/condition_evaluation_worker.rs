use std::path::Path;

use novel_codex_story_core::{RepositoryError, SqliteStoryRepository};

/// Advance one persisted condition job without creating a model task or service.
pub fn condition_evaluation_tick(path: &Path) -> Result<bool, RepositoryError> {
    SqliteStoryRepository::open(path)?.condition_evaluation_tick()
}
