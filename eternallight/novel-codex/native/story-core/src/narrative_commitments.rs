use std::collections::HashSet;

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{BranchId, ObjectId, ProjectId, RepositoryError, SnapshotId, SqliteStoryRepository};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CommitmentKind {
    Foreshadowing,
    Suspense,
    CharacterTransformation,
    Goal,
    SetPiece,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CommitmentStatus {
    Open,
    PartiallyFulfilled,
    Fulfilled,
    Abandoned,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommitmentEvidence {
    pub object_id: String,
    pub revision_id: String,
    pub quote: String,
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum CommitmentWindow {
    Open,
    ChapterRange {
        #[serde(rename = "fromChapterId")]
        from_chapter_id: String,
        #[serde(rename = "throughChapterId")]
        through_chapter_id: String,
    },
}

/// A source-bound narrative interpretation, not a world fact or a literary
/// quality verdict. Formal confirmation still requires the author Review path.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NarrativeCommitment {
    pub id: String,
    pub kind: CommitmentKind,
    pub summary: String,
    pub related_entity_ids: Vec<String>,
    pub plot_thread_id: Option<String>,
    pub character_arc_id: Option<String>,
    pub setup_evidence_refs: Vec<CommitmentEvidence>,
    pub fulfillment_criteria: String,
    pub target_window: CommitmentWindow,
    pub status: CommitmentStatus,
    pub fulfillment_evidence_refs: Vec<CommitmentEvidence>,
    pub decision_reason: Option<String>,
}

pub(crate) enum CommitmentReadControls {
    Live,
    ReadOnly,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Content {
    payload: NarrativeCommitment,
    extraction_origin: Option<crate::CommitmentExtractionOrigin>,
}

fn invalid() -> RepositoryError {
    RepositoryError::InvalidContent("invalid narrative commitment or source evidence".into())
}

fn storage(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}

fn text(value: &str, limit: usize) -> bool {
    !value.trim().is_empty() && value.chars().count() <= limit
}

pub(crate) fn validate_content(
    db: &Connection,
    project: &ProjectId,
    object: &ObjectId,
    content: &str,
) -> Result<NarrativeCommitment, RepositoryError> {
    if content.len() > 32_000 {
        return Err(invalid());
    }
    let value: Content = serde_json::from_str(content).map_err(|_| invalid())?;
    if let Some(origin) = &value.extraction_origin {
        crate::commitment_establishment::validate_origin(db, project, &value.payload.id, origin)?;
    }
    let value = value.payload;
    let mut related = HashSet::new();
    if object.as_str() != format!("commitment-{}", value.id)
        || !text(&value.id, 120)
        || !text(&value.summary, 500)
        || !text(&value.fulfillment_criteria, 1000)
        || value.related_entity_ids.len() > 16
        || value
            .related_entity_ids
            .iter()
            .any(|id| !text(id, 160) || !related.insert(id))
        || [&value.plot_thread_id, &value.character_arc_id]
            .iter()
            .any(|id| id.as_ref().is_some_and(|id| !text(id, 160)))
        || value.setup_evidence_refs.is_empty()
        || value.setup_evidence_refs.len() > 8
        || value.fulfillment_evidence_refs.len() > 8
        || value
            .decision_reason
            .as_ref()
            .is_some_and(|reason| !text(reason, 1000))
    {
        return Err(invalid());
    }
    match value.status {
        CommitmentStatus::Open if !value.fulfillment_evidence_refs.is_empty() => {
            return Err(invalid());
        }
        CommitmentStatus::PartiallyFulfilled | CommitmentStatus::Fulfilled
            if value.fulfillment_evidence_refs.is_empty() =>
        {
            return Err(invalid());
        }
        CommitmentStatus::Abandoned if value.decision_reason.is_none() => return Err(invalid()),
        CommitmentStatus::Open
        | CommitmentStatus::PartiallyFulfilled
        | CommitmentStatus::Fulfilled
        | CommitmentStatus::Abandoned => {}
    }
    for list in [&value.setup_evidence_refs, &value.fulfillment_evidence_refs] {
        let mut anchors = HashSet::new();
        if list.iter().any(|anchor| {
            !anchors.insert((
                &anchor.object_id,
                &anchor.revision_id,
                anchor.start,
                anchor.end,
            ))
        }) {
            return Err(invalid());
        }
    }
    for anchor in value
        .setup_evidence_refs
        .iter()
        .chain(&value.fulfillment_evidence_refs)
    {
        if !text(&anchor.object_id, 160)
            || !text(&anchor.revision_id, 160)
            || !text(&anchor.quote, 2000)
            || anchor.start >= anchor.end
        {
            return Err(invalid());
        }
        let source: Option<String> = db.query_row(
            "SELECT content_json FROM story_revisions_v2 WHERE project_id=?1 AND object_id=?2 AND revision_id=?3 AND object_kind='draft'",
            params![project.as_str(),anchor.object_id,anchor.revision_id], |row|row.get(0),
        ).optional().map_err(storage)?;
        let source: Value = serde_json::from_str(&source.ok_or_else(invalid)?)
            .map_err(|_| RepositoryError::CorruptState)?;
        let chars: Vec<char> = source["body"]
            .as_str()
            .ok_or_else(invalid)?
            .chars()
            .collect();
        if anchor.end > chars.len()
            || chars[anchor.start..anchor.end].iter().collect::<String>() != anchor.quote
        {
            return Err(invalid());
        }
    }
    Ok(value)
}

fn sources_current(
    db: &Connection,
    snapshot: &str,
    value: &NarrativeCommitment,
) -> Result<bool, RepositoryError> {
    for anchor in value
        .setup_evidence_refs
        .iter()
        .chain(&value.fulfillment_evidence_refs)
    {
        let selected: Option<String> = db.query_row(
            "SELECT revision_id FROM story_snapshot_members_v2 WHERE snapshot_id=?1 AND object_id=?2",
            params![snapshot,anchor.object_id], |row|row.get(0),
        ).optional().map_err(storage)?;
        if selected.as_deref() != Some(anchor.revision_id.as_str()) {
            return Ok(false);
        }
    }
    Ok(true)
}

pub(crate) fn validate_write(
    db: &Connection,
    project: &ProjectId,
    object: &ObjectId,
    content: &str,
) -> Result<(), RepositoryError> {
    let value = validate_content(db, project, object, content)?;
    let snapshot: String = db.query_row("SELECT head_snapshot_id FROM story_branches_v2 WHERE project_id=?1 AND branch_id='main'",[project.as_str()], |row|row.get(0)).map_err(storage)?;
    if !sources_current(db, &snapshot, &value)? {
        return Err(RepositoryError::TaskInputStale);
    }
    if let CommitmentWindow::ChapterRange {
        from_chapter_id,
        through_chapter_id,
    } = &value.target_window
    {
        let current_number = |chapter: &str| -> Result<Option<i64>, RepositoryError> {
            if !text(chapter, 160) {
                return Err(invalid());
            }
            let number: Option<i64> = db.query_row("SELECT COALESCE((SELECT json_extract(r.content_json,'$.chapterNumber') FROM story_snapshot_members_v2 m JOIN story_revisions_v2 r ON r.revision_id=m.revision_id AND r.object_id=m.object_id WHERE m.snapshot_id=?1 AND r.project_id=?2 AND r.object_kind='draft' AND r.object_id=?3),(SELECT number FROM story_chapters_v2 WHERE project_id=?2 AND chapter_id=?4))", params![snapshot,project.as_str(),format!("draft-{chapter}"),chapter], |row|row.get(0)).map_err(storage)?;
            Ok(number.filter(|number| *number > 0))
        };
        let from = current_number(from_chapter_id)?;
        let through = current_number(through_chapter_id)?;
        if from
            .zip(through)
            .is_none_or(|(from, through)| from > through)
        {
            return Err(invalid());
        }
    }
    Ok(())
}

fn chapter_number(
    db: &Connection,
    project: &ProjectId,
    snapshot: &str,
    chapter: &str,
) -> Result<Option<i64>, RepositoryError> {
    if !text(chapter, 160) {
        return Err(invalid());
    }
    let source: Option<String> = db.query_row(
        "SELECT r.content_json FROM story_snapshot_members_v2 m JOIN story_revisions_v2 r ON r.revision_id=m.revision_id AND r.object_id=m.object_id WHERE m.snapshot_id=?1 AND r.project_id=?2 AND r.object_id=?3 AND r.object_kind='draft'",
        params![snapshot,project.as_str(),format!("draft-{chapter}")], |row|row.get(0),
    ).optional().map_err(storage)?;
    let number = source
        .map(|content| {
            let value: Value =
                serde_json::from_str(&content).map_err(|_| RepositoryError::CorruptState)?;
            Ok(value["chapterNumber"].as_i64().filter(|number| *number > 0))
        })
        .transpose()
        .map(Option::flatten)?;
    if number.is_some() {
        return Ok(number);
    }
    db.query_row("SELECT c.number FROM story_snapshot_chapters_v1 c JOIN story_snapshots_v2 s ON s.snapshot_id=c.snapshot_id WHERE c.snapshot_id=?1 AND s.project_id=?2 AND c.chapter_id=?3", params![snapshot,project.as_str(),chapter], |row|row.get(0)).optional().map_err(storage)
}

impl SqliteStoryRepository {
    pub fn narrative_commitment(
        &self,
        project: &ProjectId,
        id: &str,
        snapshot: Option<&SnapshotId>,
    ) -> Result<Value, RepositoryError> {
        let controls = if snapshot.is_none() {
            CommitmentReadControls::Live
        } else {
            CommitmentReadControls::ReadOnly
        };
        self.with_read_snapshot(|| {
            let snapshot = if let Some(snapshot) = snapshot {
                snapshot.as_str().to_owned()
            } else {
                self.branch_head(project, &BranchId::parse("main").expect("static branch"))?
                    .ok_or(RepositoryError::ProjectNotFound)?
                    .snapshot_id
                    .as_str()
                    .to_owned()
            };
            self.narrative_commitment_in_read_snapshot(project, id, &snapshot, controls)
        })?
    }

    pub(crate) fn narrative_commitment_in_read_snapshot(
        &self,
        project: &ProjectId,
        id: &str,
        snapshot: &str,
        controls: CommitmentReadControls,
    ) -> Result<Value, RepositoryError> {
        let owner: Option<String> = self
            .connection
            .query_row(
                "SELECT project_id FROM story_snapshots_v2 WHERE snapshot_id=?1",
                [snapshot],
                |row| row.get(0),
            )
            .optional()
            .map_err(storage)?;
        if owner.as_deref() != Some(project.as_str()) {
            return Err(RepositoryError::CandidateNotFound);
        }
        let object = ObjectId::parse(format!("commitment-{id}")).map_err(|_| invalid())?;
        let stored: Option<(String,String)> = self.connection.query_row(
                "SELECT r.revision_id,r.content_json FROM story_snapshot_members_v2 m JOIN story_revisions_v2 r ON r.revision_id=m.revision_id AND r.object_id=m.object_id WHERE m.snapshot_id=?1 AND r.project_id=?2 AND r.object_id=?3 AND r.object_kind='commitment'",
                params![snapshot,project.as_str(),object.as_str()], |row|Ok((row.get(0)?,row.get(1)?)),
            ).optional().map_err(storage)?;
        let (revision, content) = stored.ok_or(RepositoryError::CandidateNotFound)?;
        let value = validate_content(&self.connection, project, &object, &content)
            .map_err(|_| RepositoryError::CorruptState)?;
        let current = sources_current(&self.connection, snapshot, &value)?;
        let accepted: Option<(String,Option<String>,Option<String>,String)> = self.connection.query_row(
                "WITH RECURSIVE ancestors(snapshot_id) AS (SELECT ?2 UNION ALL SELECT s.parent_snapshot_id FROM story_snapshots_v2 s JOIN ancestors a ON a.snapshot_id=s.snapshot_id WHERE s.parent_snapshot_id IS NOT NULL) SELECT a.operation,a.accepted_revision_id,t.previous_accepted_revision_id,a.commit_id FROM story_acceptances_v2 a LEFT JOIN story_acceptances_v2 t ON t.commit_id=a.target_commit_id WHERE a.project_id=?1 AND a.object_id=?3 AND a.result_snapshot_id IN (SELECT snapshot_id FROM ancestors) ORDER BY a.result_revision DESC LIMIT 1",
                params![project.as_str(),snapshot,object.as_str()], |row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?)),
            ).optional().map_err(storage)?;
        let confirmation_commit = accepted
            .as_ref()
            .filter(|(operation, selected, _, _)| {
                operation == "accept" && selected.as_deref() == Some(revision.as_str())
            })
            .map(|(_, _, _, commit)| commit.clone());
        let project_revision = self.project_revision(project)?;
        let undo_available = if let Some(commit) = confirmation_commit.as_ref() {
            self.connection.query_row(
                    "SELECT EXISTS(SELECT 1 FROM story_acceptances_v2 a JOIN story_branches_v2 b ON b.project_id=a.project_id AND b.branch_id=a.branch_id AND b.head_snapshot_id=a.result_snapshot_id WHERE a.project_id=?1 AND b.head_snapshot_id=?2 AND a.commit_id=?3 AND a.result_revision=?4 AND a.undone_by_commit_id IS NULL AND NOT EXISTS(SELECT 1 FROM story_proposal_application_members_v1 g WHERE g.acceptance_commit_id=a.commit_id))",
                    params![project.as_str(),snapshot,commit,project_revision], |row|row.get::<_,bool>(0),
                ).map_err(storage)?
        } else {
            false
        };
        let accepted = match accepted {
            Some((operation, accepted, _, _)) if operation == "accept" => accepted,
            Some((operation, _, previous, _)) if operation == "undo" => previous,
            None => None,
            _ => return Err(RepositoryError::CorruptState),
        };
        let window = match &value.target_window {
            CommitmentWindow::Open => json!({"kind":"open"}),
            CommitmentWindow::ChapterRange {
                from_chapter_id,
                through_chapter_id,
            } => {
                let from = chapter_number(&self.connection, project, snapshot, from_chapter_id)?;
                let through =
                    chapter_number(&self.connection, project, snapshot, through_chapter_id)?;
                json!({"kind":"chapterRange","fromChapterNumber":from,"throughChapterNumber":through,
                        "orderKnown":from.zip(through).is_some_and(|(from,through)|from<=through)})
            }
        };
        let author_controls = matches!(controls, CommitmentReadControls::Live)
            .then(|| json!({"projectRevision":project_revision,"undoAvailable":undo_available}));
        Ok(
            json!({"projectId":project.as_str(),"snapshotId":snapshot,"authorControls":author_controls,"confirmationCommitId":confirmation_commit,"objectId":object.as_str(),"revisionId":revision,
                "payload":value,"extractionOrigin":serde_json::from_str::<Value>(&content).map_err(|_|RepositoryError::CorruptState)?["extractionOrigin"],"sourceCurrent":current,"windowPosition":window,
                "confirmationState":if accepted.as_deref()==Some(revision.as_str()) {"confirmed"} else {"candidate"},
                "interpretation":"sourceBoundNarrativeClaim","literaryFulfillmentVerified":false}),
        )
    }
}
