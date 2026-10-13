use rusqlite::{OptionalExtension, params};
use serde_json::Value;
use crate::{ProjectId, RevisionId, RepositoryError, SqliteStoryRepository};

/// Public chapter history capability; SQLite retains transaction ownership.
pub trait ChapterHistoryRepository {
    /// Read an immutable chapter baseline only within its exact owning project/object.
    fn chapter_draft_history(
        &self,
        project_id: &ProjectId,
        chapter_id: &str,
        revision_id: &RevisionId,
    ) -> Result<Value, RepositoryError>;
}

impl ChapterHistoryRepository for SqliteStoryRepository {
    /// Read an immutable chapter baseline only within its exact owning project/object.
    fn chapter_draft_history(
        &self,
        project_id: &ProjectId,
        chapter_id: &str,
        revision_id: &RevisionId,
    ) -> Result<Value, RepositoryError> {
        let content: String = self.connection.query_row(
            "SELECT content_json FROM story_revisions_v2 WHERE project_id = ?1 AND object_id = ?2 AND object_kind = 'draft' AND revision_id = ?3",
            params![project_id.as_str(), format!("draft-{chapter_id}"), revision_id.as_str()],
            |row| row.get(0),
        ).optional().map_err(|error| RepositoryError::Storage(error.to_string()))?
            .ok_or(RepositoryError::CandidateNotFound)?;
        let value: Value =
            serde_json::from_str(&content).map_err(|_| RepositoryError::CorruptState)?;
        if value["chapterId"].as_str() != Some(chapter_id) {
            return Err(RepositoryError::CorruptState);
        }
        Ok(value)
    }
}
