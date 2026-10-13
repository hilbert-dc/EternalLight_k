use rusqlite::{Connection, OptionalExtension, params};
use serde_json::Value;

use crate::{ChapterBinding, ProjectId, RepositoryError, SqliteStoryRepository};



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
    let number: i64 = db
        .query_row(
            "SELECT number FROM story_chapters_v2 WHERE project_id=?1 AND chapter_id=?2",
            params![project_id.as_str(), chapter_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(storage_error)?
        .ok_or(RepositoryError::TaskBindingInvalid)?;
    let plan = bound_chapter_plan_for_chapter(db, project_id, chapter_id)?;
    let expected_pov = plan
        .as_ref()
        .map(|p| &p["povCharacterId"])
        .unwrap_or(&Value::Null);
    let expected_time = plan
        .as_ref()
        .map(|p| &p["storyTime"])
        .unwrap_or(&Value::Null);
    let valid_scope = target["scope"] == serde_json::json!({"type":"wholeChapter"})
        || (target["scope"]["type"] == "selectedScenes"
            && crate::scene_revision::selection(target).is_ok());
    if target["chapterNumber"] != number
        || target["readingChapter"] != number
        || target["sceneOrder"] != 1
        || !valid_scope
        || &target["povCharacterId"] != expected_pov
        || &target["storyTime"] != expected_time
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    Ok(())
}

fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BranchId, ProjectRecord, SnapshotId};
    use serde_json::json;

    #[test]
    fn optional_plan_never_hides_a_broken_binding_or_changes_a_bound_position() {
        let mut repo = SqliteStoryRepository::in_memory().unwrap();
        let project = ProjectId::parse("optional-plan-scope").unwrap();
        repo.create_project(
            ProjectRecord {
                project_id: project.clone(),
                title: "白塔".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            BranchId::parse("main").unwrap(),
            SnapshotId::parse("initial").unwrap(),
        )
        .unwrap();
        repo.connection.execute("INSERT INTO story_chapters_v2(chapter_id,project_id,number,title,status,summary) VALUES('chapter-one',?1,1,'来信','draft','')", [project.as_str()]).unwrap();
        let mut target = json!({"chapterId":"chapter-one","chapterNumber":1,"readingChapter":1,
            "sceneOrder":1,"scope":{"type":"wholeChapter"},"povCharacterId":null,"storyTime":null});
        assert!(validate_generation_writer_scope(&repo.connection, &project, &target).is_ok());
        target["scope"] = json!({"type":"selectedScenes","sourceRevisionId":"current-draft","groupIds":["letter"]});
        assert!(validate_generation_writer_scope(&repo.connection, &project, &target).is_ok());
        target["scope"]["groupIds"] = json!(["letter", "letter"]);
        assert_eq!(
            validate_generation_writer_scope(&repo.connection, &project, &target),
            Err(RepositoryError::TaskBindingInvalid)
        );
        target["scope"] = json!({"type":"wholeChapter"});
        target["chapterNumber"] = json!(2);
        assert_eq!(
            validate_generation_writer_scope(&repo.connection, &project, &target),
            Err(RepositoryError::TaskBindingInvalid)
        );
        target["chapterNumber"] = json!(1);
        let plan = json!({"payload":{"chapters":[{"chapterPlanId":"planned-one","chapterNumber":1,"povCharacterId":"lin","storyTime":7}]}});
        repo.connection.execute("INSERT INTO story_revisions_v2(revision_id,project_id,object_id,object_kind,content_digest,content_json) VALUES('bound-plan',?1,'artifact-plan','plan','fixture',?2)", params![project.as_str(), plan.to_string()]).unwrap();
        repo.connection.execute("INSERT INTO story_chapter_plan_bindings_v2(project_id,plan_revision_id,chapter_plan_id,chapter_id,created_at) VALUES(?1,'bound-plan','planned-one','chapter-one',1)", [project.as_str()]).unwrap();
        assert_eq!(
            validate_generation_writer_scope(&repo.connection, &project, &target),
            Err(RepositoryError::TaskBindingInvalid)
        );
        target["povCharacterId"] = json!("lin");
        target["storyTime"] = json!(7);
        assert!(validate_generation_writer_scope(&repo.connection, &project, &target).is_ok());
        repo.connection.execute("UPDATE story_chapter_plan_bindings_v2 SET chapter_plan_id='missing-entry' WHERE project_id=?1", [project.as_str()]).unwrap();
        assert_eq!(
            validate_generation_writer_scope(&repo.connection, &project, &target),
            Err(RepositoryError::CorruptState)
        );
    }

use crate::{ProjectRevisionRepository};
}

/// Public bound writer plan capability; SQLite retains transaction ownership.
pub trait BoundWriterPlanRepository {
/// Reads an immutable chapter plan shared by author preview and Writer execution.
    /// Broken or cross-project bindings never fall back to the current planning head.
    fn bound_chapter_plan(
        &self,
        project_id: &ProjectId,
        binding: &ChapterBinding,
    ) -> Result<Value, RepositoryError>;
}

impl BoundWriterPlanRepository for SqliteStoryRepository {
/// Reads an immutable chapter plan shared by author preview and Writer execution.
    /// Broken or cross-project bindings never fall back to the current planning head.
    fn bound_chapter_plan(
        &self,
        project_id: &ProjectId,
        binding: &ChapterBinding,
    ) -> Result<Value, RepositoryError> {
        read_bound_chapter_plan(&self.connection, project_id, binding)
    }
}
