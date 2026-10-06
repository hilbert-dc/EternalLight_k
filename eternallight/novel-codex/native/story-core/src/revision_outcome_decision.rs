//! Author choices about candidate analyses; these never grant execution or Canon authority.
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::authoring_runs::storage_error;
use crate::{ProjectId, RepositoryError, SqliteStoryRepository};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RevisionOutcomeChoice {
    AcceptRevision,
    RepairDraft,
    Defer,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RevisionOutcomeDecisionRequest {
    pub decision_id: String,
    pub expected_assessment_revision_id: String,
    #[serde(deserialize_with = "required_decision_head")]
    pub expected_decision_id: Option<String>,
    pub choice: RevisionOutcomeChoice,
    pub note: String,
}

// Explicit null means an observed empty head; omission must not invent that observation.
fn required_decision_head<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<String>, D::Error> {
    Option::<String>::deserialize(deserializer)
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RevisionOutcomeDecision {
    pub id: String,
    pub writer_task_id: String,
    pub assessment_revision_id: String,
    pub draft_revision_id: String,
    pub knowledge_batch_id: Option<String>,
    pub choice: RevisionOutcomeChoice,
    pub note: String,
    pub created_at: i64,
}

pub(crate) fn history(
    db: &Connection,
    project: &ProjectId,
    writer: &str,
) -> Result<Vec<RevisionOutcomeDecision>, RepositoryError> {
    let mut statement = db.prepare("SELECT record_json FROM story_revision_decisions_v1 WHERE project_id=?1 AND writer_task_id=?2 ORDER BY ordinal DESC LIMIT 20").map_err(storage_error)?;
    statement
        .query_map(params![project.as_str(), writer], |row| {
            row.get::<_, String>(0)
        })
        .map_err(storage_error)?
        .map(|row| {
            serde_json::from_str(&row.map_err(storage_error)?)
                .map_err(|_| RepositoryError::CorruptState)
        })
        .collect()
}

pub(crate) fn load(
    db: &Connection,
    project: &ProjectId,
    id: &str,
) -> Result<RevisionOutcomeDecision, RepositoryError> {
    let encoded: String = db.query_row("SELECT record_json FROM story_revision_decisions_v1 WHERE project_id=?1 AND decision_id=?2",params![project.as_str(),id],|row|row.get(0)).optional().map_err(storage_error)?.ok_or(RepositoryError::TaskBindingInvalid)?;
    serde_json::from_str(&encoded).map_err(|_| RepositoryError::CorruptState)
}

/// Validate an immutable, completed assessment and its actual owning task chain.
pub(crate) fn report(
    db: &Connection,
    project: &ProjectId,
    writer: &str,
    revision: &str,
) -> Result<(Value, bool), RepositoryError> {
    let (encoded, target): (String,String) = db.query_row(
        "SELECT r.content_json,t.target_json FROM story_revisions_v2 r JOIN story_task_outputs_v2 o ON o.revision_id=r.revision_id AND o.output_kind='revisionOutcome'
         JOIN story_tasks_v2 t ON t.task_id=o.task_id AND t.project_id=r.project_id AND t.operation='assessRevisionOutcome' AND t.status='completed'
         WHERE r.project_id=?1 AND r.revision_id=?2 AND r.object_id=?3 AND r.object_kind='revisionOutcome'",
        params![project.as_str(),revision,format!("revision-outcome-{writer}")],|row|Ok((row.get(0)?,row.get(1)?)),
    ).optional().map_err(storage_error)?.ok_or(RepositoryError::TaskBindingInvalid)?;
    let target: Value = serde_json::from_str(&target).map_err(|_| RepositoryError::CorruptState)?;
    crate::revision_outcome::validate_target(db, project, &target)?;
    crate::revision_outcome_validation::validate_report(db, project, &target, &encoded)?;
    let current_head: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM story_object_heads_v2 WHERE project_id=?1 AND object_id=?2 AND candidate_revision_id=?3)",params![project.as_str(),format!("revision-outcome-{writer}"),revision],|row|row.get(0)).map_err(storage_error)?;
    Ok((
        serde_json::from_str(&encoded).map_err(|_| RepositoryError::CorruptState)?,
        current_head && crate::revision_outcome::inputs_current(db, project, &target)?,
    ))
}

impl SqliteStoryRepository {
    pub fn decide_revision_outcome(
        &mut self,
        project: &ProjectId,
        writer: &str,
        request: RevisionOutcomeDecisionRequest,
    ) -> Result<RevisionOutcomeDecision, RepositoryError> {
        if !crate::author_message::valid_id(&request.decision_id)
            || request.decision_id.len() > 128
            || request.note.chars().count() > 1_000
            || request
                .expected_decision_id
                .as_ref()
                .is_some_and(|id| !crate::author_message::valid_id(id) || id.len() > 128)
            || crate::RevisionId::parse(request.expected_assessment_revision_id.clone()).is_err()
        {
            return Err(RepositoryError::InvalidContent(
                "revision decision id or note is invalid".into(),
            ));
        }
        let tx = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let encoded = serde_json::to_string(&request).map_err(|_| RepositoryError::CorruptState)?;
        let replay: Option<(String,String,String,String)> = tx.query_row("SELECT project_id,writer_task_id,request_json,record_json FROM story_revision_decisions_v1 WHERE decision_id=?1",[&request.decision_id],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).optional().map_err(storage_error)?;
        if let Some((stored_project, stored_writer, prior, record)) = replay {
            if stored_project != project.as_str() || stored_writer != writer || prior != encoded {
                return Err(RepositoryError::TaskBindingInvalid);
            }
            return serde_json::from_str(&record).map_err(|_| RepositoryError::CorruptState);
        }
        let latest = history(&tx, project, writer)?;
        if latest.first().map(|decision| &decision.id) != request.expected_decision_id.as_ref() {
            return Err(RepositoryError::TaskInputStale);
        }
        let (assessment, current) = report(
            &tx,
            project,
            writer,
            &request.expected_assessment_revision_id,
        )?;
        if !current {
            return Err(RepositoryError::TaskInputStale);
        }
        let text = |name: &str| {
            assessment[name]
                .as_str()
                .map(str::to_owned)
                .ok_or(RepositoryError::CorruptState)
        };
        let created_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| RepositoryError::CorruptState)?
            .as_secs()
            .try_into()
            .map_err(|_| RepositoryError::CorruptState)?;
        let decision = RevisionOutcomeDecision {
            id: request.decision_id.clone(),
            writer_task_id: writer.into(),
            assessment_revision_id: request.expected_assessment_revision_id.clone(),
            draft_revision_id: text("draftRevisionId")?,
            knowledge_batch_id: serde_json::from_value(assessment["knowledgeBatchId"].clone())
                .map_err(|_| RepositoryError::CorruptState)?,
            choice: request.choice,
            note: request.note,
            created_at,
        };
        tx.execute("INSERT INTO story_revision_decisions_v1(decision_id,project_id,writer_task_id,assessment_revision_id,request_json,record_json) VALUES(?1,?2,?3,?4,?5,?6)",params![decision.id,project.as_str(),writer,decision.assessment_revision_id,encoded,serde_json::to_string(&decision).map_err(|_|RepositoryError::CorruptState)?]).map_err(storage_error)?;
        tx.commit().map_err(storage_error)?;
        Ok(decision)
    }
}
