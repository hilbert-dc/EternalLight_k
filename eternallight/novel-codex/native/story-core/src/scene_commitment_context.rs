use std::collections::HashSet;

use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};

use crate::plan_commitments::CommitmentRef;
use crate::{ObjectId, ProjectId, RepositoryError, TaskRecord};

fn storage(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}

fn revision(
    db: &Connection,
    project: &ProjectId,
    id: &str,
    kind: &str,
) -> Result<Value, RepositoryError> {
    let content: Option<String> = db.query_row(
        "SELECT content_json FROM story_revisions_v2 WHERE project_id=?1 AND revision_id=?2 AND object_kind=?3",
        params![project.as_str(), id, kind], |row| row.get(0),
    ).optional().map_err(storage)?;
    serde_json::from_str(&content.ok_or(RepositoryError::TaskOutputForbidden)?)
        .map_err(|_| RepositoryError::CorruptState)
}

/// Resolves only one explicit frozen-plan reference. It does not authorize arbitrary
/// past-chapter queries or reinterpret a newer author revision as the original intent.
pub(crate) fn resolve(
    db: &Connection,
    project: &ProjectId,
    reference: &Value,
    chapter: i64,
) -> Result<Value, RepositoryError> {
    let parsed = CommitmentRef::parse(reference)?;
    let (payload, author_changed) = match &parsed {
        CommitmentRef::Commitment {
            commitment_id,
            revision_id,
        } => {
            let content = revision(db, project, revision_id, "commitment")?;
            if content["payload"]["id"] != *commitment_id {
                return Err(RepositoryError::TaskOutputForbidden);
            }
            let selected: bool = db.query_row(
                "SELECT EXISTS(SELECT 1 FROM story_branches_v2 b JOIN story_snapshot_members_v2 m ON m.snapshot_id=b.head_snapshot_id
                 WHERE b.project_id=?1 AND b.branch_id='main' AND m.object_id='commitment-' || ?2 AND m.revision_id=?3)",
                params![project.as_str(),commitment_id,revision_id], |row|row.get(0),
            ).map_err(storage)?;
            (
                content["payload"].clone(),
                (!selected).then_some("revisionSuperseded"),
            )
        }
        CommitmentRef::Proposal {
            report_revision_id,
            proposal_id,
            source_revision_id,
        } => {
            let report = revision(db, project, report_revision_id, "commitmentExtraction")?;
            if report["payload"]["sourceRevisionId"] != *source_revision_id {
                return Err(RepositoryError::TaskOutputForbidden);
            }
            let payload = report["payload"]["candidates"]
                .as_array()
                .and_then(|items| items.iter().find(|item| item["id"] == *proposal_id))
                .ok_or(RepositoryError::TaskOutputForbidden)?
                .clone();
            let established: bool = db.query_row(
                "SELECT EXISTS(SELECT 1 FROM story_branches_v2 b JOIN story_snapshot_members_v2 m ON m.snapshot_id=b.head_snapshot_id
                 JOIN story_revisions_v2 r ON r.project_id=b.project_id AND r.revision_id=m.revision_id AND r.object_kind='commitment'
                 WHERE b.project_id=?1 AND b.branch_id='main'
                   AND json_extract(r.content_json,'$.extractionOrigin.reportRevisionId')=?2
                   AND json_extract(r.content_json,'$.extractionOrigin.proposalId')=?3)",
                params![project.as_str(),report_revision_id,proposal_id], |row|row.get(0),
            ).map_err(storage)?;
            (payload, established.then_some("authorEstablished"))
        }
    };
    let id = payload["id"]
        .as_str()
        .ok_or(RepositoryError::CorruptState)?;
    let object =
        ObjectId::parse(format!("commitment-{id}")).map_err(|_| RepositoryError::CorruptState)?;
    let value = crate::narrative_commitments::validate_content(
        db,
        project,
        &object,
        &json!({"payload":payload}).to_string(),
    )?;
    let mut source_current = true;
    for anchor in value
        .setup_evidence_refs
        .iter()
        .chain(&value.fulfillment_evidence_refs)
    {
        let source = revision(db, project, &anchor.revision_id, "draft")?;
        let number = source["chapterNumber"]
            .as_i64()
            .ok_or(RepositoryError::CorruptState)?;
        if number < 1 || number >= chapter {
            return Err(RepositoryError::TaskOutputForbidden);
        }
        let selected: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM story_branches_v2 b JOIN story_snapshot_members_v2 m ON m.snapshot_id=b.head_snapshot_id
             WHERE b.project_id=?1 AND b.branch_id='main' AND m.object_id=?2 AND m.revision_id=?3)",
            params![project.as_str(),anchor.object_id,anchor.revision_id], |row|row.get(0),
        ).map_err(storage)?;
        source_current &= selected;
    }
    let state = if !source_current {
        "sourceChanged"
    } else {
        author_changed.unwrap_or("current")
    };
    let projection = json!({
        "reference":reference, "referenceState":state, "authority":"candidateAnalysis",
        "kind":value.kind,"summary":value.summary,"fulfillmentCriteria":value.fulfillment_criteria,
        "setupEvidenceRefs":value.setup_evidence_refs,"recordedStatus":value.status,
        "fulfillmentEvidenceRefs":value.fulfillment_evidence_refs
    });
    if projection.to_string().len() > 9_000 {
        return Err(RepositoryError::InvalidContent(
            "bound commitment input exceeds 9KB".into(),
        ));
    }
    Ok(projection)
}

pub(crate) fn context(db: &Connection, task: &TaskRecord) -> Result<Value, RepositoryError> {
    let target: Value =
        serde_json::from_str(&task.target_json).map_err(|_| RepositoryError::CorruptState)?;
    let binding: Option<(String, i64)> = db
        .query_row(
            "SELECT b.chapter_plan_id,c.number FROM story_chapter_plan_bindings_v2 b
         JOIN story_chapters_v2 c ON c.project_id=b.project_id AND c.chapter_id=b.chapter_id
         WHERE b.project_id=?1 AND b.chapter_id=?2 AND b.plan_revision_id=?3",
            params![
                task.project_id.as_str(),
                target["chapterId"].as_str(),
                target["planRevisionId"].as_str()
            ],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(storage)?;
    let (chapter_plan_id, chapter) = binding.ok_or(RepositoryError::TaskBindingInvalid)?;
    let plan_id = target["planRevisionId"]
        .as_str()
        .ok_or(RepositoryError::TaskBindingInvalid)?;
    let plan = revision(db, &task.project_id, plan_id, "plan")?;
    let planned = plan["payload"]["chapters"]
        .as_array()
        .and_then(|items| {
            items.iter().find(|item| {
                item["chapterPlanId"] == chapter_plan_id && item["chapterNumber"] == chapter
            })
        })
        .ok_or(RepositoryError::TaskBindingInvalid)?;
    let mut seen = HashSet::new();
    let mut entries = Vec::new();
    for scene in planned["scenes"]
        .as_array()
        .ok_or(RepositoryError::CorruptState)?
    {
        for effect in scene["expectedEffects"]
            .as_array()
            .ok_or(RepositoryError::CorruptState)?
        {
            let references = effect["commitmentRefs"]
                .as_array()
                .ok_or(RepositoryError::CorruptState)?;
            if references.len() > 4 {
                return Err(RepositoryError::TaskOutputForbidden);
            }
            for reference in references {
                if seen.insert(CommitmentRef::parse(reference)?) {
                    if entries.len() == 64 {
                        return Err(RepositoryError::TaskOutputForbidden);
                    }
                    entries.push(resolve(db, &task.project_id, reference, chapter)?);
                }
            }
        }
    }
    let result =
        json!({"entries":entries,"scope":"frozenPlanReferences","authority":"candidateAnalysis"});
    if result.to_string().len() > 24_000 {
        return Err(RepositoryError::InvalidContent(
            "bound commitment inputs exceed 24KB".into(),
        ));
    }
    Ok(result)
}
