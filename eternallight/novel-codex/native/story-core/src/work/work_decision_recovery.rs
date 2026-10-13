//! Failure evidence stays available to the agent; program fences repeated strategies and unknown writes.
use crate::RepositoryError;
use crate::authoring_runs::storage_error;
use crate::work_decision_status::{ReadyAction, action, retryable};
use rusqlite::{Connection, params};
use serde_json::{Value, json};

fn optional_analysis(generation: bool, operation: &str, target: &Value) -> bool {
    operation == "reviewQuality"
        || (generation || target["workDecisionAction"].is_object())
            && matches!(
                operation,
                "modelKnowledge" | "extractCommitments" | "assessSceneOutcome"
            )
}

pub(crate) fn failure_target(mut target: Value) -> Result<Value, RepositoryError> {
    let object = target
        .as_object_mut()
        .ok_or(RepositoryError::CorruptState)?;
    for key in [
        "requestedByTaskId",
        "requestedActionId",
        "sourceTaskId",
        "resumeStepId",
        "workDecisionAction",
        "runId",
        "runVersion",
        "stepToken",
        "intentGeneration",
        "batchId",
        "recoveryPlan",
        "messageDispatch",
    ] {
        object.remove(key);
    }
    Ok(target)
}

pub(crate) fn failed_actions(
    db: &Connection,
    work: &str,
    requirements: &mut Vec<Value>,
    available: &mut Vec<ReadyAction>,
) -> Result<(), RepositoryError> {
    let generation: bool = db
        .query_row(
            "SELECT kind='generation' FROM story_work_tasks_v1 WHERE id=?1",
            [work],
            |r| r.get(0),
        )
        .map_err(storage_error)?;
    let mut statement=db.prepare("SELECT t.task_id,t.operation,t.target_json FROM story_work_steps_v1 s JOIN story_tasks_v2 t ON t.task_id=s.current_attempt_id WHERE s.work_task_id=?1 AND s.status IN ('blocked','superseded') AND t.status='failed' AND (t.operation!='advisory' OR (json_extract(t.target_json,'$.workDecision') IS NULL AND json_extract(t.target_json,'$.interludeId') IS NULL AND json_extract(t.target_json,'$.runId') IS NULL)) AND NOT EXISTS(SELECT 1 FROM story_work_steps_v1 next WHERE next.work_task_id=s.work_task_id AND next.ordinal>s.ordinal AND next.operation=s.operation AND (t.operation!='advisory' OR (json_extract(next.target_json,'$.workDecision') IS NULL AND json_extract(next.target_json,'$.interludeId') IS NULL)) AND json_extract(next.target_json,'$.chapterId') IS json_extract(s.target_json,'$.chapterId')) ORDER BY s.ordinal").map_err(storage_error)?;
    let rows = statement
        .query_map([work], |r| {
            Ok((
                r.get::<_, String>(/*idx*/ 0)?,
                r.get::<_, String>(/*idx*/ 1)?,
                r.get::<_, String>(/*idx*/ 2)?,
            ))
        })
        .map_err(storage_error)?;
    for row in rows {
        let (task, operation, target) = row.map_err(storage_error)?;
        // An explicit Author replan has replaced this failed attempt. Keep its
        // evidence, but do not schedule the superseded failure as current work.
        let author_replanned: bool = generation && db.query_row(
            "SELECT EXISTS(SELECT 1 FROM story_work_steps_v1 s JOIN story_generation_runs_v1 g ON s.work_task_id='work-'||g.run_id, json_each(g.state_json,'$.replans') r WHERE s.work_task_id=?1 AND s.current_attempt_id=?2 AND s.status='superseded' AND json_extract(r.value,'$.previousTaskId')=?2)",
            params![work, task], |row| row.get(0),
        ).map_err(storage_error)?;
        if author_replanned {
            continue;
        }
        if operation == "reviewQuality" {
            let saved: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM story_quality_packets_v1 WHERE task_id=?1 AND report_json IS NOT NULL)", [&task], |r| r.get(0)).map_err(storage_error)?;
            if saved {
                continue;
            }
        }
        if operation == "advisory" && generation {
            continue;
        }
        if !retryable(db, &task)? {
            // Unclassified conversation failures are not an executable retry.
            // Preserve their existing admission fence without breaking reads.
            if operation == "advisory" { continue; }
            return Err(RepositoryError::TaskNotWritable);
        }
        let mut target: Value =
            serde_json::from_str(&target).map_err(|_| RepositoryError::CorruptState)?;
        // A verified model-selected aid remains a method, not a new author
        // deliverable. Explicit candidates and direct tasks keep their duty.
        let optional = optional_analysis(generation, &operation, &target);
        target = failure_target(target)?;
        let revision = target["draftRevisionId"]
            .as_str()
            .or_else(|| target["sourceRevisionId"].as_str())
            .map(str::to_owned);
        let mut candidate = action(&operation, target, Some(task.clone()), revision, !optional);
        let original = crate::tasks::load_task(db, &task)?;
        let source: Value = serde_json::from_str(&original.input_manifest_json)
            .map_err(|_| RepositoryError::CorruptState)?;
        let mut failures=db.prepare("SELECT t.task_id,t.target_json,t.input_manifest_json FROM story_work_attempts_v1 a JOIN story_work_steps_v1 s ON s.id=a.step_id JOIN story_tasks_v2 t ON t.task_id=a.attempt_id WHERE s.work_task_id=?1 AND t.operation=?2 AND t.status='failed'").map_err(storage_error)?;
        let attempts = failures
            .query_map(params![work, operation], |r| {
                Ok((
                    r.get::<_, String>(/*idx*/ 0)?,
                    r.get::<_, String>(/*idx*/ 1)?,
                    r.get::<_, String>(/*idx*/ 2)?,
                ))
            })
            .map_err(storage_error)?;
        let mut same_failures = 0;
        for attempt in attempts {
            let (id, prior_target, prior_manifest) = attempt.map_err(storage_error)?;
            let prior_target: Value =
                serde_json::from_str(&prior_target).map_err(|_| RepositoryError::CorruptState)?;
            let prior_manifest: Value =
                serde_json::from_str(&prior_manifest).map_err(|_| RepositoryError::CorruptState)?;
            let prior = crate::tasks::load_task(db, &id)?;
            let current_inputs = match crate::task_input::assert_task_inputs_current(db, &prior) {
                Ok(()) => true,
                Err(RepositoryError::TaskInputStale) => false,
                Err(error) => return Err(error),
            };
            let current_intent = if let Some(origin) =
                prior_manifest["authorIntentBinding"]["sourceTaskId"].as_str()
            {
                let revision:i64=db.query_row("SELECT COALESCE(MAX(intent_revision),0) FROM story_task_intents_v1 WHERE task_id=?1",[origin],|row|row.get(/*idx*/0)).map_err(storage_error)?;
                prior_manifest["authorIntent"]
                    == crate::task_author_intent::source_intent(db, origin, revision)?
            } else if let Some(run) =
                prior_manifest["generationAuthorIntentBinding"]["runId"].as_str()
            {
                let generation:i64=db.query_row("SELECT json_extract(state_json,'$.intentGeneration') FROM story_generation_runs_v1 WHERE run_id=?1 AND project_id=?2",params![run,prior.project_id.as_str()],|row|row.get(/*idx*/0)).map_err(storage_error)?;
                prior_manifest["generationAuthorIntentBinding"]["intentGeneration"] == generation
            } else if operation == "advisory" {
                let revision:i64=db.query_row("SELECT COALESCE(MAX(intent_revision),0) FROM story_task_intents_v1 WHERE task_id=?1",[&id],|row|row.get(0)).map_err(storage_error)?;
                prior_manifest["authorIntent"] == crate::task_author_intent::source_intent(db,&id,revision)?
            } else {
                false
            };
            if current_inputs
                && current_intent
                && failure_target(prior_target)? == candidate.target
                && prior_manifest["expectedCandidateRevisionId"]
                    == source["expectedCandidateRevisionId"]
                && retryable(db, &id)?
            {
                same_failures += 1;
            }
        }
        candidate.recovery = Some(
            json!({"failedTaskId":task,"sameConditionFailures":same_failures,"planRequired":same_failures>=2 || crate::conversation_receipts::failure_code(original.error.as_deref()) == Some("provider_output_limit"),"exhausted":same_failures>=4,"attemptLimit":4}),
        );
        requirements.push(json!({"kind":if optional {"optionalAnalysis"} else {"requestedOutput"},"taskId":task,"operation":operation,"status":if optional {"failed"} else {"pending"}}));
        // A freshly derived action can have the same key; failure recovery must
        // replace it, otherwise that duplicate would bypass the retry allowance.
        if let Some(existing) = available.iter_mut().find(|a| a.key == candidate.key) {
            *existing = candidate;
        } else {
            available.push(candidate);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_methods_do_not_turn_into_required_author_deliverables_on_failure() {
        let model_action = json!({"workDecisionAction":{"workTaskId":"work","decisionTaskId":"decision","actionKey":"key"}});
        for operation in ["modelKnowledge", "extractCommitments", "assessSceneOutcome"] {
            assert!(optional_analysis(false, operation, &model_action));
            assert!(optional_analysis(true, operation, &model_action));
        }
        for operation in ["generateChapter", "generateBrief", "assessRevisionOutcome"] {
            assert!(!optional_analysis(false, operation, &model_action));
        }
    }

    #[test]
    fn explicit_and_direct_analysis_requests_remain_required() {
        for target in [
            json!({}),
            json!({"requestedActionId":"explicit","requestedByTaskId":"author"}),
        ] {
            for operation in ["modelKnowledge", "extractCommitments", "assessSceneOutcome"] {
                assert!(!optional_analysis(false, operation, &target));
            }
        }
    }
}

#[derive(serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RecoveryPlan {
    failed_task_id: String,
    cause: String,
    change: String,
    verification: String,
}
fn invalid(message: &str) -> RepositoryError {
    RepositoryError::InvalidContent(format!("recoveryPlan: {message}"))
}
fn normalized(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// A plan is evidence of the agent's chosen correction, not program-certified semantic novelty.
/// Distinct wording cannot reset the immutable Work execution budget or attempts allowance.
pub(crate) fn validate_plan(
    db: &Connection,
    work: &str,
    action: &ReadyAction,
    value: Option<&Value>,
) -> Result<(), RepositoryError> {
    let Some(recovery) = &action.recovery else {
        return if value.is_some() {
            Err(invalid("only a failed action can carry a recovery plan"))
        } else {
            Ok(())
        };
    };
    if recovery["exhausted"] == true {
        return Err(invalid(
            "same-source recovery allowance exhausted; do not retry this action; use another authorized approach or report partial completion",
        ));
    }
    let Some(value) = value else {
        return if recovery["planRequired"] == true {
            Err(invalid(
                "this failure requires failedTaskId, cause, change and verification; inspect the failure and change the approach, or report partial completion",
            ))
        } else {
            Ok(())
        };
    };
    let plan: RecoveryPlan =
        serde_json::from_value(value.clone()).map_err(|_| invalid("invalid plan fields"))?;
    if recovery["failedTaskId"] != plan.failed_task_id
        || plan.failed_task_id.len() > 160
        || [&plan.cause, &plan.change, &plan.verification]
            .iter()
            .any(|s| s.trim().is_empty() || s.chars().count() > 600)
    {
        return Err(invalid(
            "plan must bind the current failed task and explain cause, changed approach and verification",
        ));
    }
    let mut query = db.prepare("SELECT t.task_id,t.target_json FROM story_work_attempts_v1 a JOIN story_work_steps_v1 s ON s.id=a.step_id JOIN story_tasks_v2 t ON t.task_id=a.attempt_id WHERE s.work_task_id=?1 AND t.operation=?2 AND t.status='failed'").map_err(storage_error)?;
    let rows = query
        .query_map(params![work, action.operation], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(storage_error)?;
    for row in rows {
        let (id, encoded) = row.map_err(storage_error)?;
        let target: Value =
            serde_json::from_str(&encoded).map_err(|_| RepositoryError::CorruptState)?;
        let prior_change = target["recoveryPlan"]["change"].as_str().map(normalized);
        if prior_change.as_deref() != Some(normalized(&plan.change).as_str())
            || failure_target(target)? != action.target
        {
            continue;
        }
        let task = crate::tasks::load_task(db, &id)?;
        match crate::task_input::assert_task_inputs_current(db, &task) {
            Ok(()) => {
                return Err(invalid(
                    "this correction already failed with unchanged sources; choose a different approach or report partial completion",
                ));
            }
            Err(RepositoryError::TaskInputStale) => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}
