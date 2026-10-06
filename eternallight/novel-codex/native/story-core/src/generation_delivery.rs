//! Transactional chapter delivery evidence, not literary quality or Canon authority.

use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::authoring_runs::storage_error;
use crate::{ProjectId, RepositoryError, SqliteStoryRepository};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GenerationChapterDelivery {
    pub chapter_id: String,
    pub chapter_number: i64,
    pub draft_revision_id: String,
    pub plan_revision_id: Option<String>,
    pub knowledge_batch_id: Option<String>,
    pub commitment_extraction_revision_id: Option<String>,
    pub assessment_revision_id: Option<String>,
    pub assessment_task_id: Option<String>,
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
    if deliveries.len() > crate::MAX_GENERATION_CHAPTERS as usize
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
            "SELECT EXISTS(SELECT 1 FROM story_chapters_v2 c JOIN story_revisions_v2 d ON d.project_id=c.project_id AND d.object_id='draft-'||c.chapter_id AND d.object_kind='draft' AND d.revision_id=?4 LEFT JOIN story_revisions_v2 p ON p.project_id=c.project_id AND p.object_id='artifact-plan' AND p.object_kind='plan' AND p.revision_id=?5 LEFT JOIN story_modeling_batches_v2 k ON k.project_id=c.project_id AND k.object_id=d.object_id AND k.source_revision_id=d.revision_id AND k.batch_id=?6 JOIN story_snapshots_v2 s ON s.project_id=c.project_id AND s.snapshot_id=?7 WHERE c.project_id=?1 AND c.chapter_id=?2 AND json_extract(d.content_json,'$.chapterNumber')=?3 AND (?5 IS NULL OR p.revision_id IS NOT NULL) AND (?6 IS NULL OR k.batch_id IS NOT NULL))",
            params![project.as_str(),delivery.chapter_id,delivery.chapter_number,delivery.draft_revision_id,delivery.plan_revision_id,delivery.knowledge_batch_id,delivery.checked_snapshot_id], |row|row.get(/*idx*/0),
        ).map_err(storage_error)?;
        if let Some(revision) = &delivery.commitment_extraction_revision_id {
            let valid:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM story_revisions_v2 WHERE project_id=?1 AND revision_id=?2 AND object_id='commitment-extraction-'||?3 AND json_extract(content_json,'$.payload.sourceRevisionId')=?4)",params![project.as_str(),revision,delivery.chapter_id,delivery.draft_revision_id],|r|r.get(/*idx*/0)).map_err(storage_error)?;
            if !valid {
                return Err(RepositoryError::TaskBindingInvalid);
            }
        }
        match (
            &delivery.assessment_revision_id,
            &delivery.assessment_task_id,
        ) {
            (None, None) => {}
            (Some(revision), Some(task)) => {
                let valid:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM story_revisions_v2 r JOIN story_task_outputs_v2 o ON o.revision_id=r.revision_id JOIN story_tasks_v2 t ON t.task_id=o.task_id AND t.task_id=?3 AND t.status='completed' JOIN story_work_attempts_v1 a ON a.attempt_id=t.task_id JOIN story_work_steps_v1 ws ON ws.id=a.step_id AND ws.work_task_id='work-'||?4 WHERE r.project_id=?1 AND r.revision_id=?2 AND r.object_id='scene-assessment-'||?5 AND json_extract(r.content_json,'$.draftRevisionId')=?6 AND json_extract(r.content_json,'$.knowledgeBatchId')=?7 AND json_extract(r.content_json,'$.planRevisionId')=?8)",params![project.as_str(),revision,task,run,delivery.chapter_id,delivery.draft_revision_id,delivery.knowledge_batch_id,delivery.plan_revision_id],|r|r.get(/*idx*/0)).map_err(storage_error)?;
                if !valid {
                    return Err(RepositoryError::TaskBindingInvalid);
                }
            }
            _ => return Err(RepositoryError::TaskBindingInvalid),
        }
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
                                    == json!(delivery.assessment_revision_id)
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
    validate_history(db, project, run, prior)?;
    if next["chapterDeliveries"] != prior["chapterDeliveries"]
        || (matches!(prior["status"].as_str(), Some("completed" | "cancelled"))
            && next["status"] != prior["status"])
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    if next["completedChapters"] != prior["completedChapters"] {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    validate_history(db, project, run, next)?;
    Ok(())
}
