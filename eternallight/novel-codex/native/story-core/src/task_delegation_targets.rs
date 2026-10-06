use rusqlite::{OptionalExtension, params};
use serde::Serialize;
use serde_json::Value;

use crate::{RepositoryError, SqliteStoryRepository, TaskRecord};

pub enum DelegationTargetSelection {
    Project,
    ChapterNumber(i64),
}

// Identity metadata is independent of optional chapter prose, never a write permit.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Target {
    chapter_id: String,
    chapter_number: i64,
    source_revision_id: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
    object_id: String,
    target: Target,
    operations: [&'static str; 2],
}

#[derive(Serialize)]
struct Coverage {
    total: usize,
    included: usize,
    truncated: bool,
}

#[derive(Serialize)]
struct Targets {
    authority: &'static str,
    scope: &'static str,
    status: &'static str,
    entries: Vec<Entry>,
    coverage: Coverage,
}

const SOURCES: &str = "FROM story_branches_v2 b
 JOIN story_snapshot_members_v2 m ON m.snapshot_id=b.head_snapshot_id
 JOIN story_revisions_v2 r ON r.revision_id=m.revision_id AND r.project_id=b.project_id AND r.object_id=m.object_id
 JOIN story_chapters_v2 c ON c.project_id=b.project_id AND m.object_id='draft-' || c.chapter_id
 WHERE b.project_id=?1 AND b.branch_id='main' AND r.object_kind='draft'
 AND json_extract(r.content_json,'$.chapterNumber')=c.number
 AND (?2 IS NULL OR c.chapter_id=?2) AND (?3 IS NULL OR c.number=?3)
 AND (?4 IS NULL OR m.object_id=?4)";

impl SqliteStoryRepository {
    /// Returns bounded current-source identities for an advisory delegation.
    /// Historical references are never replaced, and action registration still checks intent and CAS.
    pub fn task_delegation_targets(
        &self,
        task: &TaskRecord,
        now: i64,
        selection: DelegationTargetSelection,
    ) -> Result<Option<Value>, RepositoryError> {
        if task.operation != "advisory" {
            return Ok(None);
        }
        self.with_read_snapshot(|| {
            let task = self.authorize_task_capability(&task.project_id, &task.task_id,
                &task.thread_id, task.input_revision, &task.capability_hash, now)?;
            self.assert_task_inputs_current(&task)?;
            let binding: Value = serde_json::from_str(&task.target_json)
                .map_err(|_| RepositoryError::CorruptState)?;
            if !binding["revisionRepairSource"].is_null() {
                return Ok(None);
            }
            let focus = &binding["conversationFocus"];
            let reference = &focus["reference"];
            let chapter = focus["chapterId"].as_str();
            let number = focus["chapterNumber"].as_i64();
            if chapter.is_some() != number.is_some() || number.is_some_and(|number| number < 1) {
                return Err(RepositoryError::TaskBindingInvalid);
            }
            let object = reference["objectId"].as_str();
            let focused = chapter.is_some() || !reference.is_null();
            let requested = match selection {
                DelegationTargetSelection::Project => None,
                DelegationTargetSelection::ChapterNumber(number) if number > 0 => Some(number),
                DelegationTargetSelection::ChapterNumber(_) => return Err(RepositoryError::InvalidContent(
                    "delegation readingChapter must be a positive chapter number".into(),
                )),
            };
            if focused && let Some(requested) = requested {
                let bound = if let Some(number) = number {
                    Some(number)
                } else if let Some(object) = object {
                    self.connection.query_row(
                        "SELECT number FROM story_chapters_v2 WHERE project_id=?1 AND 'draft-' || chapter_id=?2",
                        params![task.project_id.as_str(), object], |row| row.get::<_, i64>(/*idx*/ 0),
                    ).optional().map_err(storage)?
                } else { None };
                if bound != Some(requested) {
                    return Err(RepositoryError::TaskBindingInvalid);
                }
            }
            let selected_number = number.or(requested);
            let mut result = Targets {
                authority: "sourceIdentityOnly",
                scope: if focused { "focused" } else if requested.is_some() { "requestedChapter" } else { "project" },
                status: "available",
                entries: Vec::new(),
                coverage: Coverage { total: 0, included: 0, truncated: false },
            };
            if !reference.is_null()
                && !matches!(reference["kind"].as_str(), Some("scene" | "claim"))
            {
                result.status = "referenceNotChapter";
            } else {
                let snapshot: String = self.connection.query_row(
                    "SELECT head_snapshot_id FROM story_branches_v2 WHERE project_id=?1 AND branch_id='main'",
                    [task.project_id.as_str()], |row| row.get(/*idx*/ 0),
                ).map_err(storage)?;
                if !reference.is_null() && (reference["projectId"] != task.project_id.as_str()
                    || reference["snapshotId"] != snapshot || object.is_none()
                    || reference["revisionId"].as_str().is_none())
                {
                    result.status = "referencedSourceNotCurrent";
                } else {
                    let total: usize = self.connection.query_row(
                        &format!("SELECT COUNT(*) {SOURCES}"),
                        params![task.project_id.as_str(), chapter, selected_number, object], |row| row.get(/*idx*/ 0),
                    ).map_err(storage)?;
                    let mut statement = self.connection.prepare(&format!(
                        "SELECT m.object_id,c.chapter_id,c.number,m.revision_id {SOURCES} ORDER BY c.number,c.chapter_id LIMIT 4"
                    )).map_err(storage)?;
                    let rows = statement.query_map(params![task.project_id.as_str(), chapter, selected_number, object], |row| {
                        Ok(Entry { object_id: row.get(/*idx*/ 0)?, target: Target { chapter_id: row.get(/*idx*/ 1)?,
                            chapter_number: row.get(/*idx*/ 2)?, source_revision_id: row.get(/*idx*/ 3)? },
                            operations: ["extractCommitments", "modelKnowledge"] })
                    }).map_err(storage)?;
                    for row in rows {
                        let entry = row.map_err(storage)?;
                        if !reference.is_null() && reference["revisionId"] != entry.target.source_revision_id {
                            result.status = "referencedSourceNotCurrent";
                            break;
                        }
                        let target = serde_json::to_value(&entry.target).map_err(|_| RepositoryError::CorruptState)?;
                        crate::commitment_extraction::validate_target(&self.connection, &task.project_id, &target)?;
                        crate::modeling_action::validate_target(&self.connection, &task.project_id, &target)?;
                        result.entries.push(entry);
                    }
                    if result.status == "available" {
                        result.coverage.total = total;
                        if total == 0 { result.status = "noSavedSource"; }
                    }
                }
            }
            loop {
                result.coverage.included = result.entries.len();
                result.coverage.truncated = result.coverage.included < result.coverage.total;
                let value = serde_json::to_value(&result).map_err(|_| RepositoryError::CorruptState)?;
                // A UTF-8 byte is an upper bound on tokens; include all status and coverage fields.
                if value.to_string().len() <= 768 { return Ok(Some(value)); }
                if focused || requested.is_some() || result.entries.is_empty() {
                    return Err(RepositoryError::InvalidContent(
                        "focused delegation target exceeds the 768-byte metadata budget".into(),
                    ));
                }
                result.entries.pop();
            }
        })?
    }
}

fn storage(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}
