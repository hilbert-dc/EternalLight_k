use novel_codex_story_core::{ProjectId, RepositoryError};
use uuid::Uuid;

use super::{
    AuthoringService, CreativeIntentRevision, CreativeIntentSource, FeedbackDecision,
    GenerationRun, RunStage, RunStatus, source_digest,
};

pub struct ResolveFeedback<'a> {
    pub project_id: &'a ProjectId,
    pub run_id: &'a str,
    pub expected_version: i64,
    pub assessment_revision_id: &'a str,
    pub choice: &'a str,
    pub instruction: &'a str,
    pub now: i64,
}

impl AuthoringService<'_> {
    pub fn resolve_feedback(
        &mut self,
        request: ResolveFeedback<'_>,
    ) -> Result<GenerationRun, RepositoryError> {
        let ResolveFeedback {
            project_id,
            run_id,
            expected_version,
            assessment_revision_id,
            choice,
            instruction,
            now,
        } = request;
        let mut run = self.get(project_id, run_id)?;
        if run.version != expected_version
            || run.status != RunStatus::Blocked
            || run.stage != RunStage::Coordinate
            || run.blocked_reason.as_deref() != Some("scene_feedback_needs_author")
            || run.active_task_id.is_some()
            || run.deadline_at.is_some_and(|deadline| now >= deadline)
            || source_digest(self.repository, project_id)? != run.source_digest
        {
            return Err(RepositoryError::AuthoringRunStale);
        }
        let chapter_id = run
            .current_chapter_id
            .clone()
            .ok_or(RepositoryError::CorruptState)?;
        let assessment = self
            .repository
            .scene_assessment(project_id, &chapter_id)?
            .filter(|assessment| {
                assessment.current && assessment.revision_id == assessment_revision_id
            })
            .ok_or(RepositoryError::AuthoringRunStale)?;
        if instruction.chars().count() > 1_000 {
            return Err(RepositoryError::InvalidContent(
                "feedback instruction exceeds 1000 characters".into(),
            ));
        }
        let decision_id = format!("feedback-decision-{}", Uuid::new_v4().simple());
        let author_message_id = (!instruction.trim().is_empty())
            .then(|| format!("author-message-{}", Uuid::new_v4().simple()));
        match choice {
            "continue" => {
                if !instruction.trim().is_empty() {
                    return Err(RepositoryError::InvalidContent(
                        "continue does not accept a repair instruction".into(),
                    ));
                }
                run.accepted_feedback_revision_id = Some(assessment_revision_id.to_owned());
            }
            "repairDraft" => {
                if run
                    .max_task_attempts
                    .is_some_and(|limit| run.used_task_attempts >= limit)
                    || run.intent_revisions.len() >= 16
                {
                    return Err(RepositoryError::InvalidContent(
                        "generation budget cannot cover another feedback repair".into(),
                    ));
                }
                let findings = assessment.content["assessments"]
                    .as_array()
                    .ok_or(RepositoryError::CorruptState)?
                    .iter()
                    .filter(|item| {
                        item["verdict"] != "realized" || item["recommendation"] != "continue"
                    })
                    .take(24)
                    .filter_map(|item| {
                        let commitments = item["effectAssessments"]
                            .as_array()?
                            .iter()
                            .flat_map(|effect| {
                                effect["commitmentAssessments"]
                                    .as_array()
                                    .into_iter()
                                    .flatten()
                            })
                            .filter(|assessment| {
                                matches!(
                                    assessment["outcome"].as_str(),
                                    Some("contradicted" | "insufficientEvidence")
                                )
                            })
                            .filter_map(|assessment| {
                                Some(format!(
                                    "承诺 {}：{}",
                                    assessment["outcome"].as_str()?,
                                    assessment["rationale"].as_str()?
                                ))
                            })
                            .collect::<Vec<_>>()
                            .join("；");
                        Some(format!(
                            "场景 {}：{}；{}",
                            item["sceneId"].as_str()?,
                            commitments,
                            item["rationale"].as_str()?
                        ))
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                run.feedback_repair_prompt = Some(format!(
                    "依据场景评估 {} 修订当前章工作稿；不得改写 Canon。评估问题：\n{}\n作者补充：{}",
                    assessment_revision_id,
                    findings.chars().take(2_400).collect::<String>(),
                    instruction
                ));
                run.feedback_repair_count += 1;
                run.intent_generation += 1;
                run.intent_revisions.push(CreativeIntentRevision {
                    id: format!("intent-{}", Uuid::new_v4().simple()),
                    generation: run.intent_generation,
                    instruction: instruction.to_owned(),
                    effective_from_chapter: run.completed_chapters + 1,
                    source: CreativeIntentSource::FeedbackDecision {
                        decision_id: decision_id.clone(),
                    },
                });
                run.accepted_feedback_revision_id = None;
                run.stage = RunStage::Coordinate;
                run.step_token = Uuid::new_v4().simple().to_string();
            }
            _ => {
                return Err(RepositoryError::InvalidContent(
                    "feedback choice must be continue or repairDraft".into(),
                ));
            }
        }
        run.feedback_decisions.push(FeedbackDecision {
            id: decision_id,
            author_message_id,
            chapter_id,
            chapter_number: run.completed_chapters + 1,
            assessment_revision_id: assessment_revision_id.to_owned(),
            choice: choice.to_owned(),
            instruction: instruction.to_owned(),
            source_digest: run.source_digest.clone(),
            decided_at: now,
        });
        run.status = RunStatus::Running;
        run.blocked_reason = None;
        self.save(project_id, run)
    }
}
