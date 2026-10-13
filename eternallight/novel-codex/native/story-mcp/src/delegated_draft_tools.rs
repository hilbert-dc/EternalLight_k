use serde_json::{Value, json};
pub(crate) fn specs() -> Vec<Value> {
    [
        ("save_delegated_draft","Direct candidate_writer created by the parent only (trusted runtime identity binds permission automatically): save an immutable task-local candidate, not the manuscript, task output, adoption or proof of delivery. Return candidateId to parent; root integrates through ordinary save_draft_checkpoint(delegatedCandidateId) under current CAS and protected-source checks. Use expectedDelegatedCandidateId=null for first save, then the exact current branch ID; retries reuse idempotencyKey and identical content.",json!({"payload":super::draft_schema::transport_payload(),"expectedDelegatedCandidateId":{"type":["string","null"],"minLength":1,"maxLength":160},"idempotencyKey":{"type":"string","minLength":1,"maxLength":160}}),vec!["payload","expectedDelegatedCandidateId","idempotencyKey"]),
        ("read_delegated_draft","Read task-local isolated candidates. Omit candidateId to list latest branches in pages of eight with total/nextCursor; copy returned nextCursor to see remaining branches. Parent can read any candidate in its task, a child only its own. With candidateId, copy the immutable ID and returned nextCursor to page content. This is not a saved manuscript or root source-read certificate.",json!({"candidateId":{"type":"string","minLength":1,"maxLength":160},"cursor":{"type":"integer","minimum":0}}),vec![]),
    ].into_iter().map(|(name,description,extra,required)|{
        let mut properties=json!({"taskId":{"type":"string","minLength":1,"maxLength":160},"projectId":{"type":"string","minLength":1,"maxLength":160},"threadId":{"type":"string","minLength":1,"maxLength":160},"expectedRevision":{"type":"integer","minimum":0}});
        properties.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        let required=["taskId","projectId","threadId","expectedRevision"].into_iter().chain(required).collect::<Vec<_>>();
        json!({"name":name,"description":description,"inputSchema":{"type":"object","additionalProperties":false,"properties":properties,"required":required}})
    }).collect()
}
pub(crate) fn request(
    name: &str,
    args: &serde_json::Map<String, Value>,
) -> Result<(&'static str, String, Option<Value>), String> {
    let base = format!(
        "/api/projects/{}/tasks/{}",
        super::percent_encode(args["projectId"].as_str().unwrap_or_default()),
        super::percent_encode(args["taskId"].as_str().unwrap_or_default())
    );
    if name == "read_delegated_draft" {
        let mut path = format!(
            "{base}/delegated-drafts?threadId={}&expectedRevision={}",
            super::percent_encode(args["threadId"].as_str().unwrap_or_default()),
            args["expectedRevision"]
        );
        if let Some(id) = args.get("candidateId").and_then(Value::as_str) {
            path.push_str(&format!("&candidateId={}", super::percent_encode(id)));
        }
        if let Some(cursor) = args.get("cursor").and_then(Value::as_u64) {
            path.push_str(&format!("&cursor={cursor}"));
        }
        return Ok(("GET", path, None));
    }
    Ok((
        "POST",
        format!("{base}/delegated-drafts"),
        Some(super::without(args, &["projectId", "taskId"])),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_directory_and_content_pages_omit_optional_fields_without_panicking() {
        let mut binding=json!({"taskId":"task","projectId":"project","threadId":"root","expectedRevision":0});
        let (_,path,_)=request("read_delegated_draft",binding.as_object().unwrap()).unwrap();
        assert_eq!(path,"/api/projects/project/tasks/task/delegated-drafts?threadId=root&expectedRevision=0");
        binding["candidateId"]=json!("immutable-candidate");
        let (_,path,_)=request("read_delegated_draft",binding.as_object().unwrap()).unwrap();
        assert!(path.ends_with("&candidateId=immutable-candidate"));
        binding.as_object_mut().unwrap().remove("candidateId");binding["cursor"]=json!(8);
        let (_,path,_)=request("read_delegated_draft",binding.as_object().unwrap()).unwrap();
        assert!(path.ends_with("&cursor=8"));
    }
}
