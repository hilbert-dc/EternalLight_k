use serde_json::{Value, json};

pub(crate) fn allowed(operation: &str, name: &str) -> bool {
    matches!(name, "read_reading_progress" | "checkpoint_reading")
        && super::allowed_tools(operation).contains(&"query_story")
}

pub(crate) fn specs() -> Vec<Value> {
    [
        ("read_reading_progress", "Root only: read the active WorkStep's bounded reading checkpoint. A fresh Attempt receives a newly authorized resumePlan. Stale checkpoints hide their note and cursor. This is model-declared progress, not fact or semantic completeness certification."),
        ("checkpoint_reading", "Root only: acknowledge one intact literal passages page after reading it. Supply its exact plan and contentHash as reportHash, the current expectedVersion (zero initially), and a summary of at most 512 UTF-8 bytes. Runtime regenerates the report, verifies continuous paging and atomically persists one bounded WorkStep checkpoint with a recovery receipt. Do not acknowledge truncated pages. Retry the identical request; otherwise use returned resumePlan/version. Maximum 10000 acknowledged pages per step, including resumed Attempts. No formal content, Canon or task output is written. Acknowledged end does not certify understanding."),
    ].into_iter().map(|(name,description)| {
        let mut properties = json!({
            "taskId":{"type":"string","minLength":1,"maxLength":160},
            "projectId":{"type":"string","minLength":1,"maxLength":160},
            "threadId":{"type":"string","minLength":1,"maxLength":160},
            "expectedRevision":{"type":"integer","minimum":0}
        });
        let mut required = vec!["taskId","projectId","threadId","expectedRevision"];
        if name == "checkpoint_reading" {
            let mut plan = super::query_plan_schema::query_plan_schema("advisory");
            plan["properties"]["question"]["maxLength"] = json!(512);
            plan["properties"]["steps"]["maxItems"] = json!(1);
            plan["properties"]["ranking"] = json!({"type":"object","properties":{"type":{"const":"literal"}},"required":["type"],"additionalProperties":false});
            let operations = plan["properties"]["steps"]["items"]["properties"]["operation"]["oneOf"].as_array_mut().expect("finite query operations");
            operations.retain(|operation| operation["properties"]["type"]["const"]=="passages");
            properties["plan"] = plan;
            properties["reportHash"] = json!({"type":"string","minLength":64,"maxLength":64,"description":"Exact contentHash of the acknowledged report; Runtime regenerates and verifies it."});
            properties["expectedVersion"] = json!({"type":"integer","minimum":0,"maximum":9999});
            properties["summary"] = json!({"type":"string","maxLength":512});
            required.extend(["plan","reportHash","expectedVersion","summary"]);
        }
        json!({"name":name,"description":description,"inputSchema":{"type":"object","properties":properties,"required":required,"additionalProperties":false}})
    }).collect()
}
