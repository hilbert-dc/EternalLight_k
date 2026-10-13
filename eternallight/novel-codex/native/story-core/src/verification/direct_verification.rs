//! Root-model verification is an explicit evidence method, not a bypass around
//! saved drafts, author sources, stable problems, or an independent-review contract.
use crate::{ProjectId, RepositoryError, TaskRecord, authoring_runs::storage_error};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub(crate) const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS story_direct_verifications_v1 (
 revision_id TEXT PRIMARY KEY REFERENCES story_revisions_v2(revision_id),
 work_task_id TEXT NOT NULL REFERENCES story_work_tasks_v1(id),
 decision_task_id TEXT NOT NULL REFERENCES story_tasks_v2(task_id),
 writer_task_id TEXT NOT NULL REFERENCES story_tasks_v2(task_id),
 draft_revision_id TEXT NOT NULL REFERENCES story_revisions_v2(revision_id),
 acceptance_json TEXT NOT NULL, target_json TEXT NOT NULL, request_json TEXT NOT NULL,
 idempotency_key TEXT NOT NULL, UNIQUE(decision_task_id,idempotency_key));";

pub(crate) fn enabled(target: &Value) -> bool {
    target["chapterAcceptance"]["verification"]["method"] == "direct"
}
fn invalid() -> RepositoryError {
    RepositoryError::TaskOutputForbidden
}

pub(crate) fn replay(
    db: &Connection,
    task: &TaskRecord,
    key: &str,
    request: &Value,
) -> Result<Option<Value>, RepositoryError> {
    let saved: Option<(String,String)> = db.query_row("SELECT revision_id,request_json FROM story_direct_verifications_v1 WHERE decision_task_id=?1 AND idempotency_key=?2",params![task.task_id,key],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(storage_error)?;
    saved.map(|(revision, encoded)| {
        if serde_json::from_str::<Value>(&encoded).map_err(|_| RepositoryError::CorruptState)? != *request { return Err(invalid()); }
        Ok(json!({"verificationSaved":true,"revisionId":revision,"method":"direct","continuesInCurrentDecision":true}))
    }).transpose()
}

pub(crate) fn save(
    db: &Connection,
    task: &TaskRecord,
    work: &str,
    target: &Value,
    request: &Value,
    key: &str,
) -> Result<Value, RepositoryError> {
    let writer_id = target["writerTaskId"].as_str().ok_or_else(invalid)?;
    let (writer, writer_target) = crate::revision_outcome::writer(db, &task.project_id, writer_id)?;
    if !enabled(&writer_target)
        || crate::work_issues::work_for(db, &task.project_id, writer_id)? != work
    {
        return Err(invalid());
    }
    crate::task_sources::require_author_requirements_read(db, task)?;
    let revision = target["draftRevisionId"].as_str().ok_or_else(invalid)?;
    crate::task_sources::require_current_revision_read(db, task, revision)?;
    if !crate::revision_outcome::inputs_current(db, &task.project_id, target)? {
        return Err(RepositoryError::TaskInputStale);
    }
    let supplied = &request["chapterReview"];
    if supplied.to_string().len() > 24_000
        || supplied.as_object().is_none_or(|o| {
            o.len() != 2 || !o.contains_key("outcomes") || !o.contains_key("uncoveredRequirements")
        })
    {
        return Err(invalid());
    }
    let mut report = supplied.clone();
    for field in [
        "writerTaskId",
        "chapterId",
        "originalRevisionId",
        "draftRevisionId",
        "knowledgeBatchId",
    ] {
        report[field] = target[field].clone();
    }
    let draft = crate::revision_outcome_validation::draft(
        db,
        &task.project_id,
        target["chapterId"].as_str().ok_or_else(invalid)?,
        revision,
    )?;
    let groups = draft["sceneGroups"].as_array().ok_or_else(invalid)?;
    for (outcome_index, item) in report["outcomes"].as_array_mut().ok_or_else(invalid)?.iter_mut().enumerate() {
        for (evidence_index, evidence) in item["evidence"].as_array_mut().ok_or_else(invalid)?.iter_mut().enumerate() {
            let group = evidence["groupId"].as_str().ok_or_else(invalid)?.to_owned();
            let body = crate::revision_outcome_validation::candidate_group_body(groups, &group, &target["draftRevisionId"],
                format!("/chapterReview/outcomes/{outcome_index}/evidence/{evidence_index}/groupId"))?;
            let mut quote = evidence.clone();
            quote.as_object_mut().ok_or_else(invalid)?.remove("groupId");
            *evidence = crate::source_evidence::resolve(&quote, body).map_err(|error|
                crate::source_evidence::located_error(error,
                    format!("/chapterReview/outcomes/{outcome_index}/evidence/{evidence_index}"), &group, &target["draftRevisionId"]))?;
            evidence["groupId"] = group.into();
        }
    }
    crate::revision_outcome_validation::validate_report(
        db,
        &task.project_id,
        target,
        &report.to_string(),
    )?;
    // The producer is a completed writer, but this record truthfully belongs to
    // the active coordinator. It is never registered as a fictitious child Task.
    let id = format!("verification-{}", uuid::Uuid::new_v4().simple());
    let encoded = report.to_string();
    db.execute("INSERT INTO story_revisions_v2(revision_id,project_id,object_id,object_kind,parent_revision_id,content_digest,content_json) VALUES(?1,?2,?3,'revisionOutcome',NULL,?4,?5)",params![id,task.project_id.as_str(),format!("direct-verification-{}",writer.task_id),format!("{:x}",Sha256::digest(encoded.as_bytes())),encoded]).map_err(storage_error)?;
    db.execute(
        "INSERT INTO story_direct_verifications_v1 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        params![
            id,
            work,
            task.task_id,
            writer_id,
            revision,
            writer_target["chapterAcceptance"].to_string(),
            target.to_string(),
            request.to_string(),
            key
        ],
    )
    .map_err(storage_error)?;
    crate::work_issues::record_report(db, task, &id)?;
    db.execute(
        "UPDATE story_work_tasks_v1 SET version=version+1 WHERE id=?1",
        [work],
    )
    .map_err(storage_error)?;
    Ok(
        json!({"verificationSaved":true,"revisionId":id,"method":"direct","continuesInCurrentDecision":true}),
    )
}

pub(crate) fn current(
    db: &Connection,
    project: &ProjectId,
    revision: &str,
) -> Result<bool, RepositoryError> {
    // Persisted evidence outlives its producer execution. Current source/acceptance
    // and recorded dependencies below determine validity; task control still gates writes.
    let row:Option<(String,String,String,String)> = db.query_row("SELECT d.decision_task_id,d.writer_task_id,d.acceptance_json,d.target_json FROM story_direct_verifications_v1 d JOIN story_tasks_v2 t ON t.task_id=d.decision_task_id AND t.project_id=?2 WHERE d.revision_id=?1",params![revision,project.as_str()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional().map_err(storage_error)?;
    let Some((decision, writer, acceptance, target)) = row else {
        return Ok(false);
    };
    let (_, writer_target) = crate::revision_outcome::writer(db, project, &writer)?;
    let acceptance: Value =
        serde_json::from_str(&acceptance).map_err(|_| RepositoryError::CorruptState)?;
    let target: Value = serde_json::from_str(&target).map_err(|_| RepositoryError::CorruptState)?;
    let latest:Option<String>=db.query_row("SELECT a.target_json FROM story_work_decision_actions_v1 a JOIN story_direct_verifications_v1 d ON d.work_task_id=a.work_task_id WHERE d.revision_id=?1 AND a.operation='generateChapter' AND json_extract(a.target_json,'$.chapterId')=json_extract(d.target_json,'$.chapterId') ORDER BY a.rowid DESC LIMIT 1",[revision],|r|r.get(0)).optional().map_err(storage_error)?;
    if let Some(latest) = latest {
        let latest: Value =
            serde_json::from_str(&latest).map_err(|_| RepositoryError::CorruptState)?;
        if !crate::chapter_delivery_review::same_acceptance(
            &acceptance,
            &latest["chapterAcceptance"],
        )? {
            return Ok(false);
        }
    }
    Ok(enabled(&writer_target)
        && crate::chapter_delivery_review::same_acceptance(
            &acceptance,
            &writer_target["chapterAcceptance"],
        )?
        && crate::revision_outcome::inputs_current(db, project, &target)?
        && crate::task_workspace::dependencies_current(db, &decision)?)
}

pub(crate) fn newer(
    db: &Connection,
    id: &str,
    other: Option<&str>,
) -> Result<bool, RepositoryError> {
    let Some(other) = other else { return Ok(true) };
    db.query_row("SELECT a.rowid>b.rowid FROM story_revisions_v2 a,story_revisions_v2 b WHERE a.revision_id=?1 AND b.revision_id=?2",params![id,other],|r|r.get(0)).map_err(storage_error)
}

pub(crate) fn report(
    db: &Connection,
    project: &ProjectId,
    writer: &str,
    other: Option<&str>,
) -> Result<Option<crate::StoredRevisionOutcome>, RepositoryError> {
    let row:Option<(String,String)>=db.query_row("SELECT r.revision_id,r.content_json FROM story_direct_verifications_v1 d JOIN story_revisions_v2 r ON r.revision_id=d.revision_id AND r.project_id=?1 WHERE d.writer_task_id=?2 ORDER BY d.rowid DESC LIMIT 1",params![project.as_str(),writer],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(storage_error)?;
    let Some((id, encoded)) = row else {
        return Ok(None);
    };
    if !newer(db, &id, other)? {
        return Ok(None);
    }
    Ok(Some(crate::StoredRevisionOutcome {
        current: current(db, project, &id)?,
        issues: crate::work_issues::report_issues(db, project, &id)?,
        revision_id: id,
        content: serde_json::from_str(&encoded).map_err(|_| RepositoryError::CorruptState)?,
        decisions: vec![],
    }))
}

pub(crate) fn state(
    db: &Connection,
    project: &ProjectId,
    writer: &str,
    revision: &str,
    mut state: Value,
) -> Result<Option<Value>, RepositoryError> {
    let row:Option<(String,String)> = db.query_row("SELECT r.revision_id,r.content_json FROM story_direct_verifications_v1 d JOIN story_revisions_v2 r ON r.revision_id=d.revision_id AND r.project_id=?1 WHERE d.writer_task_id=?2 AND d.draft_revision_id=?3 ORDER BY d.rowid DESC LIMIT 1",params![project.as_str(),writer,revision],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(storage_error)?;
    let Some((id, encoded)) = row else {
        return Ok(None);
    };
    let report: Value =
        serde_json::from_str(&encoded).map_err(|_| RepositoryError::CorruptState)?;
    let current = current(db, project, &id)?;
    state["method"] = json!("direct");
    state["reportRevisionId"] = json!(id);
    state["current"] = json!(current);
    state["checks"]=json!(report["outcomes"].as_array().ok_or_else(invalid)?.iter().map(|c|json!({"id":c["id"],"status":c["status"],"issueId":c["issue"]["issueId"],"reason":c["reason"].as_str().unwrap_or("").chars().take(96).collect::<String>()})).collect::<Vec<_>>());
    state["uncoveredRequirements"]=json!(report["uncoveredRequirements"].as_array().ok_or_else(invalid)?.iter().take(2).map(|r|json!({"requirement":r["requirement"].as_str().unwrap_or("").chars().take(96).collect::<String>(),"reason":r["reason"].as_str().unwrap_or("").chars().take(96).collect::<String>(),"issue":r["issue"],"authorSourceChecks":[]})).collect::<Vec<_>>());
    state["uncoveredRequirementCount"] = json!(
        report["uncoveredRequirements"]
            .as_array()
            .ok_or_else(invalid)?
            .len()
    );
    let unmet = report["outcomes"]
        .as_array()
        .ok_or_else(invalid)?
        .iter()
        .any(|c| c["status"] == "unmet");
    let work = crate::work_issues::work_for(db, project, writer)?;
    state["verdict"] = json!(if !current {
        "stale"
    } else if unmet
        || state["uncoveredRequirementCount"] != 0
        || state["length"]["satisfied"] != true
    {
        "notMet"
    } else if report["outcomes"]
        .as_array()
        .ok_or_else(invalid)?
        .iter()
        .any(|c| c["status"] == "insufficientEvidence")
    {
        "insufficientEvidence"
    } else if crate::work_issues::has_unresolved(db, project, &work, state["chapterId"].as_str())? {
        "notMet"
    } else {
        "passed"
    });
    Ok(Some(state))
}
