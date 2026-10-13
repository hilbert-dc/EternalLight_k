use novel_codex_story_core::{ProjectId, RepositoryError, UsageChargeOutcome};
use serde_json::Value;

use crate::{AuthoringService, GenerationRun};

impl AuthoringService<'_> {
    /// Accepts only a terminal engine receipt delivered across the trusted host boundary.
    pub fn charge_provider_usage(
        &mut self,
        project_id: &ProjectId,
        run_id: &str,
        task_id: &str,
        receipt: &Value,
    ) -> Result<GenerationRun, RepositoryError> {
        for _ in 0..3 {
            let mut run = self.get(project_id, run_id)?;
            if run.provider_budget.has_charge(task_id) {
                self.repository.reconcile_charged_provider_usage(project_id, &format!("work-{run_id}"), task_id, receipt)?;
                return self.get(project_id, run_id);
            }
            if !run.provider_budget.has_charge(task_id) {
                if run.active_task_id.as_deref() != Some(task_id) {
                    return Err(RepositoryError::TaskBindingInvalid);
                }
                let task = self
                    .repository
                    .task(project_id, task_id)?
                    .ok_or(RepositoryError::TaskNotFound)?;
                if !matches!(task.status.as_str(), "completed" | "failed" | "cancelled") {
                    return Err(RepositoryError::TaskBindingInvalid);
                }
            }
            match run.provider_budget.charge_receipt(task_id, receipt)? {
                UsageChargeOutcome::AlreadyCharged => return Ok(run),
                UsageChargeOutcome::Applied => match self.save(project_id, run) {
                    Err(RepositoryError::AuthoringRunStale) => continue,
                    Ok(_) => return self.get(project_id, run_id),
                    Err(error) => return Err(error),
                },
            }
        }
        Err(RepositoryError::AuthoringRunStale)
    }
}

use novel_codex_story_core::{TasksRepository};
