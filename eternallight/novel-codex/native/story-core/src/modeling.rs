use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashMap};

use crate::knowledge_projection::relation_definition;
use crate::{ObjectId, ProjectId, RepositoryError, RevisionId, SqliteStoryRepository};

pub struct BeginModelingBatch<'a> {
    pub project_id: &'a ProjectId,
    pub object_id: &'a ObjectId,
    pub source_revision_id: &'a RevisionId,
    pub expected_revision: i64,
    pub extractor_version: &'a str,
    pub now: i64,
}

pub struct PublishModelingBatch<'a> {
    pub project_id: &'a ProjectId,
    pub batch_id: &'a str,
    pub task_id: &'a str,
    pub thread_id: &'a str,
    pub entities: &'a [Value],
    pub assertions: &'a [Value],
    pub now: i64,
}

pub struct ModelingBatch {
    pub id: String,
    pub object_id: String,
    pub source_revision_id: String,
    pub input_project_revision: i64,
    pub status: String,
    pub entities: Vec<Value>,
    pub assertions: Vec<Value>,
}

impl ModelingBatch {
    pub fn as_json(&self) -> Value {
        json!({
            "id": self.id, "objectId": self.object_id,
            "sourceRevisionId": self.source_revision_id,
            "inputProjectRevision": self.input_project_revision,
            "status": self.status, "entities": self.entities, "assertions": self.assertions,
            "confirmationState": "candidate"
        })
    }
}

impl SqliteStoryRepository {
    pub fn begin_modeling_batch(
        &mut self,
        request: BeginModelingBatch<'_>,
    ) -> Result<ModelingBatch, RepositoryError> {
        if request.extractor_version.is_empty() || request.extractor_version.len() > 80 {
            return Err(RepositoryError::InvalidContent(
                "extractor version is invalid".into(),
            ));
        }
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let revision = current_revision(&transaction, request.project_id)?;
        if revision != request.expected_revision {
            return Err(RepositoryError::StaleProjectRevision {
                expected: request.expected_revision,
                actual: revision,
            });
        }
        let source = selected_source(&transaction, request.project_id, request.object_id)?;
        if source.as_deref() != Some(request.source_revision_id.as_str()) {
            return Err(RepositoryError::StaleCandidate {
                expected: Some(request.source_revision_id.clone()),
                actual: source
                    .map(RevisionId::parse)
                    .transpose()
                    .map_err(|_| RepositoryError::CorruptState)?,
            });
        }
        let batch_id = format!("modeling-{}", uuid::Uuid::new_v4().simple());
        transaction.execute(
            "INSERT OR IGNORE INTO story_modeling_batches_v2(batch_id, project_id, object_id, source_revision_id, input_project_revision, extractor_version, status, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'queued', ?7)",
            params![batch_id, request.project_id.as_str(), request.object_id.as_str(), request.source_revision_id.as_str(), revision, request.extractor_version, request.now],
        ).map_err(storage_error)?;
        let id = transaction.query_row(
            "SELECT batch_id FROM story_modeling_batches_v2 WHERE project_id = ?1 AND object_id = ?2 AND source_revision_id = ?3 AND input_project_revision = ?4 AND extractor_version = ?5 AND status IN ('queued','ready')",
            params![request.project_id.as_str(), request.object_id.as_str(), request.source_revision_id.as_str(), revision, request.extractor_version],
            |row| row.get::<_, String>(0),
        ).map_err(storage_error)?;
        let batch = load_batch(&transaction, request.project_id, &id)?;
        transaction.commit().map_err(storage_error)?;
        Ok(batch)
    }

    pub fn publish_modeling_batch(
        &mut self,
        request: PublishModelingBatch<'_>,
    ) -> Result<ModelingBatch, RepositoryError> {
        if request.assertions.len() > 100 || request.entities.len() > 100 {
            return Err(RepositoryError::InvalidContent(
                "too many knowledge or entity candidates".into(),
            ));
        }
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let batch = load_batch(&transaction, request.project_id, request.batch_id)?;
        let assertions_json = serde_json::to_string(request.assertions)
            .map_err(|error| RepositoryError::InvalidContent(error.to_string()))?;
        let entities_json = serde_json::to_string(request.entities)
            .map_err(|error| RepositoryError::InvalidContent(error.to_string()))?;
        if batch.status != "queued" && batch.status != "ready" {
            return Err(RepositoryError::ReviewChecksFailed);
        }
        let revision = current_revision(&transaction, request.project_id)?;
        let object_id =
            ObjectId::parse(&batch.object_id).map_err(|_| RepositoryError::CorruptState)?;
        if revision != batch.input_project_revision
            || selected_source(&transaction, request.project_id, &object_id)?.as_deref()
                != Some(batch.source_revision_id.as_str())
        {
            transaction
                .execute(
                    "UPDATE story_modeling_batches_v2 SET status = 'stale' WHERE batch_id = ?1",
                    [request.batch_id],
                )
                .map_err(storage_error)?;
            let current_ready: i64 = transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM story_knowledge_heads_v2 h JOIN story_modeling_batches_v2 b ON b.project_id=h.project_id AND b.object_id=h.object_id AND b.batch_id=h.batch_id JOIN story_object_heads_v2 o ON o.project_id=h.project_id AND o.object_id=h.object_id WHERE h.project_id=?1 AND h.object_id=?2 AND b.status='ready' AND b.source_revision_id=COALESCE(o.candidate_revision_id,o.accepted_revision_id))",
                params![request.project_id.as_str(),batch.object_id], |row| row.get(0),
            ).map_err(storage_error)?;
            if current_ready == 0 {
                crate::modeling_outbox::enqueue_dependency(
                    &transaction,
                    request.project_id,
                    &batch.object_id,
                )?;
            }
            transaction.commit().map_err(storage_error)?;
            return Err(RepositoryError::ReviewChecksFailed);
        }
        let target_json = transaction.query_row(
            "SELECT target_json FROM story_tasks_v2 WHERE project_id = ?1 AND task_id = ?2 AND thread_id = ?3 AND operation = 'modelKnowledge' AND input_revision = ?4 AND status IN ('queued','running','waiting')",
            params![request.project_id.as_str(), request.task_id, request.thread_id, revision],
            |row| row.get::<_, String>(0),
        ).optional().map_err(storage_error)?.ok_or(RepositoryError::TaskBindingInvalid)?;
        let target: Value =
            serde_json::from_str(&target_json).map_err(|_| RepositoryError::CorruptState)?;
        if target["batchId"] != request.batch_id {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        if batch.status == "queued" {
            let task = crate::tasks::load_task(&transaction, request.task_id)?;
            crate::task_input::assert_task_inputs_current(&transaction, &task)?;
        }
        if batch.status == "ready" {
            let publisher: Option<String> = transaction
                .query_row(
                    "SELECT publisher_task_id FROM story_modeling_batches_v2 WHERE batch_id = ?1",
                    [request.batch_id],
                    |row| row.get(0),
                )
                .map_err(storage_error)?;
            if publisher.as_deref() != Some(request.task_id) {
                return Err(RepositoryError::TaskBindingInvalid);
            }
            if batch.assertions == request.assertions && batch.entities == request.entities {
                crate::recovery_operations::commit_operation(
                    &transaction,
                    self.recovery_operation.as_ref(),
                    request.project_id,
                    request.task_id,
                    &["publish_knowledge_batch"],
                    &format!("runtime:knowledge:{}", request.batch_id),
                )?;
                transaction.commit().map_err(storage_error)?;
                return Ok(batch);
            }
            return Err(RepositoryError::IdempotencyConflict);
        }
        let content = transaction.query_row(
            "SELECT content_json FROM story_revisions_v2 WHERE project_id = ?1 AND object_id = ?2 AND revision_id = ?3",
            params![request.project_id.as_str(), batch.object_id, batch.source_revision_id],
            |row| row.get::<_, String>(0),
        ).map_err(storage_error)?;
        let draft: Value =
            serde_json::from_str(&content).map_err(|_| RepositoryError::CorruptState)?;
        let body = draft["body"]
            .as_str()
            .ok_or(RepositoryError::CorruptState)?;
        let chars = body.chars().collect::<Vec<_>>();
        if let Err(error) = validate_payload(request.entities, request.assertions, &chars) {
            if matches!(error, RepositoryError::InvalidContent(_)) {
                crate::recovery_operations::reject_operation(
                    &transaction,
                    self.recovery_operation.as_ref(),
                    request.project_id,
                    request.task_id,
                    "publish_knowledge_batch",
                )?;
                transaction.commit().map_err(storage_error)?;
            }
            return Err(error);
        }
        crate::event_sources::validate_batch(
            &transaction,
            request.project_id,
            crate::model_sources::ModelSourceScope::Current,
            request.batch_id,
            request.assertions,
            request.entities,
        )?;
        for entity in request.entities {
            let id = entity["id"].as_str().ok_or(RepositoryError::CorruptState)?;
            let kind = entity["kind"]
                .as_str()
                .ok_or(RepositoryError::CorruptState)?;
            let active_kind = transaction.query_row(
                "SELECT c.kind FROM story_entity_claims_v2 c JOIN story_knowledge_heads_v2 h ON h.project_id = c.project_id AND h.batch_id = c.batch_id JOIN story_modeling_batches_v2 b ON b.batch_id = c.batch_id WHERE c.project_id = ?1 AND c.entity_id = ?2 AND h.object_id != ?3 AND b.status = 'ready' LIMIT 1",
                params![request.project_id.as_str(), id, batch.object_id],
                |row| row.get::<_, String>(0),
            ).optional().map_err(storage_error)?;
            if active_kind.as_deref().is_some_and(|active| active != kind) {
                return Err(RepositoryError::InvalidContent(
                    "entity identity has a conflicting kind in another current chapter".into(),
                ));
            }
        }
        for assertion in request.assertions {
            if let Some(source_batch_id) = assertion["propositionSourceBatchId"].as_str() {
                if source_batch_id == request.batch_id {
                    return Err(RepositoryError::InvalidContent(
                        "external proposition source must be a different batch".into(),
                    ));
                }
                let proposition_id = assertion["propositionId"]
                    .as_str()
                    .ok_or(RepositoryError::CorruptState)?;
                let valid: i64 = transaction.query_row(
                    "SELECT EXISTS(SELECT 1 FROM story_modeling_batches_v2 source \
                     JOIN story_chapters_v2 prior ON prior.project_id = source.project_id AND source.object_id = 'draft-' || prior.chapter_id \
                     JOIN story_chapters_v2 current ON current.project_id = source.project_id AND ?3 = 'draft-' || current.chapter_id \
                     WHERE source.project_id = ?1 AND source.batch_id = ?2 AND prior.number < current.number)",
                    params![request.project_id.as_str(), source_batch_id, batch.object_id],
                    |row| row.get(0),
                ).map_err(storage_error)?;
                if valid == 0
                    || !crate::proposition_sources::proposition_source_visible(
                        &transaction,
                        request.project_id,
                        crate::model_sources::ModelSourceScope::Current,
                        source_batch_id,
                        proposition_id,
                    )?
                {
                    return Err(RepositoryError::InvalidContent(
                        "external proposition source is not an active prior chapter".into(),
                    ));
                }
            }
        }
        transaction.execute(
            "UPDATE story_modeling_batches_v2 SET status = 'stale' WHERE project_id = ?1 AND object_id = ?2 AND status = 'ready'",
            params![request.project_id.as_str(), batch.object_id],
        ).map_err(storage_error)?;
        transaction.execute(
            "UPDATE story_modeling_batches_v2 SET status = 'ready', assertions_json = ?1, entities_json = ?2, published_at = ?3, publisher_task_id = ?4 WHERE batch_id = ?5",
            params![assertions_json, entities_json, request.now, request.task_id, request.batch_id],
        ).map_err(storage_error)?;
        for entity in request.entities {
            transaction.execute(
                "INSERT INTO story_entity_claims_v2(project_id, batch_id, entity_id, kind, display_name, aliases_json, evidence_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![request.project_id.as_str(), request.batch_id, entity["id"].as_str(), entity["kind"].as_str(), entity["displayName"].as_str(), entity["aliases"].to_string(), entity["evidence"].to_string()],
            ).map_err(storage_error)?;
            let entity_id = entity["id"].as_str().ok_or(RepositoryError::CorruptState)?;
            let display_name = entity["displayName"]
                .as_str()
                .ok_or(RepositoryError::CorruptState)?;
            let aliases = entity["aliases"]
                .as_array()
                .ok_or(RepositoryError::CorruptState)?;
            let names = std::iter::once(display_name)
                .chain(aliases.iter().filter_map(Value::as_str))
                .map(|name| name.trim().to_lowercase())
                .filter(|name| !name.is_empty())
                .collect::<BTreeSet<_>>();
            for name in names {
                transaction.execute(
                    "INSERT INTO story_entity_names_v2(project_id, batch_id, entity_id, name) VALUES (?1, ?2, ?3, ?4)",
                    params![request.project_id.as_str(), request.batch_id, entity_id, name],
                ).map_err(storage_error)?;
            }
        }
        for (index, assertion) in request.assertions.iter().enumerate() {
            transaction.execute(
                "INSERT INTO story_assertion_claims_v1(project_id, batch_id, assertion_index, subject_id, target_id, owner_id, assertion_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![request.project_id.as_str(), request.batch_id, index as i64,
                    assertion["subjectId"].as_str(), crate::state_properties::target_entity(assertion),
                    assertion["epistemicOwnerId"].as_str(), assertion.to_string()],
            ).map_err(storage_error)?;
            if assertion["kind"] == "relation" {
                transaction.execute(
                    "INSERT INTO story_relation_claims_v1(project_id, batch_id, assertion_index, subject_id, target_id, relation_type, assertion_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![request.project_id.as_str(), request.batch_id, index as i64,
                        assertion["subjectId"].as_str(), assertion["targetId"].as_str(),
                        assertion["relationType"].as_str(), assertion.to_string()],
                ).map_err(storage_error)?;
            } else if assertion["kind"] == "epistemic" {
                transaction.execute(
                    "INSERT INTO story_epistemic_claims_v1(project_id, batch_id, assertion_index, owner_id, proposition_id, proposition_source_batch_id, epistemic_state, epistemic_time, assertion_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![request.project_id.as_str(), request.batch_id, index as i64,
                        assertion["epistemicOwnerId"].as_str(), assertion["propositionId"].as_str(),
                        assertion["propositionSourceBatchId"].as_str().unwrap_or(request.batch_id),
                        assertion["epistemicState"].as_str(), assertion["epistemicTime"].as_i64(), assertion.to_string()],
                ).map_err(storage_error)?;
            }
        }
        transaction.execute(
            "INSERT INTO story_knowledge_heads_v2(project_id, object_id, batch_id) VALUES (?1, ?2, ?3) ON CONFLICT(project_id, object_id) DO UPDATE SET batch_id = excluded.batch_id",
            params![request.project_id.as_str(), batch.object_id, request.batch_id],
        ).map_err(storage_error)?;
        crate::knowledge_audit_jobs::bump_knowledge_generation(&transaction, request.project_id)?;
        transaction.execute(
            "UPDATE story_modeling_outbox_v1 SET status='satisfied',generation=generation+1,completed_batch_id=?1 WHERE project_id=?2 AND object_id=?3 AND source_revision_id=?4 AND status='pending'",
            params![request.batch_id,request.project_id.as_str(),batch.object_id,batch.source_revision_id],
        ).map_err(storage_error)?;
        let result = load_batch(&transaction, request.project_id, request.batch_id)?;
        crate::recovery_operations::commit_operation(
            &transaction,
            self.recovery_operation.as_ref(),
            request.project_id,
            request.task_id,
            &["publish_knowledge_batch"],
            &format!("runtime:knowledge:{}", request.batch_id),
        )?;
        transaction.commit().map_err(storage_error)?;
        Ok(result)
    }

    pub fn modeling_batch(
        &self,
        project_id: &ProjectId,
        batch_id: &str,
    ) -> Result<ModelingBatch, RepositoryError> {
        load_batch(&self.connection, project_id, batch_id)
    }

    pub fn current_knowledge(&self, project_id: &ProjectId) -> Result<Vec<Value>, RepositoryError> {
        let mut statement = self.connection.prepare(
            "SELECT object_id FROM story_object_heads_v2 WHERE project_id = ?1 AND object_kind = 'draft' ORDER BY object_id LIMIT 100",
        ).map_err(storage_error)?;
        let ids = statement
            .query_map([project_id.as_str()], |row| row.get::<_, String>(0))
            .map_err(storage_error)?
            .map(|row| row.map_err(storage_error))
            .collect::<Result<Vec<_>, _>>()?;
        self.knowledge_for_ids(project_id, ids)
    }

    pub fn chapter_knowledge(
        &self,
        project_id: &ProjectId,
        chapter_id: &str,
    ) -> Result<Value, RepositoryError> {
        self.knowledge_for_ids(project_id, vec![format!("draft-{chapter_id}")])?
            .into_iter()
            .next()
            .ok_or(RepositoryError::CorruptState)
    }

    pub fn writer_knowledge(
        &self,
        project_id: &ProjectId,
        reading_chapter: i64,
    ) -> Result<Vec<Value>, RepositoryError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT h.object_id FROM story_object_heads_v2 h JOIN story_chapters_v2 c
             ON c.project_id = h.project_id AND h.object_id = 'draft-' || c.chapter_id
             WHERE h.project_id = ?1 AND h.object_kind = 'draft' AND c.number <= ?2
             ORDER BY c.number DESC LIMIT 100",
            )
            .map_err(storage_error)?;
        let mut ids = statement
            .query_map(params![project_id.as_str(), reading_chapter], |row| {
                row.get::<_, String>(0)
            })
            .map_err(storage_error)?
            .map(|row| row.map_err(storage_error))
            .collect::<Result<Vec<_>, _>>()?;
        ids.reverse();
        self.knowledge_for_ids(project_id, ids)
    }

    pub(crate) fn knowledge_for_ids(
        &self,
        project_id: &ProjectId,
        ids: Vec<String>,
    ) -> Result<Vec<Value>, RepositoryError> {
        ids.into_iter().map(|id| {
            let object_id = ObjectId::parse(&id).map_err(|_| RepositoryError::CorruptState)?;
            let source = selected_source(&self.connection, project_id, &object_id)?;
            let batch_id = self.connection.query_row(
                "SELECT batch_id FROM story_knowledge_heads_v2 WHERE project_id = ?1 AND object_id = ?2",
                params![project_id.as_str(), id], |row| row.get::<_, String>(0),
            ).optional().map_err(storage_error)?;
            let batch = batch_id.map(|batch_id| load_batch(&self.connection, project_id, &batch_id)).transpose()?;
            let ready = batch.as_ref().is_some_and(|batch| batch.status == "ready"
                && source.as_deref() == Some(batch.source_revision_id.as_str()));
            Ok(json!({
                "objectId": id, "sourceRevisionId": source,
                "status": if ready { "ready" } else { "model_pending" },
                "batchId": if ready { batch.as_ref().map(|batch| &batch.id) } else { None },
                "entities": if ready { batch.as_ref().map(|batch| batch.entities.clone()).unwrap_or_default() } else { Vec::new() },
                "assertions": if ready { batch.map(|batch| batch.assertions).unwrap_or_default() } else { Vec::new() },
                "confirmationState": "candidate"
            }))
        }).collect()
    }
}

pub(crate) fn invalidate_source_modeling(
    connection: &rusqlite::Connection,
    project_id: &ProjectId,
    object_id: &ObjectId,
) -> Result<(), RepositoryError> {
    // A later chapter can refer to a proposition from this source batch. Its
    // candidate knowledge must become pending too, including transitive references.
    let mut statement = connection
        .prepare(
            "WITH RECURSIVE affected(batch_id) AS (
           SELECT batch_id FROM story_modeling_batches_v2
             WHERE project_id = ?1 AND object_id = ?2
           UNION
           SELECT claim.batch_id FROM story_model_dependencies_v1 claim
             JOIN affected source ON claim.source_batch_id = source.batch_id
             WHERE claim.project_id = ?1
         ) SELECT b.batch_id,b.object_id,b.status FROM affected a JOIN story_modeling_batches_v2 b ON b.batch_id=a.batch_id AND b.project_id=?1",
        )
        .map_err(storage_error)?;
    let affected = statement
        .query_map(params![project_id.as_str(), object_id.as_str()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(storage_error)?
        .map(|row| row.map_err(storage_error))
        .collect::<Result<Vec<_>, _>>()?;
    drop(statement);
    let mut dependent_objects = BTreeSet::new();
    for (batch_id, source_object, status) in affected {
        if source_object != object_id.as_str() && matches!(status.as_str(), "queued" | "ready") {
            dependent_objects.insert(source_object);
        }
        connection.execute(
            "UPDATE story_modeling_batches_v2 SET status = 'stale' WHERE project_id = ?1 AND batch_id = ?2 AND status IN ('queued','ready')",
            params![project_id.as_str(), batch_id],
        ).map_err(storage_error)?;
        connection
            .execute(
                "DELETE FROM story_knowledge_heads_v2 WHERE project_id = ?1 AND batch_id = ?2",
                params![project_id.as_str(), batch_id],
            )
            .map_err(storage_error)?;
    }
    connection
        .execute(
            "DELETE FROM story_knowledge_heads_v2 WHERE project_id = ?1 AND object_id = ?2",
            params![project_id.as_str(), object_id.as_str()],
        )
        .map_err(storage_error)?;
    crate::knowledge_audit_jobs::bump_knowledge_generation(connection, project_id)?;
    for object in dependent_objects {
        crate::modeling_outbox::enqueue_dependency(connection, project_id, &object)?;
    }
    Ok(())
}

fn selected_source(
    connection: &rusqlite::Connection,
    project_id: &ProjectId,
    object_id: &ObjectId,
) -> Result<Option<String>, RepositoryError> {
    connection.query_row(
        "SELECT COALESCE(candidate_revision_id, accepted_revision_id) FROM story_object_heads_v2 WHERE project_id = ?1 AND object_id = ?2 AND object_kind = 'draft'",
        params![project_id.as_str(), object_id.as_str()],
        |row| row.get::<_, Option<String>>(0),
    ).optional().map_err(storage_error).map(Option::flatten)
}

fn load_batch(
    connection: &rusqlite::Connection,
    project_id: &ProjectId,
    batch_id: &str,
) -> Result<ModelingBatch, RepositoryError> {
    let row = connection.query_row(
        "SELECT object_id, source_revision_id, input_project_revision, status, assertions_json, entities_json FROM story_modeling_batches_v2 WHERE project_id = ?1 AND batch_id = ?2",
        params![project_id.as_str(), batch_id],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?, row.get::<_, String>(3)?, row.get::<_, String>(4)?, row.get::<_, String>(5)?)),
    ).optional().map_err(storage_error)?.ok_or(RepositoryError::CandidateNotFound)?;
    let batch = ModelingBatch {
        id: batch_id.into(),
        object_id: row.0,
        source_revision_id: row.1,
        input_project_revision: row.2,
        status: row.3,
        assertions: serde_json::from_str(&row.4).map_err(|_| RepositoryError::CorruptState)?,
        entities: serde_json::from_str(&row.5).map_err(|_| RepositoryError::CorruptState)?,
    };
    crate::story_events::validate_batch(&batch.assertions, &batch.entities)
        .map_err(|_| RepositoryError::CorruptState)?;
    if batch.status == "ready" {
        crate::event_sources::validate_batch(
            connection,
            project_id,
            crate::model_sources::ModelSourceScope::Current,
            batch_id,
            &batch.assertions,
            &batch.entities,
        )
        .map_err(|_| RepositoryError::CorruptState)?;
    }
    Ok(batch)
}

/// Shared publication contract for exact source payloads; does not grant cross-source visibility.
pub(crate) fn validate_payload(
    entities: &[Value],
    assertions: &[Value],
    chars: &[char],
) -> Result<(), RepositoryError> {
    if entities.len() > 100 || assertions.len() > 100 {
        return Err(RepositoryError::InvalidContent(
            "too many knowledge or entity candidates".into(),
        ));
    }
    let mut kinds = HashMap::new();
    for entity in entities {
        validate_entity(entity, chars)?;
        let id = entity["id"].as_str().ok_or(RepositoryError::CorruptState)?;
        let kind = entity["kind"]
            .as_str()
            .ok_or(RepositoryError::CorruptState)?;
        if kinds.insert(id, kind).is_some() {
            return Err(RepositoryError::InvalidContent(
                "duplicate entity identity in modeling batch".into(),
            ));
        }
    }
    for assertion in assertions {
        validate_assertion(assertion, chars, &kinds)?;
    }
    crate::story_events::validate_batch(assertions, entities)
}

fn validate_entity(entity: &Value, chars: &[char]) -> Result<(), RepositoryError> {
    let Some(fields) = entity.as_object() else {
        return Err(RepositoryError::InvalidContent(
            "entity candidate must be an object".into(),
        ));
    };
    let id = entity["id"].as_str().unwrap_or_default();
    let kind = entity["kind"].as_str().unwrap_or_default();
    let name = entity["displayName"].as_str().unwrap_or_default();
    let aliases = entity.get("aliases").and_then(Value::as_array);
    if fields.keys().any(|key| {
        !matches!(
            key.as_str(),
            "id" | "kind" | "displayName" | "aliases" | "evidence"
        )
    }) || id.is_empty()
        || id.len() > 160
        || !matches!(
            kind,
            "character" | "organization" | "location" | "item" | "proposition" | "event"
        )
        || name.is_empty()
        || name.chars().count() > 160
        || aliases.is_some_and(|values| {
            values.len() > 8
                || values.iter().any(|value| {
                    value
                        .as_str()
                        .is_none_or(|alias| alias.is_empty() || alias.chars().count() > 80)
                })
        })
        || (entity.get("aliases").is_some() && aliases.is_none())
        || entity.to_string().len() > 4_000
    {
        return Err(RepositoryError::InvalidContent(
            "entity candidate is invalid".into(),
        ));
    }
    validate_evidence(&entity["evidence"], chars)
}

fn validate_assertion(
    assertion: &Value,
    chars: &[char],
    entities: &HashMap<&str, &str>,
) -> Result<(), RepositoryError> {
    let Some(fields) = assertion.as_object() else {
        return Err(RepositoryError::InvalidContent(
            "knowledge candidate must be an object".into(),
        ));
    };
    let kind = assertion["kind"].as_str().unwrap_or_default();
    if kind == "state" {
        return crate::state_properties::validate(assertion, chars, entities);
    }
    if kind == "event" {
        return crate::story_events::validate(assertion, chars);
    }
    let subject = assertion["subjectId"].as_str().unwrap_or_default();
    let predicate = assertion["predicate"].as_str().unwrap_or_default();
    let external_proposition = assertion["propositionSourceBatchId"].as_str();
    let relation_valid = assertion["targetId"].as_str().is_some_and(|target| {
        relation_definition(assertion["relationType"].as_str().unwrap_or_default()).is_some_and(
            |definition| {
                definition.category != "epistemic"
                    && entities
                        .get(subject)
                        .is_some_and(|kind| definition.source_kinds.contains(kind))
                    && entities
                        .get(target)
                        .is_some_and(|kind| definition.target_kinds.contains(kind))
            },
        )
    });
    if !matches!(kind, "assertion" | "relation" | "epistemic")
        || fields.keys().any(|key| {
            !matches!(
                key.as_str(),
                "kind"
                    | "subjectId"
                    | "predicate"
                    | "value"
                    | "evidence"
                    | "targetId"
                    | "relationType"
                    | "epistemicOwnerId"
                    | "epistemicState"
                    | "epistemicTime"
                    | "epistemicUntilExclusive"
                    | "epistemicUntilEvidence"
                    | "propositionId"
                    | "propositionSourceBatchId"
                    | "storyTime"
                    | "validFrom"
                    | "validUntilExclusive"
                    | "validUntilEvidence"
            )
        })
        || subject.is_empty()
        || subject.len() > 160
        || !(entities.contains_key(subject)
            || kind == "epistemic"
                && external_proposition.is_some()
                && assertion["propositionId"] == subject)
        || predicate.is_empty()
        || predicate.len() > 160
        || assertion.get("value").is_none()
        || assertion.to_string().len() > 8_000
        || (kind == "relation"
            && (!relation_valid
                || assertion["relationType"] != predicate
                || assertion["propositionId"]
                    .as_str()
                    .is_none_or(|id| entities.get(id) != Some(&"proposition"))))
        || (kind == "epistemic"
            && (assertion["epistemicOwnerId"]
                .as_str()
                .is_none_or(|id| id.is_empty() || id.len() > 160)
                || !matches!(
                    assertion["epistemicState"].as_str().unwrap_or_default(),
                    "known" | "suspected" | "false"
                )
                || assertion["propositionId"].as_str().is_none_or(|id| {
                    external_proposition.is_none() && entities.get(id) != Some(&"proposition")
                })
                || assertion["propositionId"] != subject
                || entities.get(assertion["epistemicOwnerId"].as_str().unwrap_or_default())
                    != Some(&"character")))
        || (assertion.get("propositionSourceBatchId").is_some()
            && (kind != "epistemic"
                || external_proposition.is_none_or(|id| id.is_empty() || id.len() > 160)
                || entities.contains_key(subject)))
        || (assertion.get("storyTime").is_some()
            && !assertion["storyTime"].is_i64()
            && !assertion["storyTime"].is_null())
        || (assertion.get("epistemicTime").is_some()
            && !assertion["epistemicTime"].is_i64()
            && !assertion["epistemicTime"].is_null())
        || (kind != "epistemic" && assertion.get("epistemicTime").is_some())
        || !crate::epistemic_time::epistemic_interval_valid(assertion)
        || (kind != "relation"
            && (assertion.get("validFrom").is_some()
                || assertion.get("validUntilExclusive").is_some()
                || assertion.get("validUntilEvidence").is_some()))
        || (kind == "relation"
            && (assertion
                .get("validFrom")
                .is_some_and(|value| !value.is_i64())
                || assertion
                    .get("validUntilExclusive")
                    .is_some_and(|value| !value.is_i64())
                || assertion["validFrom"]
                    .as_i64()
                    .zip(assertion["validUntilExclusive"].as_i64())
                    .is_some_and(|(start, end)| start >= end)
                || assertion["storyTime"]
                    .as_i64()
                    .zip(assertion["validFrom"].as_i64())
                    .is_some_and(|(time, start)| time != start)
                || assertion.get("validUntilExclusive").is_some()
                    != assertion.get("validUntilEvidence").is_some()))
    {
        return Err(RepositoryError::InvalidContent(
            "knowledge candidate identity or shape is invalid".into(),
        ));
    }
    validate_evidence(&assertion["evidence"], chars)?;
    if let Some(evidence) = assertion.get("epistemicUntilEvidence") {
        validate_evidence(evidence, chars)?;
    }
    if let Some(evidence) = assertion.get("validUntilEvidence") {
        validate_evidence(evidence, chars)?;
    }
    Ok(())
}

pub(crate) fn validate_evidence(evidence: &Value, chars: &[char]) -> Result<(), RepositoryError> {
    let fields = evidence.as_object().ok_or_else(|| {
        RepositoryError::InvalidContent("knowledge evidence must be an object".into())
    })?;
    if fields
        .keys()
        .any(|key| !matches!(key.as_str(), "quote" | "start" | "end"))
    {
        return Err(RepositoryError::InvalidContent(
            "knowledge evidence allows only quote, start and end; source identity is bound by the batch, not evidence fields".into(),
        ));
    }
    let quote = evidence["quote"].as_str().unwrap_or_default();
    let start = evidence["start"]
        .as_u64()
        .and_then(|value| usize::try_from(value).ok());
    let end = evidence["end"]
        .as_u64()
        .and_then(|value| usize::try_from(value).ok());
    let anchored = match (start, end) {
        (Some(start), Some(end)) if start < end && end <= chars.len() => {
            chars[start..end].iter().collect::<String>() == quote
        }
        _ => false,
    };
    if quote.is_empty() || quote.chars().count() > 2_000 || !anchored {
        return Err(RepositoryError::InvalidContent(
            "knowledge evidence does not match the source revision".into(),
        ));
    }
    Ok(())
}

fn current_revision(
    connection: &rusqlite::Connection,
    project_id: &ProjectId,
) -> Result<i64, RepositoryError> {
    connection
        .query_row(
            "SELECT revision FROM story_workspaces_v2 WHERE project_id = ?1",
            [project_id.as_str()],
            |row| row.get(0),
        )
        .optional()
        .map_err(storage_error)?
        .ok_or(RepositoryError::ProjectNotFound)
}

fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}
