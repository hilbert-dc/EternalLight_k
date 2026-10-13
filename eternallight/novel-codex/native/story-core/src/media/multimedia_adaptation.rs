//! New adaptation editions are dependent steps of the original Work, never new budgets.
use super::*;
use crate::{CreateTaskRequest, TaskRecord};
use serde::Deserialize;

#[derive(Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RequestMediaAdaptation {
    pub medium: CreativeMedium,
    pub title: String,
    pub sources: Vec<MediaSourceRef>,
    pub instruction: String,
    pub idempotency_key: String,
}

fn source_grants(
    db: &Connection,
    task: &TaskRecord,
) -> Result<Vec<MediaSourceRef>, RepositoryError> {
    let target: Value = decode(&task.target_json)?;
    if let Some(e) = media_target(db, task.project_id.as_str(), &target)? {
        let e = assert_task_scope(db, task, &e.id, Some(&e.branch_id))?;
        return source_reader::grants(db, task, &e);
    }
    // Project advisory reads are frozen at task creation. Never offer arbitrary HEADs.
    let snapshot: String = db
        .query_row(
            "SELECT snapshot_id FROM story_task_source_origins_v2 WHERE task_id=?1",
            [&task.task_id],
            |r| r.get(0),
        )
        .map_err(storage)?;
    let mut q = db.prepare("SELECT r.object_id,r.revision_id,r.content_digest,e.scope_id,e.main_branch_id FROM story_snapshot_members_v2 m JOIN story_revisions_v2 r ON r.revision_id=m.revision_id JOIN story_editions_v1 e ON e.project_id=r.project_id AND e.medium='novel' AND e.main_branch_id='main' WHERE m.snapshot_id=?1 AND r.project_id=?2 AND r.object_kind IN ('draft','brief','outline','plan') ORDER BY r.object_id").map_err(storage)?;
    q.query_map(params![snapshot, task.project_id.as_str()], |r| {
        Ok(MediaSourceRef {
            project_id: task.project_id.as_str().into(),
            object_id: r.get(0)?,
            revision_id: r.get(1)?,
            digest: r.get(2)?,
            scope_id: r.get(3)?,
            branch_id: r.get(4)?,
            selector: MediaSelector::Whole,
        })
    })
    .map_err(storage)?
    .collect::<Result<Vec<_>, _>>()
    .map_err(storage)
}

pub(crate) fn scheduled_scope(
    db: &Connection,
    request: &CreateTaskRequest,
    e: &CreativeEdition,
) -> Result<bool, RepositoryError> {
    let target: Value = decode(&request.target_json)?;
    let Some(step) = target["resumeStepId"].as_str() else {
        return Ok(false);
    };
    db.query_row("SELECT EXISTS(SELECT 1 FROM story_media_adaptations_v1 a JOIN story_work_steps_v1 s ON s.id=a.step_id JOIN story_work_tasks_v1 w ON w.id=a.work_task_id WHERE a.step_id=?1 AND a.edition_id=?2 AND w.project_id=?3 AND w.thread_id=?4 AND s.operation='advisory' AND json_extract(s.target_json,'$.scopeId')=?5 AND json_extract(s.target_json,'$.branchId')=?6)", params![step,e.id,request.project_id.as_str(),request.thread_id,e.scope_id,e.branch_id], |r|r.get(0)).map_err(storage)
}

pub(crate) fn freeze(
    db: &Connection,
    request: &CreateTaskRequest,
    target: &Value,
    manifest: &mut Value,
) -> Result<bool, RepositoryError> {
    let Some(step) = target["resumeStepId"].as_str() else {
        return Ok(false);
    };
    let row: Option<(String,i64,String)> = db.query_row("SELECT a.source_task_id,a.intent_revision,a.instruction FROM story_media_adaptations_v1 a JOIN story_work_tasks_v1 w ON w.id=a.work_task_id WHERE a.step_id=?1 AND w.project_id=?2 AND w.thread_id=?3",params![step,request.project_id.as_str(),request.thread_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(storage)?;
    let Some((source, revision, instruction)) = row else {
        return Ok(false);
    };
    // Retried steps inherit the most recent attempt and its author amendments.
    let attempted: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM story_work_attempts_v1 WHERE step_id=?1)",
            [step],
            |r| r.get(0),
        )
        .map_err(storage)?;
    if attempted {
        return Ok(false);
    }
    crate::task_intents::assert_action_intent(db, &source, revision)?;
    manifest["authorIntent"] = json!(crate::task_author_intent::source_intent(
        db, &source, revision
    )?);
    manifest["candidateInstruction"] = json!(instruction);
    manifest["authorIntentBinding"] =
        json!({"sourceTaskId":source,"intentRevision":revision,"actionId":null});
    Ok(true)
}

pub(crate) fn work_owns_edition(
    db: &Connection,
    work: &str,
    e: &CreativeEdition,
) -> Result<bool, RepositoryError> {
    db.query_row("SELECT EXISTS(SELECT 1 FROM story_media_adaptations_v1 a JOIN story_work_tasks_v1 w ON w.id=a.work_task_id JOIN story_work_steps_v1 s ON s.id=a.step_id WHERE a.work_task_id=?1 AND w.project_id=?2 AND a.edition_id=?3 AND json_extract(s.target_json,'$.scopeId')=?4 AND json_extract(s.target_json,'$.branchId')=?5)", params![work,e.project_id,e.id,e.scope_id,e.branch_id], |r|r.get(0)).map_err(storage)
}



// Only committed, current media documents fulfill a registered adaptation step.
// This proves a document, never rendered assets or artistic quality.
pub(crate) fn assert_saved(db: &Connection, task: &TaskRecord) -> Result<(), RepositoryError> {
    if !is_attempt(db, &task.task_id)? {
        return Ok(());
    }
    let work = crate::model_work_plan::work_for(db, task)?;
    let revision: Option<String> = db.query_row("SELECT o.revision_id FROM story_task_outputs_v2 o JOIN story_media_task_bindings_v1 b ON b.task_id=o.task_id JOIN story_editions_v1 e ON e.edition_id=b.edition_id WHERE o.task_id=?1 AND o.output_kind=e.medium AND o.completeness='complete' ORDER BY o.ordinal DESC LIMIT 1", [&task.task_id], |r|r.get(0)).optional().map_err(storage)?;
    let revision = revision.ok_or(RepositoryError::TaskOutputMissing)?;
    if !crate::model_work_plan::saved_output_current(
        db,
        &work,
        &crate::model_work_plan::GoalEvidence {
            task_id: task.task_id.clone(),
            revision_id: revision,
        },
    )? {
        return Err(RepositoryError::TaskOutputMissing);
    }
    Ok(())
}
pub(crate) fn is_attempt(db: &Connection, task: &str) -> Result<bool, RepositoryError> {
    db.query_row("SELECT EXISTS(SELECT 1 FROM story_media_adaptations_v1 a JOIN story_work_steps_v1 s ON s.id=a.step_id WHERE s.current_attempt_id=?1)", [task], |r|r.get(0)).map_err(storage)
}
pub(crate) fn selected(db: &Connection, task: &str) -> Result<bool, RepositoryError> {
    db.query_row(
        "SELECT EXISTS(SELECT 1 FROM story_media_adaptations_v1 WHERE source_task_id=?1)",
        [task],
        |r| r.get(0),
    )
    .map_err(storage)
}

/// Public multimedia adaptation capability; SQLite retains transaction ownership.
pub trait MultimediaAdaptationRepository {
fn task_adaptation_sources(
        &self,
        task: &TaskRecord,
        offset: usize,
        limit: usize,
    ) -> Result<Value, RepositoryError>;
fn request_media_adaptation(
        &mut self,
        task: &TaskRecord,
        r: &RequestMediaAdaptation,
        now: i64,
    ) -> Result<Value, RepositoryError>;
}

impl MultimediaAdaptationRepository for SqliteStoryRepository {
fn task_adaptation_sources(
        &self,
        task: &TaskRecord,
        offset: usize,
        limit: usize,
    ) -> Result<Value, RepositoryError> {
        if limit == 0 || limit > 20 {
            return Err(fail("media_page_limit_invalid"));
        }
        let tx = self.connection.unchecked_transaction().map_err(storage)?;
        crate::tasks::validate_active_task(
            &tx,
            task,
            &task.project_id,
            &task.thread_id,
            task.input_revision,
        )?;
        let actual = crate::tasks::load_task(&tx, &task.task_id)?;
        if actual.operation != "advisory" {
            return Err(RepositoryError::TaskOutputForbidden);
        }
        crate::task_input::assert_task_inputs_current(&tx, &actual)?;
        let grants = source_grants(&tx, &actual)?;
        if offset > grants.len() {
            return Err(fail("media_page_offset_invalid"));
        }
        let items: Vec<_> = grants.iter().skip(offset).take(limit).collect();
        let next = offset + items.len();
        let result = json!({"sources":items,"offset":offset,"nextOffset":if next<grants.len(){Some(next)}else{None},"total":grants.len()});
        tx.commit().map_err(storage)?;
        Ok(result)
    }

fn request_media_adaptation(
        &mut self,
        task: &TaskRecord,
        r: &RequestMediaAdaptation,
        now: i64,
    ) -> Result<Value, RepositoryError> {
        validate::key(&r.idempotency_key)?;
        validate::text(&r.instruction, 4000, true)?;
        if r.medium == CreativeMedium::Novel || r.sources.is_empty() || r.sources.len() > 20 {
            return Err(fail("adaptation_sources_required"));
        }
        let tx = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage)?;
        crate::tasks::validate_active_task(
            &tx,
            task,
            &task.project_id,
            &task.thread_id,
            task.input_revision,
        )?;
        let actual = crate::tasks::load_task(&tx, &task.task_id)?;
        let target: Value = decode(&actual.target_json)?;
        if actual.operation != "advisory"
            || !matches!(actual.status.as_str(), "queued" | "running" | "waiting")
            || actual.capability_revoked_at.is_some()
            || target["readOnly"] == true
            || target["conversationFocus"]["readOnly"] == true
            || target.get("revisionRepairSource").is_some()
        {
            return Err(RepositoryError::TaskOutputForbidden);
        }
        crate::work_decision::binding(&tx, &actual)?;
        let work = crate::model_work_plan::work_for(&tx, &actual)?;
        let hash = digest(r)?;
        let prior: Option<(String,String,String)> = tx.query_row("SELECT step_id,edition_id,request_digest FROM story_media_adaptations_v1 WHERE source_task_id=?1 AND idempotency_key=?2",params![task.task_id,r.idempotency_key],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).optional().map_err(storage)?;
        if let Some((step, edition, old)) = prior {
            if old != hash {
                return Err(RepositoryError::IdempotencyConflict);
            }
            return Ok(
                json!({"workTaskId":work,"stepId":step,"editionId":edition,"registered":true,"executed":false}),
            );
        }
        crate::work_pause_barrier::assert_can_register(&tx, &work)?;
        crate::task_input::assert_task_inputs_current(&tx, &actual)?;
        crate::task_intents::assert_no_pending_intent(&tx, &task.task_id)?;
        crate::model_work_plan::action_ready(&tx, &work)?;
        let grants = source_grants(&tx, &actual)?;
        for source in &r.sources {
            if !grants.iter().any(|g| source_reader::exact_scope(g, source)) {
                return Err(RepositoryError::TaskOutputForbidden);
            }
            validate_source(
                &tx,
                task.project_id.as_str(),
                source,
                MediaFreshness::Pinned,
            )?;
        }
        let count: i64 = tx
            .query_row(
                "SELECT COUNT(*) FROM story_media_adaptations_v1 WHERE work_task_id=?1",
                [&work],
                |row| row.get(0),
            )
            .map_err(storage)?;
        if count >= 12 {
            return Err(fail("adaptation_work_limit"));
        }
        let intent: i64=tx.query_row("SELECT COALESCE(MAX(intent_revision),0) FROM story_task_intents_v1 WHERE task_id=?1",[&task.task_id],|row|row.get(0)).map_err(storage)?;
        crate::task_intents::assert_action_intent(&tx, &task.task_id, intent)?;
        let e = create_edition(
            &tx,
            &task.project_id,
            &CreateEditionRequest {
                medium: r.medium,
                title: r.title.clone(),
                sources: r.sources.clone(),
                idempotency_key: format!(
                    "adaptation-{}",
                    digest(&json!([task.task_id, r.idempotency_key]))?
                ),
            },
            now,
        )?;
        let step = id("adaptation");
        let predecessor: String=tx.query_row("SELECT id FROM story_work_steps_v1 WHERE work_task_id=?1 ORDER BY ordinal DESC LIMIT 1",[&work],|row|row.get(0)).map_err(storage)?;
        let target = json!({"type":"edition","editionId":e.id,"scopeId":e.scope_id,"branchId":e.branch_id,"revisionId":null,"selection":null,"imageAssetRevisionIds":[],"adaptation":true});
        tx.execute("INSERT INTO story_work_steps_v1(id,work_task_id,ordinal,operation,target_json,input_revision,depends_on_step_id,status) VALUES(?1,?2,(SELECT COALESCE(MAX(ordinal),0)+1 FROM story_work_steps_v1 WHERE work_task_id=?2),'advisory',?3,?4,?5,'queued')",params![step,work,encode(&target)?,task.input_revision,predecessor]).map_err(storage)?;
        tx.execute(
            "INSERT INTO story_media_adaptations_v1 VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
            params![
                step,
                task.task_id,
                work,
                e.id,
                intent,
                r.instruction,
                r.idempotency_key,
                hash
            ],
        )
        .map_err(storage)?;
        tx.execute(
            "UPDATE story_work_tasks_v1 SET version=version+1 WHERE id=?1",
            [&work],
        )
        .map_err(storage)?;
        tx.commit().map_err(storage)?;
        Ok(
            json!({"workTaskId":work,"stepId":step,"editionId":e.id,"registered":true,"executed":false}),
        )
    }
}
