use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::{RepositoryError, SqliteStoryRepository, TaskRecord};

struct RuleInput {
    revision: Option<String>,
    source: Option<String>,
    payload: String,
    count: usize,
    event_count: usize,
    hash: String,
}

fn storage(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}

impl SqliteStoryRepository {
    /// Describe the immutable rule input without injecting its entire payload into Context.
    pub fn task_property_rule_input(&self, task: &TaskRecord) -> Result<Value, RepositoryError> {
        self.with_read_snapshot(|| {
            let input = self.load_property_rule_input(task)?;
            Ok(json!({"id":input.revision,"source":input.source,"definitionCount":input.count,
                "eventRuleCount":input.event_count,
                "payloadBytes":input.payload.len(),"contentHash":input.hash,

                "payload":serde_json::from_str::<Value>(&input.payload).map_err(|_|RepositoryError::CorruptState)?}))
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
            "SELECT COALESCE(candidate_revision_id,accepted_revision_id) FROM story_task_source_heads_v2 WHERE task_id=?1 AND object_id='artifact-propertyRules'",
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
