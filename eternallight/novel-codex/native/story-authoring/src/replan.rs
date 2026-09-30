use novel_codex_story_core::{BranchId, ProjectId, RepositoryError};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    AuthoringService, CreativeIntentRevision, CreativeIntentSource, GenerationRun, RunStage,
    RunStatus, bounded_intent, source_digest,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReplanStage {
    Brief,
    Outline,
    RollingPlan,
    CurrentChapter,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplanPreview {
    pub run: GenerationRun,
    pub project_revision: i64,
    pub source_digest: String,
    pub available_from: Vec<ReplanStage>,
    pub active_attempt_status: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplanRecord {
    pub previous_version: i64,
    pub previous_stage: RunStage,
    pub previous_task_id: Option<String>,
    pub restart_from: ReplanStage,
    pub previous_source_digest: String,
    pub previous_snapshot_id: String,
    pub source_digest: String,
    pub base_snapshot_id: String,
    pub intent_generation: i64,
    pub created_at: i64,
}

pub struct ReplanRun<'a> {
    pub project_id: &'a ProjectId,
    pub run_id: &'a str,
    pub expected_version: i64,
    pub expected_project_revision: i64,
    pub expected_source_digest: &'a str,
    pub restart_from: ReplanStage,
    pub instruction: &'a str,
    pub now: i64,
}

impl AuthoringService<'_> {
    pub fn preview_replan(
        &self,
        project_id: &ProjectId,
        run_id: &str,
    ) -> Result<ReplanPreview, RepositoryError> {
        let run = self.get(project_id, run_id)?;
        let digest = source_digest(self.repository, project_id)?;
        let project_revision = self.repository.project_revision(project_id)?;
        let active_attempt_status = run
            .active_task_id
            .as_deref()
            .map(|id| {
                self.repository.task(project_id, id).and_then(|task| {
                    task.map(|value| value.status)
                        .ok_or(RepositoryError::TaskNotFound)
                })
            })
            .transpose()?;
        let available_from = if let Some(chapter_id) = run.current_chapter_id.as_deref() {
            self.repository.validate_generation_chapter_checkpoint(
                project_id,
                chapter_id,
                run.completed_chapters + 1,
            )?;
            vec![ReplanStage::CurrentChapter]
        } else {
            let position = self.repository.generation_start_position(project_id)?;
            let mut stages = vec![ReplanStage::Brief];
            if position.first_stage != "brief" {
                stages.push(ReplanStage::Outline);
            }
            if position.first_stage == "rollingPlan" {
                stages.push(ReplanStage::RollingPlan);
            }
            stages
        };
        if self.get(project_id, run_id)?.version != run.version
            || source_digest(self.repository, project_id)? != digest
        {
            return Err(RepositoryError::AuthoringRunStale);
        }
        Ok(ReplanPreview {
            run,
            project_revision,
            source_digest: digest,
            available_from,
            active_attempt_status,
        })
    }

    pub fn replan(&mut self, request: ReplanRun<'_>) -> Result<GenerationRun, RepositoryError> {
        let mut run = self.get(request.project_id, request.run_id)?;
        let chapter_replan = request.restart_from == ReplanStage::CurrentChapter;
        if run.version != request.expected_version
            || !matches!(run.status, RunStatus::Paused | RunStatus::Blocked)
            || (!chapter_replan
                && !matches!(
                    run.stage,
                    RunStage::Brief
                        | RunStage::Outline
                        | RunStage::RollingPlan
                        | RunStage::MaterializeChapter
                ))
            || (chapter_replan
                && !matches!(
                    run.stage,
                    RunStage::Draft
                        | RunStage::Extract
                        | RunStage::Commitments
                        | RunStage::Assess
                        | RunStage::Check
                ))
            || (!chapter_replan
                && (run.current_chapter_id.is_some() || run.current_chapter_number.is_some()))
            || run.completed_chapters >= run.target_chapters
            || request.now >= run.deadline_at
            || run.replans.len() >= 8
        {
            return Err(RepositoryError::AuthoringRunStale);
        }
        if let Some(task_id) = run.active_task_id.as_deref() {
            if !run.provider_budget.has_charge(task_id) {
                return Err(RepositoryError::TaskBindingInvalid);
            }
            let task = self
                .repository
                .task(request.project_id, task_id)?
                .ok_or(RepositoryError::TaskNotFound)?;
            if !matches!(task.status.as_str(), "completed" | "failed" | "cancelled") {
                return Err(RepositoryError::TaskNotWritable);
            }
        }
        let digest = source_digest(self.repository, request.project_id)?;
        if digest != request.expected_source_digest
            || self.repository.project_revision(request.project_id)?
                != request.expected_project_revision
        {
            return Err(RepositoryError::TaskInputStale);
        }
        let (stage, preparation_calls) = if chapter_replan {
            let chapter_id = run
                .current_chapter_id
                .as_deref()
                .ok_or(RepositoryError::TaskBindingInvalid)?;
            if run.current_chapter_number != Some(run.completed_chapters + 1) {
                return Err(RepositoryError::TaskBindingInvalid);
            }
            self.repository.validate_generation_chapter_checkpoint(
                request.project_id,
                chapter_id,
                run.completed_chapters + 1,
            )?;
            (RunStage::Draft, -1)
        } else {
            let position = self
                .repository
                .generation_start_position(request.project_id)?;
            if position.completed_chapters != run.completed_chapters {
                return Err(RepositoryError::InvalidContent(
                    "replanning must reconcile unfinished or externally added chapters first"
                        .into(),
                ));
            }
            let (stage, preparation_calls) = match request.restart_from {
                ReplanStage::Brief => (RunStage::Brief, 2),
                ReplanStage::Outline if position.first_stage != "brief" => (RunStage::Outline, 1),
                ReplanStage::RollingPlan if position.first_stage == "rollingPlan" => {
                    (RunStage::RollingPlan, 0)
                }
                ReplanStage::Outline | ReplanStage::RollingPlan => {
                    return Err(RepositoryError::InvalidContent(
                        "replanning cannot skip missing prerequisite artifacts".into(),
                    ));
                }
                ReplanStage::CurrentChapter => return Err(RepositoryError::TaskBindingInvalid),
            };
            (stage, preparation_calls)
        };
        let required = (run.target_chapters - run.completed_chapters) * 5 + preparation_calls;
        if run.max_task_attempts - run.used_task_attempts < required
            || run.provider_budget.remaining_requests() < required as u64
            || run.provider_budget.exhausted()
        {
            return Err(RepositoryError::InvalidContent(
                "remaining generation budget cannot cover replanning".into(),
            ));
        }
        let instruction = bounded_intent(&run, request.instruction)?;
        let branch = BranchId::parse("main").expect("static branch id");
        let snapshot = self
            .repository
            .branch_head(request.project_id, &branch)?
            .ok_or(RepositoryError::BranchNotFound)?;
        run.intent_generation += 1;
        run.replans.push(ReplanRecord {
            previous_version: run.version,
            previous_stage: run.stage,
            previous_task_id: run.active_task_id.clone(),
            restart_from: request.restart_from,
            previous_source_digest: run.source_digest.clone(),
            previous_snapshot_id: run.base_snapshot_id.clone(),
            source_digest: digest.clone(),
            base_snapshot_id: snapshot.snapshot_id.to_string(),
            intent_generation: run.intent_generation,
            created_at: request.now,
        });
        run.intent_revisions.push(CreativeIntentRevision {
            id: format!("intent-{}", Uuid::new_v4().simple()),
            generation: run.intent_generation,
            instruction,
            effective_from_chapter: run.completed_chapters + 1,
            source: CreativeIntentSource::AuthorMessage {
                author_message_id: format!("author-message-{}", Uuid::new_v4().simple()),
                created_at: request.now,
            },
        });
        run.stage = stage;
        run.active_task_id = None;
        run.pending_current_intent_generation = None;
        run.accepted_feedback_revision_id = None;
        run.feedback_repair_prompt = None;
        run.status = RunStatus::Running;
        run.blocked_reason = None;
        run.base_snapshot_id = snapshot.snapshot_id.to_string();
        run.source_digest = digest;
        run.step_token = Uuid::new_v4().simple().to_string();
        run.version += 1;
        self.repository.update_generation_run_if_source_matches(
            request.project_id,
            request.run_id,
            request.expected_version,
            &serde_json::to_string(&run).map_err(|_| RepositoryError::CorruptState)?,
            &run.source_digest,
        )?;
        Ok(run)
    }
}
