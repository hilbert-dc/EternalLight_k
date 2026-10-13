use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};

use crate::{CreateTaskRequest, ProjectId, RepositoryError, SqliteStoryRepository, TaskRecord};

pub(crate) const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS story_chapter_structure_changes_v1 (
  change_id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES story_projects_v2(project_id) ON DELETE CASCADE,
  summary_json TEXT NOT NULL,
  initial_task_id TEXT UNIQUE REFERENCES story_tasks_v2(task_id),
  assessment_state TEXT NOT NULL CHECK(assessment_state IN ('pending','skipped','bound'))
);
CREATE INDEX IF NOT EXISTS chapter_structure_project ON story_chapter_structure_changes_v1(project_id,change_id);
"#;

/// Capture references before current authority is removed; never copy or rewrite the source.
pub(crate) fn capture(
    db: &Connection,
    project: &ProjectId,
    chapter: &str,
    number: i64,
    title: &str,
    base: &str,
    result: &str,
    revision: i64,
    now: i64,
) -> Result<String, RepositoryError> {
    let object = format!("draft-{chapter}");
    let heads: Option<(Option<String>,Option<String>)> = db.query_row(
        "SELECT candidate_revision_id,accepted_revision_id FROM story_object_heads_v2 WHERE project_id=?1 AND object_id=?2 AND object_kind='draft'",
        params![project.as_str(),object], |r| Ok((r.get(/*idx*/ 0)?,r.get(/*idx*/ 1)?)),
    ).optional().map_err(storage)?;
    let (candidate, accepted) = heads.unwrap_or_default();
    let saved = candidate.or_else(|| accepted.clone());
    let working: Option<String> = db.query_row(
        "SELECT revision_id FROM story_working_baselines_v2 WHERE project_id=?1 AND object_kind='draft' AND object_id=?2",
        params![project.as_str(),chapter], |r| r.get(/*idx*/ 0),
    ).optional().map_err(storage)?;
    let batch: Option<String> = db.query_row(
        "SELECT b.batch_id FROM story_knowledge_heads_v2 k JOIN story_modeling_batches_v2 b ON b.batch_id=k.batch_id AND b.project_id=k.project_id AND b.object_id=k.object_id WHERE k.project_id=?1 AND k.object_id=?2 AND b.source_revision_id=?3 AND b.status='ready'",
        params![project.as_str(),object,saved], |r| r.get(/*idx*/ 0),
    ).optional().map_err(storage)?;
    let mut has_text = false;
    for id in [&saved, &accepted, &working].into_iter().flatten() {
        let content = source_content(db, project, &object, id)?;
        has_text |= content["body"]
            .as_str()
            .ok_or(RepositoryError::CorruptState)?
            .chars()
            .any(|c| !c.is_whitespace());
    }
    let neighbor = |comparison: &str, order: &str| -> Result<Value, RepositoryError> {
        db.query_row(&format!("SELECT chapter_id,number FROM story_chapters_v2 WHERE project_id=?1 AND number {comparison} ?2 ORDER BY number {order} LIMIT 1"),
            params![project.as_str(),number], |r| Ok(json!({"id":r.get::<_,String>(/*idx*/ 0)?,"number":r.get::<_,i64>(/*idx*/ 1)?})))
            .optional().map_err(storage).map(|row| row.unwrap_or(Value::Null))
    };
    let id = format!("structure-{}", uuid::Uuid::new_v4().simple());
    let summary = json!({"id":id,"projectId":project.as_str(),"kind":"chapterDeleted",
        "chapterId":chapter,"chapterNumber":number,"title":title,"createdAt":now,
        "baseRevision":revision,"resultRevision":revision.checked_add(1).ok_or(RepositoryError::CorruptState)?,
        "baseSnapshotId":base,"resultSnapshotId":result,
        "source":{"objectId":object,"savedRevisionId":saved,"acceptedRevisionId":accepted,"workingRevisionId":working,"knowledgeBatchId":batch},
        "previousChapter":neighbor("<","DESC")?,"nextChapter":neighbor(">","ASC")?});
    db.execute("INSERT INTO story_chapter_structure_changes_v1(change_id,project_id,summary_json,assessment_state) VALUES(?1,?2,?3,?4)",
        params![id,project.as_str(),summary.to_string(),if has_text || batch.is_some() || db.query_row("SELECT EXISTS(SELECT 1 FROM story_chapter_plan_bindings_v2 WHERE chapter_id=?1) OR EXISTS(SELECT 1 FROM story_object_heads_v2 WHERE project_id=?2 AND object_id=?3 AND COALESCE(candidate_revision_id,accepted_revision_id) IS NOT NULL)",params![chapter,project.as_str(),format!("commitment-extraction-{chapter}")],|r|r.get::<_,bool>(/*idx*/ 0)).map_err(storage)? {"pending"} else {"skipped"}]).map_err(storage)?;
    Ok(id)
}

pub(crate) fn source_content(
    db: &Connection,
    project: &ProjectId,
    object: &str,
    revision: &str,
) -> Result<Value, RepositoryError> {
    let raw: String = db.query_row("SELECT content_json FROM story_revisions_v2 WHERE project_id=?1 AND object_id=?2 AND object_kind='draft' AND revision_id=?3",
        params![project.as_str(),object,revision], |r| r.get(/*idx*/ 0)).optional().map_err(storage)?.ok_or(RepositoryError::CorruptState)?;
    serde_json::from_str(&raw).map_err(|_| RepositoryError::CorruptState)
}

pub(crate) fn read(
    db: &Connection,
    project: &ProjectId,
    id: &str,
) -> Result<Value, RepositoryError> {
    let (raw,task,state): (String,Option<String>,String) = db.query_row(
        "SELECT summary_json,initial_task_id,assessment_state FROM story_chapter_structure_changes_v1 WHERE project_id=?1 AND change_id=?2",
        params![project.as_str(),id], |r| Ok((r.get(/*idx*/ 0)?,r.get(/*idx*/ 1)?,r.get(/*idx*/ 2)?)),
    ).optional().map_err(storage)?.ok_or(RepositoryError::TaskBindingInvalid)?;
    let mut summary: Value =
        serde_json::from_str(&raw).map_err(|_| RepositoryError::CorruptState)?;
    summary["assessment"] = json!({"state":state,"skipReason":if state=="skipped" {Some("noSavedEvidence")} else {None},
        "workTaskId":task.as_ref().map(|id|format!("work-{id}")),"taskId":task});
    Ok(summary)
}

pub(crate) fn validate(
    db: &Connection,
    request: &CreateTaskRequest,
) -> Result<(), RepositoryError> {
    let target: Value = serde_json::from_str(&request.target_json)
        .map_err(|_| RepositoryError::TaskBindingInvalid)?;
    let fields = target
        .as_object()
        .ok_or(RepositoryError::TaskBindingInvalid)?;
    if target["type"] != "project"
        || fields
            .keys()
            .any(|key| !matches!(key.as_str(), "type" | "structureChangeId" | "resumeStepId"))
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let id = target["structureChangeId"]
        .as_str()
        .filter(|id| !id.is_empty() && id.len() <= 160)
        .ok_or(RepositoryError::TaskBindingInvalid)?;
    let event = read(db, &request.project_id, id)?;
    if let Some(step) = target["resumeStepId"].as_str() {
        let owned: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM story_work_steps_v1 s JOIN story_work_tasks_v1 w ON w.id=s.work_task_id WHERE s.id=?1 AND s.work_task_id=?2 AND w.project_id=?4 AND s.operation='assessChapterStructure' AND json_extract(target_json,'$.structureChangeId')=?3)",
            params![step,event["assessment"]["workTaskId"].as_str(),id,request.project_id.as_str()],|r|r.get(/*idx*/ 0)).map_err(storage)?;
        if !owned {
            return Err(RepositoryError::TaskBindingInvalid);
        }
    } else if event["assessment"]["state"] != "pending" {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    Ok(())
}

pub(crate) fn freeze(
    db: &Connection,
    request: &CreateTaskRequest,
    manifest: &mut Value,
) -> Result<(), RepositoryError> {
    if manifest.get("chapterStructureChange").is_some() {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let target: Value = serde_json::from_str(&request.target_json)
        .map_err(|_| RepositoryError::TaskBindingInvalid)?;
    let mut event = read(
        db,
        &request.project_id,
        target["structureChangeId"]
            .as_str()
            .ok_or(RepositoryError::TaskBindingInvalid)?,
    )?;
    event
        .as_object_mut()
        .ok_or(RepositoryError::CorruptState)?
        .remove("assessment");
    if event.to_string().len() > 4096 {
        return Err(RepositoryError::InvalidContent(
            "structure_metadata_budget_exceeded".into(),
        ));
    }
    manifest["chapterStructureChange"] = event;
    Ok(())
}

pub(crate) fn bind_initial(
    db: &Connection,
    request: &CreateTaskRequest,
) -> Result<(), RepositoryError> {
    let target: Value = serde_json::from_str(&request.target_json)
        .map_err(|_| RepositoryError::TaskBindingInvalid)?;
    let changed = db.execute("UPDATE story_chapter_structure_changes_v1 SET initial_task_id=?1,assessment_state='bound' WHERE project_id=?2 AND change_id=?3 AND initial_task_id IS NULL AND assessment_state='pending'",
        params![request.task_id,request.project_id.as_str(),target["structureChangeId"].as_str()]).map_err(storage)?;
    if changed != 1 {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    Ok(())
}

pub(crate) fn assert_task(db: &Connection, task: &TaskRecord) -> Result<Value, RepositoryError> {
    if task.operation != "assessChapterStructure" {
        return Err(RepositoryError::TaskOutputForbidden);
    }
    let target: Value =
        serde_json::from_str(&task.target_json).map_err(|_| RepositoryError::CorruptState)?;
    let event = read(
        db,
        &task.project_id,
        target["structureChangeId"]
            .as_str()
            .ok_or(RepositoryError::TaskBindingInvalid)?,
    )?;
    let owned: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM story_work_attempts_v1 a JOIN story_work_steps_v1 s ON s.id=a.step_id JOIN story_work_tasks_v1 w ON w.id=s.work_task_id WHERE a.attempt_id=?1 AND w.project_id=?4 AND s.work_task_id=?2 AND s.operation='assessChapterStructure' AND json_extract(s.target_json,'$.structureChangeId')=?3)",
        params![task.task_id,event["assessment"]["workTaskId"].as_str(),event["id"].as_str(),task.project_id.as_str()],|r|r.get(/*idx*/ 0)).map_err(storage)?;
    if !owned || event["assessment"]["state"] != "bound" {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    Ok(event)
}


fn storage(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}

/// Public chapter structure capability; SQLite retains transaction ownership.
pub trait ChapterStructureRepository {
    fn chapter_structure_change(
        &self,
        project: &ProjectId,
        id: &str,
    ) -> Result<Value, RepositoryError>;
    fn chapter_structure_changes(
        &self,
        project: &ProjectId,
        cursor: Option<&str>,
        limit: usize,
        assessment_state: Option<&str>,
    ) -> Result<Value, RepositoryError>;
}

impl ChapterStructureRepository for SqliteStoryRepository {
    fn chapter_structure_change(
        &self,
        project: &ProjectId,
        id: &str,
    ) -> Result<Value, RepositoryError> {
        self.with_read_snapshot(|| read(&self.connection, project, id))?
    }

    fn chapter_structure_changes(
        &self,
        project: &ProjectId,
        cursor: Option<&str>,
        limit: usize,
        assessment_state: Option<&str>,
    ) -> Result<Value, RepositoryError> {
        if !(1..=20).contains(&limit)
            || cursor.is_some_and(|id| id.len() > 180)
            || assessment_state.is_some_and(|state| state != "pending")
        {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        self.with_read_snapshot(||{
            let filter=assessment_state.unwrap_or("all");
            let after=match cursor {None=>"",Some(cursor)=>{let (scope,id)=cursor.split_once(':').ok_or(RepositoryError::TaskBindingInvalid)?;if scope!=filter {return Err(RepositoryError::TaskBindingInvalid);}id}};
            let mut statement=self.connection.prepare("SELECT change_id FROM story_chapter_structure_changes_v1 WHERE project_id=?1 AND change_id>?2 AND (?4='all' OR assessment_state=?4) ORDER BY change_id LIMIT ?3").map_err(storage)?;
            let ids=statement.query_map(params![project.as_str(),after,limit+1,filter],|r|r.get::<_,String>(/*idx*/ 0)).map_err(storage)?.collect::<Result<Vec<_>,_>>().map_err(storage)?;
            let next=if ids.len()>limit {ids.get(limit-1).map(|id|format!("{filter}:{id}"))} else {None};
            let items=ids.into_iter().take(limit).map(|id|read(&self.connection,project,&id)).collect::<Result<Vec<_>,_>>()?;
            Ok(json!({"projectId":project.as_str(),"items":items,"nextCursor":next}))
        })?
    }
}

use crate::ReadSnapshotRepository;
