use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::{RepositoryError, SqliteStoryRepository, TaskRecord};

impl SqliteStoryRepository {
    /// The complete-chapter Writer reads its own immutable input, not a POV world fact.
    pub fn task_writer_baseline(
        &self,
        task: &TaskRecord,
    ) -> Result<Option<Value>, RepositoryError> {
        let target: Value =
            serde_json::from_str(&task.target_json).map_err(|_| RepositoryError::CorruptState)?;
        if task.operation != "generateChapter" || target["scope"]["type"] != "wholeChapter" {
            return Ok(None);
        }
        self.assert_task_inputs_current(task)?;
        let chapter_id = target["chapterId"]
            .as_str()
            .ok_or(RepositoryError::TaskBindingInvalid)?;
        let object_id = format!("draft-{chapter_id}");
        let candidate_first = crate::query_scope::uses_candidate_source(task)?;
        let source: Option<(String, String, Option<String>)> = self.connection.query_row(
            "SELECT r.revision_id,r.content_json,i.accepted_revision_id FROM story_task_input_heads_v2 i
             JOIN story_tasks_v2 t ON t.task_id=i.task_id
             JOIN story_revisions_v2 r ON r.project_id=t.project_id AND r.object_id=i.object_id AND r.object_kind='draft'
               AND r.revision_id=CASE WHEN ?3 THEN COALESCE(i.candidate_revision_id,i.accepted_revision_id)
                   WHEN EXISTS(SELECT 1 FROM story_task_outputs_v2 o WHERE o.task_id=i.task_id AND o.output_kind='draft')
                   THEN (SELECT o.working_base_revision_id FROM story_task_outputs_v2 o JOIN story_revisions_v2 produced ON produced.revision_id=o.revision_id
                       AND produced.project_id=t.project_id AND produced.object_id=i.object_id AND produced.object_kind='draft'
                       WHERE o.task_id=i.task_id AND o.output_kind='draft' ORDER BY o.ordinal LIMIT 1)
                   ELSE COALESCE(i.working_revision_id,i.accepted_revision_id,i.candidate_revision_id) END
             WHERE i.task_id=?1 AND i.object_id=?2 AND t.project_id=?4",
            params![task.task_id,object_id,candidate_first,task.project_id.as_str()],
            |row| Ok((row.get(/*idx*/ 0)?,row.get(/*idx*/ 1)?,row.get(/*idx*/ 2)?)),
        ).optional().map_err(|error| RepositoryError::Storage(error.to_string()))?;
        let Some((revision, content, accepted)) = source else {
            return Ok(None);
        };
        let content: Value =
            serde_json::from_str(&content).map_err(|_| RepositoryError::CorruptState)?;
        if content["chapterId"] != chapter_id || content["chapterNumber"] != target["chapterNumber"]
        {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        Ok(Some(
            json!({"objectId":object_id,"sourceRevisionId":revision,"content":content,
            "authority":if accepted.as_deref()==Some(revision.as_str()) {"accepted"} else {"frozenWriterInput"},
            "workingSelectionRevision":null}),
        ))
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Cursor {
    binding: String,
    start: usize,
}

pub(crate) fn scan(
    repo: &SqliteStoryRepository,
    task: &TaskRecord,
    query: crate::query_passages::PassageQuery<'_>,
) -> Result<Value, RepositoryError> {
    let source = repo
        .task_writer_baseline(task)?
        .ok_or(RepositoryError::TaskOutputForbidden)?;
    let body = source["content"]["body"]
        .as_str()
        .ok_or(RepositoryError::CorruptState)?;
    if body.len() > 8_000_000 {
        return Err(RepositoryError::CorruptState);
    }
    let binding = format!("{:x}",Sha256::digest(json!({"projectId":task.project_id.as_str(),"taskId":task.task_id,
        "threadId":task.thread_id,"inputRevision":task.input_revision,"sourceRevisionId":source["sourceRevisionId"],
        "operation":"writerBaselinePassages","question":query.question,"stepId":query.step_id}).to_string().as_bytes()));
    let mut start = match query.cursor {
        Some(value) => {
            let cursor: Cursor = serde_json::from_str(value).map_err(|_| invalid_cursor())?;
            if cursor.binding != binding || cursor.start >= body.chars().count() {
                return Err(invalid_cursor());
            }
            cursor.start
        }
        None => 0,
    };
    let mut results = Vec::new();
    let length = body.chars().count();
    while start < length && results.len() < query.limit {
        let quote: String = body.chars().skip(start).take(query.max_chars).collect();
        let end = start + quote.chars().count();
        results.push(json!({"sourceObjectId":source["objectId"],"sourceRevisionId":source["sourceRevisionId"],
            "sourcePath":"/body","chapterNumber":source["content"]["chapterNumber"],"source":"story-revision",
            "evidence":{"quote":quote,"start":start,"end":end},"confirmationState":"sourceText"}));
        start = if end == length {
            length
        } else {
            end - crate::query_passages::OVERLAP_CHARS
        };
    }
    let next_cursor = if start < length {
        Some(
            serde_json::to_string(&Cursor { binding, start })
                .map_err(|_| RepositoryError::CorruptState)?,
        )
    } else {
        None
    };
    Ok(
        json!({"results":results,"nextCursor":next_cursor,"coverage":{"complete":false,"semanticCompleteness":"unknown",
        "pageComplete":true,"startsAtBeginning":query.cursor.is_none(),"endOfScopeFromCursor":next_cursor.is_none(),
        "returnedPassages":results.len(),"selection":"frozenWriterBaseline","overlapChars":crate::query_passages::OVERLAP_CHARS,
        "fullScanCertified":false,"modelJudgmentVerified":false}}),
    )
}

fn invalid_cursor() -> RepositoryError {
    RepositoryError::InvalidContent("writer baseline cursor is stale or outside this task".into())
}
