use rusqlite::params;
use serde_json::{Value, json};

use crate::{ProjectId, RepositoryError, SqliteStoryRepository};

/// Scope of source metadata admitted to a context; claims require separate scoped queries.
pub enum ContextSourceScope {
    Project,
    ThroughChapter(i64),
}

pub struct ContextSources {
    pub sources: Vec<Value>,
    pub total_sources: usize,
}



fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}

/// Public context sources capability; SQLite retains transaction ownership.
pub trait ContextSourcesRepository {
    /// Reads at most 100 source summaries without deserializing model output payloads.
    fn context_sources(
        &self,
        project_id: &ProjectId,
        scope: ContextSourceScope,
    ) -> Result<ContextSources, RepositoryError>;
}

impl ContextSourcesRepository for SqliteStoryRepository {
    /// Reads at most 100 source summaries without deserializing model output payloads.
    fn context_sources(
        &self,
        project_id: &ProjectId,
        scope: ContextSourceScope,
    ) -> Result<ContextSources, RepositoryError> {
        self.project_revision(project_id)?;
        let through = match scope {
            ContextSourceScope::Project => None,
            ContextSourceScope::ThroughChapter(chapter) => Some(chapter),
        };
        let read = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let total_sources: i64 = read.query_row(
            "SELECT COUNT(*) FROM story_object_heads_v2 h LEFT JOIN story_chapters_v2 c
             ON c.project_id = h.project_id AND h.object_id = 'draft-' || c.chapter_id
             WHERE h.project_id = ?1 AND h.object_kind = 'draft' AND (?2 IS NULL OR c.number <= ?2)",
            params![project_id.as_str(), through], |row| row.get(0),
        ).map_err(storage_error)?;
        let mut statement = read.prepare(
            "SELECT h.object_id, COALESCE(h.candidate_revision_id, h.accepted_revision_id), b.batch_id
             FROM story_object_heads_v2 h LEFT JOIN story_chapters_v2 c
             ON c.project_id = h.project_id AND h.object_id = 'draft-' || c.chapter_id
             LEFT JOIN story_knowledge_heads_v2 k ON k.project_id = h.project_id AND k.object_id = h.object_id
             LEFT JOIN story_modeling_batches_v2 b ON b.project_id = h.project_id AND b.object_id = h.object_id
             AND b.batch_id = k.batch_id AND b.status = 'ready' AND b.published_at IS NOT NULL
             AND b.source_revision_id = COALESCE(h.candidate_revision_id, h.accepted_revision_id)
             AND EXISTS (SELECT 1 FROM story_revisions_v2 r WHERE r.project_id = b.project_id
               AND r.object_id = b.object_id AND r.revision_id = b.source_revision_id AND r.object_kind = 'draft')
             WHERE h.project_id = ?1 AND h.object_kind = 'draft' AND (?2 IS NULL OR c.number <= ?2)
             ORDER BY CASE WHEN ?2 IS NOT NULL THEN c.number END DESC, h.object_id LIMIT 100",
        ).map_err(storage_error)?;
        let mut sources = statement
            .query_map(params![project_id.as_str(), through], |row| {
                let id: String = row.get(0)?;
                let source: Option<String> = row.get(1)?;
                let batch: Option<String> = row.get(2)?;
                Ok(
                    json!({"objectId":id,"sourceRevisionId":source,"batchId":batch,
                "status":if batch.is_some() {"ready"} else {"model_pending"},
                "confirmationState":"candidate"}),
                )
            })
            .map_err(storage_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage_error)?;
        drop(statement);
        read.commit().map_err(storage_error)?;
        if through.is_some() {
            sources.reverse();
        }
        Ok(ContextSources {
            sources,
            total_sources: total_sources as usize,
        })
    }
}

use crate::ProjectRevisionRepository;
