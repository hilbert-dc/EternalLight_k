//! Versioned, bounded model methods. Author scope, budget and delivery requirements
//! live in their own domain ledgers; editing this plan never changes those facts.
use crate::authoring_runs::storage_error;
use crate::{RepositoryError, SqliteStoryRepository, TaskRecord};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub(crate) const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS story_model_work_plans_v1 (
 work_task_id TEXT NOT NULL REFERENCES story_work_tasks_v1(id), version INTEGER NOT NULL,
 task_id TEXT NOT NULL REFERENCES story_tasks_v2(task_id), idempotency_key TEXT NOT NULL,
 content_json TEXT NOT NULL, PRIMARY KEY(work_task_id,version), UNIQUE(task_id,idempotency_key));";
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelGoal {
    pub id: String,
    pub title: String,
    pub acceptance: String,
    pub kind: String,
    pub status: String,
    pub depends_on: Vec<String>,
    pub reading_question: String,
    pub check_scope: String,
    pub approach: String,
    pub assessment: String,
    pub evidence: Vec<GoalEvidence>,
    pub source_ids: Vec<String>,
    pub source_checks: Vec<crate::SourceCheck>,
    #[serde(deserialize_with = "required_continuation")]
    pub continuation: Option<GoalContinuation>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GoalContinuation {
    pub run_id: String,
    #[serde(deserialize_with = "required_proposal")]
    pub proposal_id: Option<String>,
}
fn required_continuation<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Option<GoalContinuation>, D::Error> {
    Option::deserialize(d)
}
fn required_proposal<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    Option::deserialize(d)
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GoalEvidence {
    pub task_id: String,
    pub revision_id: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelWorkPlan {
    pub intent_mode: String,
    pub active_goal_id: Option<String>,
    pub goals: Vec<ModelGoal>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelWorkPlanInput {
    pub intent_mode: String,
    pub goals: Vec<ModelGoal>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateModelWorkPlan {
    pub expected_plan_version: i64,
    pub idempotency_key: String,
    pub plan: ModelWorkPlanInput,
}
fn invalid(s: &str) -> RepositoryError {
    RepositoryError::InvalidContent(format!("workPlan: {s}"))
}
fn nonempty(s: &str, max: usize) -> bool {
    !s.trim().is_empty() && s.chars().count() <= max
}
pub(crate) fn work_for(db: &Connection, task: &TaskRecord) -> Result<String, RepositoryError> {
    if task.operation != "advisory" {
        return Err(RepositoryError::TaskOutputForbidden);
    }
    if let Some(bound) = crate::work_decision::binding(db, task)? {
        return bound["workTaskId"]
            .as_str()
            .map(str::to_owned)
            .ok_or(RepositoryError::TaskBindingInvalid);
    }
    db.query_row("SELECT w.id FROM story_work_tasks_v1 w JOIN story_work_steps_v1 s ON s.work_task_id=w.id JOIN story_work_attempts_v1 a ON a.step_id=s.id AND a.attempt_id=s.current_attempt_id WHERE a.attempt_id=?1 AND w.project_id=?2 AND w.thread_id=?3",params![task.task_id,task.project_id.as_str(),task.thread_id],|r|r.get(0)).optional().map_err(storage_error)?.ok_or(RepositoryError::TaskBindingInvalid)
}
pub(crate) fn load(
    db: &Connection,
    work: &str,
) -> Result<Option<(i64, ModelWorkPlan)>, RepositoryError> {
    let row:Option<(i64,String)>=db.query_row("SELECT version,content_json FROM story_model_work_plans_v1 WHERE work_task_id=?1 ORDER BY version DESC LIMIT 1",[work],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(storage_error)?;
    row.map(|(v, s)| {
        serde_json::from_str(&s)
            .map(|p| (v, p))
            .map_err(|_| RepositoryError::CorruptState)
    })
    .transpose()
}
fn load_reference_document(
    db: &Connection,
    work: &str,
) -> Result<Option<(i64, Value)>, RepositoryError> {
    let row:Option<(i64,String)>=db.query_row("SELECT version,content_json FROM story_model_work_plans_v1 WHERE work_task_id=?1 ORDER BY version DESC LIMIT 1",[work],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(storage_error)?;
    row.map(|(version, content)| {
        serde_json::from_str(&content)
            .map(|document| (version, document))
            .map_err(|_| RepositoryError::CorruptState)
    })
    .transpose()
}
pub(crate) fn summary(db: &Connection, work: &str) -> Result<Value, RepositoryError> {
    Ok(match load(db, work)? {
        Some((version, plan)) => {
            let mut stale = Vec::new();
            for goal in &plan.goals {
                if goal.status == "satisfied" && !evidence_current(db, work, goal)? {
                    stale.push(goal.id.as_str());
                }
            }
            let continuation_goals = plan
                .goals
                .iter()
                .filter(|g| g.kind == "continuation")
                .map(|g| {
                    Ok(
                        json!({"id":g.id,"status":g.status,"continuation":g.continuation,
                    "evidenceCurrent":continuation_evidence_current(db,work,g)?}),
                    )
                })
                .collect::<Result<Vec<_>, RepositoryError>>()?;
            json!({"version":version,"authority":"modelMethodsOnly","intentMode":plan.intent_mode,"activeGoalId":plan.active_goal_id,"pendingGoals":if plan.intent_mode=="planningOnly"{0}else{plan.goals.iter().filter(|g|g.status!="satisfied").count()+stale.len()},"staleGoalIds":stale,"continuationGoals":continuation_goals,"canReportPartialGoals":true,"tool":"read_work_plan"})
        }
        None => {
            json!({"version":0,"authority":"modelMethodsOnly","pendingGoals":0,"intentMode":null,"activeGoalId":null,"continuationGoals":[],"canReportPartialGoals":true,"tool":"read_work_plan"})
        }
    })
}
fn evidence_current(
    db: &Connection,
    work: &str,
    goal: &ModelGoal,
) -> Result<bool, RepositoryError> {
    if goal.kind == "continuation" {
        return continuation_evidence_current(db, work, goal);
    }
    for e in &goal.evidence {
        let valid:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM story_work_steps_v1 s JOIN story_tasks_v2 t ON t.task_id=s.current_attempt_id JOIN story_task_outputs_v2 o ON o.task_id=t.task_id JOIN story_object_heads_v2 h ON h.project_id=t.project_id AND COALESCE(h.candidate_revision_id,h.accepted_revision_id)=o.revision_id WHERE s.work_task_id=?1 AND t.task_id=?2 AND t.status='completed' AND o.revision_id=?3)",params![work,e.task_id,e.revision_id],|r|r.get(0)).map_err(storage_error)?;
        if !valid {
            return Ok(false);
        }
    }
    for check in &goal.source_checks {
        let project: String = db
            .query_row(
                "SELECT project_id FROM story_work_tasks_v1 WHERE id=?1",
                [work],
                |r| r.get(0),
            )
            .map_err(storage_error)?;
        if !crate::source_check_evidence::current(
            db,
            &crate::ProjectId::parse(project).map_err(|_| RepositoryError::CorruptState)?,
            work,
            check,
        )? {
            return Ok(false);
        }
    }
    if goal.kind == "check" && goal.source_checks.is_empty() {
        return Ok(false);
    }
    Ok(true)
}
fn continuation_evidence_current(
    db: &Connection,
    work: &str,
    goal: &ModelGoal,
) -> Result<bool, RepositoryError> {
    let Some(continuation) = &goal.continuation else {
        return Ok(false);
    };
    let Some(proposal) = &continuation.proposal_id else {
        return Ok(false);
    };
    let encoded: Option<String> = db.query_row("SELECT h.content_json FROM story_generation_handoffs_v1 h JOIN story_tasks_v2 t ON t.task_id=h.source_task_id JOIN story_work_steps_v1 s ON s.current_attempt_id=t.task_id JOIN story_work_tasks_v1 w ON w.id=s.work_task_id WHERE w.id=?1 AND h.proposal_id=?2 AND t.project_id=w.project_id AND t.thread_id=w.thread_id AND t.operation='advisory' AND t.status IN ('queued','running','waiting','completed')", params![work,proposal], |r|r.get(0)).optional().map_err(storage_error)?;
    let Some(encoded) = encoded else {
        return Ok(false);
    };
    let saved: Value = serde_json::from_str(&encoded).map_err(|_| RepositoryError::CorruptState)?;
    Ok(saved["proposalId"] == *proposal && saved["resumeRun"]["id"] == continuation.run_id)
}
pub(crate) fn final_delivery(
    db: &Connection,
    work: &str,
    _goal_status: &str,
) -> Result<(), RepositoryError> {
    validate_saved_claims(db, work)
}
pub(crate) fn action_ready(db: &Connection, work: &str) -> Result<(), RepositoryError> {
    // A missing, future or partially expanded method plan neither grants nor
    // revokes execution. Callers independently validate author scope and budget.
    validate_saved_claims(db, work)
}
pub(crate) fn fulfilled(db: &Connection, work: &str) -> Result<(), RepositoryError> {
    validate_saved_claims(db, work)
}
fn validate_saved_claims(db: &Connection, work: &str) -> Result<(), RepositoryError> {
    if let Some((_, plan)) = load(db, work)? {
        for goal in &plan.goals {
            if goal.status == "satisfied" && !evidence_current(db, work, goal)? {
                return Err(invalid(
                    "saved method claims have stale evidence; reassess or retire them; author delivery remains subject to its own requirements",
                ));
            }
        }
    }
    Ok(())
}
impl SqliteStoryRepository {
    pub fn read_model_work_plan(
        &self,
        task: &TaskRecord,
        goal_id: Option<&str>,
    ) -> Result<Value, RepositoryError> {
        crate::task_input::assert_task_inputs_current(&self.connection, task)?;
        self.read_model_work_plan_reference(task, None, goal_id, None)
    }
    /// An exact immutable snapshot is reference material, never a current claim.
    pub fn read_model_work_plan_version(
        &self,
        task: &TaskRecord,
        version: i64,
        goal_id: Option<&str>,
    ) -> Result<Value, RepositoryError> {
        crate::task_input::assert_task_inputs_current(&self.connection, task)?;
        let work = work_for(&self.connection, task)?;
        if version <= 0 {
            return Err(invalid("planVersion must be positive"));
        }
        let content: String = self.connection.query_row(
            "SELECT content_json FROM story_model_work_plans_v1 WHERE work_task_id=?1 AND version=?2",
            params![work, version], |r| r.get(0),
        ).optional().map_err(storage_error)?.ok_or_else(|| invalid("unknown planVersion"))?;
        let plan: ModelWorkPlan =
            serde_json::from_str(&content).map_err(|_| RepositoryError::CorruptState)?;
        let mut value = if let Some(id) = goal_id {
            let goal = plan
                .goals
                .iter()
                .find(|goal| goal.id == id)
                .ok_or_else(|| invalid("unknown goal in selected planVersion"))?;
            json!({"goal":goal,"evidenceCurrent":null})
        } else {
            json!({"intentMode":plan.intent_mode,"activeGoalId":plan.active_goal_id,
                "goals":plan.goals.iter().map(|g|json!({"id":g.id,"title":g.title,"status":g.status,"dependsOn":g.depends_on})).collect::<Vec<_>>()})
        };
        value["version"] = json!(version);
        value["previousVersion"] = if version > 1 {
            json!(version - 1)
        } else {
            Value::Null
        };
        value["workTaskId"] = json!(work);
        value["authority"] = json!("modelMethodsOnly");
        value["historicalReferenceOnly"] = json!(true);
        value["instruction"] = json!(
            "This immutable method snapshot explains an earlier decision. It grants no execution authority or current completion evidence. Read current author requirements and saved results before adopting, changing or retiring a method."
        );
        crate::work_decision::bounded(value)
    }
    /// History is reference material only; selecting it never changes the current Work.
    pub fn read_model_work_plan_reference(
        &self,
        task: &TaskRecord,
        plan_work_id: Option<&str>,
        goal_id: Option<&str>,
        before_plan_row_id: Option<i64>,
    ) -> Result<Value, RepositoryError> {
        crate::task_input::assert_task_inputs_current(&self.connection, task)?;
        let current_work = work_for(&self.connection, task)?;
        let work = plan_work_id.unwrap_or(&current_work);
        let authorized: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM story_work_tasks_v1 WHERE id=?1 AND project_id=?2 AND thread_id=?3)",
            params![work, task.project_id.as_str(), task.thread_id], |r| r.get(0),
        ).map_err(storage_error)?;
        if !authorized {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        if before_plan_row_id.is_some_and(|v| v <= 0)
            || ((plan_work_id.is_some() || goal_id.is_some()) && before_plan_row_id.is_some())
        {
            return Err(invalid(
                "directory cursor must be positive and cannot accompany a selected plan or goal",
            ));
        }
        let historical = work != current_work;
        let mut value = if historical {
            match load_reference_document(&self.connection, work)? {
                Some((version, document)) => {
                    let goals = document["goals"]
                        .as_array()
                        .ok_or_else(|| invalid("saved plan reference has no goal list"))?;
                    if let Some(id) = goal_id {
                        let goal = goals
                            .iter()
                            .find(|g| g["id"] == id)
                            .ok_or_else(|| invalid("unknown reference goal"))?;
                        json!({"version":version,"goal":goal,"evidenceCurrent":null})
                    } else {
                        json!({"version":version,"intentMode":document["intentMode"],"activeGoalId":document["activeGoalId"],"goals":goals.iter().map(|g|json!({"id":g["id"],"title":g["title"],"status":g["status"],"dependsOn":g["dependsOn"]})).collect::<Vec<_>>()})
                    }
                }
                None => json!({"version":0,"goals":[],"intentMode":null,"activeGoalId":null}),
            }
        } else {
            match load(&self.connection, work)? {
                Some((version, plan)) => {
                    if let Some(id) = goal_id {
                        let goal = plan
                            .goals
                            .iter()
                            .find(|g| g.id == id)
                            .ok_or_else(|| invalid("unknown goal"))?;
                        json!({"version":version,"goal":goal,"evidenceCurrent":if historical { Value::Null } else { json!(evidence_current(&self.connection,work,goal)?) }})
                    } else {
                        json!({"version":version,"intentMode":plan.intent_mode,"activeGoalId":plan.active_goal_id,"goals":plan.goals.iter().map(|g|json!({"id":g.id,"title":g.title,"status":g.status,"dependsOn":g.depends_on})).collect::<Vec<_>>()})
                    }
                }
                None => json!({"version":0,"goals":[],"intentMode":null,"activeGoalId":null}),
            }
        };
        value["workTaskId"] = json!(work);
        value["authority"] = json!("modelMethodsOnly");
        value["previousVersion"] = value["version"]
            .as_i64()
            .filter(|v| *v > 1)
            .map(|v| json!(v - 1))
            .unwrap_or(Value::Null);
        value["historicalReferenceOnly"] = json!(historical);
        if historical {
            value["instruction"] = json!(
                "Use this plan only to understand earlier methods. Select what helps the present author request, then reassess current sources and results. Historical evidence and satisfied status do not establish current completion or grant authority."
            );
        } else if goal_id.is_none() && plan_work_id.is_none() {
            let mut stmt = self.connection.prepare(
                "SELECT p.rowid,p.work_task_id,p.version,p.content_json FROM story_model_work_plans_v1 p JOIN story_work_tasks_v1 w ON w.id=p.work_task_id WHERE w.project_id=?1 AND w.thread_id=?2 AND w.id<>?3 AND p.version=(SELECT MAX(q.version) FROM story_model_work_plans_v1 q WHERE q.work_task_id=p.work_task_id) AND (?4 IS NULL OR p.rowid<?4) ORDER BY p.rowid DESC LIMIT 7"
            ).map_err(storage_error)?;
            let rows = stmt
                .query_map(
                    params![
                        task.project_id.as_str(),
                        task.thread_id,
                        current_work,
                        before_plan_row_id
                    ],
                    |r| {
                        Ok((
                            r.get::<_, i64>(0)?,
                            r.get::<_, String>(1)?,
                            r.get::<_, i64>(2)?,
                            r.get::<_, String>(3)?,
                        ))
                    },
                )
                .map_err(storage_error)?;
            let rows = rows.collect::<Result<Vec<_>, _>>().map_err(storage_error)?;
            if before_plan_row_id.is_some() {
                // Directory pages do not repeat the potentially large current goal index.
                value["goals"] = json!([]);
                value["directoryOnly"] = json!(true);
            }
            let has_more = rows.len() > 6;
            let mut entries = Vec::new();
            for (_, id, version, content) in rows.iter().take(6) {
                let plan: Value =
                    serde_json::from_str(content).map_err(|_| RepositoryError::CorruptState)?;
                let goals = plan["goals"]
                    .as_array()
                    .ok_or_else(|| invalid("saved plan reference has no goal list"))?;
                // One short label per plan keeps directory size independent of goal count.
                let focus = goals
                    .iter()
                    .find(|g| g["status"] != "satisfied")
                    .or(goals.first());
                entries.push(json!({"workTaskId":id,"version":version,"focus":focus.map(|g|&g["title"]),"unfinishedGoals":goals.iter().filter(|g|g["status"]!="satisfied").count()}));
            }
            value["priorPlans"] = json!(entries);
            value["nextBeforePlanRowId"] = if has_more {
                json!(rows[5].0)
            } else {
                Value::Null
            };
            value["instruction"] = json!(
                "Select relevant earlier methods by planWorkId, or this Work's exact planVersion to read a retired method. Decide from the complete author request and current results. The current 0–12 goal window may be replaced, merged or retired; the directory is not completion evidence or execution authority."
            );
        }
        if value.get("priorPlans").is_some()
            && serde_json::to_vec(&value)
                .map_err(|_| RepositoryError::CorruptState)?
                .len()
                > 6400
        {
            // Separate page keeps both large accepted plans and their history readable.
            value["priorPlans"] = json!([]);
            value["nextBeforePlanRowId"] = json!(9_007_199_254_740_991_i64);
        }
        crate::work_decision::bounded(value)
    }
    pub fn update_model_work_plan(
        &mut self,
        task: &TaskRecord,
        request: &UpdateModelWorkPlan,
    ) -> Result<Value, RepositoryError> {
        let result = self.update_model_work_plan_inner(task, request);
        crate::recovery_operations::settle_prewrite_rejection(
            self,
            task,
            "update_work_plan",
            &result,
        )?;
        result
    }
    fn update_model_work_plan_inner(
        &mut self,
        task: &TaskRecord,
        request: &UpdateModelWorkPlan,
    ) -> Result<Value, RepositoryError> {
        let tx =
            rusqlite::Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
                .map_err(storage_error)?;
        let work = work_for(&tx, task)?;
        let target: Value =
            serde_json::from_str(&task.target_json).map_err(|_| RepositoryError::CorruptState)?;
        if target["conversationFocus"]["readOnly"] == true || target["readOnly"] == true {
            return Err(RepositoryError::TaskOutputForbidden);
        }
        if !nonempty(&request.idempotency_key, 160) || request.expected_plan_version < 0 {
            return Err(invalid("invalid identity or version"));
        }
        let plan = ModelWorkPlan {
            intent_mode: request.plan.intent_mode.clone(),
            active_goal_id: request
                .plan
                .goals
                .iter()
                .find(|g| g.status == "inProgress")
                .map(|g| g.id.clone()),
            goals: request.plan.goals.clone(),
        };
        let encoded = serde_json::to_string(&plan).map_err(|_| RepositoryError::CorruptState)?;
        let prior:Option<(i64,String)>=tx.query_row("SELECT version,content_json FROM story_model_work_plans_v1 WHERE task_id=?1 AND idempotency_key=?2",params![task.task_id,request.idempotency_key],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(storage_error)?;
        if let Some((version, old)) = prior {
            if old != encoded || version != request.expected_plan_version + 1 {
                return Err(invalid("idempotency key changed"));
            }
            crate::recovery_operations::commit_operation(
                &tx,
                self.recovery_operation.as_ref(),
                &task.project_id,
                &task.task_id,
                &["update_work_plan"],
                &format!("runtime:work-plan:{work}:{version}"),
            )?;
            tx.commit().map_err(storage_error)?;
            return Ok(
                json!({"version":version,"saved":true,"intentMode":plan.intent_mode,"activeGoalId":plan.active_goal_id,"grantsExecution":false}),
            );
        }
        crate::tasks::validate_active_task(
            &tx,
            task,
            &task.project_id,
            &task.thread_id,
            task.input_revision,
        )?;
        crate::task_input::assert_task_inputs_current(&tx, task)?;
        let previous = load(&tx, &work)?;
        if previous.as_ref().map_or(0, |(v, _)| *v) != request.expected_plan_version {
            return Err(RepositoryError::WorkTaskStale);
        }
        if !matches!(plan.intent_mode.as_str(), "planningOnly" | "executeGoals") {
            return Err(invalid(
                "intentMode must reflect whether the author requests only a saved future plan or execution of its goals",
            ));
        }
        let goals = &request.plan.goals;
        if goals.len() > 12 || encoded.len() > 24000 {
            return Err(invalid("plan size exceeds bounds"));
        }
        // Every accepted plan must remain readable through the bounded MCP contract.
        let index = goals
            .iter()
            .map(|g| json!({"id":g.id,"title":g.title,"status":g.status,"dependsOn":g.depends_on}))
            .collect::<Vec<_>>();
        if serde_json::to_vec(&index)
            .map_err(|_| RepositoryError::CorruptState)?
            .len()
            > 5500
        {
            return Err(invalid(
                "goal index exceeds read budget; shorten IDs and titles",
            ));
        }
        let mut ids = BTreeSet::new();
        for g in goals {
            if !nonempty(&g.id, 80)
                || !ids.insert(g.id.clone())
                || !nonempty(&g.title, 100)
                || !nonempty(&g.acceptance, 300)
                || !matches!(g.kind.as_str(), "deliverable" | "check" | "continuation")
                || !matches!(
                    g.status.as_str(),
                    "pending" | "inProgress" | "satisfied" | "blocked"
                )
                || serde_json::to_vec(g)
                    .map_err(|_| RepositoryError::CorruptState)?
                    .len()
                    > 5500
                || g.depends_on.len() > 12
                || g.evidence.len() > 8
                || g.source_ids.len() > 8
                || g.source_checks.len() > 8
                || [
                    &g.reading_question,
                    &g.check_scope,
                    &g.approach,
                    &g.assessment,
                ]
                .iter()
                .any(|s| s.chars().count() > 500)
            {
                return Err(invalid("invalid goal fields"));
            }
        }
        // These are model methods, not the author's contract. The current window
        // may replace, merge or retire goals; immutable snapshots retain history.
        // Author scope, budget, admitted actions and delivery review are separate.
        let mut resolved = BTreeSet::new();
        for _ in 0..goals.len() {
            for g in goals {
                if g.depends_on.iter().all(|d| resolved.contains(d)) {
                    resolved.insert(g.id.clone());
                }
            }
        }
        if resolved.len() != goals.len() {
            return Err(invalid(
                "dependencies must be existing goals without cycles",
            ));
        }
        if goals.iter().filter(|g| g.status == "inProgress").count() > 1 {
            return Err(invalid(
                "at most one goal may be inProgress; leave future work pending",
            ));
        }
        for g in goals {
            if g.status == "blocked" && !nonempty(&g.assessment, 500) {
                return Err(invalid(&format!(
                    "goal {}: blocked requires an actual obstacle in assessment",
                    g.id
                )));
            }
            if g.kind == "continuation" {
                let continuation = g.continuation.as_ref().ok_or_else(||invalid("continuation goals require their original runId and explicit nullable proposalId"))?;
                let known: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM story_generation_runs_v1 r JOIN story_work_tasks_v1 original ON original.id='work-'||r.run_id AND original.project_id=r.project_id JOIN story_work_tasks_v1 current ON current.id=?1 WHERE r.run_id=?2 AND original.project_id=current.project_id AND original.thread_id=current.thread_id AND original.id<>current.id AND current.kind!='generation')",params![work,continuation.run_id],|r|r.get(0)).map_err(storage_error)?;
                if !known
                    || !g.evidence.is_empty()
                    || !g.source_ids.is_empty()
                    || !g.source_checks.is_empty()
                {
                    return Err(invalid(
                        "continuation must name an existing original run in this conversation; its evidence is only the exact handoff proposal, not revision outputs or sourceIds",
                    ));
                }
                if let Some(proposal) = &continuation.proposal_id {
                    if !nonempty(proposal, 160) {
                        return Err(invalid("invalid continuation proposalId"));
                    }
                    let belongs: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM story_generation_handoffs_v1 WHERE proposal_id=?1 AND source_task_id=?2)",params![proposal,task.task_id],|r|r.get(0)).map_err(storage_error)?;
                    if !belongs || !continuation_evidence_current(&tx, &work, g)? {
                        return Err(invalid(
                            "continuation proposal must be the real resumeRun handoff registered by this current decision for the same original run",
                        ));
                    }
                }
            } else if g.continuation.is_some() {
                return Err(invalid(
                    "ordinary deliverable and check goals use continuation:null",
                ));
            }
            for check in &g.source_checks {
                crate::source_check_evidence::validate(&tx, &task.project_id, &work, check)?;
            }
            for e in &g.evidence {
                let valid:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM story_work_steps_v1 s JOIN story_tasks_v2 t ON t.task_id=s.current_attempt_id JOIN story_task_outputs_v2 o ON o.task_id=t.task_id JOIN story_object_heads_v2 h ON h.project_id=t.project_id AND COALESCE(h.candidate_revision_id,h.accepted_revision_id)=o.revision_id WHERE s.work_task_id=?1 AND t.project_id=?2 AND t.task_id=?3 AND t.status='completed' AND o.revision_id=?4)",params![work,task.project_id.as_str(),e.task_id,e.revision_id],|r|r.get(0)).map_err(storage_error)?;
                if !valid {
                    return Err(invalid(&format!(
                        "goal {}: evidence must be a current saved output of this Work; for a read-only check use its exact sourceIds instead",
                        g.id
                    )));
                }
            }
            for source in &g.source_ids {
                let valid:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM story_task_source_items_v2 i JOIN story_tasks_v2 t ON t.task_id=i.task_id JOIN story_work_attempts_v1 a ON a.attempt_id=t.task_id JOIN story_work_steps_v1 s ON s.id=a.step_id WHERE i.source_id=?1 AND t.project_id=?2 AND s.work_task_id=?3)",params![source,task.project_id.as_str(),work],|r|r.get(0)).map_err(storage_error)?;
                if !valid {
                    return Err(invalid(&format!(
                        "goal {}: sourceIds contains an unknown source. Copy the exact sourceId from read_context; preserve valid evidence and the actual assessment",
                        g.id
                    )));
                }
            }
            if g.status == "satisfied" {
                if !nonempty(&g.assessment, 500) {
                    return Err(invalid(&format!(
                        "goal {}: explain the actual verification in assessment",
                        g.id
                    )));
                }
                if g.kind == "continuation" && !continuation_evidence_current(&tx, &work, g)? {
                    return Err(invalid(
                        "satisfied continuation requires the real saved resumeRun proposal of this current decision",
                    ));
                }
                if g.kind == "check" && g.source_checks.is_empty() {
                    return Err(invalid(&format!(
                        "goal {}: a satisfied check requires sourceChecks with the current source revision, content hash and exact quote evidence; sourceIds only identifies a directory entry",
                        g.id
                    )));
                }
                if g.kind == "deliverable" && g.evidence.is_empty() {
                    return Err(invalid(&format!(
                        "goal {}: a satisfied deliverable must cite a current saved output of this Work in evidence; reading an existing source does not create the requested deliverable",
                        g.id
                    )));
                }
                if g.depends_on
                    .iter()
                    .any(|id| !goals.iter().any(|d| &d.id == id && d.status == "satisfied"))
                {
                    return Err(invalid(&format!(
                        "goal {}: satisfy its prerequisites before claiming completion",
                        g.id
                    )));
                }
            }
        }
        let version = request.expected_plan_version + 1;
        tx.execute(
            "INSERT INTO story_model_work_plans_v1 VALUES(?1,?2,?3,?4,?5)",
            params![
                work,
                version,
                task.task_id,
                request.idempotency_key,
                encoded
            ],
        )
        .map_err(storage_error)?;
        if plan.intent_mode == "executeGoals" {
            crate::work_decision::ensure_author_delivery_pending(&tx, task)?;
        }
        crate::recovery_operations::commit_operation(
            &tx,
            self.recovery_operation.as_ref(),
            &task.project_id,
            &task.task_id,
            &["update_work_plan"],
            &format!("runtime:work-plan:{work}:{version}"),
        )?;
        tx.commit().map_err(storage_error)?;
        Ok(
            json!({"version":version,"saved":true,"intentMode":plan.intent_mode,"activeGoalId":plan.active_goal_id,"grantsExecution":false}),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn active_contract_is_strict_while_historical_documents_remain_opaque() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE story_work_tasks_v1(id TEXT PRIMARY KEY); CREATE TABLE story_tasks_v2(task_id TEXT PRIMARY KEY); INSERT INTO story_work_tasks_v1 VALUES('history'); INSERT INTO story_tasks_v2 VALUES('task');").unwrap();
        db.execute_batch(SCHEMA).unwrap();
        let document = json!({"activeGoalId":null,"goals":[{"id":"prior","title":"作者保存的意图","status":"satisfied","customMethod":{"scope":"future"}}]});
        db.execute(
            "INSERT INTO story_model_work_plans_v1 VALUES('history',1,'task','saved',?1)",
            [document.to_string()],
        )
        .unwrap();
        assert_eq!(
            load_reference_document(&db, "history").unwrap().unwrap(),
            (1, document)
        );
        assert!(matches!(
            load(&db, "history"),
            Err(RepositoryError::CorruptState)
        ));
        let goal = json!({"id":"goal","title":"核对","acceptance":"读正文","kind":"check","status":"pending","dependsOn":[],"readingQuestion":"","checkScope":"","approach":"","assessment":"","evidence":[],"sourceIds":[],"sourceChecks":[],"continuation":null});
        for field in ["sourceChecks", "continuation"] {
            let mut missing = goal.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(serde_json::from_value::<ModelGoal>(missing).is_err());
        }
        assert!(serde_json::from_value::<ModelWorkPlanInput>(json!({"goals":[goal]})).is_err());
        assert!(serde_json::from_value::<GoalContinuation>(json!({"runId":"original"})).is_err());
    }
    #[test]
    fn method_dependencies_do_not_grant_execution_and_saved_claims_require_current_outputs() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE story_work_tasks_v1(id TEXT PRIMARY KEY);
            CREATE TABLE story_tasks_v2(task_id TEXT PRIMARY KEY,project_id TEXT,status TEXT);
            CREATE TABLE story_work_steps_v1(work_task_id TEXT,current_attempt_id TEXT);
            CREATE TABLE story_task_outputs_v2(task_id TEXT,revision_id TEXT);
            CREATE TABLE story_object_heads_v2(project_id TEXT,candidate_revision_id TEXT,accepted_revision_id TEXT);
            INSERT INTO story_work_tasks_v1 VALUES('work');
            INSERT INTO story_tasks_v2 VALUES('task','project','completed');
            INSERT INTO story_work_steps_v1 VALUES('work','task');
            INSERT INTO story_task_outputs_v2 VALUES('task','saved');
            INSERT INTO story_object_heads_v2 VALUES('project','saved',NULL);").unwrap();
        db.execute_batch(SCHEMA).unwrap();
        let goal = |id: &str, status: &str, deps: Vec<&str>| json!({"id":id,"title":id,"acceptance":"有保存回执","kind":"deliverable","status":status,"dependsOn":deps,"readingQuestion":"","checkScope":"","approach":"","assessment":"已核对保存成果","evidence":[{"taskId":"task","revisionId":"saved"}],"sourceIds":[],"sourceChecks":[],"continuation":null});
        let save = |version: i64, a: &str, b: &str| {
            let plan = json!({"intentMode":"executeGoals","activeGoalId":"write","goals":[goal("outline",a,vec![]),goal("write",b,vec!["outline"])]});
            db.execute(
                "INSERT INTO story_model_work_plans_v1 VALUES('work',?1,'task',?2,?3)",
                params![version, version.to_string(), plan.to_string()],
            )
            .unwrap();
        };
        assert!(action_ready(&db, "work").is_ok());
        assert!(final_delivery(&db, "work", "partial").is_ok());
        save(1, "pending", "inProgress");
        assert!(action_ready(&db, "work").is_ok());
        assert!(fulfilled(&db, "work").is_ok());
        assert!(final_delivery(&db, "work", "partial").is_ok());
        save(2, "satisfied", "inProgress");
        assert!(action_ready(&db, "work").is_ok());
        assert!(fulfilled(&db, "work").is_ok());
        save(3, "satisfied", "satisfied");
        assert!(fulfilled(&db, "work").is_ok());
        assert!(final_delivery(&db, "work", "partial").is_ok());
        db.execute(
            "UPDATE story_object_heads_v2 SET candidate_revision_id='newer'",
            [],
        )
        .unwrap();
        assert!(fulfilled(&db, "work").is_err());
        assert_eq!(summary(&db, "work").unwrap()["pendingGoals"], 2);
        assert!(action_ready(&db, "work").is_err());
        assert!(final_delivery(&db, "work", "partial").is_err());
    }
    #[test]
    fn descriptive_method_modes_are_not_permission_or_delivery_authority() {
        for mode in ["planningOnly", "executeGoals"] {
            let db = Connection::open_in_memory().unwrap();
            db.execute_batch("CREATE TABLE story_work_tasks_v1(id TEXT PRIMARY KEY); CREATE TABLE story_tasks_v2(task_id TEXT PRIMARY KEY); INSERT INTO story_work_tasks_v1 VALUES('work'); INSERT INTO story_tasks_v2 VALUES('task');").unwrap();
            db.execute_batch(SCHEMA).unwrap();
            let goal = json!({"id":"future-method","title":"以后可能使用的检查","acceptance":"按需判断","kind":"check","status":"pending","dependsOn":[],"readingQuestion":"","checkScope":"","approach":"","assessment":"","evidence":[],"sourceIds":[],"sourceChecks":[],"continuation":null});
            db.execute(
                "INSERT INTO story_model_work_plans_v1 VALUES('work',1,'task','method',?1)",
                [json!({"intentMode":mode,"activeGoalId":null,"goals":[goal]}).to_string()],
            )
            .unwrap();
            assert!(action_ready(&db, "work").is_ok());
            assert!(final_delivery(&db, "work", "partial").is_ok());
            assert_eq!(
                summary(&db, "work").unwrap()["authority"],
                "modelMethodsOnly"
            );
        }
    }
    fn continuation_fixture(status: &str, proposal: Option<&str>, assessment: &str) -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE story_work_tasks_v1(id TEXT PRIMARY KEY,project_id TEXT,thread_id TEXT,kind TEXT); CREATE TABLE story_tasks_v2(task_id TEXT PRIMARY KEY,project_id TEXT,thread_id TEXT,operation TEXT,status TEXT); CREATE TABLE story_work_steps_v1(work_task_id TEXT,current_attempt_id TEXT); CREATE TABLE story_generation_handoffs_v1(proposal_id TEXT,source_task_id TEXT,content_json TEXT); INSERT INTO story_work_tasks_v1 VALUES('work','project','thread','conversation'); INSERT INTO story_tasks_v2 VALUES('decision','project','thread','advisory','running'); INSERT INTO story_work_steps_v1 VALUES('work','decision');").unwrap();
        db.execute_batch(SCHEMA).unwrap();
        let goal = json!({"id":"resume","title":"接回原任务","acceptance":"前置完成后登记原范围交接","kind":"continuation","status":status,"dependsOn":[],"readingQuestion":"","checkScope":"原任务","approach":"登记真实resumeRun","assessment":assessment,"evidence":[],"sourceIds":[],"sourceChecks":[],"continuation":{"runId":"generation-original","proposalId":proposal}});
        db.execute("INSERT INTO story_model_work_plans_v1 VALUES('work',1,'decision','initial',?1)",[json!({"intentMode":"executeGoals","activeGoalId":if status=="inProgress"{json!("resume")}else{Value::Null},"goals":[goal]}).to_string()]).unwrap();
        db
    }
    #[test]
    fn pending_continuation_method_is_not_a_claim_that_the_author_goal_was_fulfilled() {
        for status in ["pending", "inProgress", "blocked"] {
            let db = continuation_fixture(status, None, "尚未登记交接");
            assert!(final_delivery(&db, "work", "partial").is_ok());
            assert_eq!(
                summary(&db, "work").unwrap()["authority"],
                "modelMethodsOnly"
            );
            assert_eq!(
                summary(&db, "work").unwrap()["continuationGoals"][0]["status"],
                status
            );
            assert_eq!(
                summary(&db, "work").unwrap()["continuationGoals"][0]["evidenceCurrent"],
                false
            );
            // Author completion is independently checked by delivery requirements.
            assert!(fulfilled(&db, "work").is_ok());
        }
    }
    #[test]
    fn continuation_proof_is_same_work_current_decision_and_exact_original_run() {
        let db = continuation_fixture(
            "satisfied",
            Some("proposal"),
            "前置已通过，原范围交接已登记",
        );
        assert!(fulfilled(&db, "work").is_err());
        let proposal = json!({"proposalId":"proposal","resumeRun":{"id":"generation-original","expectedVersion":4}});
        db.execute(
            "INSERT INTO story_generation_handoffs_v1 VALUES('proposal','decision',?1)",
            [proposal.to_string()],
        )
        .unwrap();
        assert!(fulfilled(&db, "work").is_ok());
        assert!(final_delivery(&db, "work", "fulfilled").is_ok());
        for state in ["queued", "waiting", "completed"] {
            db.execute("UPDATE story_tasks_v2 SET status=?1", [state])
                .unwrap();
            assert!(fulfilled(&db, "work").is_ok());
        }
        for state in ["failed", "cancelled"] {
            db.execute("UPDATE story_tasks_v2 SET status=?1", [state])
                .unwrap();
            assert!(fulfilled(&db, "work").is_err());
        }
        db.execute("UPDATE story_tasks_v2 SET status='running'", [])
            .unwrap();
        db.execute("UPDATE story_tasks_v2 SET thread_id='other'", [])
            .unwrap();
        assert!(fulfilled(&db, "work").is_err());
        db.execute("UPDATE story_tasks_v2 SET thread_id='thread'", [])
            .unwrap();
        db.execute(
            "UPDATE story_work_steps_v1 SET current_attempt_id='replacement'",
            [],
        )
        .unwrap();
        assert!(fulfilled(&db, "work").is_err());
        db.execute(
            "UPDATE story_work_steps_v1 SET current_attempt_id='decision'",
            [],
        )
        .unwrap();
        let mut foreign = proposal;
        foreign["resumeRun"]["id"] = json!("generation-other");
        db.execute(
            "UPDATE story_generation_handoffs_v1 SET content_json=?1",
            [foreign.to_string()],
        )
        .unwrap();
        assert!(fulfilled(&db, "work").is_err());
    }
}
