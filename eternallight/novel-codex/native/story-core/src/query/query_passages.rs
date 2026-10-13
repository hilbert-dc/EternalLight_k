use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::query_scope::TaskQueryScope;
use crate::{RepositoryError, SearchScope, SqliteStoryRepository, TaskRecord};

pub(crate) const OVERLAP_CHARS: usize = 32;

pub(crate) struct PassageQuery<'a> {
    pub question: &'a str,
    pub step_id: &'a str,
    pub cursor: Option<&'a str>,
    pub max_chars: usize,
    pub limit: usize,
}

#[derive(Clone, Deserialize, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Position {
    chapter: i64,
    object: String,
    field: i64,
    start: usize,
}

#[derive(Deserialize, Serialize, ts_rs::TS)]
#[ts(rename = "QueryPassagesCursor")]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Cursor {
    binding: String,
    position: Position,
}

struct Field {
    position: Position,
    revision: String,
    text: String,
}

pub(crate) fn scan(
    repo: &SqliteStoryRepository,
    task: &TaskRecord,
    scope: &TaskQueryScope,
    query: PassageQuery<'_>,
) -> Result<Value, RepositoryError> {
    if let TaskQueryScope::Bounded(scope) = scope
        && scope.pov.is_some()
    {
        // No hidden counts or cursor validation oracle for text without a POV proof.
        return Ok(json!({"results":[],"nextCursor":null,
            "coverage":{"complete":false,"semanticCompleteness":"unknown","povTextUnverified":true}}));
    }
    let binding = cursor_binding(repo, task, scope, query.question, query.step_id)?;
    let candidate_first = crate::query_scope::uses_candidate_source(task)?;
    let mut position = query
        .cursor
        .map(|value| {
            let cursor: Cursor = serde_json::from_str(value).map_err(|_| invalid_cursor())?;
            if cursor.binding != binding
                || cursor.position.chapter < 1
                || cursor.position.field < 0
                || cursor.position.object.len() > 160
                || !cursor.position.object.starts_with("draft-")
                || cursor.position.start > 8_000_000
            {
                return Err(invalid_cursor());
            }
            Ok(cursor.position)
        })
        .transpose()?;
    let mut field = next_field(
        repo,
        task,
        scope.text_scope(),
        position.as_ref(),
        candidate_first,
    )?;
    if let Some(position) = &position
        && field.as_ref().is_none_or(|field| {
            field.position.chapter != position.chapter
                || field.position.object != position.object
                || field.position.field != position.field
        })
    {
        return Err(invalid_cursor());
    }
    let mut results = Vec::new();
    while let Some(current) = field {
        let start = position.as_ref().map_or(0, |position| position.start);
        if current.text.len() > 8_000_000 {
            return Err(RepositoryError::CorruptState);
        }
        let mut chars = current.text.chars().skip(start);
        let quote = chars.by_ref().take(query.max_chars).collect::<String>();
        let has_remaining = chars.next().is_some();
        let end = start + quote.chars().count();
        if quote.is_empty() {
            return Err(invalid_cursor());
        }
        let path = if current.position.field == 0 {
            "/body".to_owned()
        } else {
            format!("/sceneGroups/{}/body", current.position.field - 1)
        };
        results.push(
            json!({"sourceObjectId":current.position.object,"sourceRevisionId":current.revision,
            "sourcePath":path,"chapterNumber":current.position.chapter,"source":"story-revision",
            "evidence":{"quote":quote,"start":start,"end":end},"confirmationState":"sourceText"}),
        );
        if has_remaining {
            // Bounded overlap protects a clue crossing a page edge without skipping characters.
            let next = Position {
                start: end - OVERLAP_CHARS,
                ..current.position
            };
            field = Some(Field {
                position: next.clone(),
                ..current
            });
            position = Some(next);
        } else {
            let after = Position {
                field: current.position.field + 1,
                start: 0,
                ..current.position
            };
            field = next_field(
                repo,
                task,
                scope.text_scope(),
                Some(&after),
                candidate_first,
            )?;
            position = field.as_ref().map(|field| field.position.clone());
        }
        if results.len() == query.limit {
            break;
        }
    }
    let next_cursor = position
        .map(|position| serde_json::to_string(&Cursor { binding, position }))
        .transpose()
        .map_err(|_| RepositoryError::CorruptState)?;
    Ok(json!({"results":results,"nextCursor":next_cursor,
        "coverage":{"complete":false,"semanticCompleteness":"unknown","pageComplete":true,
            "startsAtBeginning":query.cursor.is_none(),"endOfScopeFromCursor":next_cursor.is_none(),
            "returnedPassages":results.len(),"selection":"authorizedSourceScan","overlapChars":OVERLAP_CHARS,
            "fullScanCertified":false,"modelJudgmentVerified":false}}))
}

fn next_field(
    repo: &SqliteStoryRepository,
    task: &TaskRecord,
    scope: SearchScope,
    position: Option<&Position>,
    candidate_first: bool,
) -> Result<Option<Field>, RepositoryError> {
    let (chapter_only, through, scene_through) = match scope {
        SearchScope::All => (None, None, None),
        SearchScope::Chapter(chapter) => (Some(chapter), None, None),
        SearchScope::ThroughChapter(chapter) => (None, Some(chapter), None),
        SearchScope::ThroughScene {
            chapter,
            end_scene_order,
        } => (None, Some(chapter), Some(end_scene_order as i64)),
    };
    let mut seek = position.cloned();
    let mut field_start = seek.as_ref().map(|position| position.field);
    loop {
        // Locate one authorized source before decoding its JSON. No whole-book JSON expansion.
        let source = repo.connection.query_row(
            "SELECT c.number,h.object_id,r.revision_id,r.content_json
            FROM story_chapters_v2 c JOIN story_object_heads_v2 h
              ON h.project_id = c.project_id AND h.object_id = 'draft-' || c.chapter_id AND h.object_kind = 'draft'
            LEFT JOIN story_working_baselines_v2 b
              ON b.project_id=c.project_id AND b.object_kind='draft' AND b.object_id=c.chapter_id
            JOIN story_revisions_v2 r ON r.project_id = h.project_id AND r.object_id = h.object_id
              AND r.revision_id = CASE WHEN ?7 THEN COALESCE(h.candidate_revision_id,h.accepted_revision_id)
                ELSE COALESCE(b.revision_id,h.accepted_revision_id,h.candidate_revision_id) END
              AND r.object_kind = 'draft'
            WHERE c.project_id = ?1 AND (?2 IS NULL OR c.number = ?2) AND (?3 IS NULL OR c.number <= ?3)
              AND (?4 IS NULL OR c.number > ?4 OR (c.number = ?4 AND h.object_id > ?5)
                   OR (c.number = ?4 AND h.object_id = ?5 AND ?6 IS NOT NULL))
            ORDER BY c.number,h.object_id LIMIT 1",
            params![task.project_id.as_str(),chapter_only,through,
                seek.as_ref().map(|position| position.chapter),seek.as_ref().map(|position| position.object.as_str()),field_start,candidate_first],
            |row| Ok((row.get::<_,i64>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?,row.get::<_,String>(3)?)),
        ).optional().map_err(|error| RepositoryError::Storage(error.to_string()))?;
        let Some((chapter, object, revision, content_json)) = source else {
            return Ok(None);
        };
        let content: Value =
            serde_json::from_str(&content_json).map_err(|_| RepositoryError::CorruptState)?;
        let first_field = if seek
            .as_ref()
            .is_some_and(|position| position.chapter == chapter && position.object == object)
        {
            field_start.unwrap_or_default()
        } else {
            0
        };
        let scene_end = scene_through.filter(|_| Some(chapter) == through);
        let body = content["body"].as_str().filter(|body| !body.is_empty());
        let selected = if scene_end.is_none() && body.is_some() {
            body.filter(|_| first_field == 0).map(|body| (0, body))
        } else {
            content["sceneGroups"]
                .as_array()
                .into_iter()
                .flatten()
                .enumerate()
                .skip((first_field.max(1) - 1) as usize)
                .take_while(|(index, _)| scene_end.is_none_or(|end| (*index as i64) < end))
                .find_map(|(index, group)| {
                    group["body"]
                        .as_str()
                        .filter(|body| !body.is_empty())
                        .map(|body| (index as i64 + 1, body))
                })
        };
        if let Some((field, text)) = selected {
            return Ok(Some(Field {
                position: Position {
                    chapter,
                    object,
                    field,
                    start: 0,
                },
                revision,
                text: text.to_owned(),
            }));
        }
        // A body-less or exhausted chapter cannot stall the source walk.
        seek = Some(Position {
            chapter,
            object,
            field: 0,
            start: 0,
        });
        field_start = None;
    }
}

fn invalid_cursor() -> RepositoryError {
    RepositoryError::InvalidContent(
        "passage cursor is stale or outside the task; restart the scan".into(),
    )
}

fn cursor_binding(
    repo: &SqliteStoryRepository,
    task: &TaskRecord,
    scope: &TaskQueryScope,
    question: &str,
    step_id: &str,
) -> Result<String, RepositoryError> {
    Ok(format!(
        "{:x}",
        Sha256::digest(
            json!({"projectId":task.project_id.as_str(),
        "taskId":task.task_id,"threadId":task.thread_id,"question":question,"stepId":step_id,
        "policy":scope.policy(),"sources":scope.watermarks(repo,task)?})
            .to_string()
            .as_bytes()
        )
    ))
}

/// Reissues a persisted, source/policy-checked position for the current active attempt.
pub(crate) fn rebind_cursor(
    repo: &SqliteStoryRepository,
    task: &TaskRecord,
    scope: &TaskQueryScope,
    question: &str,
    step_id: &str,
    saved: &str,
) -> Result<String, RepositoryError> {
    let mut cursor: Cursor =
        serde_json::from_str(saved).map_err(|_| RepositoryError::CorruptState)?;
    cursor.binding = cursor_binding(repo, task, scope, question, step_id)?;
    serde_json::to_string(&cursor).map_err(|_| RepositoryError::CorruptState)
}

#[cfg(test)]
#[path = "query_passages_tests.rs"]
mod tests;
