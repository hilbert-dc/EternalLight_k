//! Transactional chapter delivery evidence, not literary quality or Canon authority.

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::authoring_runs::storage_error;
use crate::{
    KnowledgePerspective, KnowledgeStateQuery, ObjectId, ProjectId, RepositoryError,
    SqliteStoryRepository,
};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GenerationChapterDelivery {
    pub chapter_id: String,
    pub chapter_number: i64,
    pub draft_revision_id: String,
    pub plan_revision_id: String,
    pub knowledge_batch_id: String,
    pub commitment_extraction_revision_id: String,
    pub assessment_revision_id: String,
    pub assessment_task_id: String,
    pub checked_snapshot_id: String,
    pub source_digest: String,
    pub checked_run_version: i64,
    pub feedback_decision_id: Option<String>,
}

pub(crate) fn validate_history(
    db: &rusqlite::Connection,
    project: &ProjectId,
    run: &str,
    state: &Value,
) -> Result<Vec<GenerationChapterDelivery>, RepositoryError> {
    let deliveries: Vec<GenerationChapterDelivery> =
        serde_json::from_value(state["chapterDeliveries"].clone())
            .map_err(|_| RepositoryError::CorruptState)?;
    let start = state["startChapterNumber"]
        .as_i64()
        .ok_or(RepositoryError::CorruptState)?;
    let completed = state["completedChapters"]
        .as_i64()
        .ok_or(RepositoryError::CorruptState)?;
    let target = state["targetChapters"]
        .as_i64()
        .ok_or(RepositoryError::CorruptState)?;
    if deliveries.len() > 3
        || start < 1
        || completed < start - 1
        || completed > target
        || completed - (start - 1) != deliveries.len() as i64
        || (state["status"] == "completed"
            && (completed != target
                || state["stage"] != "finished"
                || !state["activeTaskId"].is_null()))
        || (state["stage"] == "finished" && state["status"] != "completed")
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    for (index, delivery) in deliveries.iter().enumerate() {
        let owned: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM story_chapters_v2 c
             JOIN story_revisions_v2 d ON d.project_id=c.project_id AND d.object_id='draft-'||c.chapter_id AND d.object_kind='draft' AND d.revision_id=?4
             JOIN story_revisions_v2 p ON p.project_id=c.project_id AND p.object_id='artifact-plan' AND p.object_kind='plan' AND p.revision_id=?5
             JOIN story_modeling_batches_v2 k ON k.project_id=c.project_id AND k.object_id=d.object_id AND k.source_revision_id=d.revision_id AND k.batch_id=?6
             JOIN story_revisions_v2 e ON e.project_id=c.project_id AND e.object_id='commitment-extraction-'||c.chapter_id AND e.object_kind='commitmentExtraction' AND e.revision_id=?7
             JOIN story_revisions_v2 a ON a.project_id=c.project_id AND a.object_id='scene-assessment-'||c.chapter_id AND a.object_kind='sceneAssessment' AND a.revision_id=?8
             JOIN story_task_outputs_v2 o ON o.revision_id=a.revision_id AND o.output_kind='sceneAssessment'
             JOIN story_tasks_v2 t ON t.task_id=o.task_id AND t.project_id=c.project_id AND t.operation='assessSceneOutcome' AND t.status='completed' AND t.task_id=?9
             JOIN story_work_attempts_v1 wa ON wa.attempt_id=t.task_id JOIN story_work_steps_v1 ws ON ws.id=wa.step_id AND ws.work_task_id='work-'||?10
             JOIN story_snapshots_v2 s ON s.project_id=c.project_id AND s.snapshot_id=?11
             WHERE c.project_id=?1 AND c.chapter_id=?2 AND json_extract(d.content_json,'$.chapterNumber')=?3 AND json_extract(t.target_json,'$.runId')=?10
               AND json_extract(a.content_json,'$.draftRevisionId')=d.revision_id AND json_extract(a.content_json,'$.planRevisionId')=p.revision_id
               AND json_extract(a.content_json,'$.knowledgeBatchId')=k.batch_id AND json_extract(e.content_json,'$.payload.sourceRevisionId')=d.revision_id
               AND (SELECT COUNT(*) FROM story_snapshot_members_v2 m WHERE m.snapshot_id=s.snapshot_id AND
                 ((m.object_id=d.object_id AND m.revision_id=d.revision_id) OR (m.object_id=p.object_id AND m.revision_id=p.revision_id)
                  OR (m.object_id=e.object_id AND m.revision_id=e.revision_id) OR (m.object_id=a.object_id AND m.revision_id=a.revision_id)))=4)",
            params![project.as_str(),delivery.chapter_id,delivery.chapter_number,delivery.draft_revision_id,delivery.plan_revision_id,delivery.knowledge_batch_id,delivery.commitment_extraction_revision_id,delivery.assessment_revision_id,delivery.assessment_task_id,run,delivery.checked_snapshot_id], |row|row.get(0),
        ).map_err(storage_error)?;
        if !owned
            || delivery.chapter_number != start + index as i64
            || delivery.checked_run_version < 0
            || state["version"]
                .as_i64()
                .is_none_or(|version| delivery.checked_run_version >= version)
            || delivery.source_digest.len() != 64
            || !delivery
                .source_digest
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            || delivery.feedback_decision_id.as_ref().is_some_and(|id| {
                !state["feedbackDecisions"]
                    .as_array()
                    .is_some_and(|decisions| {
                        decisions.iter().any(|decision| {
                            decision["id"] == *id
                                && decision["choice"] == "continue"
                                && decision["assessmentRevisionId"]
                                    == delivery.assessment_revision_id
                        })
                    })
            })
        {
            return Err(RepositoryError::TaskBindingInvalid);
        }
    }
    Ok(deliveries)
}

/// Called only inside the owning Immediate write transaction; callers cannot supply new evidence.
pub(crate) fn prepare_transition(
    repository: &SqliteStoryRepository,
    project: &ProjectId,
    run: &str,
    prior: &Value,
    next: &mut Value,
) -> Result<(), RepositoryError> {
    let db = &repository.connection;
    let mut deliveries = validate_history(db, project, run, prior)?;
    if next["chapterDeliveries"] != prior["chapterDeliveries"]
        || (matches!(prior["status"].as_str(), Some("completed" | "cancelled"))
            && next["status"] != prior["status"])
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let before = prior["completedChapters"]
        .as_i64()
        .ok_or(RepositoryError::CorruptState)?;
    let after = next["completedChapters"]
        .as_i64()
        .ok_or(RepositoryError::CorruptState)?;
    if after == before {
        if next["status"] == "completed" && prior["status"] != "completed" {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        validate_history(db, project, run, next)?;
        return Ok(());
    }
    let number = before.checked_add(1).ok_or(RepositoryError::CorruptState)?;
    let chapter = prior["currentChapterId"]
        .as_str()
        .ok_or(RepositoryError::TaskBindingInvalid)?;
    let version = prior["version"]
        .as_i64()
        .ok_or(RepositoryError::CorruptState)?;
    let target = prior["targetChapters"]
        .as_i64()
        .ok_or(RepositoryError::CorruptState)?;
    let (status, stage) = if number == target {
        ("completed", "finished")
    } else {
        ("running", "rollingPlan")
    };
    if after != number
        || number > target
        || prior["status"] != "running"
        || prior["stage"] != "check"
        || !prior["activeTaskId"].is_null()
        || prior["currentChapterNumber"] != number
        || next["version"] != version + 1
        || next["status"] != status
        || next["stage"] != stage
        || !next["activeTaskId"].is_null()
        || !next["currentChapterId"].is_null()
        || !next["currentChapterNumber"].is_null()
        || next["usedTaskAttempts"] != prior["usedTaskAttempts"]
        || next["sourceDigest"] != prior["sourceDigest"]
        || next["lastTaskId"] != prior["lastTaskId"]
        || crate::task_input::project_source_digest(db, project)? != prior["sourceDigest"]
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    crate::generation_checkpoint::validate_chapter(db, project, chapter, number)?;
    let knowledge = repository.knowledge_state_in_snapshot(KnowledgeStateQuery {
        project_id: project,
        snapshot_id: None,
        perspective: KnowledgePerspective::Reader,
        world_time: None,
        epistemic_time: None,
        reading_chapter: Some(number),
        owner_id: None,
        entity_id: None,
        predicate: None,
    })?;
    if knowledge["coverage"]["complete"] != true
        || knowledge["findings"]
            .as_array()
            .is_none_or(|items| !items.is_empty())
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let assessment = repository
        .scene_assessment(project, chapter)?
        .filter(|item| item.current)
        .ok_or(RepositoryError::TaskBindingInvalid)?;
    let findings = assessment.content["assessments"]
        .as_array()
        .filter(|items| !items.is_empty())
        .ok_or(RepositoryError::CorruptState)?;
    let accepted = prior["feedbackDecisions"]
        .as_array()
        .ok_or(RepositoryError::CorruptState)?
        .iter()
        .rev()
        .find(|decision| {
            decision["choice"] == "continue"
                && decision["assessmentRevisionId"] == assessment.revision_id
                && prior["acceptedFeedbackRevisionId"] == assessment.revision_id
        });
    if findings
        .iter()
        .any(|item| item["verdict"] != "realized" || item["recommendation"] != "continue")
        && accepted.is_none()
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let task_id = prior["lastTaskId"]
        .as_str()
        .ok_or(RepositoryError::TaskBindingInvalid)?;
    let budget = crate::work_budget_storage::load(db, project, &format!("work-{run}"))?;
    let attempts: i64 = db.query_row("SELECT COUNT(*) FROM story_work_attempts_v1 a JOIN story_work_steps_v1 s ON s.id=a.step_id WHERE s.work_task_id=?1",[format!("work-{run}")],|row|row.get(0)).map_err(storage_error)?;
    let unsettled: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM story_task_provider_budgets_v1 WHERE work_task_id=?1 AND settled=0) OR EXISTS(SELECT 1 FROM story_work_attempts_v1 a JOIN story_work_steps_v1 s ON s.id=a.step_id JOIN story_tasks_v2 t ON t.task_id=a.attempt_id WHERE s.work_task_id=?1 AND t.status NOT IN ('completed','failed','cancelled'))",[format!("work-{run}")],|row|row.get(0)).map_err(storage_error)?;
    if unsettled
        || !budget.has_charge(task_id)
        || prior["usedTaskAttempts"] != attempts
        || budget.charges.len() as i64 != attempts
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let extraction: Option<(String,String,String)> = db.query_row(
        "SELECT r.revision_id,r.content_json,t.task_id FROM story_branches_v2 b JOIN story_snapshot_members_v2 m ON m.snapshot_id=b.head_snapshot_id
         JOIN story_revisions_v2 r ON r.revision_id=m.revision_id AND r.project_id=b.project_id AND r.object_kind='commitmentExtraction'
         JOIN story_task_outputs_v2 o ON o.revision_id=r.revision_id AND o.output_kind='commitmentExtraction'
         JOIN story_tasks_v2 t ON t.task_id=o.task_id AND t.project_id=b.project_id AND t.operation='extractCommitments' AND t.status='completed'
         JOIN story_work_attempts_v1 a ON a.attempt_id=t.task_id JOIN story_work_steps_v1 s ON s.id=a.step_id AND s.work_task_id='work-'||?3
         WHERE b.project_id=?1 AND b.branch_id='main' AND m.object_id='commitment-extraction-'||?2 AND json_extract(t.target_json,'$.runId')=?3",
        params![project.as_str(),chapter,run],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?)),
    ).optional().map_err(storage_error)?;
    let (extraction_revision, content, extraction_task) =
        extraction.ok_or(RepositoryError::TaskBindingInvalid)?;
    if !budget.has_charge(&extraction_task) {
        return Err(RepositoryError::TaskBindingInvalid);
    };
    crate::commitment_extraction::validate_write(
        db,
        project,
        &ObjectId::parse(format!("commitment-extraction-{chapter}"))
            .map_err(|_| RepositoryError::CorruptState)?,
        &content,
    )?;
    crate::commitment_extraction::validate_task_write(
        db,
        &crate::tasks::load_task(db, &extraction_task)?,
        &content,
    )?;
    let snapshot: String = db.query_row("SELECT head_snapshot_id FROM story_branches_v2 WHERE project_id=?1 AND branch_id='main'",[project.as_str()],|row|row.get(0)).map_err(storage_error)?;
    deliveries.push(GenerationChapterDelivery {
        chapter_id: chapter.into(),
        chapter_number: number,
        draft_revision_id: serde_json::from_value(assessment.content["draftRevisionId"].clone())
            .map_err(|_| RepositoryError::CorruptState)?,
        plan_revision_id: serde_json::from_value(assessment.content["planRevisionId"].clone())
            .map_err(|_| RepositoryError::CorruptState)?,
        knowledge_batch_id: serde_json::from_value(assessment.content["knowledgeBatchId"].clone())
            .map_err(|_| RepositoryError::CorruptState)?,
        commitment_extraction_revision_id: extraction_revision,
        assessment_revision_id: assessment.revision_id,
        assessment_task_id: task_id.into(),
        checked_snapshot_id: snapshot,
        source_digest: serde_json::from_value(prior["sourceDigest"].clone())
            .map_err(|_| RepositoryError::CorruptState)?,
        checked_run_version: version,
        feedback_decision_id: accepted
            .map(|decision| {
                serde_json::from_value(decision["id"].clone())
                    .map_err(|_| RepositoryError::CorruptState)
            })
            .transpose()?,
    });
    next["chapterDeliveries"] = json!(deliveries);
    validate_history(db, project, run, next)?;
    Ok(())
}
