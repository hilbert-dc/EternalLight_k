use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{Connection, params};
use serde_json::{Value, json};

use crate::{ProjectId, RepositoryError, ReviewRecord, SqliteStoryRepository};

pub(crate) fn review_options(
    connection: &Connection,
    review: &ReviewRecord,
) -> Result<Value, RepositoryError> {
    let content: String = connection.query_row(
        "SELECT content_json FROM story_revisions_v2 WHERE project_id = ?1 AND object_id = ?2 AND revision_id = ?3",
        params![review.project_id.as_str(), review.object_id.as_str(), review.revision_id.as_str()], |row| row.get(0),
    ).map_err(|error| RepositoryError::Storage(error.to_string()))?;
    let content: Value =
        serde_json::from_str(&content).map_err(|_| RepositoryError::CorruptState)?;
    let mut groups = content["sceneGroups"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|group| group["id"].clone())
        .collect::<Vec<_>>();
    let previous =
        crate::workflow::object_head(connection, &review.project_id, &review.object_id)?.2;
    if review.object_kind == "draft"
        && let Some(previous) = previous
    {
        let (_, base) = crate::acceptance_shared::chapter::candidate_and_base(
            connection,
            review,
            Some(&previous),
        )?;
        for group in base
            .as_ref()
            .and_then(|base| base["sceneGroups"].as_array())
            .into_iter()
            .flatten()
        {
            if !groups.contains(&group["id"]) {
                groups.push(group["id"].clone());
            }
        }
    }
    let facts: Value =
        serde_json::from_str(&review.proposals_json).map_err(|_| RepositoryError::CorruptState)?;
    let selections: Value =
        serde_json::from_str(&review.selections_json).map_err(|_| RepositoryError::CorruptState)?;
    Ok(json!({"sceneGroupIds":groups,"factProposals":facts,"selections":selections}))
}

pub(crate) fn validate_choices(
    connection: &Connection,
    review: &ReviewRecord,
    selections: &Value,
) -> Result<(), RepositoryError> {
    let choices = selections.as_object().ok_or_else(|| {
        RepositoryError::InvalidContent("proposal choices must be an object".into())
    })?;
    if review.object_kind != "draft" {
        return if choices.is_empty() {
            Ok(())
        } else {
            Err(RepositoryError::InvalidContent(
                "non-draft proposal has no fact choices".into(),
            ))
        };
    }
    if choices
        .keys()
        .any(|key| !matches!(key.as_str(), "sceneGroupIds" | "factDecisions"))
    {
        return Err(RepositoryError::InvalidContent(
            "unknown proposal choice field".into(),
        ));
    }
    let options = review_options(connection, review)?;
    let previous =
        crate::workflow::object_head(connection, &review.project_id, &review.object_id)?.2;
    let (candidate, base) = crate::acceptance_shared::chapter::candidate_and_base(
        connection,
        review,
        previous.as_deref(),
    )?;
    crate::acceptance_shared::chapter::selected_content(&candidate, base.as_ref(), selections)?;
    let known_groups = options["sceneGroupIds"]
        .as_array()
        .ok_or(RepositoryError::CorruptState)?;
    let groups = selections
        .get("sceneGroupIds")
        .map(|value| {
            value.as_array().ok_or_else(|| {
                RepositoryError::InvalidContent("invalid source scene selection".into())
            })
        })
        .transpose()?;
    let mut seen = BTreeSet::new();
    for group in groups.into_iter().flatten() {
        let id = group
            .as_str()
            .ok_or_else(|| RepositoryError::InvalidContent("invalid source scene id".into()))?;
        if !known_groups.contains(group) || !seen.insert(id) {
            return Err(RepositoryError::InvalidContent(
                "unknown or repeated source scene".into(),
            ));
        }
    }
    let facts = options["factProposals"]
        .as_array()
        .ok_or(RepositoryError::CorruptState)?;
    let decisions = selections
        .get("factDecisions")
        .map(|value| {
            value
                .as_object()
                .ok_or_else(|| RepositoryError::InvalidContent("invalid fact decisions".into()))
        })
        .transpose()?;
    for (id, decision) in decisions.into_iter().flatten() {
        let fact = facts
            .iter()
            .find(|fact| fact["id"] == id.as_str())
            .ok_or_else(|| RepositoryError::InvalidContent("unknown proposal fact".into()))?;
        if !matches!(
            decision.as_str(),
            Some("candidate" | "confirmed" | "rejected")
        ) {
            return Err(RepositoryError::InvalidContent(
                "invalid proposal fact decision".into(),
            ));
        }
        if decision == "confirmed"
            && !groups.is_some_and(|groups| groups.contains(&fact["sceneGroupId"]))
        {
            return Err(RepositoryError::ReviewChecksFailed);
        }
        if decision == "confirmed" {
            let source = candidate["sceneGroups"]
                .as_array()
                .and_then(|groups| {
                    groups
                        .iter()
                        .find(|group| group["id"] == fact["sceneGroupId"])
                })
                .ok_or(RepositoryError::ReviewChecksFailed)?;
            let quote = fact["evidence"]
                .as_str()
                .filter(|quote| !quote.trim().is_empty())
                .ok_or(RepositoryError::ReviewChecksFailed)?;
            if !source["body"]
                .as_str()
                .is_some_and(|body| body.trim().contains(quote))
                || fact
                    .get("sourceRevisionId")
                    .is_some_and(|id| id != review.revision_id.as_str())
                || fact
                    .get("sceneId")
                    .is_some_and(|id| !id.is_null() && *id != source["sceneId"])
            {
                return Err(RepositoryError::ReviewChecksFailed);
            }
        }
    }
    Ok(())
}

impl SqliteStoryRepository {
    /// Preflights exact author choices against the same validators used by atomic apply.
    pub fn preview_proposal_application(
        &self,
        project: &ProjectId,
        id: &str,
        selected: &[String],
        choices: &BTreeMap<String, Value>,
    ) -> Result<Value, RepositoryError> {
        self.with_read_snapshot(|| {
            let mut report = self.proposal_set_report(project, id, selected)?;
            if choices.len() != selected.len()
                || selected.iter().any(|id| !choices.contains_key(id))
            {
                return Err(RepositoryError::InvalidContent(
                    "proposal choices must match selected operations".into(),
                ));
            }
            let operations: Vec<crate::proposal_sets::BoundOperation> =
                serde_json::from_value(report["operations"].clone())
                    .map_err(|_| RepositoryError::CorruptState)?;
            if report["selection"]["declaredClosureReady"] == true {
                crate::proposal_application::validate_selected_reviews(
                    &self.connection,
                    project,
                    &operations,
                    selected,
                    choices,
                    report["baseProjectRevision"]
                        .as_i64()
                        .ok_or(RepositoryError::CorruptState)?,
                )?;
                report["selection"]["applyReady"] = json!(true);
            } else {
                for operation in operations
                    .iter()
                    .filter(|op| selected.contains(&op.operation_id))
                {
                    let review =
                        crate::workflow::load_review(&self.connection, &operation.review_id)?;
                    validate_choices(&self.connection, &review, &choices[&operation.operation_id])?;
                }
            }
            report["selection"]["selectionsByOperationId"] = json!(choices);
            Ok(report)
        })?
    }
}
