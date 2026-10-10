use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{Connection, OptionalExtension, params};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::authoring_runs::storage_error;
use crate::{ProjectId, RepositoryError, SqliteStoryRepository, TaskRecord};

impl SqliteStoryRepository {
    /// Compile model-selected literal quotes against the frozen candidate scenes.
    /// This cannot grant authority or change a verdict; normal task validation
    /// still runs on the fully anchored report before it is persisted.
    pub fn prepare_revision_outcome_payload(
        &self,
        task: &TaskRecord,
        payload: &Value,
    ) -> Result<Value, RepositoryError> {
        if task.operation != "assessRevisionOutcome" || payload.to_string().len() > 24_000 {
            return Err(RepositoryError::TaskOutputForbidden);
        }
        crate::task_input::assert_task_inputs_current(&self.connection, task)?;
        crate::task_sources::require_author_requirements_read(&self.connection, task)?;
        let target: Value =
            serde_json::from_str(&task.target_json).map_err(|_| RepositoryError::CorruptState)?;
        crate::revision_outcome::validate_target(&self.connection, &task.project_id, &target)?;
        let source = draft(
            &self.connection,
            &task.project_id,
            target["chapterId"]
                .as_str()
                .ok_or(RepositoryError::TaskBindingInvalid)?,
            target["draftRevisionId"]
                .as_str()
                .ok_or(RepositoryError::TaskBindingInvalid)?,
        )?;
        let groups = source["sceneGroups"]
            .as_array()
            .ok_or(RepositoryError::CorruptState)?;
        let mut resolved = payload.clone();
        let judgments = resolved["outcomes"]
            .as_array_mut()
            .filter(|items| items.len() <= 16)
            .ok_or(RepositoryError::TaskOutputForbidden)?;
        for (outcome_index, judgment) in judgments.iter_mut().enumerate() {
            let evidence = judgment["evidence"]
                .as_array_mut()
                .filter(|items| items.len() <= 4)
                .ok_or(RepositoryError::TaskOutputForbidden)?;
            for (evidence_index, item) in evidence.iter_mut().enumerate() {
                let fields = item
                    .as_object()
                    .ok_or(RepositoryError::TaskOutputForbidden)?;
                if fields
                    .keys()
                    .any(|key| !matches!(key.as_str(), "groupId" | "quote" | "start" | "end"))
                {
                    return Err(RepositoryError::TaskOutputForbidden);
                }
                let group_id = item["groupId"]
                    .as_str()
                    .ok_or(RepositoryError::TaskOutputForbidden)?
                    .to_owned();
                let body = groups
                    .iter()
                    .find(|group| group["id"] == group_id)
                    .and_then(|group| group["body"].as_str())
                    .ok_or(RepositoryError::TaskOutputForbidden)?;
                let mut quote = item.clone();
                quote
                    .as_object_mut()
                    .ok_or(RepositoryError::TaskOutputForbidden)?
                    .remove("groupId");
                *item = crate::source_evidence::resolve(&quote, body).map_err(|error| {
                    let crate::RepositoryError::InvalidContent(reason) = &error else { return error; };
                    let code = match reason.as_str() {
                        "evidence_quote_not_found" => "evidence_quote_not_found",
                        "evidence_quote_ambiguous" => "evidence_quote_ambiguous",
                        "evidence_anchor_mismatch" => "evidence_anchor_mismatch",
                        "evidence_anchor_invalid" => "evidence_anchor_invalid",
                        "evidence_anchor_incomplete" => "evidence_anchor_incomplete",
                        "evidence_quote_invalid" => "evidence_quote_invalid",
                        "evidence_shape_invalid" => "evidence_shape_invalid",
                        _ => return error,
                    };
                    RepositoryError::PayloadValidation {
                        code,
                        instance_path: format!("/outcomes/{outcome_index}/evidence/{evidence_index}"),
                        details: json!({"sourceRef":{"groupId":group_id,"draftRevisionId":target["draftRevisionId"]}}),
                    }
                })?;
                item["groupId"] = group_id.into();
            }
        }
        validate_report(
            &self.connection,
            &task.project_id,
            &target,
            &resolved.to_string(),
        )?;
        Ok(resolved)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Report {
    writer_task_id: String,
    chapter_id: String,
    #[serde(deserialize_with = "crate::deserialize_execution_limit")]
    original_revision_id: Option<String>,
    draft_revision_id: String,
    #[serde(deserialize_with = "crate::deserialize_execution_limit")]
    knowledge_batch_id: Option<String>,
    uncovered_requirements: Vec<crate::work_issues::UncoveredRequirement>,
    outcomes: Vec<Judgment>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Judgment {
    id: String,
    status: JudgmentStatus,
    reason: String,
    evidence: Vec<Evidence>,
    #[serde(deserialize_with = "crate::deserialize_execution_limit")]
    issue: Option<crate::work_issues::IssueLink>,
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

pub(crate) fn criterion_contract(target: &Value, desired: &[crate::DesiredOutcome]) -> Value {
    json!({
        "authority":"frozenWriterAcceptanceAndSelection",
        "expectedCriterionIds":desired.iter().map(|item|item.id.as_str()).collect::<Vec<_>>(),
        "exactlyOnce":true,
        "frozenRefs":{
            "writerTaskId":target["writerTaskId"],"chapterId":target["chapterId"],
            "originalRevisionId":target["originalRevisionId"],
            "draftRevisionId":target["draftRevisionId"],"knowledgeBatchId":target["knowledgeBatchId"]
        },
        "lengthAuthority":"nativeObjectiveCheck",
        "lengthSourcePath":"/actualLength",
        "uncoveredRequirementPath":"/uncoveredRequirements",
        "instruction":"Report each desiredOutcomes ID exactly once. Do not invent an outcome ID from chapterAcceptance.length or actualLength; Native checks length separately. Record omitted original author requirements in uncoveredRequirements with actual author-source evidence. Preserve every frozen criterion and assess it independently; an unmet judgment is a valid report, not completed prose."
    })
}

fn criterion_set_error(
    target: &Value,
    desired: &[crate::DesiredOutcome],
    supplied: &[Judgment],
) -> Option<RepositoryError> {
    let expected: BTreeSet<_> = desired.iter().map(|item| item.id.as_str()).collect();
    let mut counts = BTreeMap::new();
    for item in supplied {
        *counts.entry(item.id.as_str()).or_insert(0usize) += 1;
    }
    if supplied.len() == desired.len()
        && counts.len() == desired.len()
        && counts.keys().all(|id| expected.contains(id))
    {
        return None;
    }
    // Never echo arbitrary model text in a diagnostic. Safe IDs retain their
    // exact value; unsupported values are identified only by item index.
    let safe = |id: &&str| crate::work_issues::identifier(id);
    let unexpected = counts
        .keys()
        .copied()
        .filter(|id| !expected.contains(id))
        .collect::<Vec<_>>();
    let duplicates = counts
        .iter()
        .filter_map(|(id, count)| (*count > 1).then_some(*id))
        .collect::<Vec<_>>();
    let contract = criterion_contract(target, desired);
    Some(RepositoryError::PayloadValidation {
        code: "criterion_set_mismatch",
        instance_path: "/outcomes".into(),
        details: json!({
            "expectedCriterionIds":contract["expectedCriterionIds"],
            "missingCriterionIds":expected.iter().copied().filter(|id|!counts.contains_key(id)).collect::<Vec<_>>(),
            "unexpectedCriterionIds":unexpected.iter().copied().filter(safe).take(16).collect::<Vec<_>>(),
            "duplicateCriterionIds":duplicates.iter().copied().filter(safe).take(16).collect::<Vec<_>>(),
            "unexpectedCriterionCount":unexpected.len(),"duplicateCriterionCount":duplicates.len(),
            "invalidCriterionIndexes":supplied.iter().enumerate().filter_map(|(index,item)|(!safe(&item.id.as_str())).then_some(index)).take(16).collect::<Vec<_>>(),
            "frozenRefs":contract["frozenRefs"],
            "instruction":contract["instruction"]
        }),
    })
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
    let work = crate::work_issues::work_for(db, project, &report.writer_task_id)?;
    let links = report
        .outcomes
        .iter()
        .filter_map(|judgment| judgment.issue.as_ref())
        .chain(report.uncovered_requirements.iter().map(|item| &item.issue))
        .collect::<Vec<_>>();
    crate::work_issues::validate_report_links(db, project, &work, &links)?;
    crate::work_issues::validate_uncovered(db, project, &work, &report.uncovered_requirements)?;
    for (field, value) in [
        ("writerTaskId", &report.writer_task_id),
        ("chapterId", &report.chapter_id),
        ("draftRevisionId", &report.draft_revision_id),
    ] {
        if target[field].as_str() != Some(value.as_str()) {
            return Err(invalid());
        }
    }
    if target.get("originalRevisionId") != Some(&serde_json::json!(report.original_revision_id)) {
        return Err(invalid());
    }
    let supplied: Value = serde_json::from_str(encoded).map_err(|_| invalid())?;
    if supplied.get("knowledgeBatchId") != target.get("knowledgeBatchId")
        || supplied["knowledgeBatchId"] != serde_json::json!(report.knowledge_batch_id)
    {
        return Err(invalid());
    }
    let (_, writer_target) = crate::revision_outcome::writer(db, project, &report.writer_task_id)?;
    let desired = crate::revision_outcome::assessment_outcomes(
        db,
        project,
        &writer_target,
        &report.draft_revision_id,
    )?;
    if let Some(error) = criterion_set_error(target, &desired, &report.outcomes) {
        return Err(error);
    }
    let draft = draft(db, project, &report.chapter_id, &report.draft_revision_id)?;
    let groups = draft["sceneGroups"]
        .as_array()
        .ok_or(RepositoryError::CorruptState)?;
    let original = report
        .original_revision_id
        .as_deref()
        .map(|revision| self::draft(db, project, &report.chapter_id, revision))
        .transpose()?;
    let before = original
        .as_ref()
        .and_then(|value| value["sceneGroups"].as_array());
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
        if judgment.status != JudgmentStatus::Supported && judgment.issue.is_none() {
            return Err(RepositoryError::InvalidContent(
                "negative assessment must select an existing stable issue or propose a new one"
                    .into(),
            ));
        }
        let whole_chapter_criterion = writer_target["chapterAcceptance"]["requirements"]
            .as_array()
            .is_some_and(|items| items.iter().any(|c| c["id"] == judgment.id));
        if !whole_chapter_criterion
            && judgment.status != JudgmentStatus::InsufficientEvidence
            && !judgment.evidence.iter().any(|evidence| {
                let old = before
                    .and_then(|items| items.iter().find(|group| group["id"] == evidence.group_id));
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
    crate::task_sources::require_author_requirements_read(db, task)?;
    let target: Value =
        serde_json::from_str(&task.target_json).map_err(|_| RepositoryError::CorruptState)?;
    crate::revision_outcome::validate_target(db, &task.project_id, &target)?;
    if !crate::revision_outcome::inputs_current(db, &task.project_id, &target)? {
        return Err(RepositoryError::TaskInputStale);
    }
    validate_report(db, &task.project_id, &target, encoded)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn desired() -> Vec<crate::DesiredOutcome> {
        ["setting", "suspense", "prose"]
            .into_iter()
            .map(|id| crate::DesiredOutcome {
                id: id.into(),
                criterion: "合成检查要求".into(),
                group_ids: vec!["scene".into()],
            })
            .collect()
    }
    fn judgment(id: &str) -> Judgment {
        Judgment {
            id: id.into(),
            status: JudgmentStatus::Supported,
            reason: "合成证据".into(),
            evidence: Vec::new(),
            issue: None,
        }
    }
    fn details(ids: &[&str]) -> Value {
        let target = json!({"writerTaskId":"writer","chapterId":"chapter","originalRevisionId":null,"draftRevisionId":"draft","knowledgeBatchId":null});
        let supplied = ids.iter().map(|id| judgment(id)).collect::<Vec<_>>();
        let RepositoryError::PayloadValidation {
            code,
            instance_path,
            details,
        } = criterion_set_error(&target, &desired(), &supplied).expect("reject mismatch")
        else {
            panic!("typed field error")
        };
        assert_eq!(code, "criterion_set_mismatch");
        assert_eq!(instance_path, "/outcomes");
        assert_eq!(
            details["expectedCriterionIds"],
            json!(["setting", "suspense", "prose"])
        );
        assert_eq!(details["frozenRefs"], target);
        assert!(details.to_string().len() <= 24_000);
        details
    }
    #[test]
    fn criterion_set_feedback_identifies_extra_missing_duplicate_and_foreign_ids() {
        let target = json!({});
        let good = ["prose", "setting", "suspense"].map(judgment);
        assert!(criterion_set_error(&target, &desired(), &good).is_none());
        let extra = details(&["setting", "suspense", "prose", "model-length"]);
        assert_eq!(extra["unexpectedCriterionIds"], json!(["model-length"]));
        assert_eq!(extra["missingCriterionIds"], json!([]));
        assert_eq!(
            details(&["setting", "prose"])["missingCriterionIds"],
            json!(["suspense"])
        );
        let duplicate = details(&["setting", "setting", "prose"]);
        assert_eq!(duplicate["duplicateCriterionIds"], json!(["setting"]));
        assert_eq!(duplicate["missingCriterionIds"], json!(["suspense"]));
        let foreign = details(&["setting", "foreign", "prose"]);
        assert_eq!(foreign["unexpectedCriterionIds"], json!(["foreign"]));
        assert_eq!(foreign["missingCriterionIds"], json!(["suspense"]));
    }
    #[test]
    fn criterion_feedback_does_not_echo_unsafe_or_oversized_model_values() {
        for invalid in [
            "private\nsynthetic-content".to_owned(),
            "x".repeat(161),
            "合成私有文本".repeat(300),
        ] {
            let result = details(&["setting", "suspense", "prose", &invalid]);
            assert_eq!(result["unexpectedCriterionIds"], json!([]));
            assert_eq!(result["unexpectedCriterionCount"], 1);
            assert_eq!(result["invalidCriterionIndexes"], json!([3]));
            assert!(!result.to_string().contains(&invalid));
        }
    }
}
