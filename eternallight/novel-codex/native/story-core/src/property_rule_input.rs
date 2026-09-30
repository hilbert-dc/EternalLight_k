use rusqlite::{OptionalExtension, params};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::{RepositoryError, SqliteStoryRepository, TaskRecord};

const PART_BYTES: usize = 2_800;
const PAGE_BYTES: usize = 7_000;

struct RuleInput {
    revision: Option<String>,
    source: Option<String>,
    payload: String,
    count: usize,
    event_count: usize,
    hash: String,
}

#[derive(Serialize)]
struct ByteRange {
    start: usize,
    end: usize,
    total: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RuleInputPage {
    revision_id: Option<String>,
    source: Option<String>,
    content_hash: String,
    definition_count: usize,
    event_rule_count: usize,
    payload_encoding: &'static str,
    byte_range: ByteRange,
    payload_part: String,
    next_cursor: Option<String>,
    end_of_baseline_from_cursor: bool,
    byte_limit: usize,
}

impl SqliteStoryRepository {
    /// Describe the immutable rule input without injecting its entire payload into Context.
    pub fn task_property_rule_input(&self, task: &TaskRecord) -> Result<Value, RepositoryError> {
        self.with_read_snapshot(|| {
            let input = self.load_property_rule_input(task)?;
            Ok(json!({"id":input.revision,"source":input.source,"definitionCount":input.count,
                "eventRuleCount":input.event_count,
                "payloadBytes":input.payload.len(),"contentHash":input.hash,"readTool":"read_property_rules",
                "payloadEncoding":"jsonUtf8","pageByteLimit":PAGE_BYTES}))
        })?
    }

    /// Read an exact UTF-8 range of captured input; the suffix end is not a reading certificate.
    pub fn task_property_rule_page(
        &self,
        task: &TaskRecord,
        cursor: Option<&str>,
    ) -> Result<Value, RepositoryError> {
        self.with_read_snapshot(|| {
            let input = self.load_property_rule_input(task)?;
            let fence = format!("{:x}", Sha256::digest(json!({"project":task.project_id.as_str(),"task":task.task_id,
                "thread":task.thread_id,"revision":task.input_revision,"base":input.revision,"hash":input.hash}).to_string().as_bytes()));
            let start = match cursor {
                None => 0,
                Some(cursor) => {
                    let (prefix, offset) = cursor.split_once(':').ok_or_else(invalid_cursor)?;
                    let offset: usize = offset.parse().map_err(|_| invalid_cursor())?;
                    if prefix != fence || cursor != format!("{fence}:{offset}") {
                        return Err(invalid_cursor());
                    }
                    offset
                }
            };
            if start >= input.payload.len() || !input.payload.is_char_boundary(start) {
                return Err(invalid_cursor());
            }
            let mut end = (start + PART_BYTES).min(input.payload.len());
            while !input.payload.is_char_boundary(end) { end -= 1; }
            let page = RuleInputPage {
                revision_id: input.revision, source: input.source, content_hash: input.hash,
                definition_count: input.count, payload_encoding: "jsonUtf8",
                event_rule_count: input.event_count,
                byte_range: ByteRange { start, end, total: input.payload.len() },
                payload_part: input.payload[start..end].to_owned(),
                next_cursor: (end < input.payload.len()).then(|| format!("{fence}:{end}")),
                end_of_baseline_from_cursor: end == input.payload.len(), byte_limit: PAGE_BYTES,
            };
            let value = serde_json::to_value(page).map_err(|_| RepositoryError::CorruptState)?;
            if value.to_string().len() > PAGE_BYTES { return Err(RepositoryError::CorruptState); }
            Ok(value)
        })?
    }

    fn load_property_rule_input(
        &self,
        requested: &TaskRecord,
    ) -> Result<RuleInput, RepositoryError> {
        let task = self
            .task(&requested.project_id, &requested.task_id)?
            .ok_or(RepositoryError::TaskBindingInvalid)?;
        crate::tasks::validate_active_task(
            &self.connection,
            &task,
            &requested.project_id,
            &requested.thread_id,
            requested.input_revision,
        )?;
        if task.operation != "generatePropertyRules" || task.capability_revoked_at.is_some() {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        self.assert_task_inputs_current(&task)?;
        let revision: Option<String> = self.connection.query_row(
            "SELECT COALESCE(candidate_revision_id,accepted_revision_id) FROM story_task_input_heads_v2 WHERE task_id=?1 AND object_id='artifact-propertyRules'",
            [&task.task_id], |row| row.get(0),
        ).optional().map_err(storage)?.flatten();
        let (payload, source) = if let Some(revision) = &revision {
            let content: String = self.connection.query_row(
                "SELECT content_json FROM story_revisions_v2 WHERE project_id=?1 AND object_id='artifact-propertyRules' AND object_kind='propertyRules' AND revision_id=?2",
                params![task.project_id.as_str(),revision], |row| row.get(0),
            ).map_err(storage)?;
            crate::property_rules::validate_content(&content)
                .map_err(|_| RepositoryError::CorruptState)?;
            let envelope: Value =
                serde_json::from_str(&content).map_err(|_| RepositoryError::CorruptState)?;
            (
                envelope["payload"].clone(),
                envelope["source"].as_str().map(str::to_owned),
            )
        } else {
            (json!({"definitions":[],"eventRules":[]}), None)
        };
        let count = payload["definitions"]
            .as_array()
            .ok_or(RepositoryError::CorruptState)?
            .len();
        let event_count = payload["eventRules"]
            .as_array()
            .ok_or(RepositoryError::CorruptState)?
            .len();
        let payload = payload.to_string();
        let hash = format!("{:x}", Sha256::digest(payload.as_bytes()));
        Ok(RuleInput {
            revision,
            source,
            payload,
            count,
            event_count,
            hash,
        })
    }
}

fn invalid_cursor() -> RepositoryError {
    RepositoryError::InvalidContent(
        "Rule input cursor does not match the captured task and UTF-8 range".into(),
    )
}

fn storage(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}
