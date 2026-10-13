//! HTTP DTOs shared by handlers and generated TypeScript consumers.
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "ServerSaveArtifactRequest", optional_fields = nullable)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SaveArtifactRequest {
    pub(crate) payload: Value,
    pub(crate) expected_revision: i64,
    pub(crate) expected_candidate_revision_id: Option<String>,
    pub(crate) idempotency_key: String,
    #[serde(default)]
    #[ts(as = "Option<Vec<String>>", optional)]
    pub(crate) linked_impact_ids: Vec<String>,
    pub(crate) task_id: Option<String>,
    pub(crate) thread_id: Option<String>,
}

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "ServerCreateReviewHttpRequest", optional_fields = nullable)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CreateReviewHttpRequest {
    pub(crate) artifact_kind: String,
    pub(crate) revision_id: String,
    pub(crate) modeling_batch_id: Option<String>,
    #[serde(default)]
    #[ts(as = "Option<Vec<Value>>", optional)]
    pub(crate) proposals: Vec<Value>,
    #[serde(default)]
    #[ts(as = "Option<Value>", optional)]
    pub(crate) selections: Value,
    pub(crate) idempotency_key: String,
    pub(crate) task_id: Option<String>,
    pub(crate) thread_id: Option<String>,
    pub(crate) expected_revision: Option<i64>,
}

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "ServerAcceptReviewHttpRequest", optional_fields = nullable)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AcceptReviewHttpRequest {
    pub(crate) expected_revision: i64,
    pub(crate) idempotency_key: String,
    #[serde(default)]
    #[ts(as = "Option<Value>", optional)]
    pub(crate) selections: Value,
}

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "ServerRejectIdentityReviewHttpRequest", optional_fields = nullable)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RejectIdentityReviewHttpRequest {
    pub(crate) expected_revision: i64,
}

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "ServerUndoHttpRequest", optional_fields = nullable)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UndoHttpRequest {
    pub(crate) expected_revision: i64,
    pub(crate) idempotency_key: String,
}

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "ServerLockArtifactRequest", optional_fields = nullable)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LockArtifactRequest {
    pub(crate) locked: bool,
    pub(crate) expected_revision: i64,
}

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "ServerCreateTaskHttpRequest", optional_fields = nullable)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CreateTaskHttpRequest {
    pub(crate) initial_budget: Option<novel_codex_story_core::InitialWorkBudget>,
    pub(crate) operation: String,
    pub(crate) input_revision: i64,
    pub(crate) thread_id: String,
    pub(crate) model: Option<String>,
    #[serde(default)]
    #[ts(as = "Option<Value>", optional)]
    pub(crate) target: Value,
    pub(crate) capability_hash: String,
    pub(crate) capability_expires_at: i64,
    pub(crate) author_intent: String,
    pub(crate) experimental_methods: Option<Vec<String>>,
}

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "ServerUpdateTaskHttpRequest", optional_fields = nullable)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UpdateTaskHttpRequest {
    pub(crate) thread_id: String,
    pub(crate) status: String,
    pub(crate) error: Option<String>,
}

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "ServerCompleteTaskHttpRequest", optional_fields = nullable)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CompleteTaskHttpRequest {
    pub(crate) thread_id: String,
    pub(crate) delivery_review: Option<Value>,
    #[serde(default)]
    #[ts(as = "Option<String>", optional)]
    pub(crate) summary: String,
}

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "ServerBindTaskTurnRequest", optional_fields = nullable)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BindTaskTurnRequest {
    pub(crate) thread_id: String,
    pub(crate) turn_id: String,
}

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "ServerThreadLookupRequest", optional_fields = nullable)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ThreadLookupRequest {
    pub(crate) thread_ids: Vec<String>,
}

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "ServerRecoveryLookupRequest", optional_fields = nullable)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RecoveryLookupRequest {
    pub(crate) task_ids: Vec<String>,
}

#[derive(Serialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "ServerProjectResponse")]
pub(crate) struct ProjectResponse {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) logline: String,
    pub(crate) created_at: i64,
    pub(crate) updated_at: i64,
}

#[derive(Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/")]
pub struct RuntimeErrorEnvelope {
    pub error: RuntimeError,
}

#[derive(Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/")]
pub struct RuntimeError {
    pub code: String,
    pub message: String,
    pub details: Value,
    pub recovery: Option<Value>,
}

#[cfg(test)]
mod wire_semantics {
    use super::*;
    use serde_json::json;
    use ts_rs::TS;

    #[test]
    fn request_optional_null_and_default_match_serde() {
        let minimal = json!({"payload":{},"expectedRevision":0,"idempotencyKey":"save"});
        let request: SaveArtifactRequest = serde_json::from_value(minimal.clone()).unwrap();
        assert!(request.task_id.is_none());
        assert!(request.linked_impact_ids.is_empty());
        let mut explicit = minimal;
        explicit["taskId"] = Value::Null;
        explicit["expectedCandidateRevisionId"] = Value::Null;
        assert!(serde_json::from_value::<SaveArtifactRequest>(explicit.clone()).is_ok());
        explicit["taskId"] = json!(42);
        assert!(serde_json::from_value::<SaveArtifactRequest>(explicit).is_err());
        assert!(SaveArtifactRequest::decl().contains("taskId?: string | null"));
    }
}

#[cfg(test)]
mod authority_wire_semantics {
    use serde_json::json;
    use ts_rs::TS;

    #[test]
    fn explicit_nullable_authority_limits_never_become_optional() {
        use crate::authoring_api::StartRequest;
        let mut value = json!({"expectedProjectRevision":0,"expectedBaseSnapshotId":"s","expectedSourceDigest":"d","authorIntent":"write","targetChapters":1});
        assert!(serde_json::from_value::<StartRequest>(value.clone()).is_err());
        for field in ["maxTaskAttempts", "maxProviderRequests", "maxProviderTokens", "deadlineAt"] { value[field] = serde_json::Value::Null; }
        assert!(serde_json::from_value::<StartRequest>(value).is_ok());
        let declaration = StartRequest::decl();
        assert!(declaration.contains("maxProviderRequests:"));
        assert!(!declaration.contains("maxProviderRequests?:"));
    }

    #[test]
    fn enum_wire_spelling_is_serde_owned() {
        use novel_codex_story_authoring::ReplanStage;
        let value = serde_json::to_value(ReplanStage::Outline).unwrap();
        assert_eq!(value, json!("outline"));
        assert!(ReplanStage::decl().contains("\"outline\""));
    }
}
