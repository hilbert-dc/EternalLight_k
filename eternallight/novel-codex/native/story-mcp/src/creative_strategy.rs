use serde_json::{Value, json};
use std::sync::LazyLock;

// Compiled into the bundled MCP binary: no caller-selected paths or network fetches.
static CATALOG: LazyLock<Value> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../../shared/creative-strategies.json"))
        .expect("checked creative strategy catalog")
});

pub(super) fn allowed(operation: &str) -> bool {
    matches!(
        operation,
        "advisory"
            | "generateBrief"
            | "generateOutline"
            | "generatePlan"
            | "generatePropertyRules"
            | "reviewPlanningSeat"
            | "generateChapter"
            | "assessSceneOutcome"
            | "assessRevisionOutcome"
            | "assessChapterStructure"
            | "reviseCommitment"
    )
}

pub(super) fn spec() -> Value {
    let mut ids = vec![json!("catalog")];
    ids.extend(
        CATALOG["strategies"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["id"].clone()),
    );
    json!({
        "name": "read_creative_strategy",
        "description": "Read optional creative methods, not story evidence or quality certification. Choose catalog for short applicability descriptions, or one strategyId for the method. No story access, writes, or extra authority. Use only when the author's task benefits; do not load every method.",
        "inputSchema": {
            "type":"object", "additionalProperties":false,
            "properties":{
                "threadId":{"type":"string","minLength":1,"maxLength":160},
                "strategyId":{"type":"string","enum":ids}
            },
            "required":["threadId","strategyId"]
        }
    })
}

pub(super) fn read(id: &str) -> Result<Value, String> {
    let entries = CATALOG["strategies"].as_array().unwrap();
    let content = if id == "catalog" {
        json!(entries.iter().map(|entry| json!({
            "id":entry["id"], "title":entry["title"], "when":entry["when"], "skip":entry["skip"]
        })).collect::<Vec<_>>())
    } else {
        entries
            .iter()
            .find(|entry| entry["id"] == id)
            .cloned()
            .ok_or("Unknown creative strategy")?
    };
    Ok(json!({
        "strategyId":id, "status":CATALOG["status"], "authority":CATALOG["authority"],
        "storyEvidence":false, "content":content
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalog_is_bounded_complete_and_advisory() {
        let entries = CATALOG["strategies"].as_array().unwrap();
        assert_eq!(entries.len(), 7);
        let mut ids = std::collections::BTreeSet::new();
        for entry in entries {
            let id = entry["id"].as_str().unwrap();
            assert!(ids.insert(id));
            for field in ["title", "when", "skip", "deliverable", "stop"] {
                assert!(!entry[field].as_str().unwrap().trim().is_empty());
            }
            assert!(!entry["inputs"].as_array().unwrap().is_empty());
            assert!(!entry["steps"].as_array().unwrap().is_empty());
            let result = read(id).unwrap();
            assert_eq!(result["status"], "unvalidated");
            assert_eq!(result["storyEvidence"], false);
            assert!(result.to_string().len() < 10000);
        }
        assert!(read("../secrets").is_err());
        assert!(
            read("catalog").unwrap()["content"][0]
                .get("steps")
                .is_none()
        );
    }
    #[test]
    fn production_call_checks_identity_schema_operation_and_lifecycle() {
        let client = super::super::RuntimeClient {
            argument_validation: Default::default(),
            publication_gate: Default::default(),
            read_progress: Default::default(),
            host: "127.0.0.1".into(),
            port: 9,
            candidate_token: "test".into(),
            capability: "test".into(),
            operation: "advisory".into(),
        };
        let args = json!({"threadId":"root","strategyId":"reader_review"});
        let child = json!({"rootThreadId":"root","callerThreadId":"child","parentThreadId":"root","sessionId":"root"});
        let (status, report) = client
            .call("read_creative_strategy", &args, &child)
            .unwrap();
        assert_eq!(status, 200);
        assert_eq!(report["authority"], "advisoryMethodOnly");
        assert!(
            client
                .call("read_creative_strategy", &args, &json!({}))
                .is_err()
        );
        assert!(
            client
                .call(
                    "read_creative_strategy",
                    &json!({"threadId":"other","strategyId":"catalog"}),
                    &child
                )
                .is_err()
        );
        assert!(
            client
                .call(
                    "read_creative_strategy",
                    &json!({"threadId":"root","strategyId":"unknown"}),
                    &child
                )
                .is_err()
        );
        let mut denied = client;
        denied.operation = "modelKnowledge".into();
        assert!(
            !denied
                .operation_tools()
                .iter()
                .any(|tool| tool["name"] == "read_creative_strategy")
        );
        assert!(
            denied
                .call("read_creative_strategy", &args, &child)
                .is_err()
        );
        denied.operation = "advisory".into();
        denied.publication_gate.observe("publish_knowledge_batch",
            &json!({"projectId":"p","taskId":"t","threadId":"root","batchId":"b"}),
            &(200,json!({"publicationState":"committed","nextAction":"complete_task","batchId":"b","receiptRef":"r"})));
        assert!(
            denied
                .call("read_creative_strategy", &args, &child)
                .is_err()
        );
    }
}
