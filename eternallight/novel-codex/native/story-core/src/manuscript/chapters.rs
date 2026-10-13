use rusqlite::OptionalExtension;
use rusqlite::params;
use serde_json::Value;

use crate::ProjectId;
use crate::RepositoryError;
use crate::SqliteStoryRepository;

#[path = "chapter_rebind.rs"]
mod rebind;
pub use rebind::{RebindChapterPlanRequest, ReboundChapterPlan};

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ChapterBinding {
    pub plan_revision_id: String,
    pub chapter_plan_id: String,
    pub chapter_id: String,
    pub chapter_number: i64,
    pub result_revision: i64,
}

pub struct MaterializeStandaloneChapterRequest<'a> {
    pub project_id: &'a ProjectId,
    pub title: &'a str,
    pub volume_id: Option<&'a str>,
    pub expected_revision: i64,
    pub idempotency_key: &'a str,
    pub request_digest: &'a str,
    pub now: i64,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct StandaloneChapterReceipt {
    #[serde(flatten)]
    pub binding: ChapterBinding,
    pub catalog_revision: i64,
    pub volume_id: Option<String>,
}

pub struct AssociateChapterPlanRequest<'a> {
    pub project_id: &'a ProjectId,
    pub chapter_id: &'a str,
    pub chapter_plan_id: &'a str,
    pub expected_revision: i64,
    pub idempotency_key: &'a str,
    pub request_digest: &'a str,
    pub now: i64,
}



fn validate_plan_entry(
    connection: &rusqlite::Connection,
    project_id: &ProjectId,
    plan_revision_id: &str,
    chapter_plan_id: &str,
    chapter_number: i64,
) -> Result<(), RepositoryError> {
    let content: String = connection.query_row(
        "SELECT content_json FROM story_revisions_v2 WHERE project_id=?1 AND revision_id=?2 AND object_id='artifact-plan' AND object_kind='plan'",
        params![project_id.as_str(),plan_revision_id], |row| row.get(0),
    ).map_err(storage_error)?;
    let envelope: Value =
        serde_json::from_str(&content).map_err(|_| RepositoryError::CorruptState)?;
    let chapters = envelope
        .get("payload")
        .and_then(|payload| payload.get("chapters"))
        .and_then(Value::as_array)
        .ok_or(RepositoryError::InvalidChapter)?;
    let mut matching = chapters
        .iter()
        .filter(|entry| entry["chapterPlanId"].as_str() == Some(chapter_plan_id));
    let entry = matching.next().ok_or(RepositoryError::CandidateNotFound)?;
    if matching.next().is_some() || entry["chapterNumber"].as_i64() != Some(chapter_number) {
        return Err(RepositoryError::InvalidChapter);
    }
    crate::bound_writer_plan::validate_plan_position(entry)
        .map_err(|_| RepositoryError::InvalidChapter)?;
    Ok(())
}

fn accepted_revision(
    connection: &rusqlite::Connection,
    project_id: &ProjectId,
    object_id: &str,
) -> Result<Option<String>, RepositoryError> {
    connection.query_row(
        "SELECT accepted_revision_id FROM story_object_heads_v2 WHERE project_id = ?1 AND object_id = ?2",
        params![project_id.as_str(), object_id], |row| row.get(0),
    ).optional().map_err(storage_error).map(Option::flatten)
}

fn assert_revision(
    connection: &rusqlite::Connection,
    project_id: &ProjectId,
    expected: i64,
) -> Result<(), RepositoryError> {
    let actual = connection
        .query_row(
            "SELECT revision FROM story_workspaces_v2 WHERE project_id = ?1",
            [project_id.as_str()],
            |row| row.get(0),
        )
        .optional()
        .map_err(storage_error)?
        .ok_or(RepositoryError::ProjectNotFound)?;
    if actual != expected {
        return Err(RepositoryError::StaleProjectRevision { expected, actual });
    }
    Ok(())
}

fn finish(
    connection: &rusqlite::Connection,
    project_id: &ProjectId,
    operation: &str,
    key: &str,
    digest: &str,
    result: &ChapterBinding,
    now: i64,
) -> Result<(), RepositoryError> {
    let result_json = serde_json::to_string(&serde_json::json!({
        "planRevisionId": result.plan_revision_id, "chapterPlanId": result.chapter_plan_id,
        "chapterId": result.chapter_id, "chapterNumber": result.chapter_number, "resultRevision": result.result_revision
    })).map_err(|error| RepositoryError::InvalidContent(error.to_string()))?;
    connection
        .execute(
            "UPDATE story_workspaces_v2 SET revision = ?1 WHERE project_id = ?2",
            params![result.result_revision, project_id.as_str()],
        )
        .map_err(storage_error)?;
    cancel_active_tasks_for_chapter_structure(connection, project_id, now)?;
    connection.execute("INSERT INTO story_domain_receipts_v2(project_id, operation, idempotency_key, request_digest, result_json) VALUES (?1, ?2, ?3, ?4, ?5)", params![project_id.as_str(), operation, key, digest, result_json]).map_err(storage_error)?;
    Ok(())
}

pub(crate) fn cancel_active_tasks_for_chapter_structure(
    connection: &rusqlite::Connection,
    project_id: &ProjectId,
    now: i64,
) -> Result<(), RepositoryError> {
    let active_ids = {
        let mut statement = connection
            .prepare(
                "SELECT task_id FROM story_tasks_v2 WHERE project_id = ?1 AND status IN ('queued','running','waiting')",
            )
            .map_err(storage_error)?;
        statement
            .query_map([project_id.as_str()], |row| row.get::<_, String>(0))
            .map_err(storage_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage_error)?
    };
    for task_id in active_ids {
        connection
            .execute(
                "UPDATE story_tasks_v2 SET status = 'cancelled', stage = 'cancelled', delivery_status = CASE WHEN output_refs_json = '[]' THEN 'pending' ELSE 'incomplete' END, error = 'chapter structure changed', capability_revoked_at = ?1 WHERE task_id = ?2",
                params![now, task_id],
            )
            .map_err(storage_error)?;
        let task = crate::tasks::load_task(connection, &task_id)?;
        crate::work_tasks::finish_attempt(connection, &task)?;
    }
    Ok(())
}

fn replay(
    connection: &rusqlite::Connection,
    project_id: &ProjectId,
    operation: &str,
    key: &str,
    digest: &str,
) -> Result<Option<Value>, RepositoryError> {
    let row = connection.query_row(
        "SELECT request_digest, result_json FROM story_domain_receipts_v2 WHERE project_id = ?1 AND operation = ?2 AND idempotency_key = ?3",
        params![project_id.as_str(), operation, key], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
    ).optional().map_err(storage_error)?;
    let Some((stored, result)) = row else {
        return Ok(None);
    };
    if stored != digest {
        return Err(RepositoryError::IdempotencyConflict);
    }
    serde_json::from_str(&result)
        .map(Some)
        .map_err(|_| RepositoryError::CorruptState)
}

fn parse_binding(value: Value) -> Result<ChapterBinding, RepositoryError> {
    Ok(ChapterBinding {
        plan_revision_id: value
            .get("planRevisionId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .into(),
        chapter_plan_id: value
            .get("chapterPlanId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .into(),
        chapter_id: value
            .get("chapterId")
            .and_then(Value::as_str)
            .ok_or(RepositoryError::CorruptState)?
            .into(),
        chapter_number: value
            .get("chapterNumber")
            .and_then(Value::as_i64)
            .ok_or(RepositoryError::CorruptState)?,
        result_revision: value
            .get("resultRevision")
            .and_then(Value::as_i64)
            .ok_or(RepositoryError::CorruptState)?,
    })
}

fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}

#[cfg(test)]
#[path = "chapters_tests.rs"]
mod tests;

/// Public chapters capability; SQLite retains transaction ownership.
pub trait ChaptersRepository {
    fn materialize_standalone_chapter(
        &mut self,
        request: MaterializeStandaloneChapterRequest<'_>,
    ) -> Result<StandaloneChapterReceipt, RepositoryError>;
    fn materialize_chapter_from_plan(
        &mut self,
        project_id: &ProjectId,
        chapter_plan_id: &str,
        expected_revision: i64,
        idempotency_key: &str,
        request_digest: &str,
        now: i64,
    ) -> Result<ChapterBinding, RepositoryError>;
    fn associate_chapter_plan(
        &mut self,
        request: AssociateChapterPlanRequest<'_>,
    ) -> Result<ChapterBinding, RepositoryError>;
    fn chapter_bindings(
        &self,
        project_id: &ProjectId,
    ) -> Result<Vec<ChapterBinding>, RepositoryError>;
}

impl ChaptersRepository for SqliteStoryRepository {
    fn materialize_standalone_chapter(
        &mut self,
        request: MaterializeStandaloneChapterRequest<'_>,
    ) -> Result<StandaloneChapterReceipt, RepositoryError> {
        let MaterializeStandaloneChapterRequest {
            project_id,
            title,
            volume_id,
            expected_revision,
            idempotency_key,
            request_digest,
            now,
        } = request;
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        if let Some(result) = replay(
            &transaction,
            project_id,
            "materialize-standalone",
            idempotency_key,
            request_digest,
        )? {
            return serde_json::from_value(result).map_err(|_| RepositoryError::CorruptState);
        }
        assert_revision(&transaction, project_id, expected_revision)?;
        if volume_id.is_some() {
            crate::manuscript_catalog::assert_writable(&transaction, project_id)?;
        }
        let number = transaction
            .query_row(
                "SELECT COALESCE(MAX(number), 0) + 1 FROM story_chapters_v2 WHERE project_id = ?1",
                [project_id.as_str()],
                |row| row.get::<_, i64>(0),
            )
            .map_err(storage_error)?;
        let chapter_id = format!("chapter-{}", uuid::Uuid::new_v4().simple());
        let title = if title.trim().is_empty() {
            format!("第 {number} 章")
        } else {
            title.trim().chars().take(160).collect()
        };
        transaction.execute(
            "INSERT INTO story_chapters_v2(chapter_id, project_id, number, title, status, summary) VALUES (?1, ?2, ?3, ?4, 'draft', '')",
            params![chapter_id, project_id.as_str(), number, title],
        ).map_err(storage_error)?;
        if let Some(volume_id) = volume_id {
            crate::manuscript_catalog::place_new_chapter(
                &transaction,
                project_id,
                &chapter_id,
                volume_id,
            )?;
            crate::manuscript_catalog::advance_revision(&transaction, project_id)?;
        }
        let result = StandaloneChapterReceipt {
            binding: ChapterBinding {
                plan_revision_id: String::new(),
                chapter_plan_id: String::new(),
                chapter_id,
                chapter_number: number,
                result_revision: expected_revision + 1,
            },
            catalog_revision: crate::manuscript_catalog::read_catalog(&transaction, project_id)?
                .revision,
            volume_id: volume_id.map(str::to_owned),
        };
        finish(
            &transaction,
            project_id,
            "materialize-standalone",
            idempotency_key,
            request_digest,
            &result.binding,
            now,
        )?;
        transaction.execute("UPDATE story_domain_receipts_v2 SET result_json=?1 WHERE project_id=?2 AND operation='materialize-standalone' AND idempotency_key=?3",
            params![serde_json::to_string(&result).map_err(|_| RepositoryError::CorruptState)?,project_id.as_str(),idempotency_key]).map_err(storage_error)?;
        transaction.commit().map_err(storage_error)?;
        Ok(result)
    }

    fn materialize_chapter_from_plan(
        &mut self,
        project_id: &ProjectId,
        chapter_plan_id: &str,
        expected_revision: i64,
        idempotency_key: &str,
        request_digest: &str,
        now: i64,
    ) -> Result<ChapterBinding, RepositoryError> {
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        if let Some(result) = replay(
            &transaction,
            project_id,
            "materialize-chapter",
            idempotency_key,
            request_digest,
        )? {
            return parse_binding(result);
        }
        assert_revision(&transaction, project_id, expected_revision)?;
        let plan_revision_id = accepted_revision(&transaction, project_id, "artifact-plan")?
            .ok_or(RepositoryError::CandidateNotFound)?;
        let content = transaction.query_row(
            "SELECT content_json FROM story_revisions_v2 WHERE project_id = ?1 AND revision_id = ?2",
            params![project_id.as_str(), plan_revision_id], |row| row.get::<_, String>(0),
        ).map_err(storage_error)?;
        let envelope: Value = serde_json::from_str(&content)
            .map_err(|error| RepositoryError::InvalidContent(error.to_string()))?;
        let payload = envelope.get("payload").unwrap_or(&envelope);
        let entry = payload
            .get("chapters")
            .and_then(Value::as_array)
            .and_then(|chapters| {
                chapters.iter().find(|chapter| {
                    chapter.get("chapterPlanId").and_then(Value::as_str) == Some(chapter_plan_id)
                })
            })
            .ok_or(RepositoryError::CandidateNotFound)?;
        let number = entry
            .get("chapterNumber")
            .and_then(Value::as_i64)
            .filter(|value| *value > 0)
            .ok_or(RepositoryError::InvalidChapter)?;
        crate::bound_writer_plan::validate_plan_position(entry)
            .map_err(|_| RepositoryError::InvalidChapter)?;
        let title = entry
            .get("publicTitle")
            .or_else(|| entry.get("title"))
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| format!("第 {number} 章"));
        let chapter_id = format!("chapter-{}", uuid::Uuid::new_v4().simple());
        transaction.execute(
            "INSERT INTO story_chapters_v2(chapter_id, project_id, number, title, status, summary) VALUES (?1, ?2, ?3, ?4, 'draft', '')",
            params![chapter_id, project_id.as_str(), number, title],
        ).map_err(storage_error)?;
        transaction.execute(
            "INSERT INTO story_chapter_plan_bindings_v2(project_id, plan_revision_id, chapter_plan_id, chapter_id, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![project_id.as_str(), plan_revision_id, chapter_plan_id, chapter_id, now],
        ).map_err(storage_error)?;
        let result = ChapterBinding {
            plan_revision_id,
            chapter_plan_id: chapter_plan_id.into(),
            chapter_id,
            chapter_number: number,
            result_revision: expected_revision + 1,
        };
        finish(
            &transaction,
            project_id,
            "materialize-chapter",
            idempotency_key,
            request_digest,
            &result,
            now,
        )?;
        transaction.commit().map_err(storage_error)?;
        Ok(result)
    }

    fn associate_chapter_plan(
        &mut self,
        request: AssociateChapterPlanRequest<'_>,
    ) -> Result<ChapterBinding, RepositoryError> {
        let AssociateChapterPlanRequest {
            project_id,
            chapter_id,
            chapter_plan_id,
            expected_revision,
            idempotency_key,
            request_digest,
            now,
        } = request;
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        if let Some(result) = replay(
            &transaction,
            project_id,
            "associate-plan",
            idempotency_key,
            request_digest,
        )? {
            return parse_binding(result);
        }
        assert_revision(&transaction, project_id, expected_revision)?;
        let number = transaction
            .query_row(
                "SELECT number FROM story_chapters_v2 WHERE project_id = ?1 AND chapter_id = ?2",
                params![project_id.as_str(), chapter_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(storage_error)?
            .ok_or(RepositoryError::CandidateNotFound)?;
        let plan_revision_id = accepted_revision(&transaction, project_id, "artifact-plan")?
            .ok_or(RepositoryError::CandidateNotFound)?;
        validate_plan_entry(
            &transaction,
            project_id,
            &plan_revision_id,
            chapter_plan_id,
            number,
        )?;
        let occupied: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM story_chapter_plan_bindings_v2 WHERE project_id=?1 AND (chapter_id=?2 OR chapter_plan_id=?3))",
            params![project_id.as_str(),chapter_id,chapter_plan_id], |row| row.get(0),
        ).map_err(storage_error)?;
        if occupied {
            return Err(RepositoryError::InvalidContent(
                "chapter or plan entry is already associated".into(),
            ));
        }
        transaction.execute(
            "INSERT INTO story_chapter_plan_bindings_v2(project_id, plan_revision_id, chapter_plan_id, chapter_id, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![project_id.as_str(), plan_revision_id, chapter_plan_id, chapter_id, now],
        ).map_err(storage_error)?;
        let result = ChapterBinding {
            plan_revision_id,
            chapter_plan_id: chapter_plan_id.into(),
            chapter_id: chapter_id.into(),
            chapter_number: number,
            result_revision: expected_revision + 1,
        };
        finish(
            &transaction,
            project_id,
            "associate-plan",
            idempotency_key,
            request_digest,
            &result,
            now,
        )?;
        transaction.commit().map_err(storage_error)?;
        Ok(result)
    }

    fn chapter_bindings(
        &self,
        project_id: &ProjectId,
    ) -> Result<Vec<ChapterBinding>, RepositoryError> {
        let revision = self.project_revision(project_id)?;
        let mut statement = self.connection.prepare(
            "SELECT b.plan_revision_id, b.chapter_plan_id, b.chapter_id, c.number FROM story_chapter_plan_bindings_v2 b JOIN story_chapters_v2 c ON c.chapter_id = b.chapter_id WHERE b.project_id = ?1 ORDER BY c.number",
        ).map_err(storage_error)?;
        statement
            .query_map([project_id.as_str()], |row| {
                Ok(ChapterBinding {
                    plan_revision_id: row.get(0)?,
                    chapter_plan_id: row.get(1)?,
                    chapter_id: row.get(2)?,
                    chapter_number: row.get(3)?,
                    result_revision: revision,
                })
            })
            .map_err(storage_error)?
            .map(|row| row.map_err(storage_error))
            .collect()
    }
}

pub use rebind::ChapterRebindRepository;

use crate::ProjectRevisionRepository;
