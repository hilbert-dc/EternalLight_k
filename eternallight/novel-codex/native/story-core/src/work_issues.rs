//! A reference index over actual saved reports, not another semantic truth or scheduler.
//! Models choose problem identity; Native validates provenance and applicable versions.
use crate::{
    ProjectId, RepositoryError, SqliteStoryRepository, TaskRecord, authoring_runs::storage_error,
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS story_work_issues_v1 (
 ordinal INTEGER PRIMARY KEY AUTOINCREMENT,
 issue_id TEXT NOT NULL UNIQUE,
 project_id TEXT NOT NULL REFERENCES story_projects_v2(project_id),
 work_task_id TEXT NOT NULL REFERENCES story_work_tasks_v1(id),
 summary TEXT NOT NULL,
 chapter_id TEXT,
 created_by_task_id TEXT NOT NULL REFERENCES story_tasks_v2(task_id)
);
CREATE INDEX IF NOT EXISTS story_work_issues_work_v1 ON story_work_issues_v1(work_task_id,ordinal);
CREATE TABLE IF NOT EXISTS story_issue_observations_v1 (
 ordinal INTEGER PRIMARY KEY AUTOINCREMENT,
 observation_id TEXT NOT NULL UNIQUE,
 issue_id TEXT NOT NULL REFERENCES story_work_issues_v1(issue_id),
 task_id TEXT NOT NULL REFERENCES story_tasks_v2(task_id),
 report_revision_id TEXT REFERENCES story_revisions_v2(revision_id),
 draft_revision_id TEXT REFERENCES story_revisions_v2(revision_id),
 criterion_id TEXT,
 source TEXT NOT NULL CHECK(source IN ('revisionOutcome','deliveryReview')),
 source_path TEXT NOT NULL,
 source_digest TEXT NOT NULL,
 status TEXT NOT NULL CHECK(status IN ('supported','unmet','insufficientEvidence')),
 resolution_json TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS story_issue_observations_issue_v1 ON story_issue_observations_v1(issue_id,ordinal);
CREATE TABLE IF NOT EXISTS story_issue_resolution_refs_v1 (
 observation_id TEXT NOT NULL REFERENCES story_issue_observations_v1(observation_id),
 resolves_observation_id TEXT NOT NULL REFERENCES story_issue_observations_v1(observation_id),
 PRIMARY KEY(observation_id,resolves_observation_id)
);
CREATE INDEX IF NOT EXISTS story_issue_resolution_target_v1 ON story_issue_resolution_refs_v1(resolves_observation_id);

"#;

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct IssueLink {
    pub issue_id: String,
    #[serde(deserialize_with = "crate::deserialize_execution_limit")]
    pub new_issue: Option<NewIssue>,
    pub resolution_of: Vec<String>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NewIssue {
    pub summary: String,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AuthorQuote {
    pub source_id: String,
    pub content_hash: String,
    pub quote: String,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct UncoveredRequirement {
    pub issue: IssueLink,
    pub requirement: String,
    pub reason: String,
    pub author_source_checks: Vec<AuthorQuote>,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct UnresolvedIssue {
    pub issue: IssueLink,
    #[serde(deserialize_with = "crate::deserialize_execution_limit")]
    pub observation_id: Option<String>,
    pub reason: String,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct IssueResolution {
    pub issue: IssueLink,
    pub reason: String,
}
fn invalid(s: &str) -> RepositoryError {
    RepositoryError::InvalidContent(format!("workIssue: {s}"))
}
fn text(s: &str, max: usize) -> bool {
    !s.trim().is_empty() && s.chars().count() <= max
}
pub(crate) fn identifier(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 160
        && s.as_bytes()[0].is_ascii_alphanumeric()
        && s.bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._:-".contains(&c))
}
pub(crate) fn work_for(
    db: &Connection,
    project: &ProjectId,
    task: &str,
) -> Result<String, RepositoryError> {
    db.query_row("SELECT w.id FROM story_work_attempts_v1 a JOIN story_work_steps_v1 s ON s.id=a.step_id JOIN story_work_tasks_v1 w ON w.id=s.work_task_id WHERE a.attempt_id=?1 AND w.project_id=?2",params![task,project.as_str()],|r|r.get(0)).optional().map_err(storage_error)?.ok_or(RepositoryError::TaskBindingInvalid)
}
fn owner(db: &Connection, project: &ProjectId, work: &str) -> Result<(), RepositoryError> {
    let exists: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM story_work_tasks_v1 WHERE id=?1 AND project_id=?2)",
            params![work, project.as_str()],
            |r| r.get(0),
        )
        .map_err(storage_error)?;
    if exists {
        Ok(())
    } else {
        Err(RepositoryError::TaskBindingInvalid)
    }
}
pub(crate) fn validate_link(
    db: &Connection,
    project: &ProjectId,
    work: &str,
    link: &IssueLink,
) -> Result<(), RepositoryError> {
    owner(db, project, work)?;
    if !identifier(&link.issue_id)
        || link.resolution_of.len() > 16
        || link.resolution_of.iter().any(|s| !identifier(s))
        || link.resolution_of.iter().collect::<BTreeSet<_>>().len() != link.resolution_of.len()
    {
        return Err(invalid("invalid stable identity or observation references"));
    }
    let existing: Option<(String, String, String)> = db
        .query_row(
            "SELECT project_id,work_task_id,summary FROM story_work_issues_v1 WHERE issue_id=?1",
            [&link.issue_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .map_err(storage_error)?;
    match (&existing, &link.new_issue) {
        (None, Some(new)) if text(&new.summary, 600) && link.resolution_of.is_empty() => {}
        (Some((p, w, summary)), new)
            if p == project.as_str()
                && w == work
                && new.as_ref().is_none_or(|n| n.summary == *summary) => {}
        _ => {
            return Err(invalid(
                "new problem needs a description; an existing problem belongs to this exact Work",
            ));
        }
    }
    for reference in &link.resolution_of {
        let valid:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM story_issue_observations_v1 WHERE observation_id=?1 AND issue_id=?2)",params![reference,link.issue_id],|r|r.get(0)).map_err(storage_error)?;
        if !valid {
            return Err(invalid(
                "resolutionOf must cite a real viewpoint of this problem",
            ));
        }
    }
    Ok(())
}
/// Validate all links of one immutable report together. The model may associate
/// several criteria with one newly declared problem without requiring a second
/// report or an extra registration action. Definitions never escape this batch.
pub(crate) fn validate_report_links(
    db: &Connection,
    project: &ProjectId,
    work: &str,
    links: &[&IssueLink],
) -> Result<(), RepositoryError> {
    let mut definitions = BTreeMap::new();
    for link in links {
        if let Some(new) = &link.new_issue {
            if definitions
                .insert(&link.issue_id, &new.summary)
                .is_some_and(|old| old != &new.summary)
            {
                return Err(invalid(
                    "one actual report cannot redefine a new issue with different summaries",
                ));
            }
        }
    }
    for link in links {
        let mut resolved = (*link).clone();
        if resolved.new_issue.is_none() {
            let existing: bool = db
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM story_work_issues_v1 WHERE issue_id=?1)",
                    [&resolved.issue_id],
                    |r| r.get(0),
                )
                .map_err(storage_error)?;
            if !existing {
                if let Some(summary) = definitions.get(&resolved.issue_id) {
                    resolved.new_issue = Some(NewIssue {
                        summary: (*summary).clone(),
                    });
                }
            }
        }
        validate_link(db, project, work, &resolved)?;
    }
    Ok(())
}
pub(crate) fn validate_uncovered(
    db: &Connection,
    project: &ProjectId,
    work: &str,
    items: &[UncoveredRequirement],
) -> Result<(), RepositoryError> {
    if items.len() > 8 {
        return Err(invalid("at most eight uncovered requirements"));
    }
    for (requirement_index, item) in items.iter().enumerate() {
        if !text(&item.requirement, 600)
            || !text(&item.reason, 1600)
            || item.author_source_checks.is_empty()
            || item.author_source_checks.len() > 4
        {
            return Err(invalid(
                "uncovered author requirement needs its actual source and reason",
            ));
        }
        // Identity links are validated with outcome links as one report batch.
        for (source_check_index, quote) in item.author_source_checks.iter().enumerate() {
            crate::task_sources::validate_author_requirement_quote(
                db,
                project,
                work,
                &quote.source_id,
                &quote.content_hash,
                &quote.quote,
            )
            .map_err(|error| match error {
                RepositoryError::PayloadValidation {
                    code: "author_requirement_source_not_admitted",
                    instance_path,
                    mut details,
                } => {
                    details["requirementIndex"] = json!(requirement_index);
                    details["authorSourceCheckIndex"] = json!(source_check_index);
                    details["relativeInstancePath"] = json!(instance_path);
                    RepositoryError::PayloadValidation {
                        code: "author_requirement_source_not_admitted",
                        instance_path: "/uncoveredRequirements".into(),
                        details,
                    }
                }
                other => other,
            })?
        }
    }
    Ok(())
}
fn register(
    db: &Connection,
    task: &TaskRecord,
    work: &str,
    chapter: Option<&str>,
    link: &IssueLink,
) -> Result<(), RepositoryError> {
    validate_link(db, &task.project_id, work, link)?;
    let created: Option<String> = db
        .query_row(
            "SELECT created_by_task_id FROM story_work_issues_v1 WHERE issue_id=?1",
            [&link.issue_id],
            |r| r.get(0),
        )
        .optional()
        .map_err(storage_error)?;
    match created {
        None => {
            db.execute("INSERT INTO story_work_issues_v1(issue_id,project_id,work_task_id,summary,chapter_id,created_by_task_id) VALUES(?1,?2,?3,?4,?5,?6)",params![link.issue_id,task.project_id.as_str(),work,link.new_issue.as_ref().ok_or_else(||invalid("newIssue required"))?.summary,chapter,task.task_id]).map_err(storage_error)?;
        }
        Some(_) if link.new_issue.is_some() => {
            return Err(invalid(
                "existing issue must be selected with newIssue:null; identity cannot be redefined",
            ));
        }
        _ => {}
    }
    let bound: Option<String> = db
        .query_row(
            "SELECT chapter_id FROM story_work_issues_v1 WHERE issue_id=?1",
            [&link.issue_id],
            |r| r.get(0),
        )
        .map_err(storage_error)?;
    if bound.as_deref() != chapter {
        return Err(invalid(
            "a chapter-scoped issue cannot be reassessed or resolved by another chapter's grant",
        ));
    }
    Ok(())
}
fn observe(
    db: &Connection,
    task: &TaskRecord,
    revision: Option<&str>,
    draft: Option<&str>,
    criterion: Option<&str>,
    source: &str,
    path: &str,
    status: &str,
    link: &IssueLink,
) -> Result<(), RepositoryError> {
    let exists:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM story_issue_observations_v1 WHERE task_id=?1 AND report_revision_id IS ?2 AND source_path=?3)",params![task.task_id,revision,path],|r|r.get(0)).map_err(storage_error)?;
    if exists {
        return Ok(());
    }
    let observation_id = format!("observation-{}", uuid::Uuid::new_v4().simple());
    db.execute("INSERT INTO story_issue_observations_v1(observation_id,issue_id,task_id,report_revision_id,draft_revision_id,criterion_id,source,source_path,status,resolution_json,source_digest) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",params![observation_id,link.issue_id,task.task_id,revision,draft,criterion,source,path,status,serde_json::to_string(&link.resolution_of).map_err(|_|RepositoryError::CorruptState)?,if source=="deliveryReview"{db.query_row("SELECT json_extract(content_json,'$.expectedSourceDigest') FROM story_work_delivery_reviews_v1 WHERE decision_task_id=?1",[&task.task_id],|r|r.get::<_,String>(0)).map_err(storage_error)?}else{crate::task_input::project_source_digest(db,&task.project_id)?}]).map_err(storage_error)?;
    for reference in &link.resolution_of {
        db.execute("INSERT INTO story_issue_resolution_refs_v1(observation_id,resolves_observation_id) VALUES(?1,?2)",params![observation_id,reference]).map_err(storage_error)?;
    }
    Ok(())
}
/// Called after the actual revision and output exist, in their save transaction.
pub(crate) fn record_report(
    db: &Connection,
    task: &TaskRecord,
    revision: &str,
) -> Result<(), RepositoryError> {
    let work = work_for(db, &task.project_id, &task.task_id)?;
    let encoded:String=db.query_row("SELECT content_json FROM story_revisions_v2 WHERE project_id=?1 AND revision_id=?2 AND object_kind='revisionOutcome'",params![task.project_id.as_str(),revision],|r|r.get(0)).map_err(storage_error)?;
    let report: Value =
        serde_json::from_str(&encoded).map_err(|_| RepositoryError::CorruptState)?;
    let mut declarations = Vec::new();
    for field in ["outcomes", "uncoveredRequirements"] {
        for item in report[field]
            .as_array()
            .ok_or(RepositoryError::CorruptState)?
        {
            if !item["issue"].is_null() {
                declarations.push(
                    serde_json::from_value::<IssueLink>(item["issue"].clone())
                        .map_err(|_| RepositoryError::TaskOutputForbidden)?,
                );
            }
        }
    }
    validate_report_links(
        db,
        &task.project_id,
        &work,
        &declarations.iter().collect::<Vec<_>>(),
    )?;
    let mut registered = BTreeSet::new();
    for link in &declarations {
        if link.new_issue.is_some() && registered.insert(link.issue_id.as_str()) {
            // Existing identities cannot be redeclared by a later report. The
            // outer actual save transaction handles idempotent replay first.
            register(db, task, &work, report["chapterId"].as_str(), link)?;
        }
    }
    for (field, uncovered) in [("outcomes", false), ("uncoveredRequirements", true)] {
        for (i, item) in report[field]
            .as_array()
            .ok_or(RepositoryError::CorruptState)?
            .iter()
            .enumerate()
        {
            if item["issue"].is_null() {
                continue;
            }
            let mut link: IssueLink = serde_json::from_value(item["issue"].clone())
                .map_err(|_| RepositoryError::TaskOutputForbidden)?;
            link.new_issue = None;
            register(db, task, &work, report["chapterId"].as_str(), &link)?;
            observe(
                db,
                task,
                Some(revision),
                report["draftRevisionId"].as_str(),
                if uncovered { None } else { item["id"].as_str() },
                "revisionOutcome",
                &format!("/{field}/{i}"),
                if uncovered {
                    "unmet"
                } else {
                    item["status"]
                        .as_str()
                        .ok_or(RepositoryError::CorruptState)?
                },
                &link,
            )?;
        }
    }
    Ok(())
}
pub(crate) fn record_delivery(
    db: &Connection,
    task: &TaskRecord,
    work: &str,
    items: &[UnresolvedIssue],
) -> Result<(), RepositoryError> {
    for (i, item) in items.iter().enumerate() {
        if !text(&item.reason, 600) || !item.issue.resolution_of.is_empty() {
            return Err(invalid(
                "unresolved item cites a viewpoint; it cannot resolve another judgment",
            ));
        }
        validate_link(db, &task.project_id, work, &item.issue)?;
        match (&item.issue.new_issue, &item.observation_id) {
            (None, Some(id)) => {
                let same:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM story_issue_observations_v1 WHERE observation_id=?1 AND issue_id=?2)",params![id,item.issue.issue_id],|r|r.get(0)).map_err(storage_error)?;
                if !same {
                    return Err(invalid(
                        "unresolved must reference a real observation of this issue",
                    ));
                }
            }
            (Some(_), None) => {
                register(db, task, work, None, &item.issue)?;
                observe(
                    db,
                    task,
                    None,
                    None,
                    None,
                    "deliveryReview",
                    &format!("/unresolved/{i}"),
                    "unmet",
                    &item.issue,
                )?;
            }
            _ => {
                return Err(invalid(
                    "reference an existing issue viewpoint or create the issue in this actual review",
                ));
            }
        }
    }
    Ok(())
}
pub(crate) fn record_resolutions(
    db: &Connection,
    task: &TaskRecord,
    work: &str,
    items: &[IssueResolution],
) -> Result<(), RepositoryError> {
    if items.len() > 16 {
        return Err(invalid("at most sixteen issue resolutions"));
    }
    for (i, item) in items.iter().enumerate() {
        validate_link(db, &task.project_id, work, &item.issue)?;
        let chapter: Option<String> = db
            .query_row(
                "SELECT chapter_id FROM story_work_issues_v1 WHERE issue_id=?1",
                [&item.issue.issue_id],
                |r| r.get(0),
            )
            .map_err(storage_error)?;
        if chapter.is_some()
            || item.issue.new_issue.is_some()
            || item.issue.resolution_of.is_empty()
            || !text(&item.reason, 600)
        {
            return Err(invalid(
                "delivery self-assessment may recheck only actual non-prose issues with real referenced viewpoints",
            ));
        }
        observe(
            db,
            task,
            None,
            None,
            None,
            "deliveryReview",
            &format!("/issueResolutions/{i}"),
            "supported",
            &item.issue,
        )?;
    }
    Ok(())
}
fn report_current(
    db: &Connection,
    project: &ProjectId,
    task: &str,
    revision: Option<&str>,
    source: &str,
) -> Result<bool, RepositoryError> {
    if let Some(revision) = revision {
        let direct: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM story_direct_verifications_v1 WHERE revision_id=?1 AND decision_task_id=?2)",params![revision,task],|r|r.get(0)).map_err(storage_error)?;
        if direct { return crate::direct_verification::current(db,project,revision); }
    }
    if source == "deliveryReview" {
        let digest:Option<String>=db.query_row("SELECT source_digest FROM story_issue_observations_v1 WHERE task_id=?1 AND source='deliveryReview' ORDER BY ordinal DESC LIMIT 1",[task],|r|r.get(0)).optional().map_err(storage_error)?;
        return Ok(digest.is_some_and(|d| {
            crate::task_input::project_source_digest(db, project).is_ok_and(|current| current == d)
        }));
    }
    let record = crate::tasks::load_task(db, task)?;
    if record.status != "completed" {
        return Ok(false);
    }
    let target: Value =
        serde_json::from_str(&record.target_json).map_err(|_| RepositoryError::CorruptState)?;
    let actual:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM story_task_outputs_v2 WHERE task_id=?1 AND revision_id=?2 AND output_kind='revisionOutcome')",params![task,revision],|r|r.get(0)).map_err(storage_error)?;
    if !actual {
        return Ok(false);
    }
    if crate::revision_outcome::validate_target(db, project, &target).is_err() {
        return Ok(false);
    }
    let work = work_for(db, project, task)?;
    let latest:Option<String>=db.query_row("SELECT target_json FROM story_work_decision_actions_v1 WHERE work_task_id=?1 AND operation='generateChapter' AND json_extract(target_json,'$.chapterId')=?2 ORDER BY rowid DESC LIMIT 1",params![work,target["chapterId"].as_str()],|r|r.get(0)).optional().map_err(storage_error)?;
    if let Some(latest) = latest {
        let latest: Value =
            serde_json::from_str(&latest).map_err(|_| RepositoryError::CorruptState)?;
        let (_, writer) = crate::revision_outcome::writer(
            db,
            project,
            target["writerTaskId"]
                .as_str()
                .ok_or(RepositoryError::CorruptState)?,
        )?;
        if !crate::chapter_delivery_review::same_acceptance(
            &latest["chapterAcceptance"],
            &writer["chapterAcceptance"],
        )? {
            return Ok(false);
        }
    }
    let current = crate::revision_outcome::inputs_current(db, project, &target)?
        && crate::revision_outcome::assessment_sources_current(
            db,
            project,
            task,
            target["chapterNumber"]
                .as_i64()
                .ok_or(RepositoryError::CorruptState)?,
        )?;
    Ok(current)
}
fn current_draft(
    db: &Connection,
    project: &ProjectId,
    chapter: Option<&str>,
) -> Result<Option<String>, RepositoryError> {
    chapter.map(|c|db.query_row("SELECT COALESCE(candidate_revision_id,accepted_revision_id) FROM story_object_heads_v2 WHERE project_id=?1 AND object_id='draft-'||?2",params![project.as_str(),c],|r|r.get(0)).optional().map_err(storage_error).map(Option::flatten)).transpose().map(Option::flatten)
}
fn observation(db: &Connection, id: &str) -> Result<Value, RepositoryError> {
    let (issue,task,revision,draft,criterion,source,path,status,resolutions):(String,String,Option<String>,Option<String>,Option<String>,String,String,String,String)=db.query_row("SELECT issue_id,task_id,report_revision_id,draft_revision_id,criterion_id,source,source_path,status,resolution_json FROM story_issue_observations_v1 WHERE observation_id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?,r.get(7)?,r.get(8)?))).map_err(storage_error)?;
    let encoded: String = if let Some(r) = &revision {
        db.query_row(
            "SELECT content_json FROM story_revisions_v2 WHERE revision_id=?1",
            [r],
            |r| r.get(0),
        )
        .map_err(storage_error)?
    } else {
        db.query_row(
            "SELECT content_json FROM story_work_delivery_reviews_v1 WHERE decision_task_id=?1",
            [&task],
            |r| r.get(0),
        )
        .map_err(storage_error)?
    };
    let report: Value =
        serde_json::from_str(&encoded).map_err(|_| RepositoryError::CorruptState)?;
    let item = report.pointer(&path).ok_or(RepositoryError::CorruptState)?;
    let project = crate::tasks::load_task(db, &task)?.project_id;
    let current = report_current(db, &project, &task, revision.as_deref(), &source)?;
    let superseded:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM story_issue_resolution_refs_v1 WHERE resolves_observation_id=?1)",[id],|r|r.get(0)).map_err(storage_error)?;
    Ok(
        json!({"current":current,"recheckRequired":!current&&!superseded,"superseded":superseded,"observationId":id,"issueId":issue,"taskId":task,"reportRevisionId":revision,"draftRevisionId":draft,"criterionId":criterion,"source":source,"status":status,"reason":item["reason"],"resolutionOf":serde_json::from_str::<Value>(&resolutions).map_err(|_|RepositoryError::CorruptState)?,"evidence":item.get("evidence").or_else(||item.get("authorSourceChecks")).cloned().unwrap_or_else(||json!([]))}),
    )
}
fn state(db: &Connection, project: &ProjectId, id: &str) -> Result<&'static str, RepositoryError> {
    // Stream only the surviving reference graph. No full reports, quotations, or
    // historical body copies are materialized to count current issue states.
    let mut q=db.prepare("SELECT o.task_id,o.report_revision_id,o.source,o.status FROM story_issue_observations_v1 o WHERE o.issue_id=?1 AND NOT EXISTS(SELECT 1 FROM story_issue_resolution_refs_v1 r WHERE r.resolves_observation_id=o.observation_id)").map_err(storage_error)?;
    let mut rows = q.query([id]).map_err(storage_error)?;
    let (mut supported, mut negative, mut stale) = (false, false, false);
    while let Some(r) = rows.next().map_err(storage_error)? {
        let task: String = r.get(0).map_err(storage_error)?;
        let revision: Option<String> = r.get(1).map_err(storage_error)?;
        let source: String = r.get(2).map_err(storage_error)?;
        let status: String = r.get(3).map_err(storage_error)?;
        if report_current(db, project, &task, revision.as_deref(), &source)? {
            if status == "supported" {
                supported = true
            } else {
                negative = true
            }
        } else {
            stale = true
        }
    }
    Ok(match (supported, negative, stale) {
        (true, true, _) => "disputed",
        (_, true, _) => "open",
        (true, false, false) => "resolved",
        _ => "stale",
    })
}
fn projection(
    db: &Connection,
    project: &ProjectId,
    work: &str,
    id: &str,
    detail: bool,
) -> Result<Value, RepositoryError> {
    let (summary,chapter):(String,Option<String>)=db.query_row("SELECT summary,chapter_id FROM story_work_issues_v1 WHERE issue_id=?1 AND project_id=?2 AND work_task_id=?3",params![id,project.as_str(),work],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(storage_error)?.ok_or(RepositoryError::TaskBindingInvalid)?;
    let draft = current_draft(db, project, chapter.as_deref())?;
    let total: i64 = db
        .query_row(
            "SELECT count(*) FROM story_issue_observations_v1 WHERE issue_id=?1",
            [id],
            |r| r.get(0),
        )
        .map_err(storage_error)?;
    let mut q=db.prepare("SELECT observation_id FROM story_issue_observations_v1 WHERE issue_id=?1 ORDER BY ordinal DESC LIMIT 8").map_err(storage_error)?;
    let ids = q
        .query_map([id], |r| r.get::<_, String>(0))
        .map_err(storage_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(storage_error)?;
    let observations = if detail {
        ids.iter()
            .map(|id| observation(db, id))
            .collect::<Result<Vec<_>, _>>()?
    } else {
        Vec::new()
    };
    Ok(
        json!({"issueId":id,"summary":summary,"status":state(db,project,id)?,"chapterId":chapter,"currentDraftRevisionId":draft,"observations":observations,"observationCount":total,"hasMoreObservations":if detail{total>8}else{total>0}}),
    )
}
pub(crate) fn page(
    db: &Connection,
    project: &ProjectId,
    work: &str,
    issue: Option<&str>,
    cursor: u64,
    limit: usize,
    chapter: Option<&str>,
) -> Result<Value, RepositoryError> {
    owner(db, project, work)?;
    if !(1..=16).contains(&limit) || cursor > i64::MAX as u64 {
        return Err(invalid(
            "limit 1..16 and nonnegative keyset cursor required",
        ));
    }
    if let Some(issue) = issue {
        let p = projection(db, project, work, issue, false)?;
        if chapter.is_some_and(|c| p["chapterId"] != c) {
            return Err(RepositoryError::TaskOutputForbidden);
        }
        let total: i64 = db
            .query_row(
                "SELECT count(*) FROM story_issue_observations_v1 WHERE issue_id=?1",
                [issue],
                |r| r.get(0),
            )
            .map_err(storage_error)?;
        let mut q=db.prepare("SELECT ordinal,observation_id FROM story_issue_observations_v1 WHERE issue_id=?1 AND (?2=0 OR ordinal<?2) ORDER BY ordinal DESC LIMIT ?3").map_err(storage_error)?;
        let ids = q
            .query_map(params![issue, cursor as i64, (limit + 1) as i64], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
            })
            .map_err(storage_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage_error)?;
        let mut entries = Vec::new();
        let mut returned = 0;
        for (_, id) in ids.iter().take(limit) {
            let view = observation(db, id)?;
            if !entries.is_empty()
                && serde_json::to_string(&entries)
                    .map_err(|_| RepositoryError::CorruptState)?
                    .len()
                    + view.to_string().len()
                    > 30_000
            {
                break;
            }
            entries.push(view);
            returned += 1;
        }
        return Ok(
            json!({"issueId":issue,"entries":entries,"totalCount":total,"nextCursor":if ids.len()>returned{ids.get(returned-1).map(|(n,_)|*n)}else{None}}),
        );
    }
    let total:i64=db.query_row("SELECT count(*) FROM story_work_issues_v1 WHERE project_id=?1 AND work_task_id=?2 AND (?3 IS NULL OR chapter_id=?3)",params![project.as_str(),work,chapter],|r|r.get(0)).map_err(storage_error)?;
    let mut q=db.prepare("SELECT ordinal,issue_id FROM story_work_issues_v1 WHERE project_id=?1 AND work_task_id=?2 AND (?3 IS NULL OR chapter_id=?3) AND (?4=0 OR ordinal<?4) ORDER BY ordinal DESC LIMIT ?5").map_err(storage_error)?;
    let ids = q
        .query_map(
            params![
                project.as_str(),
                work,
                chapter,
                cursor as i64,
                (limit + 1) as i64
            ],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)),
        )
        .map_err(storage_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(storage_error)?;
    let mut entries = Vec::new();
    for (_, id) in ids.iter().take(limit) {
        let metadata = projection(db, project, work, id, false)?;
        if !entries.is_empty()
            && serde_json::to_string(&entries)
                .map_err(|_| RepositoryError::CorruptState)?
                .len()
                + metadata.to_string().len()
                > 30_000
        {
            break;
        }
        entries.push(metadata);
    }
    let returned = entries.len();
    Ok(
        json!({"entries":entries,"totalCount":total,"nextCursor":if ids.len()>returned{ids.get(returned-1).map(|(n,_)|*n)}else{None}}),
    )
}
pub(crate) fn summary(
    db: &Connection,
    project: &ProjectId,
    work: &str,
) -> Result<Value, RepositoryError> {
    owner(db, project, work)?;
    let total: i64 = db
        .query_row(
            "SELECT count(*) FROM story_work_issues_v1 WHERE project_id=?1 AND work_task_id=?2",
            params![project.as_str(), work],
            |r| r.get(0),
        )
        .map_err(storage_error)?;
    let mut q=db.prepare("SELECT issue_id FROM story_work_issues_v1 WHERE project_id=?1 AND work_task_id=?2 ORDER BY ordinal DESC").map_err(storage_error)?;
    let mut rows = q
        .query(params![project.as_str(), work])
        .map_err(storage_error)?;
    let (mut open, mut stale, mut disputed) = (0, 0, 0);
    let mut entries = Vec::new();
    while let Some(r) = rows.next().map_err(storage_error)? {
        let id: String = r.get(0).map_err(storage_error)?;
        match state(db, project, &id)? {
            "open" => open += 1,
            "stale" => stale += 1,
            "disputed" => disputed += 1,
            _ => {}
        }
        if entries.len() < 2 {
            let mut entry = projection(db, project, work, &id, false)?;
            entry["summary"] = json!(
                entry["summary"]
                    .as_str()
                    .unwrap_or_default()
                    .chars()
                    .take(120)
                    .collect::<String>()
            );
            entries.push(entry);
        }
    }
    Ok(
        json!({"totalCount":total,"openCount":open,"staleCount":stale,"disputedCount":disputed,"entries":entries,"hasMore":total>2}),
    )
}
pub(crate) fn has_unresolved(
    db: &Connection,
    project: &ProjectId,
    work: &str,
    chapter: Option<&str>,
) -> Result<bool, RepositoryError> {
    let mut q=db.prepare("SELECT issue_id FROM story_work_issues_v1 WHERE project_id=?1 AND work_task_id=?2 AND (?3 IS NULL OR chapter_id=?3)").map_err(storage_error)?;
    let mut rows = q
        .query(params![project.as_str(), work, chapter])
        .map_err(storage_error)?;
    while let Some(r) = rows.next().map_err(storage_error)? {
        let id: String = r.get(0).map_err(storage_error)?;
        if state(db, project, &id)? != "resolved" {
            return Ok(true);
        }
    }
    Ok(false)
}
pub(crate) fn require_resolved(
    db: &Connection,
    project: &ProjectId,
    work: &str,
    chapter: Option<&str>,
) -> Result<(), RepositoryError> {
    if has_unresolved(db, project, work, chapter)? {
        return Err(invalid(
            "actual unresolved, stale or disputed problem requires current recheck; omitting it cannot fulfill delivery",
        ));
    }
    Ok(())
}
pub(crate) fn report_issues(
    db: &Connection,
    project: &ProjectId,
    revision: &str,
) -> Result<Vec<Value>, RepositoryError> {
    let mut q=db.prepare("SELECT DISTINCT i.work_task_id,i.issue_id FROM story_work_issues_v1 i JOIN story_issue_observations_v1 o ON o.issue_id=i.issue_id WHERE i.project_id=?1 AND o.report_revision_id=?2 ORDER BY i.ordinal").map_err(storage_error)?;
    let ids = q
        .query_map(params![project.as_str(), revision], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(storage_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(storage_error)?;
    ids.iter()
        .map(|(w, id)| projection(db, project, w, id, false))
        .collect()
}
impl SqliteStoryRepository {
    pub fn work_issues(
        &self,
        project: &ProjectId,
        work: &str,
        issue: Option<&str>,
        cursor: u64,
        limit: usize,
    ) -> Result<Value, RepositoryError> {
        self.with_read_snapshot(|| {
            page(&self.connection, project, work, issue, cursor, limit, None)
        })?
    }
    pub fn task_work_issues(
        &self,
        task: &TaskRecord,
        issue: Option<&str>,
        cursor: u64,
        limit: usize,
    ) -> Result<Value, RepositoryError> {
        self.with_read_snapshot(|| {
            crate::task_input::assert_task_inputs_current(&self.connection, task)?;
            let work = work_for(&self.connection, &task.project_id, &task.task_id)?;
            let target: Value = serde_json::from_str(&task.target_json)
                .map_err(|_| RepositoryError::CorruptState)?;
            if task.operation=="generateChapter" && (crate::task_scope::reader_scene_limit(&target)?.is_some()
                || target["povCharacterId"].is_string() || target["storyTime"].is_number()) {
                return Err(invalid("the explicit reading fence does not admit whole-report summaries or history; read authorized source evidence or use a reader with the appropriate existing grant"));
            }
            let chapter = if task.operation == "generateChapter" || task.operation == "assessRevisionOutcome"
            {
                target["chapterId"].as_str()
            } else {
                None
            };
            page(
                &self.connection,
                &task.project_id,
                &work,
                issue,
                cursor,
                limit,
                chapter,
            )
        })?
    }
}
