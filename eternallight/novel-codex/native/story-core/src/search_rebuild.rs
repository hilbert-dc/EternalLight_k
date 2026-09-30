use std::collections::BTreeMap;

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

use crate::search_index::{grams, index_revision, revision_index_fields};
use crate::{
    ObjectId, ProjectId, RepositoryError, RevisionId, RevisionRecord, SqliteStoryRepository,
};

const MAX_REPORTED_MISMATCHES: usize = 20;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SearchIndexAudit {
    pub revision_count: usize,
    pub document_count: usize,
    pub gram_count: usize,
    pub mismatch_count: usize,
    pub mismatched_revision_ids: Vec<String>,
    pub orphaned_documents: usize,
    pub orphaned_grams: usize,
    pub index_generation: i64,
}

impl SearchIndexAudit {
    pub fn complete(&self) -> bool {
        self.mismatch_count == 0 && self.orphaned_documents == 0 && self.orphaned_grams == 0
    }
}

impl SqliteStoryRepository {
    pub fn search_index_generation(&self, project_id: &ProjectId) -> Result<i64, RepositoryError> {
        index_generation_on(&self.connection, project_id)
    }

    /// Reconstruct the expected index from every immutable revision and compare it to the
    /// incremental index. Callers must inspect `complete`; normal search does not run this audit.
    pub fn audit_search_index(
        &self,
        project_id: &ProjectId,
    ) -> Result<SearchIndexAudit, RepositoryError> {
        self.project_revision(project_id)?;
        let read = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let mut audit = SearchIndexAudit {
            revision_count: 0,
            document_count: 0,
            gram_count: 0,
            mismatch_count: 0,
            mismatched_revision_ids: Vec::new(),
            orphaned_documents: 0,
            orphaned_grams: 0,
            index_generation: index_generation_on(&read, project_id)?,
        };
        let mut revisions = read
            .prepare("SELECT revision_id, object_id, content_json FROM story_revisions_v2 WHERE project_id = ?1 ORDER BY rowid")
            .map_err(storage_error)?;
        let rows = revisions
            .query_map([project_id.as_str()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(storage_error)?;
        for row in rows {
            let (revision_id, object_id, content_json) = row.map_err(storage_error)?;
            audit_revision_on(
                &read,
                project_id,
                &revision_id,
                &object_id,
                &content_json,
                &mut audit,
            )?;
        }
        drop(revisions);
        audit_orphans_on(&read, project_id, &mut audit)?;
        read.commit().map_err(storage_error)?;
        Ok(audit)
    }

    /// Rebuild derived search rows only; immutable story revisions and snapshots are untouched.
    /// The generation bump invalidates search cursors created before the rebuild.
    pub fn rebuild_search_index(
        &mut self,
        project_id: &ProjectId,
        expected_index_generation: i64,
    ) -> Result<SearchIndexAudit, RepositoryError> {
        self.project_revision(project_id)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        if index_generation_on(&transaction, project_id)? != expected_index_generation {
            return Err(RepositoryError::InvalidContent(
                "search index generation changed".into(),
            ));
        }
        let next_generation = expected_index_generation.checked_add(1).ok_or_else(|| {
            RepositoryError::InvalidContent("search index generation exhausted".into())
        })?;
        transaction
            .execute(
                "DELETE FROM story_search_grams_v2 WHERE project_id = ?1",
                [project_id.as_str()],
            )
            .map_err(storage_error)?;
        transaction
            .execute(
                "DELETE FROM story_search_documents_v2 WHERE project_id = ?1",
                [project_id.as_str()],
            )
            .map_err(storage_error)?;
        let mut revisions = transaction.prepare(
            "SELECT object_id, object_kind, revision_id, parent_revision_id, content_digest, content_json FROM story_revisions_v2 WHERE project_id = ?1 ORDER BY rowid",
        ).map_err(storage_error)?;
        let rows = revisions
            .query_map([project_id.as_str()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })
            .map_err(storage_error)?;
        for row in rows {
            let (
                object_id,
                object_kind,
                revision_id,
                parent_revision_id,
                content_digest,
                content_json,
            ) = row.map_err(storage_error)?;
            let revision = RevisionRecord {
                project_id: project_id.clone(),
                object_id: ObjectId::parse(object_id).map_err(|_| RepositoryError::CorruptState)?,
                object_kind,
                revision_id: RevisionId::parse(revision_id)
                    .map_err(|_| RepositoryError::CorruptState)?,
                parent_revision_id: parent_revision_id
                    .map(RevisionId::parse)
                    .transpose()
                    .map_err(|_| RepositoryError::CorruptState)?,
                content_digest,
                content_json,
            };
            index_revision(&transaction, &revision)?;
        }
        drop(revisions);
        transaction.execute(
            "INSERT INTO story_search_index_state_v1(project_id, generation) VALUES (?1, ?2) ON CONFLICT(project_id) DO UPDATE SET generation = excluded.generation",
            params![project_id.as_str(), next_generation],
        ).map_err(storage_error)?;
        transaction.commit().map_err(storage_error)?;
        self.audit_search_index(project_id)
    }
}

pub(crate) fn index_generation_on(
    connection: &Connection,
    project_id: &ProjectId,
) -> Result<i64, RepositoryError> {
    connection
        .query_row(
            "SELECT generation FROM story_search_index_state_v1 WHERE project_id = ?1",
            [project_id.as_str()],
            |row| row.get(0),
        )
        .optional()
        .map_err(storage_error)
        .map(|generation| generation.unwrap_or(0))
}

pub(crate) fn audit_revision_on(
    connection: &Connection,
    project_id: &ProjectId,
    revision_id: &str,
    object_id: &str,
    content_json: &str,
    audit: &mut SearchIndexAudit,
) -> Result<(), RepositoryError> {
    audit.revision_count += 1;
    let expected = revision_index_fields(content_json)?
        .into_iter()
        .collect::<BTreeMap<_, _>>();
    let mut documents = connection.prepare(
        "SELECT doc_id, field_path, text FROM story_search_documents_v2 WHERE project_id = ?1 AND revision_id = ?2 AND object_id = ?3 ORDER BY field_path",
    ).map_err(storage_error)?;
    let actual = documents
        .query_map(
            params![project_id.as_str(), revision_id, object_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .map_err(storage_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(storage_error)?;
    audit.document_count += actual.len();
    let mut mismatch = actual.len() != expected.len();
    for (doc_id, field_path, text) in actual {
        if expected.get(&field_path) != Some(&text) {
            mismatch = true;
        }
        let mut gram_statement = connection.prepare(
            "SELECT gram FROM story_search_grams_v2 WHERE project_id = ?1 AND doc_id = ?2 ORDER BY gram",
        ).map_err(storage_error)?;
        let actual_grams = gram_statement
            .query_map(params![project_id.as_str(), doc_id], |row| {
                row.get::<_, String>(0)
            })
            .map_err(storage_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage_error)?;
        audit.gram_count += actual_grams.len();
        if actual_grams != grams(&text.to_lowercase()) {
            mismatch = true;
        }
    }
    if mismatch {
        audit.mismatch_count += 1;
        if audit.mismatched_revision_ids.len() < MAX_REPORTED_MISMATCHES {
            audit.mismatched_revision_ids.push(revision_id.to_owned());
        }
    }
    Ok(())
}

pub(crate) fn audit_orphans_on(
    connection: &Connection,
    project_id: &ProjectId,
    audit: &mut SearchIndexAudit,
) -> Result<(), RepositoryError> {
    audit.orphaned_documents = connection.query_row(
        "SELECT COUNT(*) FROM story_search_documents_v2 d LEFT JOIN story_revisions_v2 r ON r.revision_id = d.revision_id AND r.project_id = d.project_id AND r.object_id = d.object_id WHERE d.project_id = ?1 AND r.revision_id IS NULL",
        [project_id.as_str()], |row| row.get::<_, i64>(0),
    ).map_err(storage_error)? as usize;
    audit.orphaned_grams = connection.query_row(
        "SELECT COUNT(*) FROM story_search_grams_v2 g LEFT JOIN story_search_documents_v2 d ON d.doc_id = g.doc_id AND d.project_id = g.project_id WHERE g.project_id = ?1 AND d.doc_id IS NULL",
        [project_id.as_str()], |row| row.get::<_, i64>(0),
    ).map_err(storage_error)? as usize;
    Ok(())
}

fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}

#[cfg(test)]
#[path = "search_rebuild_tests.rs"]
mod tests;
