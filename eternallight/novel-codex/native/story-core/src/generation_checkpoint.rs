use crate::{ProjectId, RepositoryError, SqliteStoryRepository};
use rusqlite::params;

impl SqliteStoryRepository {
    pub fn validate_generation_chapter_checkpoint(
        &self,
        project_id: &ProjectId,
        chapter_id: &str,
        chapter_number: i64,
    ) -> Result<(), RepositoryError> {
        validate_chapter(&self.connection, project_id, chapter_id, chapter_number)
    }
}

pub(crate) fn validate_chapter(
    db: &rusqlite::Connection,
    project_id: &ProjectId,
    chapter_id: &str,
    chapter_number: i64,
) -> Result<(), RepositoryError> {
    let valid: bool = db.query_row(
            "SELECT EXISTS(
                SELECT 1 FROM story_chapters_v2 c
                JOIN story_object_heads_v2 d ON d.project_id=c.project_id AND d.object_id='draft-' || c.chapter_id AND d.object_kind='draft'
                JOIN story_chapter_plan_bindings_v2 b ON b.project_id=c.project_id AND b.chapter_id=c.chapter_id
                JOIN story_object_heads_v2 p ON p.project_id=c.project_id AND p.object_id='artifact-plan'
                WHERE c.project_id=?1 AND c.chapter_id=?2 AND c.number=?3 AND d.locked=0
                  AND COALESCE(d.candidate_revision_id,d.accepted_revision_id) IS NOT NULL
                  AND b.plan_revision_id=COALESCE(p.candidate_revision_id,p.accepted_revision_id)
            ) AND (SELECT COUNT(*) FROM story_chapters_v2 WHERE project_id=?1)=?3
              AND (SELECT MIN(number) FROM story_chapters_v2 WHERE project_id=?1)=1
              AND (SELECT MAX(number) FROM story_chapters_v2 WHERE project_id=?1)=?3
              AND NOT EXISTS(
                SELECT 1 FROM story_chapters_v2 c
                LEFT JOIN story_object_heads_v2 h ON h.project_id=c.project_id AND h.object_id='draft-' || c.chapter_id AND h.object_kind='draft'
                LEFT JOIN story_knowledge_heads_v2 k ON k.project_id=c.project_id AND k.object_id=h.object_id
                LEFT JOIN story_modeling_batches_v2 m ON m.project_id=c.project_id AND m.batch_id=k.batch_id
                WHERE c.project_id=?1 AND c.number<?3 AND
                  (COALESCE(h.candidate_revision_id,h.accepted_revision_id) IS NULL OR m.status IS NOT 'ready' OR m.source_revision_id IS NOT COALESCE(h.candidate_revision_id,h.accepted_revision_id))
              )",
            params![project_id.as_str(),chapter_id,chapter_number], |row| row.get(0),
        ).map_err(crate::authoring_runs::storage_error)?;
    if !valid {
        return Err(RepositoryError::InvalidContent("current chapter requires an unlocked draft, current plan binding and fully modeled preceding chapters".into()));
    }
    Ok(())
}
