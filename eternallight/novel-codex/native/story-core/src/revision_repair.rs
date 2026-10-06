//! Source-bound repair discussions use ordinary Work/Step/Attempt execution.
//! A candidate analysis is context, not inherited author intent or write permission.
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::authoring_runs::storage_error;
use crate::{
    CreateTaskRequest, DesiredOutcome, ProjectId, RepositoryError, RevisionOutcomeChoice,
    RevisionOutcomeDecision, RevisionPredecessor, SqliteStoryRepository, TaskRecord,
};

const MAX_INPUT_BYTES: usize = 8_000;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RepairAssessment {
    id: String,
    status: String,
    reason: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RepairInput {
    authority: String,
    source: RevisionPredecessor,
    author_decision: RevisionOutcomeDecision,
    chapter_id: String,
    chapter_number: i64,
    selected_group_ids: Vec<String>,
    desired_outcomes: Vec<DesiredOutcome>,
    assessment: Vec<RepairAssessment>,
}

fn input(
    db: &Connection,
    project: &ProjectId,
    source: RevisionPredecessor,
) -> Result<(RepairInput, bool), RepositoryError> {
    if !crate::author_message::valid_id(&source.writer_task_id)
        || !crate::author_message::valid_id(&source.decision_id)
        || source.decision_id.len() > 128
        || crate::RevisionId::parse(source.assessment_revision_id.clone()).is_err()
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let decision = crate::revision_outcome_decision::load(db, project, &source.decision_id)?;
    if decision.writer_task_id != source.writer_task_id
        || decision.assessment_revision_id != source.assessment_revision_id
        || decision.choice != RevisionOutcomeChoice::RepairDraft
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let (report, current_report) = crate::revision_outcome_decision::report(
        db,
        project,
        &source.writer_task_id,
        &source.assessment_revision_id,
    )?;
    if report["draftRevisionId"] != decision.draft_revision_id
        || report["knowledgeBatchId"] != serde_json::json!(decision.knowledge_batch_id)
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let (_, target) = crate::revision_outcome::writer(db, project, &source.writer_task_id)?;
    let text = |value: &Value| {
        value
            .as_str()
            .map(str::to_owned)
            .ok_or(RepositoryError::CorruptState)
    };
    let assessment = report["outcomes"]
        .as_array()
        .ok_or(RepositoryError::CorruptState)?
        .iter()
        .map(|item| {
            Ok(RepairAssessment {
                id: text(&item["id"])?,
                status: text(&item["status"])?,
                reason: text(&item["reason"])?,
            })
        })
        .collect::<Result<Vec<_>, RepositoryError>>()?;
    let current = current_report
        && crate::revision_outcome_decision::history(db, project, &source.writer_task_id)?.first()
            == Some(&decision);
    let frozen = RepairInput {
        authority: "candidateAnalysis".into(),
        source,
        author_decision: decision,
        chapter_id: text(&target["chapterId"])?,
        chapter_number: target["chapterNumber"]
            .as_i64()
            .ok_or(RepositoryError::CorruptState)?,
        selected_group_ids: serde_json::from_value(target["scope"]["groupIds"].clone())
            .map_err(|_| RepositoryError::CorruptState)?,
        desired_outcomes: crate::revision_outcome::desired_outcomes(&target)?,
        assessment,
    };
    if serde_json::to_vec(&frozen)
        .map_err(|_| RepositoryError::CorruptState)?
        .len()
        > MAX_INPUT_BYTES
    {
        return Err(RepositoryError::InvalidContent(
            "complete repair context exceeds 8000 bytes; start an ordinary discussion instead"
                .into(),
        ));
    }
    Ok((frozen, current))
}

// Follow only actual preceding attempts in the same Work. Target IDs alone are
// insufficient proof, and the hard depth limit rejects corrupt/cyclic lineage.
fn inherited(
    db: &Connection,
    task: &TaskRecord,
) -> Result<Option<(RepairInput, bool)>, RepositoryError> {
    let mut current = task.clone();
    for _ in 0..32 {
        let manifest: Value = serde_json::from_str(&current.input_manifest_json)
            .map_err(|_| RepositoryError::CorruptState)?;
        let target: Value = serde_json::from_str(&current.target_json)
            .map_err(|_| RepositoryError::CorruptState)?;
        if let Some(encoded) = manifest.get("revisionRepairInput") {
            let frozen: RepairInput = serde_json::from_value(encoded.clone())
                .map_err(|_| RepositoryError::TaskBindingInvalid)?;
            let source: RevisionPredecessor =
                serde_json::from_value(target["revisionRepairSource"].clone())
                    .map_err(|_| RepositoryError::TaskBindingInvalid)?;
            if current.operation != "advisory" || source != frozen.source {
                return Err(RepositoryError::TaskBindingInvalid);
            }
            let (actual, still_current) = input(db, &task.project_id, source)?;
            if frozen != actual {
                return Err(RepositoryError::TaskBindingInvalid);
            }
            return Ok(Some((actual, still_current)));
        }
        let Some(binding) = manifest.get("authorIntentBinding") else {
            if target.get("revisionRepairSource").is_some() {
                return Err(RepositoryError::TaskBindingInvalid);
            }
            return Ok(None);
        };
        let prior = binding["sourceTaskId"]
            .as_str()
            .ok_or(RepositoryError::TaskBindingInvalid)?;
        let valid: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM story_work_attempts_v1 a JOIN story_work_steps_v1 s ON s.id=a.step_id
             JOIN story_work_attempts_v1 p ON p.attempt_id=?2 AND p.sequence<a.sequence
             JOIN story_work_steps_v1 ps ON ps.id=p.step_id AND ps.work_task_id=s.work_task_id WHERE a.attempt_id=?1)",
            params![current.task_id,prior], |row| row.get(0),
        ).map_err(storage_error)?;
        if !valid {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        current = crate::tasks::load_task(db, prior)?;
        if current.project_id != task.project_id {
            return Err(RepositoryError::TaskBindingInvalid);
        }
    }
    Err(RepositoryError::TaskBindingInvalid)
}

pub(crate) fn freeze(
    db: &Connection,
    request: &CreateTaskRequest,
    target: &Value,
    manifest: &mut Value,
) -> Result<(), RepositoryError> {
    if manifest.get("revisionRepairInput").is_some() {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let prior = manifest["authorIntentBinding"]["sourceTaskId"]
        .as_str()
        .map(|id| crate::tasks::load_task(db, id))
        .transpose()?;
    let inherited_input = prior
        .as_ref()
        .map(|task| inherited(db, task))
        .transpose()?
        .flatten();
    if let Some(source) = target.get("revisionRepairSource") {
        if request.operation != "advisory"
            || target["type"] != "project"
            || target
                .as_object()
                .ok_or(RepositoryError::TaskBindingInvalid)?
                .keys()
                .any(|key| {
                    !matches!(
                        key.as_str(),
                        "type" | "revisionRepairSource" | "resumeStepId"
                    )
                })
        {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        let source: RevisionPredecessor = serde_json::from_value(source.clone())
            .map_err(|_| RepositoryError::TaskBindingInvalid)?;
        let (frozen, current) = input(db, &request.project_id, source)?;
        if !current {
            return Err(RepositoryError::TaskInputStale);
        }
        if prior.is_some() && inherited_input.as_ref().map(|(input, _)| input) != Some(&frozen) {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        manifest["revisionRepairInput"] =
            serde_json::to_value(frozen).map_err(|_| RepositoryError::CorruptState)?;
    } else if let Some((frozen, current)) = inherited_input {
        if request.project_id
            != prior
                .as_ref()
                .ok_or(RepositoryError::TaskBindingInvalid)?
                .project_id
        {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        if !current {
            match request.operation.as_str() {
                "advisory" => {
                    // A coordinator continues an already authorized Work after its
                    // own writer replaced the report source. Validate the registered
                    // step and author lineage; an arbitrary target key is no grant.
                    let mut verified = manifest.clone();
                    if target.get("workDecision").is_none()
                        || !crate::work_decision::freeze(db, request, target, &mut verified)?
                        || verified != *manifest
                    {
                        return Err(RepositoryError::TaskInputStale);
                    }
                }
                "generateChapter" => return Err(RepositoryError::TaskInputStale),
                _ => {}
            }
        }
        if request.operation == "generateChapter" {
            writer_target(&frozen, &request.operation, target)?;
        }
    }
    Ok(())
}

pub(crate) fn assert_discussion_current(
    db: &Connection,
    task: &TaskRecord,
) -> Result<(), RepositoryError> {
    if let Some((_, current)) = inherited(db, task)?
        && task.operation == "advisory"
        && !current
        && crate::work_decision::binding(db, task)?.is_none()
    {
        return Err(RepositoryError::TaskInputStale);
    }
    Ok(())
}

fn writer_target(
    input: &RepairInput,
    operation: &str,
    target: &Value,
) -> Result<(), RepositoryError> {
    if operation != "generateChapter"
        || target["type"] != "chapter"
        || target["chapterId"] != input.chapter_id
        || target["chapterNumber"] != input.chapter_number
        || target["scope"]["type"] != "selectedScenes"
        || target["scope"]["sourceRevisionId"] != input.author_decision.draft_revision_id
        || target.get("revisionRepairSource").is_some()
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    crate::scene_revision::selection(target)?;
    let selected = target["scope"]["groupIds"]
        .as_array()
        .ok_or(RepositoryError::TaskBindingInvalid)?;
    if selected
        .iter()
        .any(|id| !input.selected_group_ids.iter().any(|allowed| id == allowed))
        || crate::revision_outcome::desired_outcomes(target)?.is_empty()
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    Ok(())
}

pub(crate) fn validate_action(
    db: &Connection,
    task: &TaskRecord,
    operation: &str,
    target: &Value,
) -> Result<(), RepositoryError> {
    if let Some((frozen, current)) = inherited(db, task)? {
        if !current {
            return Err(RepositoryError::TaskInputStale);
        }
        writer_target(&frozen, operation, target)?;
    }
    Ok(())
}

impl SqliteStoryRepository {
    /// A bounded, immutable source pack; returned as candidate data, never author intent.
    pub fn revision_repair_input(
        &self,
        task: &TaskRecord,
    ) -> Result<Option<Value>, RepositoryError> {
        let read = || {
            let actual = crate::tasks::load_task(&self.connection, &task.task_id)?;
            if actual.project_id != task.project_id
                || actual.thread_id != task.thread_id
                || actual.operation != task.operation
                || actual.target_json != task.target_json
                || actual.input_manifest_json != task.input_manifest_json
                || actual.input_revision != task.input_revision
                || actual.capability_hash != task.capability_hash
            {
                return Err(RepositoryError::TaskBindingInvalid);
            }
            inherited(&self.connection, &actual)?
                .map(|(frozen, _)| {
                    serde_json::to_value(frozen).map_err(|_| RepositoryError::CorruptState)
                })
                .transpose()
        };
        if self.connection.is_autocommit() {
            self.with_read_snapshot(read)?
        } else {
            read()
        }
    }
}
