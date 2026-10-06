use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{ProjectId, RepositoryError, SqliteStoryRepository};

pub(crate) const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS story_deleted_chapters_v1 (
  chapter_id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES story_projects_v2(project_id) ON DELETE CASCADE,
  deleted_at INTEGER NOT NULL
);
"#;

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeleteChapterRequest {
    pub expected_revision: i64,
    pub idempotency_key: String,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DeletedChapterReceipt {
    pub operation: String,
    pub structure_change_id: String,
    pub project_id: String,
    pub chapter_id: String,
    pub chapter_number: i64,
    pub title: String,
    pub base_revision: i64,
    pub result_revision: i64,
    pub catalog_revision: i64,
    pub base_snapshot_id: String,
    pub result_snapshot_id: String,
}

/// Deleted identities cannot be recreated by a delayed draft projection.
pub(crate) fn assert_not_deleted(db: &Connection, chapter: &str) -> Result<(), RepositoryError> {
    let deleted: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM story_deleted_chapters_v1 WHERE chapter_id=?1)",
            [chapter],
            |row| row.get(/*idx*/ 0),
        )
        .map_err(storage)?;
    if deleted {
        return Err(invalid("chapter_deleted"));
    }
    Ok(())
}

impl SqliteStoryRepository {
    /// Remove current chapter authority while retaining immutable source and operation history.
    pub fn delete_chapter(
        &mut self,
        project: &ProjectId,
        chapter: &str,
        request: &DeleteChapterRequest,
        now: i64,
    ) -> Result<DeletedChapterReceipt, RepositoryError> {
        if chapter.is_empty()
            || chapter.len() > 160
            || chapter.trim() != chapter
            || chapter.chars().any(char::is_control)
            || request.expected_revision < 0
            || request.idempotency_key.is_empty()
            || request.idempotency_key.len() > 160
            || request.idempotency_key.trim() != request.idempotency_key
            || request.idempotency_key.chars().any(char::is_control)
        {
            return Err(invalid("invalid_chapter_delete"));
        }
        let digest = format!(
            "{:x}",
            Sha256::digest(
                serde_json::json!({
                    "chapterId":chapter,"expectedRevision":request.expected_revision
                })
                .to_string()
                .as_bytes()
            )
        );
        let tx = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage)?;
        let stored: Option<(String,String)> = tx.query_row(
            "SELECT request_digest,result_json FROM story_domain_receipts_v2 WHERE project_id=?1 AND operation='delete-chapter' AND idempotency_key=?2",
            params![project.as_str(),request.idempotency_key], |row| Ok((row.get(/*idx*/ 0)?,row.get(/*idx*/ 1)?)),
        ).optional().map_err(storage)?;
        if let Some((hash, raw)) = stored {
            if hash != digest {
                return Err(RepositoryError::IdempotencyConflict);
            }
            return serde_json::from_str(&raw).map_err(|_| RepositoryError::CorruptState);
        }
        let current: i64 = tx
            .query_row(
                "SELECT revision FROM story_workspaces_v2 WHERE project_id=?1",
                [project.as_str()],
                |row| row.get(/*idx*/ 0),
            )
            .optional()
            .map_err(storage)?
            .ok_or(RepositoryError::ProjectNotFound)?;
        if current != request.expected_revision {
            return Err(RepositoryError::StaleProjectRevision {
                expected: request.expected_revision,
                actual: current,
            });
        }
        let metadata: Option<(i64, String)> = tx
            .query_row(
                "SELECT number,title FROM story_chapters_v2 WHERE project_id=?1 AND chapter_id=?2",
                params![project.as_str(), chapter],
                |row| Ok((row.get(/*idx*/ 0)?, row.get(/*idx*/ 1)?)),
            )
            .optional()
            .map_err(storage)?;
        let (number, title) = metadata.ok_or_else(|| invalid("chapter_delete_not_found"))?;
        crate::manuscript_catalog::assert_writable(&tx, project)?;
        let object = format!("draft-{chapter}");
        let extraction = format!("commitment-extraction-{chapter}");
        let locked: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM story_object_heads_v2 h WHERE h.project_id=?1 AND h.object_id=?2 AND (h.locked=1 OR EXISTS(SELECT 1 FROM story_revisions_v2 r, json_each(r.content_json,'$.sceneGroups') s WHERE r.project_id=h.project_id AND r.object_id=h.object_id AND r.revision_id IN(h.candidate_revision_id,h.accepted_revision_id) AND json_extract(s.value,'$.locked')=1)))",
            params![project.as_str(),object], |row| row.get(/*idx*/ 0),
        ).map_err(storage)?;
        if locked {
            return Err(RepositoryError::ObjectLocked);
        }
        let base: String = tx.query_row("SELECT head_snapshot_id FROM story_branches_v2 WHERE project_id=?1 AND branch_id='main'",
            [project.as_str()], |row| row.get(/*idx*/ 0)).map_err(storage)?;
        let next = format!("snapshot-{}", uuid::Uuid::new_v4().simple());
        let structure_change_id = crate::chapter_structure::capture(
            &tx, project, chapter, number, &title, &base, &next, current, now,
        )?;
        tx.execute("INSERT INTO story_deleted_chapters_v1(chapter_id,project_id,deleted_at) VALUES(?1,?2,?3)",params![chapter,project.as_str(),now]).map_err(storage)?;
        tx.execute(
            "DELETE FROM story_chapters_v2 WHERE project_id=?1 AND chapter_id=?2",
            params![project.as_str(), chapter],
        )
        .map_err(storage)?;
        for table in ["story_object_heads_v2", "story_knowledge_heads_v2"] {
            tx.execute(
                &format!("DELETE FROM {table} WHERE project_id=?1 AND object_id IN(?2,?3)"),
                params![project.as_str(), object, extraction],
            )
            .map_err(storage)?;
        }
        tx.execute("DELETE FROM story_working_baselines_v2 WHERE project_id=?1 AND object_kind='draft' AND object_id=?2",params![project.as_str(),chapter]).map_err(storage)?;
        tx.execute("DELETE FROM story_continuation_intents_v2 WHERE project_id=?1 AND target_kind='draft' AND target_id=?2",params![project.as_str(),chapter]).map_err(storage)?;
        tx.execute("INSERT INTO story_snapshots_v2(snapshot_id,project_id,branch_id,parent_snapshot_id) VALUES(?1,?2,'main',?3)",params![next,project.as_str(),base]).map_err(storage)?;
        tx.execute("INSERT INTO story_snapshot_members_v2(snapshot_id,object_id,revision_id) SELECT ?1,object_id,revision_id FROM story_snapshot_members_v2 WHERE snapshot_id=?2 AND object_id NOT IN(?3,?4)",params![next,base,object,extraction]).map_err(storage)?;
        // Capture triggers use current knowledge. Preserve the exact base snapshot bindings instead.
        tx.execute(
            "DELETE FROM story_snapshot_knowledge_v2 WHERE snapshot_id=?1",
            [&next],
        )
        .map_err(storage)?;
        tx.execute("INSERT INTO story_snapshot_knowledge_v2(snapshot_id,object_id,batch_id) SELECT ?1,object_id,batch_id FROM story_snapshot_knowledge_v2 WHERE snapshot_id=?2 AND object_id NOT IN(?3,?4)",params![next,base,object,extraction]).map_err(storage)?;
        tx.execute("UPDATE story_branches_v2 SET head_snapshot_id=?1 WHERE project_id=?2 AND branch_id='main' AND head_snapshot_id=?3",params![next,project.as_str(),base]).map_err(storage)?;
        let result_revision = current
            .checked_add(1)
            .ok_or(RepositoryError::CorruptState)?;
        tx.execute(
            "UPDATE story_workspaces_v2 SET revision=?1 WHERE project_id=?2",
            params![result_revision, project.as_str()],
        )
        .map_err(storage)?;
        crate::manuscript_catalog::advance_revision(&tx, project)?;
        tx.execute(
            "UPDATE story_projects_v2 SET updated_at=?1 WHERE project_id=?2",
            params![now, project.as_str()],
        )
        .map_err(storage)?;
        let receipt = DeletedChapterReceipt {
            operation: "delete-chapter".into(),
            structure_change_id,
            project_id: project.as_str().into(),
            chapter_id: chapter.into(),
            chapter_number: number,
            title,
            base_revision: current,
            result_revision,
            catalog_revision: crate::manuscript_catalog::read_catalog(&tx, project)?.revision,
            base_snapshot_id: base,
            result_snapshot_id: next,
        };
        let raw = serde_json::to_string(&receipt).map_err(|_| RepositoryError::CorruptState)?;
        tx.execute("INSERT INTO story_domain_receipts_v2(project_id,operation,idempotency_key,request_digest,result_json) VALUES(?1,'delete-chapter',?2,?3,?4)",params![project.as_str(),request.idempotency_key,digest,raw]).map_err(storage)?;
        tx.commit().map_err(storage)?;
        Ok(receipt)
    }
}

fn invalid(code: &str) -> RepositoryError {
    RepositoryError::InvalidContent(code.into())
}
fn storage(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}
