use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};

use crate::{RepositoryError, SearchScope, SqliteStoryRepository, TaskRecord};

impl SqliteStoryRepository {
    /// Resolves an exact quote against the task's authorized current source.
    /// It never repairs text, chooses an ambiguous match, or authorizes a write.
    pub fn resolve_task_evidence(
        &self,
        task: &TaskRecord,
        object_id: &str,
        revision_id: &str,
        quote: &str,
        now: i64,
    ) -> Result<Value, RepositoryError> {
        if object_id.len() > 160
            || revision_id.is_empty()
            || revision_id.len() > 160
            || quote.is_empty()
            || quote.chars().count() > 2_000
        {
            return Err(RepositoryError::InvalidContent(
                "invalid exact evidence request".into(),
            ));
        }
        self.with_read_snapshot(|| {
            let task = self.authorize_task_capability(&task.project_id, &task.task_id,
                &task.thread_id, task.input_revision, &task.capability_hash, now)?;
            self.assert_task_inputs_current(&task)?;
            let scope = crate::query_scope::TaskQueryScope::from_task(self, &task)?;
            // Whole-body anchors cannot expose a Writer's hidden suffix or POV text.
            if !matches!(task.operation.as_str(), "advisory" | "extractCommitments" | "reviseCommitment"
                | "modelKnowledge" | "assessSceneOutcome" | "assessRevisionOutcome" | "assessChapterStructure") {
                return Err(RepositoryError::TaskOutputForbidden);
            }
            let candidate_first = crate::query_scope::uses_candidate_source(&task)?;
            let source = self.connection.query_row(
                "SELECT c.number,r.content_json FROM story_chapters_v2 c
                 JOIN story_object_heads_v2 h ON h.project_id=c.project_id AND h.object_id='draft-' || c.chapter_id AND h.object_kind='draft'
                 LEFT JOIN story_working_baselines_v2 b ON b.project_id=c.project_id AND b.object_kind='draft' AND b.object_id=c.chapter_id
                 JOIN story_revisions_v2 r ON r.project_id=h.project_id AND r.object_id=h.object_id AND r.object_kind='draft'
                   AND r.revision_id=CASE WHEN ?4 THEN COALESCE(h.candidate_revision_id,h.accepted_revision_id)
                     ELSE COALESCE(b.revision_id,h.accepted_revision_id,h.candidate_revision_id) END
                 WHERE c.project_id=?1 AND h.object_id=?2 AND r.revision_id=?3",
                params![task.project_id.as_str(),object_id,revision_id,candidate_first],
                |row| Ok((row.get::<_,i64>(0)?,row.get::<_,String>(1)?)),
            ).optional().map_err(|error| RepositoryError::Storage(error.to_string()))?
                .ok_or(RepositoryError::TaskInputStale)?;
            let allowed = match scope.text_scope() {
                SearchScope::All => true,
                SearchScope::Chapter(chapter) => source.0 == chapter,
                SearchScope::ThroughChapter(chapter) => source.0 <= chapter,
                SearchScope::ThroughScene { .. } => false,
            };
            if !allowed || matches!(&scope, crate::query_scope::TaskQueryScope::Bounded(bound)
                if bound.source_revision_id.as_deref().is_some_and(|id| id != revision_id))
            {
                return Err(RepositoryError::TaskOutputForbidden);
            }
            let content: Value = serde_json::from_str(&source.1).map_err(|_| RepositoryError::CorruptState)?;
            let body = content["body"].as_str().ok_or(RepositoryError::CorruptState)?;
            let mut matches = Vec::new();
            let (status, truncated) = match crate::source_evidence::resolve(&json!({"quote":quote}), body) {
                Ok(anchor) => { matches.push(json!({"start":anchor["start"],"end":anchor["end"]})); ("unique", false) }
                Err(RepositoryError::InvalidContent(code)) if code == "evidence_quote_not_found" => ("notFound", false),
                Err(RepositoryError::InvalidContent(code)) if code == "evidence_quote_ambiguous" => {
                    let mut cursor = 0;
                    let mut truncated = false;
                    while let Some(relative) = body[cursor..].find(quote) {
                        if matches.len() == 4 { truncated = true; break; }
                        let byte = cursor + relative;
                        let start = body[..byte].chars().count();
                        matches.push(json!({"start":start,"end":start+quote.chars().count()}));
                        cursor = byte + body[byte..].chars().next().ok_or(RepositoryError::CorruptState)?.len_utf8();
                    }
                    ("ambiguous", truncated)
                }
                Err(error) => return Err(error),
            };
            Ok(json!({"status":status,"source":{"objectId":object_id,"revisionId":revision_id,
                "sourcePath":"/body","unicodeLength":body.chars().count()},"quote":quote,
                "matches":matches,"truncated":truncated,"authority":"exactSourceText"}))
        })?
    }
}
