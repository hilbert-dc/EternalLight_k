use super::*;
fn setup() -> (McpAuthority, Value, Value, Value) {
    let mut authority = McpAuthority::default();
    let schema = json!({"type":"object","properties":{"kind":{"const":"scene"}},"if":{"required":["kind"]},"then":{"required":["text"]},"additionalProperties":false});
    let catalog = authority.register(&json!({"scope":"task","namespace":"mcp__workspace","tools":[{"name":"save","inputSchema":schema}]})).unwrap();
    let lease = json!({"leaseId":"lease","epoch":1});
    let identity = json!({"threadId":"child","sessionId":"root","parentThreadId":"root","turnId":"turn","agentRole":"writer"});
    (authority, catalog, lease, identity)
}
#[test]
fn restores_only_present_declarations_and_rejects_catalog_drift() {
    let (mut authority, _, _, _) = setup();
    let mut request = json!({"tools":[{"type":"namespace","name":"mcp__workspace","tools":[{"type":"function","name":"save","parameters":{}}]}]});
    authority.prepare(&mut request, "task").unwrap();
    assert_eq!(
        request["tools"][0]["tools"][0]["parameters"]["properties"]["kind"]["const"],
        "scene"
    );
    assert_eq!(request["tools"].as_array().unwrap().len(), 1);
    let mut flat =
        json!({"tools":[{"type":"function","name":"mcp__workspace__save","parameters":{}}]});
    authority.prepare(&mut flat, "task").unwrap();
    assert_eq!(
        flat["tools"][0]["parameters"],
        request["tools"][0]["tools"][0]["parameters"]
    );
    flat["tools"][0]["name"] = json!("mcp__workspace__unknown");
    assert!(authority.prepare(&mut flat, "task").is_err());
    assert!(
        authority
            .register(&json!({"scope":"task","namespace":"mcp__workspace","tools":[]}))
            .is_err()
    );
    assert!(authority.prepare(&mut request, "other").is_err());
}
#[test]
fn invocation_correlates_parallel_calls_and_rejects_forgery_and_stale_leases() {
    let (mut authority, catalog, lease, identity) = setup();
    let mut input = json!({"scope":"task","namespace":"mcp__workspace","name":"save","catalogDigest":catalog["catalogDigest"],"arguments":{"text":"正文"},"meta":{"threadId":"child","callId":"call1","sessionId":"root","agentRole":"forged"}});
    for call in ["call1", "call2"] {
        authority.record("task",&lease,&identity,"batch",&json!({"type":"function_call","namespace":"mcp__workspace","name":"save","call_id":call,"arguments":"{\"text\":\"正文\"}"})).unwrap();
        input["meta"]["callId"] = json!(call);
        let resolved = authority.resolve(&input, &lease).unwrap();
        assert_eq!(resolved["samplingRequestId"], "batch");
        assert_eq!(resolved["agentRole"], "writer");
        assert_eq!(resolved["rootThreadId"], "root");
    }
    for (pointer, value) in [
        ("/scope", json!("other")),
        ("/arguments/text", json!("替换")),
        ("/meta/threadId", json!("other")),
        ("/meta/sessionId", json!("other")),
        ("/catalogDigest", json!("stale")),
    ] {
        let mut forged = input.clone();
        *forged.pointer_mut(pointer).unwrap() = value;
        assert!(authority.resolve(&forged, &lease).is_err(), "{pointer}");
    }
    assert!(
        authority
            .resolve(&input, &json!({"leaseId":"lease","epoch":2}))
            .is_err()
    );
    authority.begin_execution();
    assert!(authority.resolve(&input, &lease).is_err());
}
