//! Constructs the persistent input envelope for a new author-originated task.
use crate::{CreateTaskRequest, ProjectId};
use serde_json::{Value, json};
use uuid::Uuid;

/// Internal domain input; HTTP transport, capability validation and funding stay
/// with their existing callers and transactional admission paths.
pub struct AuthorTaskSubmission {
    pub project_id: ProjectId,
    pub thread_id: String,
    pub operation: String,
    pub input_revision: i64,
    pub model: Option<String>,
    pub target: Value,
    pub author_intent: String,
    pub experimental_methods: Option<Vec<String>>,
    pub capability_hash: String,
    pub capability_expires_at: i64,
}

impl CreateTaskRequest {
    pub fn from_author_submission(input: AuthorTaskSubmission) -> Self {
        let target = if input.target.is_object() {
            input.target
        } else {
            json!({})
        };
        let mut manifest = json!({
            "operation": input.operation,
            "projectId": input.project_id.as_str(),
            "projectRevision": input.input_revision,
            "target": target,
            "authorIntent": input.author_intent,
            "expectedCandidateRevisionId": null,
            "branchContext": null
        });
        if let Some(methods) = input.experimental_methods {
            manifest["experimentalMethods"] = json!(methods);
        }
        Self {
            task_id: format!("task-{}", Uuid::new_v4().simple()),
            project_id: input.project_id,
            thread_id: input.thread_id,
            operation: input.operation,
            input_revision: input.input_revision,
            model: input.model,
            target_json: target.to_string(),
            input_manifest_json: manifest.to_string(),
            capability_hash: input.capability_hash,
            capability_expires_at: input.capability_expires_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn submission(target: Value, methods: Option<Vec<String>>) -> AuthorTaskSubmission {
        AuthorTaskSubmission {
            project_id: ProjectId::parse("book").unwrap(),
            thread_id: "thread".into(),
            operation: "advisory".into(),
            input_revision: 7,
            model: None,
            target,
            author_intent: "review this scene".into(),
            experimental_methods: methods,
            capability_hash: "a".repeat(64),
            capability_expires_at: 200,
        }
    }

    #[test]
    fn author_manifest_keeps_target_and_revision_bound_to_persisted_request() {
        let request = CreateTaskRequest::from_author_submission(submission(
            json!({"chapterId":"chapter-1"}),
            Some(vec!["reader".into()]),
        ));
        let manifest: Value = serde_json::from_str(&request.input_manifest_json).unwrap();
        assert_eq!(
            manifest["target"],
            serde_json::from_str::<Value>(&request.target_json).unwrap()
        );
        assert_eq!(manifest["projectId"], request.project_id.as_str());
        assert_eq!(manifest["projectRevision"], request.input_revision);
        assert_eq!(manifest["operation"], request.operation);
        assert_eq!(manifest["authorIntent"], "review this scene");
        assert_eq!(manifest["experimentalMethods"], json!(["reader"]));
        assert!(manifest["branchContext"].is_null());
        assert!(manifest["expectedCandidateRevisionId"].is_null());
        assert!(Uuid::parse_str(request.task_id.strip_prefix("task-").unwrap()).is_ok());
    }

    #[test]
    fn absent_target_and_methods_do_not_invent_context_or_reuse_identity() {
        let first = CreateTaskRequest::from_author_submission(submission(Value::Null, None));
        let second = CreateTaskRequest::from_author_submission(submission(Value::Null, None));
        let manifest: Value = serde_json::from_str(&first.input_manifest_json).unwrap();
        assert_eq!(manifest["target"], json!({}));
        assert_eq!(first.target_json, "{}");
        assert!(manifest.get("experimentalMethods").is_none());
        assert_ne!(first.task_id, second.task_id);
    }
}
