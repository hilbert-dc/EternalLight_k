use novel_codex_story_core::{
    BranchId, KnowledgePerspective, KnowledgeStateQuery, ProjectId, ProviderBudget,
    RepositoryError, SqliteStoryRepository, StoredGenerationRun,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

mod commitments;
mod feedback;
mod model_budget;
mod replan;

pub use feedback::ResolveFeedback;
use novel_codex_story_core::generation_author_intent;
pub use novel_codex_story_core::{
    CreativeIntentRevision, CreativeIntentSource, GenerationFeedbackDecision as FeedbackDecision,
};
pub use replan::{ReplanPreview, ReplanRecord, ReplanRun, ReplanStage};

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
    Brief,
    Outline,
    RollingPlan,
    MaterializeChapter,
    Draft,
    Extract,
    Commitments,
    Assess,
    Check,
    Finished,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationRun {
    pub id: String,
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
    pub max_task_attempts: i64,
    pub used_task_attempts: i64,
    pub provider_budget: ProviderBudget,
    pub retry_attempts: i64,
    pub max_retry_attempts: i64,
    pub deadline_at: i64,
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
    pub target_chapters: i64,
    pub max_task_attempts: i64,
    pub max_provider_requests: u64,
    pub max_provider_tokens: u64,
    pub deadline_at: i64,
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
            || !(1..=3).contains(&request.target_chapters)
            || !(1..=32).contains(&request.max_task_attempts)
            || request.max_provider_requests < request.max_task_attempts as u64
            || request.max_provider_requests > 2_048
            || !(1..=32_000_000).contains(&request.max_provider_tokens)
            || request.deadline_at <= request.now
            || request.deadline_at - request.now > 86_400
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
        let stage = match position.first_stage.as_str() {
            "brief" => RunStage::Brief,
            "outline" => RunStage::Outline,
            "rollingPlan" => RunStage::RollingPlan,
            _ => return Err(RepositoryError::CorruptState),
        };
        let required_calls = request.target_chapters * 5
            + match stage {
                RunStage::Brief => 2,
                RunStage::Outline => 1,
                RunStage::RollingPlan => 0,
                _ => return Err(RepositoryError::CorruptState),
            };
        if request.max_task_attempts < required_calls {
            return Err(RepositoryError::InvalidContent(
                "generation permit cannot cover its required task attempts".into(),
            ));
        }
        let target_chapters = position
            .completed_chapters
            .checked_add(request.target_chapters)
            .ok_or_else(|| {
                RepositoryError::InvalidContent("chapter target exceeds range".into())
            })?;
        let author_message_id = format!("author-message-{}", Uuid::new_v4().simple());
        let run = GenerationRun {
            id: format!("generation-{}", Uuid::new_v4().simple()),
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
        Ok(run)
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
                if now >= run.deadline_at {
                    return Err(RepositoryError::AuthoringRunStale);
                }
                if let Some(task_id) = run.active_task_id.clone() {
                    let task = self
                        .repository
                        .task(project_id, &task_id)?
                        .ok_or(RepositoryError::TaskNotFound)?;
                    if !matches!(
                        task.status.as_str(),
                        "queued" | "running" | "waiting" | "completed"
                    ) {
                        return Err(RepositoryError::AuthoringRunStale);
                    }
                    self.repository.assert_task_inputs_current(&task)?;
                    if task.status == "completed" {
                        advance_completed_task(self.repository, project_id, &mut run, &task)?;
                    } else {
                        run.source_digest = source_digest(self.repository, project_id)?;
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
        self.save(project_id, run)
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
        let next_chapter = if active_task.is_some()
            || matches!(
                run.stage,
                RunStage::Extract | RunStage::Commitments | RunStage::Assess | RunStage::Check
            ) {
            run.completed_chapters + 2
        } else {
            run.completed_chapters + 1
        };
        if run.version != expected_version
            || run.status != RunStatus::Running
            || (active_task.is_some()
                && !matches!(
                    run.stage,
                    RunStage::Draft | RunStage::Extract | RunStage::Commitments | RunStage::Assess
                ))
            || now >= run.deadline_at
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
            || run.stage != RunStage::Draft
            || run.pending_current_intent_generation.is_some()
            || now >= run.deadline_at
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
        if !matches!(
            task.status.as_str(),
            "queued" | "running" | "waiting" | "completed"
        ) {
            return Err(RepositoryError::AuthoringRunStale);
        }
        self.repository.assert_task_inputs_current(&task)?;
        let chapter = run
            .current_chapter_number
            .ok_or(RepositoryError::CorruptState)?;
        let remaining_chapters = run.target_chapters - chapter;
        // The in-flight Draft attempt was already charged when its task was claimed.
        let required_calls = 4 + 5 * remaining_chapters;
        if run.max_task_attempts - run.used_task_attempts < required_calls {
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
        let mut run = self.get(project_id, run_id)?;
        if run.status != RunStatus::Running {
            return Ok(json!({"run": run, "action": null}));
        }
        if now >= run.deadline_at {
            run.status = RunStatus::Blocked;
            run.blocked_reason = Some("deadline_exceeded".into());
            run = self.save(project_id, run)?;
            return Ok(json!({"run": run, "action": null}));
        }
        if let Some(task_id) = run.active_task_id.as_deref() {
            let task = self
                .repository
                .task(project_id, task_id)?
                .ok_or(RepositoryError::TaskNotFound)?;
            if matches!(task.status.as_str(), "failed" | "cancelled") {
                run.status = RunStatus::Blocked;
                run.blocked_reason = Some(if task.output_refs_json == "[]" {
                    format!("agent_task_{}", task.status)
                } else {
                    format!("agent_task_{}_after_output", task.status)
                });
                run = self.save(project_id, run)?;
                return Ok(json!({"run": run, "action": null}));
            }
            return Ok(json!({"run": run, "action": {
                "kind": "awaitTask", "taskId": task_id, "taskStatus": task.status
            }}));
        }
        let blocked_reason =
            if run.used_task_attempts >= run.max_task_attempts && needs_model(run.stage) {
                Some("model_call_budget_exhausted")
            } else if run.provider_budget.exhausted() && needs_model(run.stage) {
                Some("provider_budget_exhausted")
            } else if source_digest(self.repository, project_id)? != run.source_digest {
                Some("source_heads_changed")
            } else {
                None
            };
        if let Some(reason) = blocked_reason {
            run.status = RunStatus::Blocked;
            run.blocked_reason = Some(reason.into());
            run = self.save(project_id, run)?;
            return Ok(json!({"run": run, "action": null}));
        }
        let assessment_target = if run.stage == RunStage::Assess {
            let chapter_id = run
                .current_chapter_id
                .as_deref()
                .ok_or(RepositoryError::CorruptState)?;
            let binding = self
                .repository
                .chapter_bindings(project_id)?
                .into_iter()
                .find(|binding| binding.chapter_id == chapter_id)
                .ok_or(RepositoryError::TaskBindingInvalid)?;
            let knowledge = self.repository.chapter_knowledge(project_id, chapter_id)?;
            if knowledge["status"] != "ready" {
                return Err(RepositoryError::TaskInputStale);
            }
            let input_snapshot_id = self
                .repository
                .branch_head(
                    project_id,
                    &BranchId::parse("main").expect("static branch id"),
                )?
                .ok_or(RepositoryError::BranchNotFound)?
                .snapshot_id;
            Some(json!({
                "type": "sceneAssessment", "runId": run.id,
                "stepToken": run.step_token, "runVersion": run.version,
                "intentGeneration": run.intent_generation,
                "chapterId": chapter_id, "chapterNumber": run.current_chapter_number,
                "planRevisionId": binding.plan_revision_id,
                "draftRevisionId": knowledge["sourceRevisionId"],
                "knowledgeBatchId": knowledge["batchId"],
                "inputSnapshotId": input_snapshot_id.as_str()
            }))
        } else {
            None
        };
        let mut action = match run.stage {
            RunStage::Brief | RunStage::Outline => {
                let operation = match run.stage {
                    RunStage::Brief => "generateBrief",
                    RunStage::Outline => "generateOutline",
                    _ => unreachable!(),
                };
                json!({"kind": "agentTask", "operation": operation,
                    "target": {"type": "artifact", "runId": run.id, "stepToken": run.step_token,
                        "runVersion": run.version, "intentGeneration": run.intent_generation},
                    "authorIntent": effective_intent(&run, run.completed_chapters + 1)?})
            }
            RunStage::RollingPlan => json!({"kind": "agentTask", "operation": "generatePlan",
                "target": {"type": "artifact", "runId": run.id, "stepToken": run.step_token,
                    "runVersion": run.version, "intentGeneration": run.intent_generation,
                    "planFromChapter": run.completed_chapters + 1},
                "authorIntent": effective_intent(&run, run.completed_chapters + 1)?}),
            RunStage::MaterializeChapter => json!({"kind": "materializeChapter",
                "chapterNumber": run.completed_chapters + 1}),
            RunStage::Draft => json!({"kind": "agentTask", "operation": "generateChapter",
            "target": {"type": "chapter", "runId": run.id, "stepToken": run.step_token,
                "runVersion": run.version, "intentGeneration": run.intent_generation,
                "chapterId": run.current_chapter_id, "chapterNumber": run.current_chapter_number,
                "readingChapter": run.current_chapter_number, "sceneOrder": 1,
                "scope": {"type": "wholeChapter"}},
                "authorIntent": effective_intent(&run, run.completed_chapters + 1)?}),
            RunStage::Extract => json!({"kind": "agentTask", "operation": "modelKnowledge",
                "target": {"type": "knowledge", "runId": run.id, "stepToken": run.step_token,
                    "runVersion": run.version, "intentGeneration": run.intent_generation,
                    "chapterId": run.current_chapter_id}, "authorIntent": effective_intent(&run, run.completed_chapters + 1)?}),
            RunStage::Commitments => commitments::action(self.repository, project_id, &run)?,
            RunStage::Assess => json!({"kind": "agentTask", "operation": "assessSceneOutcome",
                "target": assessment_target, "authorIntent": effective_intent(&run, run.completed_chapters + 1)?}),
            RunStage::Check => json!({"kind": "checkKnowledge"}),
            RunStage::Finished => Value::Null,
        };
        if run.stage == RunStage::Draft {
            if let Some(prompt) = &run.feedback_repair_prompt {
                action["candidateInstruction"] = json!(prompt);
            }
            let binding = self
                .repository
                .chapter_bindings(project_id)?
                .into_iter()
                .find(|binding| {
                    Some(binding.chapter_id.as_str()) == run.current_chapter_id.as_deref()
                        && Some(binding.chapter_number) == run.current_chapter_number
                })
                .ok_or(RepositoryError::TaskBindingInvalid)?;
            let plan = self.repository.bound_chapter_plan(project_id, &binding)?;
            action["target"]["povCharacterId"] = plan["povCharacterId"].clone();
            action["target"]["storyTime"] = plan["storyTime"].clone();
        }
        Ok(json!({"run": run, "action": action}))
    }

    pub fn advance_task(
        &mut self,
        project_id: &ProjectId,
        run_id: &str,
        expected_version: i64,
        task_id: &str,
        now: i64,
    ) -> Result<GenerationRun, RepositoryError> {
        let mut run = self.get(project_id, run_id)?;
        assert_step(&run, expected_version, now)?;
        let task = self
            .repository
            .task(project_id, task_id)?
            .ok_or(RepositoryError::TaskNotFound)?;
        self.repository.assert_task_inputs_current(&task)?;
        advance_completed_task(self.repository, project_id, &mut run, &task)?;
        self.save(project_id, run)
    }

    pub fn materialize_chapter(
        &mut self,
        project_id: &ProjectId,
        run_id: &str,
        expected_version: i64,
        now: i64,
    ) -> Result<GenerationRun, RepositoryError> {
        let mut run = self.get(project_id, run_id)?;
        assert_step(&run, expected_version, now)?;
        if run.stage != RunStage::MaterializeChapter
            || source_digest(self.repository, project_id)? != run.source_digest
        {
            return Err(RepositoryError::AuthoringRunStale);
        }
        let number = run.completed_chapters + 1;
        run.current_chapter_id = Some(format!("chapter-{}", Uuid::new_v4().simple()));
        run.current_chapter_number = Some(number);
        run.stage = RunStage::Draft;
        run.step_token = Uuid::new_v4().simple().to_string();
        run.version += 1;
        parse_run(self.repository.materialize_generation_chapter(
            project_id,
            run_id,
            expected_version,
            &serde_json::to_string(&run).map_err(|_| RepositoryError::CorruptState)?,
            now,
        )?)
    }

    pub fn skip_ready_extract(
        &mut self,
        project_id: &ProjectId,
        run_id: &str,
        expected_version: i64,
        now: i64,
    ) -> Result<GenerationRun, RepositoryError> {
        let mut run = self.get(project_id, run_id)?;
        assert_step(&run, expected_version, now)?;
        if run.stage != RunStage::Extract
            || run.active_task_id.is_some()
            || source_digest(self.repository, project_id)? != run.source_digest
        {
            return Err(RepositoryError::AuthoringRunStale);
        }
        let object_id = format!(
            "draft-{}",
            run.current_chapter_id
                .as_deref()
                .ok_or(RepositoryError::CorruptState)?
        );
        let ready = self
            .repository
            .writer_knowledge(
                project_id,
                run.current_chapter_number
                    .ok_or(RepositoryError::CorruptState)?,
            )?
            .iter()
            .any(|source| source["objectId"] == object_id && source["status"] == "ready");
        if !ready {
            return Err(RepositoryError::InvalidContent(
                "chapter knowledge is not ready".into(),
            ));
        }
        run.stage = RunStage::Commitments;
        run.step_token = Uuid::new_v4().simple().to_string();
        self.save(project_id, run)
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
            || now >= run.deadline_at
            || run.used_task_attempts >= run.max_task_attempts
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

    pub fn check_knowledge(
        &mut self,
        project_id: &ProjectId,
        run_id: &str,
        expected_version: i64,
        now: i64,
    ) -> Result<GenerationRun, RepositoryError> {
        let mut run = self.get(project_id, run_id)?;
        assert_step(&run, expected_version, now)?;
        if run.stage != RunStage::Check
            || run.active_task_id.is_some()
            || source_digest(self.repository, project_id)? != run.source_digest
        {
            return Err(RepositoryError::AuthoringRunStale);
        }
        let state = self.repository.knowledge_state(KnowledgeStateQuery {
            project_id,
            snapshot_id: None,
            perspective: KnowledgePerspective::Reader,
            world_time: None,
            epistemic_time: None,
            reading_chapter: run.current_chapter_number,
            owner_id: None,
            entity_id: None,
            predicate: None,
        })?;
        if state["coverage"]["complete"] != true
            || state["findings"]
                .as_array()
                .is_some_and(|items| !items.is_empty())
        {
            run.status = RunStatus::Blocked;
            run.blocked_reason = Some("knowledge_coverage_or_conflict".into());
        } else {
            let chapter_id = run
                .current_chapter_id
                .as_deref()
                .ok_or(RepositoryError::CorruptState)?;
            let assessment = self.repository.scene_assessment(project_id, chapter_id)?;
            if let Some(assessment) = assessment.filter(|item| item.current) {
                let findings = assessment.content["assessments"]
                    .as_array()
                    .ok_or(RepositoryError::CorruptState)?;
                if findings.iter().any(|item| {
                    item["verdict"] != "realized" || item["recommendation"] != "continue"
                }) && run.accepted_feedback_revision_id.as_deref()
                    != Some(assessment.revision_id.as_str())
                {
                    run.status = RunStatus::Blocked;
                    run.blocked_reason = Some("scene_feedback_needs_author".into());
                } else {
                    run.completed_chapters += 1;
                    run.current_chapter_id = None;
                    run.current_chapter_number = None;
                    run.accepted_feedback_revision_id = None;
                    run.feedback_repair_prompt = None;
                    run.feedback_repair_count = 0;
                    if run.completed_chapters == run.target_chapters {
                        run.stage = RunStage::Finished;
                        run.status = RunStatus::Completed;
                    } else {
                        run.stage = RunStage::RollingPlan;
                    }
                    run.step_token = Uuid::new_v4().simple().to_string();
                }
            } else {
                run.status = RunStatus::Blocked;
                run.blocked_reason = Some("scene_assessment_stale".into());
            }
        }
        let version = run.version;
        run.version += 1;
        let state = serde_json::to_string(&run).map_err(|_| RepositoryError::CorruptState)?;
        parse_run(self.repository.update_generation_run_if_source_matches(
            project_id,
            &run.id,
            version,
            &state,
            &run.source_digest,
        )?)
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

fn effective_intent(run: &GenerationRun, chapter_number: i64) -> Result<String, RepositoryError> {
    generation_author_intent(&run.intent_revisions, chapter_number)
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

fn assert_step(
    run: &GenerationRun,
    expected_version: i64,
    now: i64,
) -> Result<(), RepositoryError> {
    if run.version != expected_version || run.status != RunStatus::Running || now >= run.deadline_at
    {
        return Err(RepositoryError::AuthoringRunStale);
    }
    Ok(())
}

fn needs_model(stage: RunStage) -> bool {
    matches!(
        stage,
        RunStage::Brief
            | RunStage::Outline
            | RunStage::RollingPlan
            | RunStage::Draft
            | RunStage::Extract
            | RunStage::Commitments
            | RunStage::Assess
    )
}

fn step_operation(stage: RunStage) -> Option<&'static str> {
    match stage {
        RunStage::Brief => Some("generateBrief"),
        RunStage::Outline => Some("generateOutline"),
        RunStage::RollingPlan => Some("generatePlan"),
        RunStage::Draft => Some("generateChapter"),
        RunStage::Extract => Some("modelKnowledge"),
        RunStage::Commitments => Some("extractCommitments"),
        RunStage::Assess => Some("assessSceneOutcome"),
        RunStage::MaterializeChapter | RunStage::Check | RunStage::Finished => None,
    }
}

fn target_matches_chapter(run: &GenerationRun, target: &Value) -> bool {
    match run.stage {
        RunStage::RollingPlan => {
            target["planFromChapter"].as_i64() == Some(run.completed_chapters + 1)
        }
        RunStage::Draft => {
            target["chapterId"].as_str() == run.current_chapter_id.as_deref()
                && target["chapterNumber"].as_i64() == run.current_chapter_number
        }
        RunStage::Extract | RunStage::Commitments | RunStage::Assess => {
            target["chapterId"].as_str() == run.current_chapter_id.as_deref()
        }
        _ => true,
    }
}

fn advance_completed_task(
    repository: &SqliteStoryRepository,
    project_id: &ProjectId,
    run: &mut GenerationRun,
    task: &novel_codex_story_core::TaskRecord,
) -> Result<(), RepositoryError> {
    let operation = step_operation(run.stage).ok_or(RepositoryError::TaskBindingInvalid)?;
    let target: Value =
        serde_json::from_str(&task.target_json).map_err(|_| RepositoryError::CorruptState)?;
    let intent_matches = target["intentGeneration"]
        .as_i64()
        .is_some_and(|generation| {
            generation == run.intent_generation
                || (generation >= 0
                    && generation < run.intent_generation
                    && matches!(
                        run.stage,
                        RunStage::Draft
                            | RunStage::Extract
                            | RunStage::Commitments
                            | RunStage::Assess
                    )
                    && run.current_chapter_number.is_some_and(|chapter| {
                        run.intent_revisions
                            .iter()
                            .filter(|intent| intent.generation > generation)
                            .all(|intent| {
                                intent.effective_from_chapter > chapter
                                    || (intent.effective_from_chapter == chapter
                                        && run.pending_current_intent_generation
                                            == Some(intent.generation))
                            })
                    }))
        });
    if task.status != "completed"
        || task.operation != operation
        || target["runId"] != run.id
        || target["stepToken"] != run.step_token
        || !intent_matches
        || run.active_task_id.as_deref() != Some(task.task_id.as_str())
        || !target_matches_chapter(run, &target)
        || (run.stage != RunStage::Extract && task.output_refs_json == "[]")
        || run.last_task_id.as_deref() == Some(task.task_id.as_str())
        || !run.provider_budget.has_charge(&task.task_id)
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    run.stage = match run.stage {
        RunStage::Brief => RunStage::Outline,
        RunStage::Outline => RunStage::RollingPlan,
        RunStage::RollingPlan => RunStage::MaterializeChapter,
        RunStage::Draft if run.pending_current_intent_generation.is_some() => RunStage::Draft,
        RunStage::Draft => RunStage::Extract,
        RunStage::Extract => RunStage::Commitments,
        RunStage::Commitments => RunStage::Assess,
        RunStage::Assess => RunStage::Check,
        RunStage::Check | RunStage::Finished | RunStage::MaterializeChapter => {
            return Err(RepositoryError::TaskBindingInvalid);
        }
    };
    run.pending_current_intent_generation = None;
    if run.stage == RunStage::Extract {
        run.feedback_repair_prompt = None;
    }
    run.last_task_id = Some(task.task_id.clone());
    run.active_task_id = None;
    run.step_token = Uuid::new_v4().simple().to_string();
    run.source_digest = source_digest(repository, project_id)?;
    Ok(())
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
