use crate::{AuthoringService, RebindRun, ReplanStage, RunStatus, source_digest};
use novel_codex_story_core::{ProjectId, RepositoryError};
use serde_json::{Value, json};

impl AuthoringService<'_> {
    /// Only a persisted, fulfilled model decision can start this transition.
    /// A repeated call reuses its atomic receipt and never resumes a later
    /// author pause or obtains a new range/allowance.
    pub fn execute_generation_handoff(
        &mut self,
        project: &ProjectId,
        source: &str,
        now: i64,
    ) -> Result<Value, RepositoryError> {
        let admission = self
            .repository
            .generation_handoff_execution(project, source)?;
        let proposal = &admission["proposal"];
        let id = proposal["resumeRun"]["id"]
            .as_str()
            .ok_or(RepositoryError::CorruptState)?;
        if !admission["receipt"].is_null() {
            return Ok(
                json!({"run":self.get(project,id)?,"receipt":admission["receipt"],"shouldExecute":admission["shouldExecute"]}),
            );
        }
        let mut run = self.get(project, id)?;
        if !matches!(run.status, RunStatus::Paused | RunStatus::Blocked)
            || run.deadline_at.is_some_and(|deadline| now >= deadline)
            || run.provider_budget.exhausted()
            || run
                .max_task_attempts
                .is_some_and(|max| run.used_task_attempts >= max)
        {
            return Err(RepositoryError::AuthoringRunStale);
        }
        let digest = source_digest(self.repository, project)?;
        if run.status == RunStatus::Paused
            && run.active_task_id.is_none()
            && run.source_digest == digest
        {
            let version = run.version;
            run.status = RunStatus::Running;
            run.blocked_reason = None;
            run.version += 1;
            self.repository.update_generation_run_for_handoff(
                project,
                id,
                version,
                &serde_json::to_string(&run).map_err(|_| RepositoryError::CorruptState)?,
                &digest,
                source,
            )?;
        } else {
            // Rebinding is checkpoint recovery, not a new author instruction.
            // Native verifies terminal attempt receipts and unknown writes.
            let preview = self.preview_replan(project, id)?;
            let from = if run.current_chapter_id.is_some() {
                ReplanStage::CurrentChapter
            } else {
                ReplanStage::RollingPlan
            };
            run = self.rebind_handoff(
                RebindRun {
                    project_id: project,
                    run_id: id,
                    expected_version: run.version,
                    expected_project_revision: preview.project_revision,
                    expected_source_digest: &preview.source_digest,
                    restart_from: from,
                    now,
                },
                source,
            )?;
        }
        let saved = self
            .repository
            .generation_handoff_execution(project, source)?;
        Ok(json!({"run":run,"receipt":saved["receipt"],"shouldExecute":saved["shouldExecute"]}))
    }
}

use novel_codex_story_core::{AuthoringRunsRepository, GenerationHandoffExecutionRepository};
