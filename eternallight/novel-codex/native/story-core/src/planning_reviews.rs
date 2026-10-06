use rusqlite::OptionalExtension;
use rusqlite::params;
use serde_json::Value;

use crate::CreateTaskRequest;
use crate::ObjectId;
use crate::ProjectId;
use crate::RepositoryError;
use crate::RevisionId;
use crate::SqliteStoryRepository;
use crate::TaskOutputBinding;

pub struct CreatePlanningReview<'a> {
    pub run_id: &'a str,
    pub project_id: &'a ProjectId,
    pub artifact_kind: &'a str,
    pub revision_id: &'a str,
    pub target_node_id: Option<&'a str>,
    pub author_intent: &'a str,
    pub author_message_id: &'a str,
    pub roles: &'a [String],
    pub now: i64,
}

pub struct SavePlanningReviewReport<'a> {
    pub project_id: &'a ProjectId,
    pub run_id: &'a str,
    pub task_id: &'a str,
    pub thread_id: &'a str,
    pub expected_revision: i64,
    pub report: &'a Value,
    pub now: i64,
}

pub(crate) fn valid_roles(roles: &[String]) -> bool {
    (1..=3).contains(&roles.len())
        && roles
            .iter()
            .all(|role| matches!(role.as_str(), "structure" | "character" | "experience"))
        && roles
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            == roles.len()
}

impl SqliteStoryRepository {
    pub fn create_planning_review(
        &mut self,
        request: CreatePlanningReview<'_>,
    ) -> Result<Value, RepositoryError> {
        if !matches!(request.artifact_kind, "outline" | "plan")
            || !crate::author_message::valid_id(request.author_message_id)
            || request.author_intent.trim().is_empty()
            || request.author_intent.chars().count() > 4_000
            || request.now < 0
            || !valid_roles(request.roles)
        {
            return Err(RepositoryError::InvalidContent(
                "invalid planning review scope or roles".into(),
            ));
        }
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let view = crate::planning_review_registration::create(
            &transaction,
            &request,
            crate::planning_review_registration::Origin::Direct,
        )?;
        transaction.commit().map_err(storage_error)?;
        Ok(view)
    }

    pub fn planning_reviews(&self, project_id: &ProjectId) -> Result<Vec<Value>, RepositoryError> {
        let mut statement = self.connection.prepare("SELECT run_id FROM story_planning_review_runs_v2 WHERE project_id = ?1 ORDER BY created_at DESC, run_id DESC LIMIT 10").map_err(storage_error)?;
        statement
            .query_map([project_id.as_str()], |row| row.get::<_, String>(0))
            .map_err(storage_error)?
            .map(|row| load_run(&self.connection, project_id, &row.map_err(storage_error)?))
            .collect()
    }

    pub fn planning_review(
        &self,
        project_id: &ProjectId,
        run_id: &str,
    ) -> Result<Value, RepositoryError> {
        let read = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let run = load_run(&read, project_id, run_id)?;
        read.commit().map_err(storage_error)?;
        Ok(run)
    }

    pub fn create_planning_review_seat_task(
        &mut self,
        project_id: &ProjectId,
        run_id: &str,
        role: &str,
        mut request: CreateTaskRequest,
    ) -> Result<crate::TaskRecord, RepositoryError> {
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let run = load_run(&transaction, project_id, run_id)?;
        let expected_target = crate::planning_review_work::target(
            run_id,
            role,
            run["artifactKind"]
                .as_str()
                .ok_or(RepositoryError::CorruptState)?,
            run["revisionId"]
                .as_str()
                .ok_or(RepositoryError::CorruptState)?,
        );
        let mut target: Value = serde_json::from_str(&request.target_json).map_err(json_error)?;
        if request.project_id != *project_id
            || target != expected_target
            || run["status"] != "running"
            || !run["roles"]
                .as_array()
                .is_some_and(|roles| roles.iter().any(|value| value.as_str() == Some(role)))
            || request.operation != "reviewPlanningSeat"
            || request.input_revision != run["inputRevision"].as_i64().unwrap_or(-1)
        {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        target["resumeStepId"] = Value::String(format!("review-seat-{run_id}-{role}"));
        request.target_json = target.to_string();
        let mut manifest: Value =
            serde_json::from_str(&request.input_manifest_json).map_err(json_error)?;
        manifest["target"] = target;
        request.input_manifest_json = manifest.to_string();
        let task = crate::tasks::create_task_record(&transaction, &request)?;
        transaction.commit().map_err(storage_error)?;
        Ok(task)
    }

    pub fn planning_review_context(
        &self,
        project_id: &ProjectId,
        task_id: &str,
    ) -> Result<Option<Value>, RepositoryError> {
        let task = crate::tasks::load_task(&self.connection, task_id)?;
        if task.project_id != *project_id {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        crate::task_input::assert_task_inputs_current(&self.connection, &task)?;
        self.connection.query_row(
            "SELECT run.run_id, seat.role, run.source_packet_json, run.source_packet_hash, run.checks_json, run.author_intent FROM story_planning_review_seats_v2 seat JOIN story_planning_review_runs_v2 run ON run.run_id = seat.run_id WHERE run.project_id = ?1 AND seat.task_id = ?2",
            params![project_id.as_str(), task_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?, row.get::<_, String>(4)?, row.get::<_, String>(5)?)),
        ).optional().map_err(storage_error)?.map(|row| {
            let packet: Value = serde_json::from_str(&row.2).map_err(json_error)?;
            let checks: Value = serde_json::from_str(&row.4).map_err(json_error)?;
            Ok(serde_json::json!({"reviewRunId": row.0, "role": row.1, "sourcePacket": packet, "sourcePacketHash": row.3, "checks": checks.get(&row.1).cloned().unwrap_or_else(|| serde_json::json!([])), "authorIntent": row.5}))
        }).transpose()
    }

    pub fn save_planning_review_report(
        &mut self,
        request: SavePlanningReviewReport<'_>,
    ) -> Result<Value, RepositoryError> {
        let SavePlanningReviewReport {
            project_id,
            run_id,
            task_id,
            thread_id,
            expected_revision,
            report,
            now,
        } = request;
        validate_report(report)?;
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let task = transaction.query_row(
            "SELECT operation, thread_id, input_revision, status FROM story_tasks_v2 WHERE project_id = ?1 AND task_id = ?2",
            params![project_id.as_str(), task_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?, row.get::<_, String>(3)?)),
        ).optional().map_err(storage_error)?.ok_or(RepositoryError::TaskNotFound)?;
        if task.0 != "reviewPlanningSeat"
            || task.1 != thread_id
            || task.2 != expected_revision
            || !matches!(task.3.as_str(), "queued" | "running" | "waiting")
        {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        let source_task = crate::tasks::load_task(&transaction, task_id)?;
        crate::task_input::assert_task_inputs_current(&transaction, &source_task)?;
        let role = transaction.query_row("SELECT role FROM story_planning_review_seats_v2 WHERE run_id = ?1 AND task_id = ?2", params![run_id, task_id], |row| row.get::<_, String>(0)).optional().map_err(storage_error)?.ok_or(RepositoryError::TaskBindingInvalid)?;
        let run = load_run(&transaction, project_id, run_id)?;
        if run["status"] != "running" || run["inputRevision"] != expected_revision {
            return Err(RepositoryError::TaskNotWritable);
        }
        let expected = run["checksByRole"][&role]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let actual = report["coverage"].as_array().cloned().unwrap_or_default();
        let complete = actual.len() == expected.len()
            && actual.iter().all(|item| item["status"] == "checked")
            && actual
                .iter()
                .all(|item| expected.contains(&item["checkId"]));
        let status = if complete { "complete" } else { "partial" };
        if let Some(existing) = transaction
            .query_row(
                "SELECT report_json FROM story_planning_review_reports_v2 WHERE task_id = ?1",
                [task_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(storage_error)?
        {
            if serde_json::from_str::<Value>(&existing).map_err(json_error)? != *report {
                return Err(RepositoryError::IdempotencyConflict);
            }
            let id: String = transaction
                .query_row(
                    "SELECT report_id FROM story_planning_review_reports_v2 WHERE task_id=?1",
                    [task_id],
                    |row| row.get(0),
                )
                .map_err(storage_error)?;
            crate::recovery_operations::commit_operation(
                &transaction,
                self.recovery_operation.as_ref(),
                project_id,
                task_id,
                &["submit_planning_review_report"],
                &format!("runtime:report:{id}"),
            )?;
            transaction.commit().map_err(storage_error)?;
            return Ok(
                serde_json::json!({"id":id,"runId":run_id,"role":role,"status":status,"report":report}),
            );
        }
        let report_id = format!("planning-report-{}", uuid::Uuid::new_v4().simple());
        transaction.execute("INSERT INTO story_planning_review_reports_v2(report_id, run_id, role, task_id, report_json, status, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)", params![report_id, run_id, role, task_id, report.to_string(), status, now]).map_err(storage_error)?;
        crate::tasks::record_task_output(
            &transaction,
            &TaskOutputBinding {
                task_id: task_id.into(),
                thread_id: thread_id.into(),
                output_kind: "critique".into(),
                completeness: Some(status.into()),
            },
            &RevisionId::parse(&report_id).map_err(|_| RepositoryError::CorruptState)?,
            &ObjectId::parse(format!("planning-review-{run_id}"))
                .map_err(|_| RepositoryError::CorruptState)?,
            /*working_base_revision_id*/ None,
        )?;
        transaction
            .execute(
                "UPDATE story_planning_review_runs_v2 SET updated_at = ?1 WHERE run_id = ?2",
                params![now, run_id],
            )
            .map_err(storage_error)?;
        let result = serde_json::json!({"id": report_id, "runId": run_id, "role": role, "status": status, "report": report});
        crate::recovery_operations::commit_operation(
            &transaction,
            self.recovery_operation.as_ref(),
            project_id,
            task_id,
            &["submit_planning_review_report"],
            &format!("runtime:report:{report_id}"),
        )?;
        transaction.commit().map_err(storage_error)?;
        Ok(result)
    }

    pub fn finish_planning_review(
        &mut self,
        project_id: &ProjectId,
        run_id: &str,
        cancel: bool,
        now: i64,
    ) -> Result<Value, RepositoryError> {
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let current = load_run(&transaction, project_id, run_id)?;
        if current["status"] != "running" {
            return Ok(current);
        }
        if cancel {
            transaction.execute("UPDATE story_tasks_v2 SET status = 'cancelled', stage = 'cancelled', delivery_status = 'incomplete', capability_revoked_at = ?1 WHERE task_id IN (SELECT task_id FROM story_planning_review_seats_v2 WHERE run_id = ?2) AND status IN ('queued','running','waiting')", params![now, run_id]).map_err(storage_error)?;
            transaction.execute("UPDATE story_planning_review_runs_v2 SET status = 'cancelled', updated_at = ?1 WHERE run_id = ?2", params![now, run_id]).map_err(storage_error)?;
        } else {
            let reports = current["reports"].as_array().cloned().unwrap_or_default();
            let roles = current["roles"].as_array().cloned().unwrap_or_default();
            let incomplete = roles
                .iter()
                .filter(|role| {
                    !reports
                        .iter()
                        .any(|report| report["role"] == **role && report["status"] == "complete")
                })
                .cloned()
                .collect::<Vec<_>>();
            let priorities = reports.iter().flat_map(|report| report["report"]["findings"].as_array().into_iter().flatten().enumerate().map(|(index, _)| serde_json::json!({"role": report["role"], "reportId": report["id"], "findingIndex": index}))).collect::<Vec<_>>();
            let status = if incomplete.is_empty() {
                "complete"
            } else {
                "partial"
            };
            let synthesis = serde_json::json!({"priorities": priorities, "disagreements": [], "incompleteRoles": incomplete});
            transaction.execute("UPDATE story_planning_review_runs_v2 SET status = ?1, synthesis_json = ?2, updated_at = ?3 WHERE run_id = ?4", params![status, synthesis.to_string(), now, run_id]).map_err(storage_error)?;
        }
        crate::planning_review_work::close(
            &transaction,
            project_id,
            run_id,
            if cancel {
                crate::planning_review_work::ReviewEnd::Cancelled
            } else {
                crate::planning_review_work::ReviewEnd::Finished
            },
            now,
        )?;
        let result = load_run(&transaction, project_id, run_id)?;
        transaction.commit().map_err(storage_error)?;
        Ok(result)
    }
}

pub(crate) fn load_run(
    connection: &rusqlite::Connection,
    project_id: &ProjectId,
    run_id: &str,
) -> Result<Value, RepositoryError> {
    let row = connection.query_row(
        "SELECT artifact_kind, revision_id, target_node_id, author_intent, input_revision, roles_json, source_packet_hash, checks_json, status, synthesis_json, created_at, updated_at, work_task_id, source_task_id, source_intent_revision, focus FROM story_planning_review_runs_v2 WHERE project_id = ?1 AND run_id = ?2",
        params![project_id.as_str(), run_id],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, Option<String>>(2)?, row.get::<_, String>(3)?, row.get::<_, i64>(4)?, row.get::<_, String>(5)?, row.get::<_, String>(6)?, row.get::<_, String>(7)?, row.get::<_, String>(8)?, row.get::<_, Option<String>>(9)?, row.get::<_, i64>(10)?, row.get::<_, i64>(11)?, row.get::<_, String>(12)?,row.get::<_, Option<String>>(13)?,row.get::<_, Option<i64>>(14)?,row.get::<_, String>(15)?)),
    ).optional().map_err(storage_error)?.ok_or(RepositoryError::ReviewNotFound)?;
    let source = crate::planning_review_author_source::root(connection, project_id, run_id)?;
    let roles: Value = serde_json::from_str(&row.5).map_err(json_error)?;
    let checks: Value = serde_json::from_str(&row.7).map_err(json_error)?;
    let mut seat_statement = connection.prepare("SELECT seat.role, seat.task_id, task.status FROM story_planning_review_seats_v2 seat LEFT JOIN story_tasks_v2 task ON task.task_id = seat.task_id WHERE seat.run_id = ?1 ORDER BY seat.role").map_err(storage_error)?;
    let seats = seat_statement.query_map([run_id], |seat| Ok(serde_json::json!({"role": seat.get::<_, String>(0)?, "taskId": seat.get::<_, Option<String>>(1)?, "status": seat.get::<_, Option<String>>(2)?.unwrap_or_else(|| "pending".into())}))).map_err(storage_error)?.map(|row| row.map_err(storage_error)).collect::<Result<Vec<_>, _>>()?;
    let mut report_statement = connection.prepare("SELECT report_id, role, status, report_json FROM story_planning_review_reports_v2 WHERE run_id = ?1 ORDER BY role").map_err(storage_error)?;
    let reports = report_statement.query_map([run_id], |report| Ok((report.get::<_, String>(0)?, report.get::<_, String>(1)?, report.get::<_, String>(2)?, report.get::<_, String>(3)?))).map_err(storage_error)?.map(|row| { let row = row.map_err(storage_error)?; Ok(serde_json::json!({"id": row.0, "runId": run_id, "role": row.1, "status": row.2, "report": serde_json::from_str::<Value>(&row.3).map_err(json_error)?})) }).collect::<Result<Vec<_>, RepositoryError>>()?;
    let status = match row.8.as_str() {
        "open" => "running",
        "complete" => "completed",
        value => value,
    };
    let coverage = if row.8 == "complete" {
        "complete"
    } else if row.8 == "open" {
        "pending"
    } else {
        "partial"
    };
    Ok(serde_json::json!({
        "id": run_id, "projectId": project_id.as_str(), "workTaskId": row.12, "sourceTaskId": row.13, "sourceIntentRevision": row.14, "focus": row.15, "artifactKind": row.0, "revisionId": row.1,
        "targetNodeId": row.2, "authorIntent": row.3, "authorMessageId": source.author_message_id, "inputRevision": row.4, "roles": roles,
        "sourcePacketHash": row.6, "status": status, "coverageStatus": coverage, "checksByRole": checks,
        "seats": seats, "reports": reports,
        "synthesis": row.9.map(|value| serde_json::from_str::<Value>(&value).unwrap_or(Value::Null)),
        "createdAt": row.10.to_string(), "updatedAt": row.11.to_string()
    }))
}

fn validate_report(report: &Value) -> Result<(), RepositoryError> {
    let object = report.as_object().ok_or_else(|| {
        RepositoryError::InvalidContent("planning review report must be an object".into())
    })?;
    if !["summary", "strengths", "findings", "coverage"]
        .iter()
        .all(|key| object.contains_key(*key))
        || !report["summary"].is_string()
        || !report["strengths"].is_array()
        || !report["findings"].is_array()
        || !report["coverage"].is_array()
        || report["strengths"]
            .as_array()
            .is_some_and(|items| items.len() > 3)
        || report["findings"]
            .as_array()
            .is_some_and(|items| items.len() > 6)
        || report["coverage"]
            .as_array()
            .is_some_and(|items| items.len() > 120)
    {
        return Err(RepositoryError::InvalidContent(
            "invalid planning review report".into(),
        ));
    }
    Ok(())
}

pub(crate) fn current_revision(
    connection: &rusqlite::Connection,
    project_id: &ProjectId,
) -> Result<i64, RepositoryError> {
    connection
        .query_row(
            "SELECT revision FROM story_workspaces_v2 WHERE project_id = ?1",
            [project_id.as_str()],
            |row| row.get(0),
        )
        .optional()
        .map_err(storage_error)?
        .ok_or(RepositoryError::ProjectNotFound)
}

pub(crate) fn json_error(error: serde_json::Error) -> RepositoryError {
    RepositoryError::InvalidContent(error.to_string())
}
pub(crate) fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}
