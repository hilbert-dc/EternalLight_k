use novel_codex_story_core::{
    BranchId, ProjectId, ProviderBudget, RepositoryError, SqliteStoryRepository,
    StoredGenerationRun,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

mod feedback;
mod model_budget;
mod replan;
mod handoff;

pub use feedback::ResolveFeedback;
pub use novel_codex_story_core::{
    CreativeIntentRevision, CreativeIntentSource, GenerationFeedbackDecision as FeedbackDecision,
};
pub use replan::{RebindRun, ReplanPreview, ReplanRecord, ReplanRun, ReplanStage};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RunStatus {
    Running,
    Paused,
    Blocked,
    Completed,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RunStage {
    Coordinate,
    Finished,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationRun {
    pub id: String,
    pub generation_proposal_id: Option<String>,
    pub origin_task_id: Option<String>,
    pub root_thread_id: Option<String>,
    pub author_message_id: String,
    pub revision_predecessor: Option<novel_codex_story_core::RevisionPredecessor>,
    pub project_id: String,
    pub version: i64,
    pub status: RunStatus,
    pub stage: RunStage,
    pub intent_revisions: Vec<CreativeIntentRevision>,
    pub intent_generation: i64,
    pub pending_current_intent_generation: Option<i64>,
    pub step_token: String,
    pub active_task_id: Option<String>,
    pub base_snapshot_id: String,
    pub source_digest: String,
    pub start_chapter_number: i64,
    pub target_chapters: i64,
    pub completed_chapters: i64,
    pub chapter_deliveries: Vec<novel_codex_story_core::GenerationChapterDelivery>,
    pub current_chapter_id: Option<String>,
    pub current_chapter_number: Option<i64>,
    #[serde(deserialize_with = "novel_codex_story_core::deserialize_execution_limit")]
    pub max_task_attempts: Option<i64>,
    pub used_task_attempts: i64,
    pub provider_budget: ProviderBudget,
    pub retry_attempts: i64,
    pub max_retry_attempts: i64,
    #[serde(deserialize_with = "novel_codex_story_core::deserialize_execution_limit")]
    pub deadline_at: Option<i64>,
    pub blocked_reason: Option<String>,
    pub last_task_id: Option<String>,
    pub accepted_feedback_revision_id: Option<String>,
    pub feedback_repair_prompt: Option<String>,
    pub feedback_repair_count: i64,
    pub feedback_decisions: Vec<FeedbackDecision>,
    pub replans: Vec<ReplanRecord>,
}

pub struct StartRun<'a> {
    pub project_id: &'a ProjectId,
    pub expected_project_revision: i64,
    pub expected_base_snapshot_id: &'a str,
    pub expected_source_digest: &'a str,
    pub author_intent: &'a str,
    pub revision_predecessor: Option<novel_codex_story_core::RevisionPredecessor>,
    pub generation_proposal_id: Option<&'a str>,
    pub target_chapters: i64,
    pub max_task_attempts: Option<i64>,
    pub max_provider_requests: Option<u64>,
    pub max_provider_tokens: Option<u64>,
    pub deadline_at: Option<i64>,
    pub now: i64,
}

pub struct AuthoringService<'a> {
    repository: &'a mut SqliteStoryRepository,
}

impl<'a> AuthoringService<'a> {
    pub fn new(repository: &'a mut SqliteStoryRepository) -> Self {
        Self { repository }
    }

    pub fn start(&mut self, request: StartRun<'_>) -> Result<GenerationRun, RepositoryError> {
        let intent = request.author_intent;
        if intent.trim().is_empty()
            || intent.chars().count() > 4_000
            || !(1..=novel_codex_story_core::MAX_GENERATION_CHAPTERS)
                .contains(&request.target_chapters)
            || request
                .max_task_attempts
                .is_some_and(|limit| !(1..=32).contains(&limit))
            || request.max_provider_requests.is_some_and(|limit| {
                request
                    .max_task_attempts
                    .is_some_and(|attempts| limit < attempts as u64)
                    || limit > 2_048
            })
            || request
                .max_provider_tokens
                .is_some_and(|limit| !(1..=32_000_000).contains(&limit))
            || request
                .deadline_at
                .is_some_and(|deadline| deadline <= request.now || deadline - request.now > 86_400)
        {
            return Err(RepositoryError::InvalidContent(
                "generation permit scope or budget is invalid".into(),
            ));
        }
        let revision = self.repository.project_revision(request.project_id)?;
        if revision != request.expected_project_revision {
            return Err(RepositoryError::StaleProjectRevision {
                expected: request.expected_project_revision,
                actual: revision,
            });
        }
        if self
            .repository
            .has_open_generation_run(request.project_id)?
        {
            return Err(RepositoryError::InvalidContent(
                "an unfinished generation run already owns this creative workspace".into(),
            ));
        }
        let branch = BranchId::parse("main").expect("main branch id");
        let snapshot = self
            .repository
            .branch_head(request.project_id, &branch)?
            .ok_or(RepositoryError::BranchNotFound)?;
        let source_digest = source_digest(self.repository, request.project_id)?;
        if snapshot.snapshot_id.as_str() != request.expected_base_snapshot_id
            || source_digest != request.expected_source_digest
        {
            return Err(RepositoryError::TaskInputStale);
        }
        let position = self
            .repository
            .generation_start_position(request.project_id)?;
        let stage = RunStage::Coordinate;
        // A budget limits execution, not the semantic strategy. The agent may
        // deliver useful partial work when the granted allowance is exhausted.
        let target_chapters = position
            .completed_chapters
            .checked_add(request.target_chapters)
            .ok_or_else(|| {
                RepositoryError::InvalidContent("chapter target exceeds range".into())
            })?;
        let author_message_id = format!("author-message-{}", Uuid::new_v4().simple());
        let run = GenerationRun {
            id: format!("generation-{}", Uuid::new_v4().simple()),
            generation_proposal_id: request.generation_proposal_id.map(str::to_owned),
            origin_task_id: None,
            root_thread_id: None,
            author_message_id: author_message_id.clone(),
            revision_predecessor: request.revision_predecessor,
            project_id: request.project_id.to_string(),
            version: 0,
            status: RunStatus::Running,
            stage,
            intent_revisions: vec![CreativeIntentRevision {
                id: format!("intent-{}", Uuid::new_v4().simple()),
                generation: 0,
                instruction: intent.to_owned(),
                effective_from_chapter: position.completed_chapters + 1,
                source: CreativeIntentSource::AuthorMessage {
                    author_message_id,
                    created_at: request.now,
                },
            }],
            intent_generation: 0,
            pending_current_intent_generation: None,
            step_token: Uuid::new_v4().simple().to_string(),
            active_task_id: None,
            base_snapshot_id: snapshot.snapshot_id.to_string(),
            source_digest,
            start_chapter_number: position.completed_chapters + 1,
            target_chapters,
            completed_chapters: position.completed_chapters,
            chapter_deliveries: Vec::new(),
            current_chapter_id: None,
            current_chapter_number: None,
            max_task_attempts: request.max_task_attempts,
            used_task_attempts: 0,
            provider_budget: ProviderBudget {
                max_requests: request.max_provider_requests,
                max_tokens: request.max_provider_tokens,
                used_requests: 0,
                used_tokens: 0,
                unknown_tokens: 0,
                overrun: false,
                charges: Vec::new(),
            },
            retry_attempts: 0,
            max_retry_attempts: 2,
            deadline_at: request.deadline_at,
            blocked_reason: None,
            last_task_id: None,
            accepted_feedback_revision_id: None,
            feedback_repair_prompt: None,
            feedback_repair_count: 0,
            feedback_decisions: Vec::new(),
            replans: Vec::new(),
        };
        self.repository.create_generation_run(
            request.project_id,
            &run.id,
            &serde_json::to_string(&run).map_err(|_| RepositoryError::CorruptState)?,
        )?;
        self.get(request.project_id, &run.id)
    }

    pub fn get(
        &self,
        project_id: &ProjectId,
        run_id: &str,
    ) -> Result<GenerationRun, RepositoryError> {
        let stored = self.repository.generation_run(project_id, run_id)?;
        parse_run(stored)
    }

    pub fn control(
        &mut self,
        project_id: &ProjectId,
        run_id: &str,
        expected_version: i64,
        command: &str,
        now: i64,
    ) -> Result<GenerationRun, RepositoryError> {
        let mut run = self.get(project_id, run_id)?;
        if run.version != expected_version {
            return Err(RepositoryError::AuthoringRunStale);
        }
        run.status = match (command, run.status) {
            ("pause", RunStatus::Running | RunStatus::Blocked) => RunStatus::Paused,
            ("resume", RunStatus::Paused) => {
                if run.deadline_at.is_some_and(|deadline| now >= deadline) {
                    return Err(RepositoryError::AuthoringRunStale);
                }
                if let Some(task_id) = run.active_task_id.clone() {
                    let task = self
                        .repository
                        .task(project_id, &task_id)?
                        .ok_or(RepositoryError::TaskNotFound)?;
                    if task.was_author_paused() {
                        if !run.provider_budget.has_charge(&task_id) {
                            return Err(RepositoryError::TaskNotWritable);
                        }
                        self.repository.assert_task_inputs_current(&task)?;
                        run.active_task_id = None;
                        run.step_token = Uuid::new_v4().simple().to_string();
                        run.source_digest = source_digest(self.repository, project_id)?;
                    } else if task.status == "failed" && self.repository.can_resume_failed_after_step(project_id, run_id, &task_id)? {
                        if !run.provider_budget.has_charge(&task_id) || run.provider_budget.exhausted()
                            || run.retry_attempts >= run.max_retry_attempts
                            || run.max_task_attempts.is_some_and(|limit| run.used_task_attempts >= limit) {
                            return Err(RepositoryError::TaskNotWritable);
                        }
                        self.repository.assert_task_inputs_current(&task)?;
                        run.retry_attempts += 1;
                        run.active_task_id = None;
                        run.step_token = Uuid::new_v4().simple().to_string();
                        run.source_digest = source_digest(self.repository, project_id)?;
                    } else {
                        if !matches!(
                            task.status.as_str(),
                            "queued" | "running" | "waiting" | "completed"
                        ) {
                            return Err(RepositoryError::AuthoringRunStale);
                        }
                        self.repository.assert_task_inputs_current(&task)?;
                        if task.status == "completed" {
                            self.repository.advance_generation_decision(
                                project_id,
                                run_id,
                                run.version,
                                &task.task_id,
                            )?;
                            run = self.get(project_id, run_id)?;
                        } else {
                            run.source_digest = source_digest(self.repository, project_id)?;
                        }
                    }
                } else if source_digest(self.repository, project_id)? != run.source_digest {
                    return Err(RepositoryError::AuthoringRunStale);
                }
                RunStatus::Running
            }
            ("cancel", RunStatus::Running | RunStatus::Paused | RunStatus::Blocked) => {
                RunStatus::Cancelled
            }
            _ => return Err(RepositoryError::AuthoringRunStale),
        };
        if run.status == RunStatus::Cancelled {
            let version = run.version;
            run.version += 1;
            self.repository.cancel_generation_run(
                project_id,
                run_id,
                version,
                &serde_json::to_string(&run).map_err(|_| RepositoryError::CorruptState)?,
                now,
            )?;
            return Ok(run);
        }
        let version = run.version;
        run.version += 1;
        self.repository.update_generation_run_for_author_control(
            project_id, run_id, version,
            &serde_json::to_string(&run).map_err(|_| RepositoryError::CorruptState)?,
        )?;
        Ok(run)
    }

    pub fn list(&self, project_id: &ProjectId) -> Result<Vec<GenerationRun>, RepositoryError> {
        self.repository
            .generation_runs(project_id)?
            .into_iter()
            .map(parse_run)
            .collect()
    }

    pub fn add_future_intent(
        &mut self,
        project_id: &ProjectId,
        run_id: &str,
        expected_version: i64,
        instruction: &str,
        now: i64,
    ) -> Result<GenerationRun, RepositoryError> {
        let mut run = self.get(project_id, run_id)?;
        let active_task = if let Some(task_id) = run.active_task_id.as_deref() {
            let task = self
                .repository
                .task(project_id, task_id)?
                .ok_or(RepositoryError::TaskNotFound)?;
            if !matches!(
                task.status.as_str(),
                "queued" | "running" | "waiting" | "completed"
            ) {
                return Err(RepositoryError::AuthoringRunStale);
            }
            self.repository.assert_task_inputs_current(&task)?;
            Some(task)
        } else {
            None
        };
        let next_chapter = if run.current_chapter_id.is_some() {
            run.completed_chapters + 2
        } else {
            run.completed_chapters + 1
        };
        if run.version != expected_version
            || run.status != RunStatus::Running
            || run.stage != RunStage::Coordinate
            || (active_task.is_some() && run.current_chapter_id.is_none())
            || run.deadline_at.is_some_and(|deadline| now >= deadline)
            || next_chapter > run.target_chapters
            || (active_task.is_none()
                && source_digest(self.repository, project_id)? != run.source_digest)
        {
            return Err(RepositoryError::AuthoringRunStale);
        }
        let instruction = bounded_intent(&run, instruction)?;
        run.intent_generation += 1;
        run.intent_revisions.push(CreativeIntentRevision {
            id: format!("intent-{}", Uuid::new_v4().simple()),
            generation: run.intent_generation,
            instruction,
            effective_from_chapter: next_chapter,
            source: CreativeIntentSource::AuthorMessage {
                author_message_id: format!("author-message-{}", Uuid::new_v4().simple()),
                created_at: now,
            },
        });
        if active_task.is_none() {
            run.step_token = Uuid::new_v4().simple().to_string();
        }
        self.save(project_id, run)
    }

    pub fn revise_current_chapter(
        &mut self,
        project_id: &ProjectId,
        run_id: &str,
        expected_version: i64,
        instruction: &str,
        now: i64,
    ) -> Result<GenerationRun, RepositoryError> {
        let mut run = self.get(project_id, run_id)?;
        if run.version != expected_version
            || run.status != RunStatus::Running
            || run.stage != RunStage::Coordinate
            || run.pending_current_intent_generation.is_some()
            || run.deadline_at.is_some_and(|deadline| now >= deadline)
        {
            return Err(RepositoryError::AuthoringRunStale);
        }
        let task_id = run
            .active_task_id
            .as_deref()
            .ok_or(RepositoryError::AuthoringRunStale)?;
        let task = self
            .repository
            .task(project_id, task_id)?
            .ok_or(RepositoryError::TaskNotFound)?;
        if task.operation != "generateChapter"
            || !matches!(
                task.status.as_str(),
                "queued" | "running" | "waiting" | "completed"
            )
        {
            return Err(RepositoryError::AuthoringRunStale);
        }
        self.repository.assert_task_inputs_current(&task)?;
        let chapter = run
            .current_chapter_number
            .ok_or(RepositoryError::CorruptState)?;
        // The author changes the goal; Native decisions admit only the next necessary
        // attempt under the remaining original allowance. Optional checks are not prepaid.
        if run
            .max_task_attempts
            .is_some_and(|limit| run.used_task_attempts >= limit)
        {
            return Err(RepositoryError::InvalidContent(
                "generation budget cannot cover a current-chapter revision".into(),
            ));
        }
        let instruction = bounded_intent(&run, instruction)?;
        run.intent_generation += 1;
        run.intent_revisions.push(CreativeIntentRevision {
            id: format!("intent-{}", Uuid::new_v4().simple()),
            generation: run.intent_generation,
            instruction,
            effective_from_chapter: chapter,
            source: CreativeIntentSource::AuthorMessage {
                author_message_id: format!("author-message-{}", Uuid::new_v4().simple()),
                created_at: now,
            },
        });
        run.pending_current_intent_generation = Some(run.intent_generation);
        self.save(project_id, run)
    }

    pub fn next_action(
        &mut self,
        project_id: &ProjectId,
        run_id: &str,
        now: i64,
    ) -> Result<Value, RepositoryError> {
        let result = self
            .repository
            .next_generation_decision(project_id, run_id)?;
        let run = self.get(project_id, run_id)?;
        let _ = now;
        Ok(json!({"run":run,"action":result["action"],"stopReason":result["stopReason"]}))
    }

    pub fn advance_task(
        &mut self,
        project_id: &ProjectId,
        run_id: &str,
        expected_version: i64,
        task_id: &str,
        now: i64,
    ) -> Result<GenerationRun, RepositoryError> {
        self.repository.advance_generation_decision(
            project_id,
            run_id,
            expected_version,
            task_id,
        )?;
        let _ = now;
        self.get(project_id, run_id)
    }

    pub fn retry_failed_task(
        &mut self,
        project_id: &ProjectId,
        run_id: &str,
        expected_version: i64,
        now: i64,
    ) -> Result<GenerationRun, RepositoryError> {
        let mut run = self.get(project_id, run_id)?;
        if run.version != expected_version
            || run.status != RunStatus::Blocked
            || run.retry_attempts >= run.max_retry_attempts
            || run.deadline_at.is_some_and(|deadline| now >= deadline)
            || run
                .max_task_attempts
                .is_some_and(|limit| run.used_task_attempts >= limit)
            || run.provider_budget.exhausted()
        {
            return Err(RepositoryError::AuthoringRunStale);
        }
        let task_id = run
            .active_task_id
            .as_deref()
            .ok_or(RepositoryError::TaskBindingInvalid)?;
        let task = self
            .repository
            .task(project_id, task_id)?
            .ok_or(RepositoryError::TaskNotFound)?;
        if !matches!(task.status.as_str(), "failed" | "cancelled") {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        if !run.provider_budget.has_charge(task_id) {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        let digest = source_digest(self.repository, project_id)?;
        if task.output_refs_json == "[]" && digest != run.source_digest {
            return Err(RepositoryError::TaskInputStale);
        }
        self.repository.assert_task_inputs_current(&task)?;
        run.pending_current_intent_generation = None;
        run.source_digest = digest;
        run.retry_attempts += 1;
        run.status = RunStatus::Running;
        run.blocked_reason = None;
        run.active_task_id = None;
        run.step_token = Uuid::new_v4().simple().to_string();
        run.version += 1;
        self.repository.update_generation_run_if_source_matches(
            project_id,
            run_id,
            expected_version,
            &serde_json::to_string(&run).map_err(|_| RepositoryError::CorruptState)?,
            &run.source_digest,
        )?;
        Ok(run)
    }

    fn save(
        &mut self,
        project_id: &ProjectId,
        mut run: GenerationRun,
    ) -> Result<GenerationRun, RepositoryError> {
        let version = run.version;
        run.version += 1;
        let state = serde_json::to_string(&run).map_err(|_| RepositoryError::CorruptState)?;
        self.repository
            .update_generation_run(project_id, &run.id, version, &state)?;
        Ok(run)
    }
}

fn parse_run(stored: StoredGenerationRun) -> Result<GenerationRun, RepositoryError> {
    let run: GenerationRun =
        serde_json::from_str(&stored.state_json).map_err(|_| RepositoryError::CorruptState)?;
    if run.id != stored.run_id
        || run.project_id != stored.project_id.as_str()
        || run.version != stored.version
    {
        return Err(RepositoryError::CorruptState);
    }
    Ok(run)
}

fn bounded_intent(run: &GenerationRun, instruction: &str) -> Result<String, RepositoryError> {
    let size = instruction.chars().count();
    let current_size: usize = run
        .intent_revisions
        .iter()
        .map(|intent| intent.instruction.chars().count())
        .sum();
    if instruction.trim().is_empty()
        || size > 1_000
        || current_size + size > 8_000
        || run.intent_revisions.len() >= 16
    {
        return Err(RepositoryError::InvalidContent(
            "intent exceeds the bounded authoring scope".into(),
        ));
    }
    Ok(instruction.to_owned())
}

fn source_digest(
    repository: &SqliteStoryRepository,
    project_id: &ProjectId,
) -> Result<String, RepositoryError> {
    repository.project_source_watermarks(project_id)?["digest"]
        .as_str()
        .map(str::to_owned)
        .ok_or(RepositoryError::CorruptState)
}
