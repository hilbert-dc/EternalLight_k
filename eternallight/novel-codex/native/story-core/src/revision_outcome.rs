use std::collections::HashSet;

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::authoring_runs::storage_error;
use crate::{ProjectId, RepositoryError, SqliteStoryRepository, TaskRecord};

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
/// An author-approved criterion restricted to the writer's selected scene groups.
pub struct DesiredOutcome {
    pub id: String,
    pub criterion: String,
    pub group_ids: Vec<String>,
}

/// A source-bound candidate interpretation, never a semantic completion certificate.
pub struct StoredRevisionOutcome {
    pub revision_id: String,
    pub content: Value,
    pub current: bool,
    pub issues: Vec<Value>,
    pub decisions: Vec<crate::RevisionOutcomeDecision>,
}

pub(crate) fn desired_outcomes(target: &Value) -> Result<Vec<DesiredOutcome>, RepositoryError> {
    let Some(value) = target.get("desiredOutcomes") else {
        return Ok(Vec::new());
    };
    let invalid = || RepositoryError::TaskBindingInvalid;
    crate::scene_revision::selection(target)?;
    let outcomes: Vec<DesiredOutcome> =
        serde_json::from_value(value.clone()).map_err(|_| invalid())?;
    let selected = target["scope"]["groupIds"].as_array().ok_or_else(invalid)?;
    if outcomes.is_empty()
        || outcomes.len() > 4
        || outcomes
            .iter()
            .map(|item| &item.id)
            .collect::<HashSet<_>>()
            .len()
            != outcomes.len()
        || outcomes.iter().any(|item| {
            item.id.is_empty()
                || item.id.len() > 64
                || !item
                    .id
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
                || item.criterion.trim().is_empty()
                || item.criterion.chars().count() > 500
                || item.group_ids.is_empty()
                || item.group_ids.len() > 16
                || item.group_ids.iter().collect::<HashSet<_>>().len() != item.group_ids.len()
                || item
                    .group_ids
                    .iter()
                    .any(|id| !selected.contains(&json!(id)))
        })
    {
        return Err(invalid());
    }
    Ok(outcomes)
}

pub(crate) fn writer(
    db: &Connection,
    project: &ProjectId,
    id: &str,
) -> Result<(TaskRecord, Value), RepositoryError> {
    let task = crate::tasks::load_task(db, id)?;
    let target: Value =
        serde_json::from_str(&task.target_json).map_err(|_| RepositoryError::CorruptState)?;
    if task.project_id != *project
        || task.operation != "generateChapter"
        || task.status != "completed"
        || !target["chapterAcceptance"].is_object()
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    crate::chapter_delivery_review::validate_acceptance(&target["chapterAcceptance"])?;
    if target.get("workDecisionAction").is_some() {
        let actual:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM story_work_attempts_v1 a JOIN story_work_decision_actions_v1 d ON d.step_id=a.step_id JOIN story_work_steps_v1 s ON s.id=d.step_id AND s.current_attempt_id=a.attempt_id WHERE a.attempt_id=?1 AND s.operation='generateChapter' AND s.work_task_id=json_extract(?2,'$.workDecisionAction.workTaskId'))",params![id,task.target_json],|r|r.get(/*idx*/0)).map_err(storage_error)?;
        if !actual {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        crate::author_message::validate_execution(db, &task)?;
        return Ok((task, target));
    }
    let (source, action_id): (String,String) = db.query_row(
        "SELECT a.source_task_id,a.action_id FROM story_task_action_requests_v1 a JOIN story_work_attempts_v1 w ON w.step_id=a.action_id
         WHERE w.attempt_id=?1 AND a.project_id=?2 AND a.operation='generateChapter'",
        params![id,project.as_str()], |row|Ok((row.get(0)?,row.get(1)?)),
    ).optional().map_err(storage_error)?.ok_or(RepositoryError::TaskBindingInvalid)?;
    let action = crate::task_actions::load_action(db, project, &source, &action_id)?;
    let mut proposed = target.clone();
    let fields = proposed
        .as_object_mut()
        .ok_or(RepositoryError::TaskBindingInvalid)?;
    fields.remove("requestedByTaskId");
    fields.remove("requestedActionId");
    fields.remove("resumeStepId");
    if action.target != proposed
        || action.operation != "generateChapter"
        || !crate::task_action_permit::granted(db, project, &action)?
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    crate::author_message::validate_execution(db, &task)?;
    Ok((task, target))
}

/// Initial writing has no original draft. A comparison uses the writer's frozen baseline.
pub(crate) fn original_revision(
    db: &Connection,
    task: &TaskRecord,
    target: &Value,
) -> Result<Option<String>, RepositoryError> {
    if let Some(revision) = target["scope"]["sourceRevisionId"].as_str() {
        return Ok(Some(revision.into()));
    }
    db.query_row("SELECT COALESCE(working_revision_id,candidate_revision_id,accepted_revision_id) FROM story_task_source_heads_v2 WHERE task_id=?1 AND object_id='draft-'||?2",params![task.task_id,target["chapterId"].as_str()],|row|row.get(0)).optional().map_err(storage_error).map(Option::flatten)
}
/// Frozen whole-chapter standards coexist with the original scene-scoped desired outcomes.
pub(crate) fn assessment_outcomes(
    db: &Connection,
    project: &ProjectId,
    target: &Value,
    revision: &str,
) -> Result<Vec<DesiredOutcome>, RepositoryError> {
    let acceptance =
        crate::chapter_delivery_review::validate_acceptance(&target["chapterAcceptance"])?;
    let draft = crate::revision_outcome_validation::draft(
        db,
        project,
        target["chapterId"]
            .as_str()
            .ok_or(RepositoryError::TaskBindingInvalid)?,
        revision,
    )?;
    let groups = draft["sceneGroups"]
        .as_array()
        .ok_or(RepositoryError::CorruptState)?;
    let ids = groups
        .iter()
        .map(|g| {
            g["id"]
                .as_str()
                .map(str::to_owned)
                .ok_or(RepositoryError::CorruptState)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut outcomes: Vec<_> = acceptance
        .requirements
        .iter()
        .map(|criterion| DesiredOutcome {
            id: criterion.id.clone(),
            criterion: criterion.text.clone(),
            group_ids: ids.clone(),
        })
        .collect();
    outcomes.extend(desired_outcomes(target)?);
    if outcomes
        .iter()
        .map(|o| &o.id)
        .collect::<std::collections::BTreeSet<_>>()
        .len()
        != outcomes.len()
    {
        return Err(RepositoryError::InvalidContent(
            "criterion IDs must be unique across chapterAcceptance and selected desiredOutcomes"
                .into(),
        ));
    }
    Ok(outcomes)
}

/// Validate the immutable predecessor chain; current draft and batch are separate checks.
pub(crate) fn validate_target(
    db: &Connection,
    project: &ProjectId,
    target: &Value,
) -> Result<(), RepositoryError> {
    let id = target["writerTaskId"]
        .as_str()
        .ok_or(RepositoryError::TaskBindingInvalid)?;
    let (task, original) = writer(db, project, id)?;
    let output: Option<String> = db.query_row(
        "SELECT revision_id FROM story_task_outputs_v2 WHERE task_id=?1 AND output_kind='draft' AND completeness='chapterComplete' ORDER BY ordinal DESC LIMIT 1",
        [id], |row| row.get(0),
    ).optional().map_err(storage_error)?;
    let step = target["resumeStepId"]
        .as_str()
        .ok_or(RepositoryError::TaskBindingInvalid)?;
    let mut actual = target.clone();
    actual
        .as_object_mut()
        .ok_or(RepositoryError::TaskBindingInvalid)?
        .remove("resumeStepId");
    if target.get("workDecisionAction").is_none() {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    // The registered target identifies the draft's producer. The selection
    // source separately identifies either that writer or a failed assessment.
    let registered: Option<(String, String, Option<String>, String, String, String, String)> = db.query_row(
        "SELECT s.target_json,d.target_json,d.source_task_id,s.work_task_id,d.decision_task_id,parent.id,d.action_key
         FROM story_work_steps_v1 s
         JOIN story_work_tasks_v1 w ON w.id=s.work_task_id AND w.project_id=?4
         JOIN story_work_decision_actions_v1 d ON d.step_id=s.id AND d.work_task_id=w.id AND d.operation=s.operation
         JOIN story_work_steps_v1 parent ON parent.id=s.depends_on_step_id AND parent.work_task_id=w.id
           AND parent.operation='advisory' AND parent.status='completed'
         JOIN story_tasks_v2 decision ON decision.task_id=parent.current_attempt_id AND decision.task_id=d.decision_task_id
           AND decision.project_id=w.project_id AND decision.operation='advisory' AND decision.status='completed'
         JOIN story_work_attempts_v1 parent_attempt ON parent_attempt.attempt_id=decision.task_id AND parent_attempt.step_id=parent.id
         JOIN story_work_attempts_v1 a ON a.attempt_id=?2
         JOIN story_work_steps_v1 writer ON writer.id=a.step_id AND writer.work_task_id=w.id
         JOIN story_task_outputs_v2 own_output ON own_output.task_id=a.attempt_id AND own_output.revision_id=?3
           AND own_output.output_kind='draft' AND own_output.completeness='chapterComplete'
         JOIN story_revisions_v2 draft ON draft.revision_id=own_output.revision_id AND draft.project_id=w.project_id
           AND draft.object_kind='draft' AND draft.object_id='draft-'||json_extract(d.target_json,'$.chapterId')
         WHERE s.id=?1 AND s.operation='assessRevisionOutcome' AND d.source_revision_id=?3
           AND json_extract(d.target_json,'$.writerTaskId')=?2 AND json_extract(d.target_json,'$.draftRevisionId')=?3",
        params![step,id,output,project.as_str()],
        |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?,row.get(6)?)),
    ).optional().map_err(storage_error)?;
    let (step_target, action_target, source, work, decision_task, parent_step, action_key) =
        registered.ok_or(RepositoryError::TaskBindingInvalid)?;
    let step_target: Value =
        serde_json::from_str(&step_target).map_err(|_| RepositoryError::CorruptState)?;
    let action_target: Value =
        serde_json::from_str(&action_target).map_err(|_| RepositoryError::CorruptState)?;
    if step_target != actual
        || action_target != actual
        || action_target["workDecisionAction"]
            != json!({"workTaskId":work,"decisionStepId":parent_step,
                "decisionTaskId":decision_task,"actionKey":action_key})
        || target["chapterId"] != original["chapterId"]
        || target["chapterNumber"] != original["chapterNumber"]
        || target.get("originalRevisionId")
            != Some(
                &serde_json::to_value(original_revision(db, &task, &original)?)
                    .map_err(|_| RepositoryError::CorruptState)?,
            )
        || output.as_deref() != target["draftRevisionId"].as_str()
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    validate_selection_source(
        db,
        project,
        &work,
        &decision_task,
        source
            .as_deref()
            .ok_or(RepositoryError::TaskBindingInvalid)?,
        id,
        &action_target,
    )?;
    // Published provenance survives head replacement and source invalidation.
    // Freshness is checked separately by inputs_current at every execution/write gate.
    match target.get("knowledgeBatchId") {
        Some(Value::Null) => {}
        Some(Value::String(batch)) => {
            let valid: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM story_modeling_batches_v2 b WHERE b.batch_id=?1 AND b.project_id=?2 AND b.object_id='draft-'||?3 AND b.source_revision_id=?4 AND b.published_at IS NOT NULL)",params![batch,project.as_str(),original["chapterId"].as_str(),output],|r|r.get(0)).map_err(storage_error)?;
            if !valid {
                return Err(RepositoryError::TaskBindingInvalid);
            }
        }
        _ => return Err(RepositoryError::TaskBindingInvalid),
    }
    if task.project_id != *project {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    Ok(())
}

fn validate_selection_source(
    db: &Connection,
    project: &ProjectId,
    work: &str,
    decision_task: &str,
    source: &str,
    producer: &str,
    registered_target: &Value,
) -> Result<(), RepositoryError> {
    let source_task = crate::tasks::load_task(db, source)?;
    if source_task.project_id != *project {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    match source_task.operation.as_str() {
        "generateChapter" => {
            if source != producer || registered_target.get("recoveryPlan").is_some() {
                return Err(RepositoryError::TaskBindingInvalid);
            }
        }
        "assessRevisionOutcome" => {
            let earlier: bool = db.query_row(
                "SELECT EXISTS(SELECT 1 FROM story_work_attempts_v1 prior
                 JOIN story_work_steps_v1 prior_step ON prior_step.id=prior.step_id AND prior_step.work_task_id=?1
                   AND prior_step.operation='assessRevisionOutcome'
                 JOIN story_work_attempts_v1 parent ON parent.attempt_id=?3 AND parent.sequence>prior.sequence
                 JOIN story_work_steps_v1 parent_step ON parent_step.id=parent.step_id AND parent_step.work_task_id=?1
                 WHERE prior.attempt_id=?2)",
                params![work,source,decision_task],
                |row| row.get(0),
            ).map_err(storage_error)?;
            if !earlier
                || source_task.status != "failed"
                || source_task.capability_revoked_at.is_none()
                || !crate::work_decision_status::retryable(db, source)?
                || registered_target
                    .get("recoveryPlan")
                    .is_some_and(|plan| plan["failedTaskId"].as_str() != Some(source))
            {
                return Err(RepositoryError::TaskBindingInvalid);
            }
            let failed_target: Value = serde_json::from_str(&source_task.target_json)
                .map_err(|_| RepositoryError::CorruptState)?;
            // Compare frozen domain inputs using the same projection as the
            // recovery catalog; do not recheck mutable readiness or retry counts.
            if crate::work_decision_recovery::failure_target(failed_target)?
                != crate::work_decision_recovery::failure_target(registered_target.clone())?
            {
                return Err(RepositoryError::TaskBindingInvalid);
            }
        }
        _ => return Err(RepositoryError::TaskBindingInvalid),
    }
    Ok(())
}

pub(crate) fn inputs_current(
    db: &Connection,
    project: &ProjectId,
    target: &Value,
) -> Result<bool, RepositoryError> {
    db.query_row(
        "SELECT EXISTS(SELECT 1 FROM story_object_heads_v2 h
         WHERE h.project_id=?1 AND h.object_id='draft-'||?2 AND COALESCE(h.candidate_revision_id,h.accepted_revision_id)=?3
         AND (?4 IS NULL OR EXISTS(SELECT 1 FROM story_knowledge_heads_v2 k
           JOIN story_modeling_batches_v2 b ON b.batch_id=k.batch_id AND b.project_id=k.project_id
           WHERE k.project_id=h.project_id AND k.object_id=h.object_id AND k.batch_id=?4
           AND b.status='ready' AND b.source_revision_id=?3)))",
        params![project.as_str(),target["chapterId"].as_str(),target["draftRevisionId"].as_str(),target["knowledgeBatchId"].as_str()], |row|row.get(0),
    ).map_err(storage_error)
}

/// Catalog presence never implies a source dependency. Only actually returned prior
/// prose fields (or a completely returned raw input) invalidate an assessment.
pub(crate) fn assessment_sources_current(
    db: &Connection,
    _project: &ProjectId,
    task: &str,
    _chapter: i64,
) -> Result<bool, RepositoryError> {
    crate::task_workspace::dependencies_current(db, task)
}

/// A report is evidence only. Verdict and measured length determine current readiness.
pub(crate) fn assessment_state(
    db: &Connection,
    project: &ProjectId,
    writer_id: &str,
    revision: &str,
) -> Result<Value, RepositoryError> {
    let (_, target) = writer(db, project, writer_id)?;
    let chapter = target["chapterId"]
        .as_str()
        .ok_or(RepositoryError::CorruptState)?;
    let draft = crate::revision_outcome_validation::draft(db, project, chapter, revision)?;
    let length = crate::chapter_delivery_review::length_result(
        &target["chapterAcceptance"],
        crate::work_decision_status::chapter_character_count(
            draft["body"]
                .as_str()
                .ok_or(RepositoryError::CorruptState)?,
        ),
    )?;
    let row:Option<(String,String,String,String)>=db.query_row("SELECT r.revision_id,r.content_json,t.target_json,t.task_id FROM story_object_heads_v2 h JOIN story_revisions_v2 r ON r.project_id=h.project_id AND r.revision_id=h.candidate_revision_id JOIN story_task_outputs_v2 o ON o.revision_id=r.revision_id AND o.output_kind='revisionOutcome' JOIN story_tasks_v2 t ON t.task_id=o.task_id AND t.status='completed' AND t.operation='assessRevisionOutcome' WHERE h.project_id=?1 AND h.object_id=?2",params![project.as_str(),format!("revision-outcome-{writer_id}")],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).optional().map_err(storage_error)?;
    let mut state = json!({"writerTaskId":writer_id,"chapterId":chapter,"draftRevisionId":revision,"chapterAcceptance":target["chapterAcceptance"],"reportRevisionId":null,"current":false,"verdict":"pending","length":length,"checks":[]});
    if crate::direct_verification::enabled(&target) {
        if let Some(direct) = crate::direct_verification::state(db,project,writer_id,revision,state.clone())? {
            if crate::direct_verification::newer(db,direct["reportRevisionId"].as_str().ok_or(RepositoryError::CorruptState)?,row.as_ref().map(|r|r.0.as_str()))? {
                return Ok(direct);
            }
        }
    }
    if let Some((report_id, encoded, report_target, assessment_task)) = row {
        let report_target: Value =
            serde_json::from_str(&report_target).map_err(|_| RepositoryError::CorruptState)?;
        validate_target(db, project, &report_target)?;
        crate::revision_outcome_validation::validate_report(db, project, &report_target, &encoded)?;
        let report: Value =
            serde_json::from_str(&encoded).map_err(|_| RepositoryError::CorruptState)?;
        let current = report_target["draftRevisionId"] == revision
            && inputs_current(db, project, &report_target)?
            && assessment_sources_current(
                db,
                project,
                &assessment_task,
                target["chapterNumber"]
                    .as_i64()
                    .ok_or(RepositoryError::CorruptState)?,
            )?;
        let outcomes = report["outcomes"]
            .as_array()
            .ok_or(RepositoryError::CorruptState)?;
        state["reportRevisionId"] = json!(report_id);
        state["current"] = json!(current);
        let uncovered = report["uncoveredRequirements"]
            .as_array()
            .ok_or(RepositoryError::CorruptState)?;
        state["uncoveredRequirementCount"] = json!(uncovered.len());
        state["uncoveredRequirements"] = json!(uncovered.iter().take(2).map(|r|json!({"requirement":r["requirement"].as_str().unwrap_or("").chars().take(96).collect::<String>(),"reason":r["reason"].as_str().unwrap_or("").chars().take(96).collect::<String>(),"issue":r["issue"],"authorSourceChecks":[]})).collect::<Vec<_>>());
        state["checks"]=json!(outcomes.iter().map(|c|json!({"id":c["id"],"status":c["status"],"issueId":c["issue"]["issueId"],"reason":c["reason"].as_str().unwrap_or("").chars().take(96).collect::<String>()})).collect::<Vec<_>>());
        state["verdict"] = json!(if !current {
            "stale"
        } else if outcomes.iter().any(|c| c["status"] == "unmet")
            || !uncovered.is_empty()
            || state["length"]["satisfied"] != true
        {
            "notMet"
        } else if outcomes
            .iter()
            .any(|c| c["status"] == "insufficientEvidence")
        {
            "insufficientEvidence"
        } else if crate::work_issues::has_unresolved(
            db,
            project,
            &crate::work_issues::work_for(db, project, writer_id)?,
            Some(chapter)
        )? {
            "notMet"
        } else {
            "passed"
        });
    }
    Ok(state)
}
pub(crate) fn verify_chapter(
    db: &Connection,
    project: &ProjectId,
    work: &str,
    chapter: &str,
    revision: &str,
) -> Result<Value, RepositoryError> {
    let writer:Option<String>=db.query_row("SELECT t.task_id FROM story_work_steps_v1 s JOIN story_tasks_v2 t ON t.task_id=s.current_attempt_id AND t.operation='generateChapter' AND t.status='completed' JOIN story_task_outputs_v2 o ON o.task_id=t.task_id AND o.output_kind='draft' AND o.completeness='chapterComplete' WHERE s.work_task_id=?1 AND t.project_id=?2 AND json_extract(t.target_json,'$.chapterId')=?3 AND o.revision_id=?4 ORDER BY s.ordinal DESC LIMIT 1",params![work,project.as_str(),chapter,revision],|row|row.get(0)).optional().map_err(storage_error)?;
    let state = assessment_state(
        db,
        project,
        &writer.ok_or(RepositoryError::TaskOutputMissing)?,
        revision,
    )?;
    if state["verdict"] != "passed" {
        return Err(RepositoryError::InvalidContent("Current saved chapter must pass the selected evidence-based assessment of every frozen requirement and actual length before delivery or continuation".into()));
    }
    Ok(state)
}

impl SqliteStoryRepository {
    pub fn task_revision_outcome(
        &self,
        task: &TaskRecord,
        writer_id: &str,
    ) -> Result<Option<StoredRevisionOutcome>, RepositoryError> {
        self.with_read_snapshot(|| {
            if task.operation!="advisory" {return Err(RepositoryError::TaskOutputForbidden);}
            crate::tasks::validate_active_task(&self.connection,task,&task.project_id,&task.thread_id,task.input_revision)?;
            crate::task_input::assert_task_inputs_current(&self.connection,task)?;
            let source_work=crate::model_work_plan::work_for(&self.connection,task)?;
            let writer_work:String=self.connection.query_row("SELECT s.work_task_id FROM story_work_steps_v1 s JOIN story_work_attempts_v1 a ON a.step_id=s.id JOIN story_tasks_v2 t ON t.task_id=a.attempt_id WHERE a.attempt_id=?1 AND t.project_id=?2 AND t.operation='generateChapter' AND t.status='completed'",params![writer_id,task.project_id.as_str()],|row|row.get(0)).optional().map_err(storage_error)?.ok_or(RepositoryError::TaskBindingInvalid)?;
            if source_work!=writer_work {return Err(RepositoryError::TaskBindingInvalid);}
            self.revision_outcome_in_snapshot(&task.project_id,writer_id)
        })?
    }
    pub fn revision_outcome(
        &self,
        project: &ProjectId,
        writer_id: &str,
    ) -> Result<Option<StoredRevisionOutcome>, RepositoryError> {
        self.with_read_snapshot(|| self.revision_outcome_in_snapshot(project, writer_id))?
    }
    fn revision_outcome_in_snapshot(
        &self,
        project: &ProjectId,
        writer_id: &str,
    ) -> Result<Option<StoredRevisionOutcome>, RepositoryError> {
        let read = &self.connection;
        let task = crate::tasks::load_task(read, writer_id)?;
        if task.project_id != *project || task.operation != "generateChapter" {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        if task.status != "completed" {
            return Ok(None);
        }
        let _ = writer(&read, project, writer_id)?;
        let row: Option<(String,String,String,String)> = read.query_row(
            "SELECT r.revision_id,r.content_json,t.target_json,t.task_id FROM story_object_heads_v2 h
             JOIN story_revisions_v2 r ON r.revision_id=h.candidate_revision_id AND r.project_id=h.project_id AND r.object_id=h.object_id
             JOIN story_task_outputs_v2 o ON o.revision_id=r.revision_id AND o.output_kind='revisionOutcome'
             JOIN story_tasks_v2 t ON t.task_id=o.task_id AND t.operation='assessRevisionOutcome' AND t.project_id=h.project_id
             WHERE h.project_id=?1 AND h.object_id=?2 AND r.object_kind='revisionOutcome'",
            params![project.as_str(),format!("revision-outcome-{writer_id}")], |row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?)),
        ).optional().map_err(storage_error)?;
        if let Some(direct)=crate::direct_verification::report(read,project,writer_id,row.as_ref().map(|r|r.0.as_str()))? { return Ok(Some(direct)); }
        row.map(|(revision_id, encoded, target, assessment_task)| {
            let target: Value =
                serde_json::from_str(&target).map_err(|_| RepositoryError::CorruptState)?;
            validate_target(&read, project, &target)?;
            crate::revision_outcome_validation::validate_report(&read, project, &target, &encoded)?;
            Ok(StoredRevisionOutcome {
                issues: crate::work_issues::report_issues(read, project, &revision_id)?,
                revision_id,
                content: serde_json::from_str(&encoded)
                    .map_err(|_| RepositoryError::CorruptState)?,
                current: inputs_current(&read, project, &target)?
                    && assessment_sources_current(
                        &read,
                        project,
                        &assessment_task,
                        target["chapterNumber"]
                            .as_i64()
                            .ok_or(RepositoryError::CorruptState)?,
                    )?,
                decisions: crate::revision_outcome_decision::history(&read, project, writer_id)?,
            })
        })
        .transpose()
    }
}
