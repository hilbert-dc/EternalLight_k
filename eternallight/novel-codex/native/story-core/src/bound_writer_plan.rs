use rusqlite::{Connection, OptionalExtension, params};
use serde_json::Value;

use crate::{ChapterBinding, ProjectId, RepositoryError, SqliteStoryRepository};

impl SqliteStoryRepository {
    /// Reads an immutable chapter plan shared by author preview and Writer execution.
    /// Broken or cross-project bindings never fall back to the current planning head.
    pub fn bound_chapter_plan(
        &self,
        project_id: &ProjectId,
        binding: &ChapterBinding,
    ) -> Result<Value, RepositoryError> {
        read_bound_chapter_plan(&self.connection, project_id, binding)
    }
}

fn read_bound_chapter_plan(
    db: &Connection,
    project_id: &ProjectId,
    binding: &ChapterBinding,
) -> Result<Value, RepositoryError> {
    let content: String = db.query_row(
        "SELECT content_json FROM story_revisions_v2 WHERE project_id=?1 AND revision_id=?2 AND object_id='artifact-plan' AND object_kind='plan'",
        params![project_id.as_str(), binding.plan_revision_id], |row| row.get(0),
    ).optional().map_err(storage_error)?.ok_or(RepositoryError::CorruptState)?;
    let plan: Value = serde_json::from_str(&content).map_err(|_| RepositoryError::CorruptState)?;
    let chapters = plan["payload"]["chapters"]
        .as_array()
        .ok_or(RepositoryError::CorruptState)?;
    let mut matching = chapters
        .iter()
        .filter(|chapter| chapter["chapterPlanId"] == binding.chapter_plan_id);
    let chapter = matching.next().ok_or(RepositoryError::CorruptState)?;
    if chapter["chapterNumber"] != binding.chapter_number || matching.next().is_some() {
        return Err(RepositoryError::CorruptState);
    }
    validate_plan_position(chapter)?;
    Ok(chapter.clone())
}

pub(crate) fn bound_chapter_plan_for_chapter(
    db: &Connection,
    project_id: &ProjectId,
    chapter_id: &str,
) -> Result<Option<Value>, RepositoryError> {
    let binding = db.query_row(
        "SELECT b.plan_revision_id,b.chapter_plan_id,c.number FROM story_chapter_plan_bindings_v2 b
         JOIN story_chapters_v2 c ON c.project_id=b.project_id AND c.chapter_id=b.chapter_id
         WHERE b.project_id=?1 AND b.chapter_id=?2",
        params![project_id.as_str(),chapter_id], |row| Ok(ChapterBinding {
            plan_revision_id:row.get(0)?,chapter_plan_id:row.get(1)?,chapter_id:chapter_id.to_owned(),
            chapter_number:row.get(2)?,result_revision:0
        }),
    ).optional().map_err(storage_error)?;
    binding
        .as_ref()
        .map(|binding| read_bound_chapter_plan(db, project_id, binding))
        .transpose()
}

pub(crate) fn validate_plan_position(chapter: &Value) -> Result<(), RepositoryError> {
    if (!chapter["povCharacterId"].is_null()
        && chapter["povCharacterId"]
            .as_str()
            .is_none_or(|id| id.trim().is_empty() || id.len() > 160))
        || (!chapter["storyTime"].is_null() && chapter["storyTime"].as_i64().is_none())
    {
        return Err(RepositoryError::CorruptState);
    }
    Ok(())
}

pub(crate) fn validate_generation_writer_scope(
    db: &Connection,
    project_id: &ProjectId,
    target: &Value,
) -> Result<(), RepositoryError> {
    let chapter_id = target["chapterId"]
        .as_str()
        .ok_or(RepositoryError::TaskBindingInvalid)?;
    let plan = bound_chapter_plan_for_chapter(db, project_id, chapter_id)?
        .ok_or(RepositoryError::TaskBindingInvalid)?;
    if target["chapterNumber"] != plan["chapterNumber"]
        || target["readingChapter"] != plan["chapterNumber"]
        || target["sceneOrder"] != 1
        || target["scope"] != serde_json::json!({"type":"wholeChapter"})
        || target["povCharacterId"] != plan["povCharacterId"]
        || target["storyTime"] != plan["storyTime"]
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    Ok(())
}

fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}
