use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::{ProjectId, RepositoryError, SnapshotId, SqliteStoryRepository};

const MAX_OPERATIONS: usize = 20;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProposedReview {
    pub operation_id: String,
    pub review_id: String,
    pub depends_on: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BoundOperation {
    pub operation_id: String,
    pub review_id: String,
    pub object_id: String,
    pub object_kind: String,
    pub candidate_revision_id: String,
    pub expected_accepted_revision_id: Option<String>,
    pub depends_on: Vec<String>,
}

pub struct CreateProposalSet<'a> {
    pub project_id: &'a ProjectId,
    pub base_snapshot_id: &'a SnapshotId,
    pub intent: &'a str,
    pub operations: &'a [ProposedReview],
    pub idempotency_key: &'a str,
    pub now: i64,
}

impl SqliteStoryRepository {
    

    

    

    pub(crate) fn proposal_set_report(
        &self,
        project_id: &ProjectId,
        proposal_set_id: &str,
        selected: &[String],
    ) -> Result<Value, RepositoryError> {
        if !proposal_set_id.starts_with("proposal-set-") || proposal_set_id.len() > 80 {
            return Err(RepositoryError::InvalidContent(
                "invalid proposal set id".into(),
            ));
        }
        let row = self.connection.query_row(
                "SELECT base_snapshot_id, project_revision, intent, operations_json, created_at FROM story_proposal_sets_v1 WHERE project_id = ?1 AND proposal_set_id = ?2 AND branch_id = 'main'",
                params![project_id.as_str(), proposal_set_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?, row.get::<_, i64>(4)?)),
            ).optional().map_err(storage_error)?.ok_or(RepositoryError::CandidateNotFound)?;
        let operations: Vec<BoundOperation> =
            serde_json::from_str(&row.3).map_err(|_| RepositoryError::CorruptState)?;
        let known = operations
            .iter()
            .map(|op| op.operation_id.as_str())
            .collect::<BTreeSet<_>>();
        let selected_ids = selected.iter().map(String::as_str).collect::<BTreeSet<_>>();
        if selected_ids.len() != selected.len() || selected_ids.iter().any(|id| !known.contains(id))
        {
            return Err(RepositoryError::InvalidContent(
                "invalid proposal selection".into(),
            ));
        }
        let current_revision = self
            .connection
            .query_row(
                "SELECT revision FROM story_workspaces_v2 WHERE project_id = ?1",
                [project_id.as_str()],
                |row| row.get::<_, i64>(0),
            )
            .map_err(storage_error)?;
        let current_snapshot = self.connection.query_row(
                "SELECT head_snapshot_id FROM story_branches_v2 WHERE project_id = ?1 AND branch_id = 'main'",
                [project_id.as_str()], |row| row.get::<_, String>(0),
            ).map_err(storage_error)?;
        let mut stale_operations = Vec::new();
        for op in &operations {
            let current = self.connection.query_row(
                    "SELECT candidate_revision_id, accepted_revision_id, locked FROM story_object_heads_v2 WHERE project_id = ?1 AND object_id = ?2",
                    params![project_id.as_str(), op.object_id],
                    |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, Option<String>>(1)?, row.get::<_, bool>(2)?)),
                ).optional().map_err(storage_error)?;
            let review_ready = self
                .connection
                .query_row(
                    "SELECT status FROM story_reviews_v2 WHERE project_id = ?1 AND review_id = ?2",
                    params![project_id.as_str(), op.review_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(storage_error)?;
            if current.as_ref().is_none_or(|head| {
                head.0.as_deref() != Some(&op.candidate_revision_id)
                    || head.1 != op.expected_accepted_revision_id
                    || head.2
            }) || review_ready.as_deref() != Some("ready")
            {
                stale_operations.push(op.operation_id.clone());
            }
        }
        let missing_dependencies = operations
            .iter()
            .filter(|op| selected_ids.contains(op.operation_id.as_str()))
            .flat_map(|op| {
                op.depends_on
                    .iter()
                    .filter(|id| !selected_ids.contains(id.as_str()))
                    .map(|id| json!({"operationId":op.operation_id,"requires":id}))
            })
            .collect::<Vec<_>>();
        let current =
            row.0 == current_snapshot && row.1 == current_revision && stale_operations.is_empty();
        let mut application = self.connection.query_row(
                "SELECT application_id, status, result_revision, result_snapshot_id, undone_by_application_id FROM story_proposal_applications_v1 WHERE project_id = ?1 AND proposal_set_id = ?2 ORDER BY rowid DESC LIMIT 1",
                params![project_id.as_str(), proposal_set_id],
                |row| Ok(json!({"id": row.get::<_, String>(0)?, "status": row.get::<_, String>(1)?,
                    "resultRevision": row.get::<_, i64>(2)?, "resultSnapshotId": row.get::<_, String>(3)?,
                    "undoneByApplicationId": row.get::<_, Option<String>>(4)?})),
            ).optional().map_err(storage_error)?;
        if let Some(receipt) = application.as_mut() {
            let mut members = self.connection.prepare(
                    "SELECT operation_id FROM story_proposal_application_members_v1 WHERE application_id = ?1 ORDER BY rowid",
                ).map_err(storage_error)?;
            let selected = members
                .query_map(
                    [receipt["id"]
                        .as_str()
                        .ok_or(RepositoryError::CorruptState)?],
                    |row| row.get::<_, String>(0),
                )
                .map_err(storage_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(storage_error)?;
            receipt["selectedOperationIds"] = json!(selected);
        }
        let status = application
            .as_ref()
            .and_then(|value| value["status"].as_str())
            .unwrap_or(if current { "ready" } else { "stale" });
        let review_options = operations
            .iter()
            .map(|op| {
                let review = crate::workflow::load_review(&self.connection, &op.review_id)?;
                Ok((
                    op.operation_id.clone(),
                    crate::proposal_selection::review_options(&self.connection, &review)?,
                ))
            })
            .collect::<Result<BTreeMap<_, _>, RepositoryError>>()?;
        Ok(json!({
            "id": proposal_set_id, "projectId": project_id.as_str(), "branchId": "main",
            "baseSnapshotId": row.0, "baseProjectRevision": row.1, "intent": row.2,
            "createdAt": row.4, "operations": operations, "reviewOptions":review_options,
            "selection": {"operationIds": selected, "missingDependencies": missing_dependencies,
                "declaredClosureReady": !selected.is_empty() && current && missing_dependencies.is_empty(),
                "applyReady": false},
            "status": status, "application": application,
            "staleOperations": stale_operations,
            "coverage": {"dependencyClosure": "declaredOnly", "semanticDependenciesComplete": false},
            "authority": "proposalOnly"
        }))
    }
}

fn validate_dependency_graph(operations: &[ProposedReview]) -> Result<(), RepositoryError> {
    let mut ids = BTreeMap::new();
    let mut reviews = BTreeSet::new();
    for (index, op) in operations.iter().enumerate() {
        if op.operation_id.trim().is_empty()
            || op.operation_id.len() > 160
            || op.review_id.trim().is_empty()
            || op.review_id.len() > 160
            || ids.insert(op.operation_id.as_str(), index).is_some()
            || !reviews.insert(op.review_id.as_str())
        {
            return Err(RepositoryError::InvalidContent(
                "invalid proposal operation".into(),
            ));
        }
    }
    let mut states = vec![0_u8; operations.len()];
    for index in 0..operations.len() {
        visit_dependency(index, operations, &ids, &mut states)?;
    }
    Ok(())
}

fn visit_dependency(
    index: usize,
    operations: &[ProposedReview],
    ids: &BTreeMap<&str, usize>,
    states: &mut [u8],
) -> Result<(), RepositoryError> {
    if states[index] == 2 {
        return Ok(());
    }
    if states[index] == 1 {
        return Err(RepositoryError::InvalidContent(
            "cyclic proposal dependencies".into(),
        ));
    }
    states[index] = 1;
    let mut seen = BTreeSet::new();
    for dependency in &operations[index].depends_on {
        if !seen.insert(dependency) {
            return Err(RepositoryError::InvalidContent(
                "duplicate proposal dependency".into(),
            ));
        }
        let target = ids
            .get(dependency.as_str())
            .copied()
            .ok_or_else(|| RepositoryError::InvalidContent("unknown proposal dependency".into()))?;
        visit_dependency(target, operations, ids, states)?;
    }
    states[index] = 2;
    Ok(())
}

fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}

#[cfg(test)]
#[path = "proposal_sets_tests.rs"]
mod tests;

/// Public proposal sets capability; SQLite retains transaction ownership.
pub trait ProposalSetsRepository {
    /// Lists persisted proposals for author discovery; authority is checked again on read/apply.
    fn proposal_sets_page(
        &self,
        project_id: &ProjectId,
        cursor: Option<i64>,
    ) -> Result<Value, RepositoryError>;
    /// Pins reviewed candidate revisions and declared cross-object dependencies without adopting them.
    fn create_proposal_set(
        &mut self,
        request: CreateProposalSet<'_>,
    ) -> Result<Value, RepositoryError>;
    /// Reports which selected reviews could be adopted together; it never changes authority.
    fn preview_proposal_set(
        &self,
        project_id: &ProjectId,
        proposal_set_id: &str,
        selected: &[String],
    ) -> Result<Value, RepositoryError>;
}

impl ProposalSetsRepository for SqliteStoryRepository {
    /// Lists persisted proposals for author discovery; authority is checked again on read/apply.
    fn proposal_sets_page(
        &self,
        project_id: &ProjectId,
        cursor: Option<i64>,
    ) -> Result<Value, RepositoryError> {
        if cursor.is_some_and(|value| value <= 0) {
            return Err(RepositoryError::InvalidContent(
                "invalid proposal cursor".into(),
            ));
        }
        self.with_read_snapshot(|| {
            let mut statement = self.connection.prepare(
                "SELECT rowid, proposal_set_id, intent, created_at FROM story_proposal_sets_v1 WHERE project_id = ?1 AND (?2 IS NULL OR rowid < ?2) ORDER BY rowid DESC LIMIT 21",
            ).map_err(storage_error)?;
            let rows = statement.query_map(params![project_id.as_str(), cursor], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, i64>(3)?))
            }).map_err(storage_error)?.collect::<Result<Vec<_>, _>>().map_err(storage_error)?;
            drop(statement);
            let next_cursor = (rows.len() > 20).then(|| rows[19].0.to_string());
            let mut data = Vec::with_capacity(rows.len().min(20));
            for (_, id, intent, created_at) in rows.into_iter().take(20) {
                let application = self.connection.query_row(
                    "SELECT application_id, status FROM story_proposal_applications_v1 WHERE project_id = ?1 AND proposal_set_id = ?2 ORDER BY rowid DESC LIMIT 1",
                    params![project_id.as_str(), id],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                ).optional().map_err(storage_error)?;
                data.push(json!({"id": id, "intent": intent, "createdAt": created_at,
                    "status": application.as_ref().map_or("proposed", |row| row.1.as_str()),
                    "applicationId": application.map(|row| row.0)}));
            }
            Ok(json!({"data": data, "nextCursor": next_cursor}))
        })?
    }

    /// Pins reviewed candidate revisions and declared cross-object dependencies without adopting them.
    fn create_proposal_set(
        &mut self,
        request: CreateProposalSet<'_>,
    ) -> Result<Value, RepositoryError> {
        if request.intent.trim().is_empty()
            || request.intent.chars().count() > 1_000
            || request.idempotency_key.is_empty()
            || request.idempotency_key.len() > 160
            || !(2..=MAX_OPERATIONS).contains(&request.operations.len())
        {
            return Err(RepositoryError::InvalidContent(
                "invalid proposal set request".into(),
            ));
        }
        validate_dependency_graph(request.operations)?;
        let digest = format!(
            "{:x}",
            Sha256::digest(
                json!({
                    "snapshotId": request.base_snapshot_id.as_str(), "intent": request.intent,
                    "operations": request.operations
                })
                .to_string()
                .as_bytes()
            )
        );
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let existing = transaction.query_row(
            "SELECT proposal_set_id, request_digest FROM story_proposal_sets_v1 WHERE project_id = ?1 AND idempotency_key = ?2",
            params![request.project_id.as_str(), request.idempotency_key],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        ).optional().map_err(storage_error)?;
        if let Some((id, prior_digest)) = existing {
            if prior_digest != digest {
                return Err(RepositoryError::IdempotencyConflict);
            }
            return Ok(
                json!({"id": id, "baseSnapshotId": request.base_snapshot_id.as_str(), "replayed": true}),
            );
        }
        let revision = transaction
            .query_row(
                "SELECT revision FROM story_workspaces_v2 WHERE project_id = ?1",
                [request.project_id.as_str()],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(storage_error)?
            .ok_or(RepositoryError::ProjectNotFound)?;
        let head = transaction.query_row(
            "SELECT head_snapshot_id FROM story_branches_v2 WHERE project_id = ?1 AND branch_id = 'main'",
            [request.project_id.as_str()], |row| row.get::<_, String>(0),
        ).optional().map_err(storage_error)?.ok_or(RepositoryError::BranchNotFound)?;
        if head != request.base_snapshot_id.as_str() {
            return Err(RepositoryError::InvalidContent(
                "proposal base is not the current main snapshot".into(),
            ));
        }
        let mut operations = Vec::with_capacity(request.operations.len());
        let mut objects = BTreeSet::new();
        for proposed in request.operations {
            let row = transaction.query_row(
                "SELECT r.object_id, r.object_kind, r.revision_id, r.project_revision, r.checks_json, r.status, h.candidate_revision_id, h.accepted_revision_id, h.locked FROM story_reviews_v2 r JOIN story_object_heads_v2 h ON h.project_id = r.project_id AND h.object_id = r.object_id WHERE r.project_id = ?1 AND r.review_id = ?2",
                params![request.project_id.as_str(), proposed.review_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, i64>(3)?, row.get::<_, String>(4)?, row.get::<_, String>(5)?, row.get::<_, Option<String>>(6)?, row.get::<_, Option<String>>(7)?, row.get::<_, bool>(8)?)),
            ).optional().map_err(storage_error)?.ok_or(RepositoryError::ReviewNotFound)?;
            if row.3 != revision || row.5 != "ready" || row.6.as_deref() != Some(&row.2) || row.8 {
                return Err(RepositoryError::InvalidContent(
                    "proposal review is stale or locked".into(),
                ));
            }
            let snapshot_revision = transaction.query_row(
                "SELECT revision_id FROM story_snapshot_members_v2 WHERE snapshot_id = ?1 AND object_id = ?2",
                params![request.base_snapshot_id.as_str(), row.0],
                |row| row.get::<_, String>(0),
            ).optional().map_err(storage_error)?;
            if snapshot_revision.as_deref() != Some(&row.2) {
                return Err(RepositoryError::InvalidContent(
                    "proposal source is not in the base snapshot".into(),
                ));
            }
            if !objects.insert(row.0.clone()) {
                return Err(RepositoryError::InvalidContent(
                    "proposal repeats an object".into(),
                ));
            }
            let checks: Value =
                serde_json::from_str(&row.4).map_err(|_| RepositoryError::CorruptState)?;
            if checks.as_array().is_none_or(|items| {
                items
                    .iter()
                    .any(|check| check["blocking"] == true && check["passed"] != true)
            }) {
                return Err(RepositoryError::ReviewChecksFailed);
            }
            operations.push(BoundOperation {
                operation_id: proposed.operation_id.clone(),
                review_id: proposed.review_id.clone(),
                object_id: row.0,
                object_kind: row.1,
                candidate_revision_id: row.2,
                expected_accepted_revision_id: row.7,
                depends_on: proposed.depends_on.clone(),
            });
        }
        let id = format!("proposal-set-{}", uuid::Uuid::new_v4().simple());
        let operations_json =
            serde_json::to_string(&operations).map_err(|_| RepositoryError::CorruptState)?;
        if operations_json.len() > 32_000 {
            return Err(RepositoryError::InvalidContent(
                "proposal set too large".into(),
            ));
        }
        transaction.execute(
            "INSERT INTO story_proposal_sets_v1(proposal_set_id, project_id, branch_id, base_snapshot_id, project_revision, intent, operations_json, idempotency_key, request_digest, created_at) VALUES (?1, ?2, 'main', ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![id, request.project_id.as_str(), request.base_snapshot_id.as_str(), revision, request.intent, operations_json, request.idempotency_key, digest, request.now],
        ).map_err(storage_error)?;
        transaction.commit().map_err(storage_error)?;
        Ok(
            json!({"id": id, "baseSnapshotId": request.base_snapshot_id.as_str(), "replayed": false}),
        )
    }

    /// Reports which selected reviews could be adopted together; it never changes authority.
    fn preview_proposal_set(
        &self,
        project_id: &ProjectId,
        proposal_set_id: &str,
        selected: &[String],
    ) -> Result<Value, RepositoryError> {
        self.with_read_snapshot(|| self.proposal_set_report(project_id, proposal_set_id, selected))?
    }
}

use crate::ReadSnapshotRepository;
