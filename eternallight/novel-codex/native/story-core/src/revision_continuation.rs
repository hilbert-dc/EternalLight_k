//! A frozen provenance edge, not inherited execution authority or a second scheduler.
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::authoring_runs::storage_error;
use crate::{ProjectId, RepositoryError, RevisionOutcomeChoice};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RevisionPredecessor {
    pub writer_task_id: String,
    pub assessment_revision_id: String,
    pub decision_id: String,
}

fn source(state: &Value) -> Result<Option<RevisionPredecessor>, RepositoryError> {
    serde_json::from_value(state["revisionPredecessor"].clone())
        .map_err(|_| RepositoryError::TaskBindingInvalid)
}

fn validate_decision(
    db: &Connection,
    project: &ProjectId,
    predecessor: &RevisionPredecessor,
) -> Result<crate::RevisionOutcomeDecision, RepositoryError> {
    let decision = crate::revision_outcome_decision::load(db, project, &predecessor.decision_id)?;
    if decision.writer_task_id != predecessor.writer_task_id
        || decision.assessment_revision_id != predecessor.assessment_revision_id
        || decision.choice != RevisionOutcomeChoice::AcceptRevision
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let (report, _) = crate::revision_outcome_decision::report(
        db,
        project,
        &predecessor.writer_task_id,
        &predecessor.assessment_revision_id,
    )?;
    if report["draftRevisionId"] != decision.draft_revision_id
        || report["knowledgeBatchId"] != decision.knowledge_batch_id
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    Ok(decision)
}

pub(crate) fn current(
    db: &Connection,
    project: &ProjectId,
    state: &Value,
) -> Result<(), RepositoryError> {
    let Some(predecessor) = source(state)? else {
        return Ok(());
    };
    let decision = validate_decision(db, project, &predecessor)?;
    let latest =
        crate::revision_outcome_decision::history(db, project, &predecessor.writer_task_id)?;
    let (_, current) = crate::revision_outcome_decision::report(
        db,
        project,
        &predecessor.writer_task_id,
        &predecessor.assessment_revision_id,
    )?;
    if !current || latest.first() != Some(&decision) {
        return Err(RepositoryError::TaskInputStale);
    }
    Ok(())
}

pub(crate) fn record(
    db: &Connection,
    project: &ProjectId,
    run: &str,
    state: &Value,
) -> Result<(), RepositoryError> {
    current(db, project, state)?;
    if let Some(predecessor) = source(state)? {
        db.execute("INSERT INTO story_generation_revision_sources_v1(run_id,project_id,decision_id,source_json) VALUES(?1,?2,?3,?4)",params![run,project.as_str(),predecessor.decision_id,serde_json::to_string(&predecessor).map_err(|_|RepositoryError::CorruptState)?]).map_err(storage_error)?;
    }
    Ok(())
}

pub(crate) fn validate_history(
    db: &Connection,
    project: &ProjectId,
    run: &str,
    state: &Value,
) -> Result<(), RepositoryError> {
    let stored: Option<(String,String)> = db.query_row("SELECT decision_id,source_json FROM story_generation_revision_sources_v1 WHERE project_id=?1 AND run_id=?2",params![project.as_str(),run],|row|Ok((row.get(0)?,row.get(1)?))).optional().map_err(storage_error)?;
    match (source(state)?, stored) {
        (None, None) => Ok(()),
        (Some(predecessor), Some((id, encoded))) => {
            let recorded: RevisionPredecessor =
                serde_json::from_str(&encoded).map_err(|_| RepositoryError::CorruptState)?;
            if predecessor != recorded || predecessor.decision_id != id {
                return Err(RepositoryError::TaskBindingInvalid);
            }
            validate_decision(db, project, &predecessor)?;
            Ok(())
        }
        (None, Some(_)) | (Some(_), None) => Err(RepositoryError::TaskBindingInvalid),
    }
}
