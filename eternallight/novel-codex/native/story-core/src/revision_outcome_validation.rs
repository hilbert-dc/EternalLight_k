use std::collections::HashSet;

use rusqlite::{Connection, OptionalExtension, params};
use serde::Deserialize;
use serde_json::Value;

use crate::authoring_runs::storage_error;
use crate::{ProjectId, RepositoryError, TaskRecord};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Report {
    writer_task_id: String,
    chapter_id: String,
    original_revision_id: String,
    draft_revision_id: String,
    knowledge_batch_id: String,
    outcomes: Vec<Judgment>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Judgment {
    id: String,
    status: JudgmentStatus,
    reason: String,
    evidence: Vec<Evidence>,
}

#[derive(PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
enum JudgmentStatus {
    Supported,
    Unmet,
    InsufficientEvidence,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Evidence {
    group_id: String,
    quote: String,
    start: usize,
    end: usize,
}

pub(crate) fn draft(
    db: &Connection,
    project: &ProjectId,
    chapter: &str,
    revision: &str,
) -> Result<Value, RepositoryError> {
    let encoded: String = db.query_row(
        "SELECT content_json FROM story_revisions_v2 WHERE project_id=?1 AND object_id='draft-'||?2 AND revision_id=?3 AND object_kind='draft'",
        params![project.as_str(),chapter,revision], |row|row.get(0),
    ).optional().map_err(storage_error)?.ok_or(RepositoryError::TaskBindingInvalid)?;
    serde_json::from_str(&encoded).map_err(|_| RepositoryError::CorruptState)
}

pub(crate) fn validate_report(
    db: &Connection,
    project: &ProjectId,
    target: &Value,
    encoded: &str,
) -> Result<(), RepositoryError> {
    let invalid = || RepositoryError::TaskOutputForbidden;
    if encoded.len() > 24_000 {
        return Err(invalid());
    }
    let report: Report = serde_json::from_str(encoded).map_err(|_| invalid())?;
    for (field, value) in [
        ("writerTaskId", &report.writer_task_id),
        ("chapterId", &report.chapter_id),
        ("originalRevisionId", &report.original_revision_id),
        ("draftRevisionId", &report.draft_revision_id),
        ("knowledgeBatchId", &report.knowledge_batch_id),
    ] {
        if target[field].as_str() != Some(value.as_str()) {
            return Err(invalid());
        }
    }
    let (_, writer_target) = crate::revision_outcome::writer(db, project, &report.writer_task_id)?;
    let desired = crate::revision_outcome::desired_outcomes(&writer_target)?;
    if report.outcomes.len() != desired.len()
        || report
            .outcomes
            .iter()
            .map(|item| &item.id)
            .collect::<HashSet<_>>()
            .len()
            != desired.len()
    {
        return Err(invalid());
    }
    let draft = draft(db, project, &report.chapter_id, &report.draft_revision_id)?;
    let groups = draft["sceneGroups"]
        .as_array()
        .ok_or(RepositoryError::CorruptState)?;
    let original = self::draft(
        db,
        project,
        &report.chapter_id,
        &report.original_revision_id,
    )?;
    let before = original["sceneGroups"]
        .as_array()
        .ok_or(RepositoryError::CorruptState)?;
    for judgment in &report.outcomes {
        let criterion = desired
            .iter()
            .find(|item| item.id == judgment.id)
            .ok_or_else(invalid)?;
        if judgment.reason.trim().is_empty()
            || judgment.reason.chars().count() > 1000
            || judgment.evidence.len() > 4
            || (judgment.status != JudgmentStatus::InsufficientEvidence
                && judgment.evidence.is_empty())
        {
            return Err(invalid());
        }
        if judgment.status != JudgmentStatus::InsufficientEvidence
            && !judgment.evidence.iter().any(|evidence| {
                let old = before.iter().find(|group| group["id"] == evidence.group_id);
                let new = groups.iter().find(|group| group["id"] == evidence.group_id);
                matches!((old, new), (Some(old), Some(new)) if old["body"] != new["body"])
            })
        {
            return Err(invalid());
        }
        for evidence in &judgment.evidence {
            if !criterion.group_ids.contains(&evidence.group_id)
                || evidence.quote.is_empty()
                || evidence.quote.chars().count() > 1000
                || evidence.start >= evidence.end
            {
                return Err(invalid());
            }
            let body = groups
                .iter()
                .find(|group| group["id"] == evidence.group_id)
                .and_then(|group| group["body"].as_str())
                .ok_or_else(invalid)?;
            let count = body.chars().count();
            if evidence.end > count
                || body
                    .chars()
                    .skip(evidence.start)
                    .take(evidence.end - evidence.start)
                    .collect::<String>()
                    != evidence.quote
            {
                return Err(invalid());
            }
        }
    }
    Ok(())
}

pub(crate) fn validate_write(
    db: &Connection,
    task: &TaskRecord,
    encoded: &str,
) -> Result<(), RepositoryError> {
    let target: Value =
        serde_json::from_str(&task.target_json).map_err(|_| RepositoryError::CorruptState)?;
    crate::revision_outcome::validate_target(db, &task.project_id, &target)?;
    if !crate::revision_outcome::inputs_current(db, &task.project_id, &target)? {
        return Err(RepositoryError::TaskInputStale);
    }
    validate_report(db, &task.project_id, &target, encoded)
}
