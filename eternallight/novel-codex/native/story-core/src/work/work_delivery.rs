use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

use crate::authoring_runs::storage_error;
use crate::{ProjectId, RepositoryError, WorkStep, WorkTask};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub enum WorkEvidenceState {
    Pending,
    Missing,
    Recorded,
    Superseded,
}

/// References persisted domain evidence, not the model's private reasoning or a
/// semantic certificate that the author's free-form goal has been achieved.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct WorkDeliveryEvidence {
    pub step_id: String,
    pub attempt_id: Option<String>,
    pub kind: String,
    pub reference_id: Option<String>,
    pub state: WorkEvidenceState,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct WorkDeliveryReport {
    pub delivery_review: Option<serde_json::Value>,
    pub issues: serde_json::Value,
    pub chapter_acceptance_reports: Vec<serde_json::Value>,
    pub evidence: Vec<WorkDeliveryEvidence>,
    pub pending_action_ids: Vec<String>,
    pub unsettled_attempt_ids: Vec<String>,
}

impl WorkDeliveryReport {
    pub(crate) fn status(
        &self,
        db: &Connection,
        project: &ProjectId,
        work: &WorkTask,
    ) -> Result<&'static str, RepositoryError> {
        if work.kind == "generation" {
            // Historical rolling plans need not remain today's selected plan.
            // The run's existing transactional chapter checkpoints are the
            // authority; do not replace them with ordinary candidate-head rules.
            if generation_status(db, project, &work.id)? != work.status {
                return Err(RepositoryError::TaskBindingInvalid);
            }
            if work.status == "completed"
                && self.pending_action_ids.is_empty()
                && self.unsettled_attempt_ids.is_empty()
            {
                return Ok("ready_for_review");
            }
        }
        let execution = work.status.as_str();
        let steps = &work.steps;
        let all_ended = !steps.is_empty()
            && steps
                .iter()
                .all(|step| matches!(step.status.as_str(), "completed" | "superseded"));
        let required = self
            .evidence
            .iter()
            .filter(|item| item.kind != "completionReport");
        let has_required = required.clone().next().is_some();
        if execution == "completed"
            && all_ended
            && !self.evidence.is_empty()
            && self.pending_action_ids.is_empty()
            && self.unsettled_attempt_ids.is_empty()
            && required
                .clone()
                .all(|item| item.state == WorkEvidenceState::Recorded)
        {
            return Ok(if has_required {
                "ready_for_review"
            } else {
                "discussion_ended"
            });
        }
        if self.evidence.iter().any(|item| {
            item.kind != "completionReport"
                && matches!(
                    item.state,
                    WorkEvidenceState::Recorded | WorkEvidenceState::Superseded
                )
        }) {
            Ok("partial")
        } else {
            Ok("pending")
        }
    }
}

/// GenerationRun owns execution state. Work stores a query index, not a second
/// completion authority; public generation state always comes from the Run.
pub(crate) fn generation_status(
    db: &Connection,
    project: &ProjectId,
    work: &str,
) -> Result<String, RepositoryError> {
    let (run_id, stored): (String, String) = db.query_row(
        "SELECT run_id,state_json FROM story_generation_runs_v1 WHERE project_id=?1 AND 'work-'||run_id=?2",
        params![project.as_str(),work], |row| Ok((row.get(0)?,row.get(1)?)),
    ).optional().map_err(storage_error)?.ok_or(RepositoryError::CorruptState)?;
    let hydrated = crate::work_budget_storage::hydrate(db, project, &run_id, &stored)?;
    let state: serde_json::Value =
        serde_json::from_str(&hydrated).map_err(|_| RepositoryError::CorruptState)?;
    crate::generation_author_source::validate_run(db, project, &run_id, &state)?;
    if crate::work_interludes::top(db, work)?.is_some()
        && !matches!(state["status"].as_str(), Some("paused" | "cancelled"))
    {
        return Ok("running".to_owned());
    }
    state["status"]
        .as_str()
        .map(str::to_owned)
        .ok_or(RepositoryError::CorruptState)
}

/// Derived inside the work-list/get read transaction. No duplicate persisted
/// completion flag, mutation, new scheduling authority, or nested transaction.
pub(crate) fn load(
    db: &Connection,
    project: &ProjectId,
    work_id: &str,
    steps: &[WorkStep],
) -> Result<WorkDeliveryReport, RepositoryError> {
    let mut chapter_acceptance_reports = Vec::new();
    for step in steps
        .iter()
        .rev()
        .filter(|s| s.operation == "generateChapter" && s.status == "completed")
    {
        if let Some(writer) = &step.current_attempt_id {
            let task = crate::tasks::load_task(db, writer)?;
            let target: serde_json::Value = serde_json::from_str(&task.target_json)
                .map_err(|_| RepositoryError::CorruptState)?;
            if target["chapterAcceptance"].is_object()
                && !chapter_acceptance_reports
                    .iter()
                    .any(|s: &serde_json::Value| s["chapterId"] == target["chapterId"])
            {
                let revision:Option<String>=db.query_row("SELECT revision_id FROM story_task_outputs_v2 WHERE task_id=?1 AND output_kind='draft' AND completeness='chapterComplete' ORDER BY ordinal DESC LIMIT 1",[writer],|r|r.get(0)).optional().map_err(storage_error)?;
                if let Some(revision) = revision {
                    chapter_acceptance_reports.push(crate::revision_outcome::assessment_state(
                        db, project, writer, &revision,
                    )?);
                }
            }
        }
    }
    let mut evidence = Vec::new();
    for step in steps.iter().filter(|step| step.status != "superseded") {
        let kind = if matches!(
            step.operation.as_str(),
            "advisory" | "assessChapterStructure"
        ) {
            "completionReport".to_owned()
        } else if step.operation == "reviewPlanningSeat" {
            "planningReview".to_owned()
        } else {
            step.expected_output_kind
                .clone()
                .ok_or(RepositoryError::CorruptState)?
        };
        let mut item = WorkDeliveryEvidence {
            step_id: step.id.clone(),
            attempt_id: step.current_attempt_id.clone(),
            kind,
            reference_id: None,
            state: WorkEvidenceState::Pending,
        };
        if let Some(attempt_id) = &step.current_attempt_id {
            let task = crate::tasks::load_task(db, attempt_id)?;
            let owned: bool = db.query_row(
                "SELECT EXISTS(SELECT 1 FROM story_work_attempts_v1 WHERE step_id=?1 AND attempt_id=?2)",
                params![step.id, attempt_id], |row| row.get(0),
            ).map_err(storage_error)?;
            if !owned
                || task.project_id != *project
                || task.operation != step.operation
                || (step.status == "completed" && task.status != "completed")
            {
                return Err(RepositoryError::TaskBindingInvalid);
            }
            item.state = WorkEvidenceState::Missing;
            if step.operation == "advisory" {
                for id in crate::task_hypothesis::saved_evidence(db, &task)? {
                    evidence.push(WorkDeliveryEvidence {
                        step_id: step.id.clone(),
                        attempt_id: Some(attempt_id.clone()),
                        kind: "hypothesis".into(),
                        reference_id: Some(id),
                        state: WorkEvidenceState::Recorded,
                    });
                }
                // complete_task(summary) is a completion declaration, not a stored
                // final Agent reply. Never present it as verified reply delivery.
                if task.status == "completed" && !task.result_summary.trim().is_empty() {
                    item.reference_id = Some(task.task_id.clone());
                    item.state = WorkEvidenceState::Recorded;
                }
            } else if step.operation == "assessChapterStructure" {
                crate::chapter_structure::assert_task(db, &task)?;
                if task.status == "completed" && !task.result_summary.trim().is_empty() {
                    item.reference_id = Some(task.task_id.clone());
                    item.state = WorkEvidenceState::Recorded;
                }
            } else if step.operation == "reviewPlanningSeat" {
                let manifest = serde_json::from_str(&task.input_manifest_json)
                    .map_err(|_| RepositoryError::CorruptState)?;
                let target = serde_json::from_str(&task.target_json)
                    .map_err(|_| RepositoryError::CorruptState)?;
                crate::planning_review_author_source::validate_task(db, &task, &manifest, &target)?;
                item.reference_id = db.query_row("SELECT p.report_id FROM story_planning_review_reports_v2 p
                    JOIN story_planning_review_runs_v2 r ON r.run_id=p.run_id AND r.project_id=?2 AND r.work_task_id=?3
                    JOIN story_planning_review_seats_v2 seat ON seat.run_id=p.run_id AND seat.role=p.role AND seat.task_id=p.task_id
                    WHERE p.task_id=?1 AND p.run_id=json_extract(?4,'$.reviewRunId') AND p.role=json_extract(?4,'$.role')
                    AND r.revision_id=json_extract(?4,'$.revisionId') AND r.artifact_kind=json_extract(?4,'$.artifactKind')",
                    params![attempt_id,project.as_str(),work_id,task.target_json], |row|row.get(0)).optional().map_err(storage_error)?;
                if item.reference_id.is_some() {
                    item.state = WorkEvidenceState::Recorded;
                }
            } else if step.operation == "reviewQuality" {
                item.reference_id = db.query_row(
                    "SELECT id FROM story_quality_packets_v1 WHERE project_id=?1 AND task_id=?2 AND id=json_extract(?3,'$.qualityPacketId') AND report_json IS NOT NULL AND json_extract(report_json,'$.taskId')=task_id",
                    params![project.as_str(),attempt_id,task.target_json], |row| row.get(0),
                ).optional().map_err(storage_error)?;
                if item.reference_id.is_some() {
                    item.state = WorkEvidenceState::Recorded;
                }
            } else if step.operation == "modelKnowledge" {
                let batch: Option<(String, bool)> = db.query_row(
                    "SELECT b.batch_id,EXISTS(SELECT 1 FROM story_knowledge_heads_v2 k
                     JOIN story_object_heads_v2 h ON h.project_id=k.project_id AND h.object_id=k.object_id
                     WHERE k.project_id=b.project_id AND k.object_id=b.object_id AND k.batch_id=b.batch_id
                     AND b.status='ready' AND COALESCE(h.candidate_revision_id,h.accepted_revision_id)=b.source_revision_id)
                     FROM story_modeling_batches_v2 b WHERE b.project_id=?1 AND b.publisher_task_id=?2
                     AND b.batch_id=json_extract(?3,'$.batchId') AND b.input_project_revision=?4
                     AND b.published_at IS NOT NULL AND b.status IN ('ready','stale')",
                    params![project.as_str(),attempt_id,task.target_json,task.input_revision],
                    |row| Ok((row.get(0)?,row.get(1)?)),
                ).optional().map_err(storage_error)?;
                if let Some((id, current)) = batch {
                    item.reference_id = Some(id);
                    item.state = if current {
                        WorkEvidenceState::Recorded
                    } else {
                        WorkEvidenceState::Superseded
                    };
                }
            } else {
                // output_refs_json is presentation metadata; the canonical output
                // registry, owned revision and frozen object are the proof.
                let output: Option<(String, bool)> = db.query_row(
                    "SELECT r.revision_id,COALESCE(h.candidate_revision_id,h.accepted_revision_id)=r.revision_id
                     FROM story_task_outputs_v2 o JOIN story_revisions_v2 r ON r.revision_id=o.revision_id
                     LEFT JOIN story_object_heads_v2 h ON h.project_id=r.project_id AND h.object_id=r.object_id
                     WHERE o.task_id=?1 AND o.ordinal=(SELECT MAX(ordinal) FROM story_task_outputs_v2 WHERE task_id=?1)
                     AND r.project_id=?2 AND o.output_kind=?3 AND r.object_kind=?3
                     AND r.object_id=json_extract(?4,'$.outputObjectId')
                     AND (?3!='draft' OR o.completeness='chapterComplete')",
                    params![attempt_id,project.as_str(),item.kind,task.input_manifest_json],
                    |row| Ok((row.get(0)?,row.get::<_,Option<bool>>(1)?.unwrap_or(false))),
                ).optional().map_err(storage_error)?;
                if let Some((id, current)) = output {
                    item.reference_id = Some(id);
                    item.state = if current {
                        WorkEvidenceState::Recorded
                    } else {
                        WorkEvidenceState::Superseded
                    };
                }
            }
            if item.reference_id.is_none()
                && matches!(task.status.as_str(), "queued" | "running" | "waiting")
            {
                item.state = WorkEvidenceState::Pending;
            }
        }
        evidence.push(item);
    }
    let mut actions = db
        .prepare(
            "SELECT r.action_id FROM story_task_action_requests_v1 r
         JOIN story_work_attempts_v1 a ON a.attempt_id=r.source_task_id
         JOIN story_work_steps_v1 s ON s.id=a.step_id
         WHERE s.work_task_id=?1 AND r.project_id=?2 AND r.status='proposed' ORDER BY r.rowid",
        )
        .map_err(storage_error)?;
    let pending_action_ids = actions
        .query_map(params![work_id, project.as_str()], |row| row.get(0))
        .map_err(storage_error)?
        .map(|row| row.map_err(storage_error))
        .collect::<Result<_, _>>()?;
    let mut grants = db.prepare(
        "SELECT task_id FROM story_task_provider_budgets_v1 WHERE work_task_id=?1 AND settled=0 ORDER BY rowid",
    ).map_err(storage_error)?;
    let unsettled_attempt_ids = grants
        .query_map([work_id], |row| row.get(0))
        .map_err(storage_error)?
        .map(|row| row.map_err(storage_error))
        .collect::<Result<_, _>>()?;
    let delivery_review: Option<String> = db.query_row("SELECT r.content_json FROM story_work_delivery_reviews_v1 r JOIN story_work_steps_v1 s ON s.current_attempt_id=r.decision_task_id WHERE r.work_task_id=?1 ORDER BY s.ordinal DESC LIMIT 1", [work_id], |row| row.get(0)).optional().map_err(storage_error)?;
    let delivery_review = delivery_review
        .map(|raw| serde_json::from_str(&raw).map_err(|_| RepositoryError::CorruptState))
        .transpose()?;
    Ok(WorkDeliveryReport {
        issues: crate::work_issues::summary(db, project, work_id)?,
        chapter_acceptance_reports,
        delivery_review,
        evidence,
        pending_action_ids,
        unsettled_attempt_ids,
    })
}
