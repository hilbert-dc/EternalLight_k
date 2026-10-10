use std::collections::HashSet;

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    CommitmentStatus, CommitmentWindow, NarrativeCommitment, ObjectId, ProjectId, RepositoryError,
    RevisionId, SqliteStoryRepository, TaskRecord,
};

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ExaminedRange {
    start: usize,
    end: usize,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Report {
    chapter_id: String,
    source_revision_id: String,
    examined_ranges: Vec<ExaminedRange>,
    candidates: Vec<NarrativeCommitment>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Content {
    payload: Report,
}

fn storage(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}

pub(crate) fn validate_target(
    db: &Connection,
    project: &ProjectId,
    target: &Value,
) -> Result<i64, RepositoryError> {
    let chapter = target["chapterId"]
        .as_str()
        .ok_or(RepositoryError::TaskBindingInvalid)?;
    ObjectId::parse(format!("commitment-extraction-{chapter}"))
        .map_err(|_| RepositoryError::TaskBindingInvalid)?;
    let number = target["chapterNumber"]
        .as_i64()
        .filter(|number| *number > 0)
        .ok_or(RepositoryError::TaskBindingInvalid)?;
    let source = target["sourceRevisionId"]
        .as_str()
        .ok_or(RepositoryError::TaskBindingInvalid)?;
    let valid: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM story_branches_v2 b JOIN story_snapshot_members_v2 m ON m.snapshot_id=b.head_snapshot_id
         JOIN story_revisions_v2 r ON r.revision_id=m.revision_id AND r.project_id=b.project_id
         JOIN story_chapters_v2 c ON c.project_id=b.project_id AND c.chapter_id=?2
         WHERE b.project_id=?1 AND b.branch_id='main' AND m.object_id='draft-' || ?2
           AND r.object_kind='draft' AND r.revision_id=?3 AND c.number=?4 AND json_extract(r.content_json,'$.chapterNumber')=?4)",
        params![project.as_str(),chapter,source,number], |row|row.get(0),
    ).map_err(storage)?;
    if !valid {
        return Err(RepositoryError::TaskInputStale);
    }
    Ok(number)
}

fn parse_report(
    db: &Connection,
    project: &ProjectId,
    content: &str,
) -> Result<Report, RepositoryError> {
    let invalid = || {
        RepositoryError::InvalidContent("invalid commitment extraction evidence or ranges".into())
    };
    if content.len() > 40_000 {
        return Err(invalid());
    }
    let raw: Value = serde_json::from_str(content).map_err(|_| invalid())?;
    for (index, candidate) in raw["payload"]["candidates"]
        .as_array()
        .into_iter()
        .flatten()
        .take(16)
        .enumerate()
    {
        for field in ["plotThreadId", "characterArcId"] {
            if candidate
                .get(field)
                .and_then(Value::as_str)
                .is_some_and(|id| id.trim().is_empty())
            {
                return Err(RepositoryError::InvalidContent(format!(
                    "candidates[{index}].{field}: use null or a non-empty ID"
                )));
            }
        }
        for (anchor_index, anchor) in candidate["setupEvidenceRefs"]
            .as_array()
            .into_iter()
            .flatten()
            .take(8)
            .enumerate()
        {
            if anchor.as_object().is_some_and(|fields| {
                fields.keys().any(|field| {
                    !["objectId", "revisionId", "quote", "start", "end"].contains(&field.as_str())
                })
            }) {
                return Err(RepositoryError::InvalidContent(format!(
                    "candidates[{index}].setupEvidenceRefs[{anchor_index}]: unexpected field; only objectId, revisionId, quote, start, end are allowed"
                )));
            }
        }
    }
    let report: Content = serde_json::from_value(raw).map_err(|_| invalid())?;
    let report = report.payload;
    if report.candidates.len() > 16 || report.examined_ranges.len() > 16 {
        return Err(invalid());
    }
    let source: Option<String> = db.query_row(
        "SELECT content_json FROM story_revisions_v2 WHERE project_id=?1 AND object_id='draft-' || ?2 AND revision_id=?3 AND object_kind='draft'",
        params![project.as_str(),report.chapter_id,report.source_revision_id], |row|row.get(0),
    ).optional().map_err(storage)?;
    let source: Value = serde_json::from_str(&source.ok_or_else(invalid)?)
        .map_err(|_| RepositoryError::CorruptState)?;
    let size = source["body"]
        .as_str()
        .ok_or(RepositoryError::CorruptState)?
        .chars()
        .count();
    let mut end = 0;
    for range in &report.examined_ranges {
        if range.start < end || range.start >= range.end || range.end > size {
            return Err(invalid());
        }
        end = range.end;
    }
    let mut ids = HashSet::new();
    for candidate in &report.candidates {
        if !ids.insert(&candidate.id)
            || !matches!(candidate.status, CommitmentStatus::Open)
            || !matches!(candidate.target_window, CommitmentWindow::Open)
            || candidate.decision_reason.is_some()
            || !candidate.fulfillment_evidence_refs.is_empty()
            || candidate.setup_evidence_refs.iter().any(|anchor| {
                anchor.object_id != format!("draft-{}", report.chapter_id)
                    || anchor.revision_id != report.source_revision_id
                    || !report
                        .examined_ranges
                        .iter()
                        .any(|range| range.start <= anchor.start && range.end >= anchor.end)
            })
        {
            return Err(invalid());
        }
        crate::narrative_commitments::validate_content(
            db,
            project,
            &ObjectId::parse(format!("commitment-{}", candidate.id)).map_err(|_| invalid())?,
            &json!({"payload":candidate}).to_string(),
        )?;
    }
    Ok(report)
}

pub(crate) fn validate_write(
    db: &Connection,
    project: &ProjectId,
    object: &ObjectId,
    content: &str,
) -> Result<(), RepositoryError> {
    let report = parse_report(db, project, content)?;
    if object.as_str() != format!("commitment-extraction-{}", report.chapter_id) {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let source: Value = serde_json::from_str(
        &db.query_row(
            "SELECT content_json FROM story_revisions_v2 WHERE revision_id=?1",
            [&report.source_revision_id],
            |row| row.get::<_, String>(0),
        )
        .map_err(storage)?,
    )
    .map_err(|_| RepositoryError::CorruptState)?;
    validate_target(
        db,
        project,
        &json!({"chapterId":report.chapter_id,"chapterNumber":source["chapterNumber"],"sourceRevisionId":report.source_revision_id}),
    )?;
    Ok(())
}

pub(crate) fn validate_task_write(
    db: &Connection,
    task: &TaskRecord,
    content: &str,
) -> Result<(), RepositoryError> {
    let target: Value =
        serde_json::from_str(&task.target_json).map_err(|_| RepositoryError::CorruptState)?;
    let output: Value = serde_json::from_str(content).map_err(|_| RepositoryError::CorruptState)?;
    if output["payload"]["chapterId"] != target["chapterId"]
        || output["payload"]["sourceRevisionId"] != target["sourceRevisionId"]
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    if target.get("runId").is_some() {
        let report = parse_report(db, &task.project_id, content)?;
        let body: String = db.query_row(
            "SELECT json_extract(content_json,'$.body') FROM story_revisions_v2 WHERE project_id=?1 AND revision_id=?2 AND object_kind='draft'",
            params![task.project_id.as_str(),report.source_revision_id], |row|row.get(0),
        ).map_err(storage)?;
        let size = body.chars().count();
        let mut end = 0;
        for range in report.examined_ranges {
            if range.start != end {
                return Err(RepositoryError::TaskOutputForbidden);
            }
            end = range.end;
        }
        if end != size {
            return Err(RepositoryError::TaskOutputForbidden);
        }
    }
    Ok(())
}

impl SqliteStoryRepository {
    /// Reads candidate proposals without turning extraction claims into established
    /// commitments, Canon facts, or a semantic completeness certificate.
    pub fn commitment_extraction(
        &self,
        project: &ProjectId,
        chapter: &str,
        report_revision: Option<&RevisionId>,
    ) -> Result<Option<Value>, RepositoryError> {
        self.with_read_snapshot(|| {
            let head: Option<(String,Option<String>)> = self.connection.query_row(
                "SELECT b.head_snapshot_id,m.revision_id FROM story_branches_v2 b LEFT JOIN story_snapshot_members_v2 m
                 ON m.snapshot_id=b.head_snapshot_id AND m.object_id='commitment-extraction-' || ?2
                 WHERE b.project_id=?1 AND b.branch_id='main'",
                params![project.as_str(),chapter], |row|Ok((row.get(0)?,row.get(1)?)),
            ).optional().map_err(storage)?;
            let Some((snapshot,current_revision)) = head else {return Ok(None)};
            let revision = report_revision.map(RevisionId::as_str).or(current_revision.as_deref());
            let Some(revision) = revision else {return Ok(None)};
            // A requested revision is immutable display evidence. Missing or
            // foreign revisions never fall through to the current selection.
            let content: Option<String> = self.connection.query_row(
                "SELECT content_json FROM story_revisions_v2 WHERE project_id=?1 AND revision_id=?2
                 AND object_id='commitment-extraction-' || ?3 AND object_kind='commitmentExtraction'",
                params![project.as_str(),revision,chapter], |row|row.get(0),
            ).optional().map_err(storage)?;
            let Some(content) = content else {return Ok(None)};
            let report_current = current_revision.as_deref() == Some(revision);
            let report = parse_report(&self.connection,project,&content)?;
            let current: bool = self.connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM story_branches_v2 b JOIN story_snapshot_members_v2 m ON m.snapshot_id=b.head_snapshot_id WHERE b.project_id=?1 AND b.branch_id='main' AND m.object_id='draft-' || ?2 AND m.revision_id=?3)",
                params![project.as_str(),report.chapter_id,report.source_revision_id], |row|row.get(0),
            ).map_err(storage)?;
            let mut established = Vec::new();
            for candidate in &report.candidates {
                let id = crate::commitment_establishment::identity(project, &crate::CommitmentExtractionOrigin {
                    chapter_id:chapter.into(), report_revision_id:revision.into(), proposal_id:candidate.id.clone(),
                });
                let present: bool = self.connection.query_row(
                    "SELECT EXISTS(SELECT 1 FROM story_snapshot_members_v2 m JOIN story_revisions_v2 r ON r.revision_id=m.revision_id AND r.object_id=m.object_id WHERE m.snapshot_id=?1 AND r.project_id=?2 AND r.object_id='commitment-' || ?3 AND r.object_kind='commitment')",
                    params![snapshot,project.as_str(),id], |row|row.get(0),
                ).map_err(storage)?;
                if present { established.push(json!({"proposalId":candidate.id,"commitmentId":id})); }
            }
            Ok(Some(json!({"revisionId":revision,"snapshotId":snapshot,"projectRevision":self.project_revision(project)?,"established":established,"authority":"candidateProposalBatch","reportCurrent":report_current,"sourceCurrent":current,
                "payload":report,"coverage":{"semanticCompleteness":"unknown","modelReportedRanges":true,
                    "literaryFulfillmentVerified":false,"globalCompletenessClaimed":false}})))
        })?
    }
}
