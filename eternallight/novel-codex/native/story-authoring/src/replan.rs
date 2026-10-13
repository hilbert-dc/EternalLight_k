use novel_codex_story_core::{BranchId, ProjectId, RepositoryError};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    AuthoringService, CreativeIntentRevision, CreativeIntentSource, GenerationRun, RunStage,
    RunStatus, bounded_intent, source_digest,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub enum ReplanStage {
    Brief,
    Outline,
    RollingPlan,
    CurrentChapter,
}

#[derive(Clone, Debug, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ReplanPreview {
    pub run: GenerationRun,
    pub project_revision: i64,
    pub source_digest: String,
    pub available_from: Vec<ReplanStage>,
    pub active_attempt_status: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
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

/// Refreshing execution context is not a new author instruction. Both paths
/// share checkpoint guards, but only an author replan changes intent identity.
pub struct RebindRun<'a> {
    pub project_id: &'a ProjectId,
    pub run_id: &'a str,
    pub expected_version: i64,
    pub expected_project_revision: i64,
    pub expected_source_digest: &'a str,
    pub restart_from: ReplanStage,
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
            vec![
                ReplanStage::Brief,
                ReplanStage::Outline,
                ReplanStage::RollingPlan,
            ]
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
        self.refresh_checkpoint(
            RebindRun {
                project_id: request.project_id,
                run_id: request.run_id,
                expected_version: request.expected_version,
                expected_project_revision: request.expected_project_revision,
                expected_source_digest: request.expected_source_digest,
                restart_from: request.restart_from,
                now: request.now,
            },
            Some(request.instruction), None,
        )
    }

    pub fn rebind(&mut self, request: RebindRun<'_>) -> Result<GenerationRun, RepositoryError> {
        self.refresh_checkpoint(request, None, None)
    }

    pub(crate) fn rebind_handoff(&mut self, request:RebindRun<'_>, source_task_id:&str)->Result<GenerationRun,RepositoryError>{
        self.refresh_checkpoint(request,None,Some(source_task_id))
    }

    fn refresh_checkpoint(
        &mut self,
        request: RebindRun<'_>,
        author_instruction: Option<&str>,
        handoff_source: Option<&str>,
    ) -> Result<GenerationRun, RepositoryError> {
        let mut run = self.get(request.project_id, request.run_id)?;
        let chapter_replan = request.restart_from == ReplanStage::CurrentChapter;
        if run.version != request.expected_version
            || !matches!(run.status, RunStatus::Paused | RunStatus::Blocked)
            || run.stage != RunStage::Coordinate
            || (!chapter_replan
                && (run.current_chapter_id.is_some() || run.current_chapter_number.is_some()))
            || run.completed_chapters >= run.target_chapters
            || run
                .deadline_at
                .is_some_and(|deadline| request.now >= deadline)
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
        if chapter_replan {
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
        } else if self
            .repository
            .generation_start_position(request.project_id)?
            .completed_chapters
            != run.completed_chapters
        {
            return Err(RepositoryError::TaskInputStale);
        }
        if run
            .max_task_attempts
            .is_some_and(|limit| run.used_task_attempts >= limit)
            || run.provider_budget.exhausted()
        {
            return Err(RepositoryError::InvalidContent(
                "remaining generation allowance is exhausted".into(),
            ));
        }
        let instruction = author_instruction
            .map(|value| bounded_intent(&run, value))
            .transpose()?;
        let branch = BranchId::parse("main").expect("static branch id");
        let snapshot = self
            .repository
            .branch_head(request.project_id, &branch)?
            .ok_or(RepositoryError::BranchNotFound)?;
        if instruction.is_some() {
            run.intent_generation += 1;
        }
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
        if let Some(instruction) = instruction {
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
        }
        run.stage = RunStage::Coordinate;
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
        let encoded=serde_json::to_string(&run).map_err(|_|RepositoryError::CorruptState)?;
        if let Some(source)=handoff_source {
            self.repository.update_generation_run_for_handoff(request.project_id,request.run_id,request.expected_version,&encoded,&run.source_digest,source)?;
        } else {
            self.repository.update_generation_run_if_source_matches(request.project_id,request.run_id,request.expected_version,&encoded,&run.source_digest)?;
        }
        Ok(run)
    }
}

use novel_codex_story_core::{AuthoringRunsRepository, GenerationCheckpointRepository, ProjectRevisionRepository, TasksRepository};
