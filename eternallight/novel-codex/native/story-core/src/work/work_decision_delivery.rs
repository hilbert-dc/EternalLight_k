//! Exact saved chapter evidence. Optional reports remain absent, never implicitly successful.
use crate::authoring_runs::storage_error;
use crate::{GenerationChapterDelivery, ProjectId, RepositoryError};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};

pub(crate) fn checkpoint(
    db: &Connection,
    project: &ProjectId,
    run: &str,
    state: &mut Value,
) -> Result<(), RepositoryError> {
    let work = format!("work-{run}");
    let chapter = state["currentChapterId"]
        .as_str()
        .ok_or(RepositoryError::TaskBindingInvalid)?
        .to_owned();
    let number = state["currentChapterNumber"]
        .as_i64()
        .ok_or(RepositoryError::TaskBindingInvalid)?;
    let expected = state["completedChapters"]
        .as_i64()
        .ok_or(RepositoryError::CorruptState)?
        + 1;
    if number != expected {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let parent = crate::tasks::load_task(
        db,
        state["lastTaskId"]
            .as_str()
            .ok_or(RepositoryError::TaskBindingInvalid)?,
    )?;
    if !crate::work_decision_generation::completion_selected(db, &parent)? {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    if state["targetChapters"] == number {
        let reviewed: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM story_work_delivery_reviews_v1 WHERE work_task_id=?1 AND decision_task_id=?2)", params![work, parent.task_id], |row| row.get(0)).map_err(storage_error)?;
        if !reviewed {
            return Err(RepositoryError::TaskOutputMissing);
        }
    }
    let unsettled:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM story_task_provider_budgets_v1 WHERE work_task_id=?1 AND settled=0) OR EXISTS(SELECT 1 FROM story_work_steps_v1 WHERE work_task_id=?1 AND status IN ('queued','running','blocked'))",[&work],|r|r.get(/*idx*/0)).map_err(storage_error)?;
    if unsettled {
        return Err(RepositoryError::TaskNotWritable);
    }
    crate::generation_checkpoint::validate_chapter(db, project, &chapter, number)?;
    let (draft,plan,batch):(String,Option<String>,Option<String>)=db.query_row("SELECT COALESCE(d.candidate_revision_id,d.accepted_revision_id),p.plan_revision_id,k.batch_id FROM story_object_heads_v2 d LEFT JOIN story_chapter_plan_bindings_v2 p ON p.project_id=d.project_id AND d.object_id='draft-'||p.chapter_id LEFT JOIN story_knowledge_heads_v2 h ON h.project_id=d.project_id AND h.object_id=d.object_id LEFT JOIN story_modeling_batches_v2 k ON k.project_id=h.project_id AND k.batch_id=h.batch_id AND k.source_revision_id=COALESCE(d.candidate_revision_id,d.accepted_revision_id) AND k.status='ready' WHERE d.project_id=?1 AND d.object_id='draft-'||?2",params![project.as_str(),chapter],|r|Ok((r.get(/*idx*/0)?,r.get(/*idx*/1)?,r.get(/*idx*/2)?))).optional().map_err(storage_error)?.ok_or(RepositoryError::TaskOutputMissing)?;
    let writer:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM story_task_outputs_v2 o JOIN story_tasks_v2 t ON t.task_id=o.task_id AND t.status='completed' JOIN story_work_attempts_v1 a ON a.attempt_id=t.task_id JOIN story_work_steps_v1 s ON s.id=a.step_id WHERE s.work_task_id=?1 AND o.revision_id=?2 AND o.output_kind='draft' AND o.completeness='chapterComplete')",params![work,draft],|r|r.get(/*idx*/0)).map_err(storage_error)?;
    if !writer {
        return Err(RepositoryError::TaskOutputMissing);
    }
    let (requirements, _) = crate::work_decision_status::actions(db, project, &work)?;
    if requirements
        .iter()
        .any(|r| r["status"] == "pending" && r["kind"] != "chapterDelivery")
    {
        return Err(RepositoryError::TaskOutputMissing);
    }
    let assessment_state = scene_feedback(db, project, &work, &chapter)?;
    if assessment_state.as_ref().is_some_and(|a| a.needs_recheck()) {
        return Err(RepositoryError::InvalidContent(
            "chapterAssessmentNeedsRecheck".into(),
        ));
    }
    let assessment = assessment_state
        .filter(|a| a.current)
        .map(|a| (a.revision, a.content.to_string(), a.task_id));
    let mut feedback = None;
    if let Some((revision, content, _)) = &assessment {
        let content: Value =
            serde_json::from_str(content).map_err(|_| RepositoryError::CorruptState)?;
        if content["assessments"]
            .as_array()
            .ok_or(RepositoryError::CorruptState)?
            .iter()
            .any(|v| v["verdict"] != "realized" || v["recommendation"] != "continue")
        {
            feedback = state["feedbackDecisions"]
                .as_array()
                .ok_or(RepositoryError::CorruptState)?
                .iter()
                .rev()
                .find(|d| d["choice"] == "continue" && d["assessmentRevisionId"] == *revision)
                .and_then(|d| d["id"].as_str())
                .map(str::to_owned);
            if feedback.is_none() {
                return Err(RepositoryError::InvalidContent(
                    "chapterAssessmentNeedsAuthorDecision".into(),
                ));
            }
        }
    }
    let extraction:Option<String>=db.query_row("SELECT r.revision_id FROM story_object_heads_v2 h JOIN story_revisions_v2 r ON r.project_id=h.project_id AND r.revision_id=h.candidate_revision_id JOIN story_task_outputs_v2 o ON o.revision_id=r.revision_id JOIN story_tasks_v2 t ON t.task_id=o.task_id AND t.status='completed' JOIN story_work_attempts_v1 a ON a.attempt_id=t.task_id JOIN story_work_steps_v1 s ON s.id=a.step_id AND s.work_task_id=?1 WHERE h.project_id=?2 AND h.object_id='commitment-extraction-'||?3 AND json_extract(r.content_json,'$.payload.sourceRevisionId')=?4",params![work,project.as_str(),chapter,draft],|r|r.get(/*idx*/0)).optional().map_err(storage_error)?;
    let snapshot:String=db.query_row("SELECT head_snapshot_id FROM story_branches_v2 WHERE project_id=?1 AND branch_id='main'",[project.as_str()],|r|r.get(/*idx*/0)).map_err(storage_error)?;
    let delivery = GenerationChapterDelivery {
        chapter_id: chapter,
        chapter_number: number,
        draft_revision_id: draft,
        plan_revision_id: plan,
        knowledge_batch_id: batch,
        commitment_extraction_revision_id: extraction,
        assessment_revision_id: assessment.as_ref().map(|a| a.0.clone()),
        assessment_task_id: assessment.map(|a| a.2),
        checked_snapshot_id: snapshot,
        source_digest: crate::task_input::project_source_digest(db, project)?,
        checked_run_version: state["version"]
            .as_i64()
            .ok_or(RepositoryError::CorruptState)?,
        feedback_decision_id: feedback,
    };
    state["chapterDeliveries"]
        .as_array_mut()
        .ok_or(RepositoryError::CorruptState)?
        .push(json!(delivery));
    state["completedChapters"] = json!(number);
    state["currentChapterId"] = Value::Null;
    state["currentChapterNumber"] = Value::Null;
    if state["targetChapters"] == number {
        state["status"] = json!("completed");
        state["stage"] = json!("finished");
        db.execute(
            "UPDATE story_work_decisions_v1 SET finished=1 WHERE work_task_id=?1",
            [&work],
        )
        .map_err(storage_error)?;
        db.execute(
            "UPDATE story_work_tasks_v1 SET status='completed',version=version+1 WHERE id=?1",
            [work],
        )
        .map_err(storage_error)?;
    }
    Ok(())
}

/// A saved negative assessment remains unresolved after an edit until a source-bound
/// recheck supersedes it. An absent optional assessment creates no new obligation.
pub(crate) struct SceneFeedback {
    pub revision: String,
    pub content: Value,
    pub task_id: String,
    pub current: bool,
    pub unresolved: bool,
}
impl SceneFeedback {
    pub(crate) fn needs_recheck(&self) -> bool {
        self.unresolved && !self.current
    }
}

pub(crate) fn scene_feedback(
    db: &Connection,
    project: &ProjectId,
    work: &str,
    chapter: &str,
) -> Result<Option<SceneFeedback>, RepositoryError> {
    let saved: Option<(String,String,String)> = db.query_row(
        "SELECT r.revision_id,r.content_json,t.task_id FROM story_object_heads_v2 h
         JOIN story_revisions_v2 r ON r.project_id=h.project_id AND r.revision_id=COALESCE(h.candidate_revision_id,h.accepted_revision_id)
         JOIN story_task_outputs_v2 o ON o.revision_id=r.revision_id
         JOIN story_tasks_v2 t ON t.task_id=o.task_id AND t.status='completed'
         JOIN story_work_attempts_v1 a ON a.attempt_id=t.task_id
         JOIN story_work_steps_v1 s ON s.id=a.step_id AND s.work_task_id=?1
         WHERE h.project_id=?2 AND h.object_id='scene-assessment-'||?3",
        params![work,project.as_str(),chapter], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?)),
    ).optional().map_err(storage_error)?;
    let Some((revision, encoded, task_id)) = saved else {
        return Ok(None);
    };
    let content: Value =
        serde_json::from_str(&encoded).map_err(|_| RepositoryError::CorruptState)?;
    let (draft, plan, batch): (Option<String>,Option<String>,Option<String>) = db.query_row(
        "SELECT COALESCE(d.candidate_revision_id,d.accepted_revision_id),p.plan_revision_id,k.batch_id
         FROM story_object_heads_v2 d
         LEFT JOIN story_chapter_plan_bindings_v2 p ON p.project_id=d.project_id AND d.object_id='draft-'||p.chapter_id
         LEFT JOIN story_knowledge_heads_v2 h ON h.project_id=d.project_id AND h.object_id=d.object_id
         LEFT JOIN story_modeling_batches_v2 k ON k.project_id=h.project_id AND k.batch_id=h.batch_id
          AND k.source_revision_id=COALESCE(d.candidate_revision_id,d.accepted_revision_id) AND k.status='ready'
         WHERE d.project_id=?1 AND d.object_id='draft-'||?2",
        params![project.as_str(),chapter],|r| Ok((r.get(0)?,r.get(1)?,r.get(2)?)),
    ).optional().map_err(storage_error)?.ok_or(RepositoryError::CorruptState)?;
    let number: i64 = db
        .query_row(
            "SELECT number FROM story_chapters_v2 WHERE project_id=?1 AND chapter_id=?2",
            params![project.as_str(), chapter],
            |r| r.get(0),
        )
        .map_err(storage_error)?;
    let current = feedback_sources_current(
        &content,
        draft.as_deref(),
        plan.as_deref(),
        batch.as_deref(),
    ) && crate::commitment_assessment::inputs_current(db, project, number, &content)?;
    let unresolved = content["assessments"]
        .as_array()
        .ok_or(RepositoryError::CorruptState)?
        .iter()
        .any(|a| a["verdict"] != "realized" || a["recommendation"] != "continue");
    Ok(Some(SceneFeedback {
        revision,
        content,
        task_id,
        current,
        unresolved,
    }))
}

fn feedback_sources_current(
    content: &Value,
    draft: Option<&str>,
    plan: Option<&str>,
    batch: Option<&str>,
) -> bool {
    content["draftRevisionId"]
        .as_str()
        .is_some_and(|id| Some(id) == draft)
        && content["planRevisionId"].as_str() == plan
        && content["knowledgeBatchId"].as_str() == batch
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_report_survives_a_rewrite_until_a_current_recheck_replaces_it() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE story_object_heads_v2(project_id TEXT,object_id TEXT,candidate_revision_id TEXT,accepted_revision_id TEXT);
            CREATE TABLE story_revisions_v2(project_id TEXT,revision_id TEXT,content_json TEXT);
            CREATE TABLE story_task_outputs_v2(revision_id TEXT,task_id TEXT);
            CREATE TABLE story_tasks_v2(task_id TEXT,status TEXT);
            CREATE TABLE story_work_attempts_v1(attempt_id TEXT,step_id TEXT);
            CREATE TABLE story_work_steps_v1(id TEXT,work_task_id TEXT);
            CREATE TABLE story_chapter_plan_bindings_v2(project_id TEXT,chapter_id TEXT,plan_revision_id TEXT);
            CREATE TABLE story_knowledge_heads_v2(project_id TEXT,object_id TEXT,batch_id TEXT);
            CREATE TABLE story_modeling_batches_v2(project_id TEXT,batch_id TEXT,source_revision_id TEXT,status TEXT);
            CREATE TABLE story_chapters_v2(project_id TEXT,chapter_id TEXT,number INTEGER);
            INSERT INTO story_chapters_v2 VALUES('project','chapter',1);
            INSERT INTO story_object_heads_v2 VALUES('project','draft-chapter','draft-1',NULL);
            INSERT INTO story_chapter_plan_bindings_v2 VALUES('project','chapter','plan-1');
            INSERT INTO story_knowledge_heads_v2 VALUES('project','draft-chapter','batch-1');
            INSERT INTO story_modeling_batches_v2 VALUES('project','batch-1','draft-1','ready');
            INSERT INTO story_tasks_v2 VALUES('check-1','completed');
            INSERT INTO story_work_attempts_v1 VALUES('check-1','step-1');
            INSERT INTO story_work_steps_v1 VALUES('step-1','work-run');").unwrap();
        let project = ProjectId::parse("project").unwrap();
        assert!(
            scene_feedback(&db, &project, "work-run", "chapter")
                .unwrap()
                .is_none()
        );
        let report = json!({"draftRevisionId":"draft-1","planRevisionId":"plan-1","knowledgeBatchId":"batch-1",
            "assessments":[{"verdict":"deviated","recommendation":"repairDraft","effectAssessments":[]}]});
        db.execute(
            "INSERT INTO story_revisions_v2 VALUES('project','report-1',?1)",
            [report.to_string()],
        )
        .unwrap();
        db.execute_batch("INSERT INTO story_object_heads_v2 VALUES('project','scene-assessment-chapter','report-1',NULL);
            INSERT INTO story_task_outputs_v2 VALUES('report-1','check-1');").unwrap();
        assert!(
            scene_feedback(&db, &project, "other-work", "chapter")
                .unwrap()
                .is_none()
        );
        let initial = scene_feedback(&db, &project, "work-run", "chapter")
            .unwrap()
            .unwrap();
        assert!(initial.current && initial.unresolved);
        db.execute("UPDATE story_object_heads_v2 SET candidate_revision_id='draft-2' WHERE object_id='draft-chapter'",[]).unwrap();
        let stale = scene_feedback(&db, &project, "work-run", "chapter")
            .unwrap()
            .unwrap();
        assert_eq!(stale.revision, "report-1");
        assert!(stale.needs_recheck());
        // A real new source-bound report, not editing alone, resolves the pending finding.
        let checked = json!({"draftRevisionId":"draft-2","planRevisionId":"plan-1","knowledgeBatchId":"batch-2",
            "assessments":[{"verdict":"realized","recommendation":"continue","effectAssessments":[]}]});
        db.execute(
            "INSERT INTO story_revisions_v2 VALUES('project','report-2',?1)",
            [checked.to_string()],
        )
        .unwrap();
        db.execute_batch("INSERT INTO story_tasks_v2 VALUES('check-2','completed');
            INSERT INTO story_work_attempts_v1 VALUES('check-2','step-2');
            INSERT INTO story_work_steps_v1 VALUES('step-2','work-run');
            INSERT INTO story_task_outputs_v2 VALUES('report-2','check-2');
            INSERT INTO story_modeling_batches_v2 VALUES('project','batch-2','draft-2','ready');
            UPDATE story_knowledge_heads_v2 SET batch_id='batch-2';
            UPDATE story_object_heads_v2 SET candidate_revision_id='report-2' WHERE object_id='scene-assessment-chapter';").unwrap();
        let rechecked = scene_feedback(&db, &project, "work-run", "chapter")
            .unwrap()
            .unwrap();
        assert!(rechecked.current && !rechecked.unresolved && !rechecked.needs_recheck());
        assert_eq!(rechecked.task_id, "check-2");
    }

    #[test]
    fn saved_negative_findings_need_recheck_after_any_bound_source_changes() {
        let report = json!({"draftRevisionId":"draft-1","planRevisionId":"plan-1",
            "knowledgeBatchId":"batch-1","assessments":[{"verdict":"deviated","recommendation":"repairDraft"}]});
        for (draft, plan, batch) in [
            (Some("draft-2"), Some("plan-1"), Some("batch-1")),
            (Some("draft-1"), Some("plan-2"), Some("batch-1")),
            (Some("draft-1"), Some("plan-1"), None),
        ] {
            let feedback = SceneFeedback {
                revision: "report-1".into(),
                task_id: "check-1".into(),
                current: feedback_sources_current(&report, draft, plan, batch),
                content: report.clone(),
                unresolved: true,
            };
            assert!(feedback.needs_recheck());
        }
        let current = SceneFeedback {
            revision: "report-1".into(),
            task_id: "check-1".into(),
            current: feedback_sources_current(
                &report,
                Some("draft-1"),
                Some("plan-1"),
                Some("batch-1"),
            ),
            content: report,
            unresolved: true,
        };
        assert!(!current.needs_recheck());
        assert!(current.unresolved); // Current adverse evidence still requires a real decision.
        let repaired = json!({"draftRevisionId":"draft-2","planRevisionId":"plan-1","knowledgeBatchId":"batch-2"});
        assert!(feedback_sources_current(
            &repaired,
            Some("draft-2"),
            Some("plan-1"),
            Some("batch-2")
        ));
        let nullable =
            json!({"draftRevisionId":"draft-2","planRevisionId":null,"knowledgeBatchId":null});
        assert!(feedback_sources_current(
            &nullable,
            Some("draft-2"),
            None,
            None
        ));
        assert!(!feedback_sources_current(
            &nullable,
            Some("draft-1"),
            None,
            None
        ));
    }
}
