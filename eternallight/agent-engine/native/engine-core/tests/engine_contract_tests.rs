use eternallight_engine_core::{
    ChatStream, ExecutionAuthority, ResponsesStream, TaskBudget, dispatch,
};
use serde_json::{Value, json};

#[test]
fn portable_tool_round_trip_rejects_invalid_arguments_before_commit() {
    let request = json!({"input":[{"type":"message","role":"user","content":"read"}],"tools":[{"type":"namespace","name":"workspace","tools":[{"type":"function","name":"read","parameters":{"type":"object","properties":{"path":{"type":"string"}},"required":["path"],"additionalProperties":false}}]}]});
    let connection = json!({"endpoint":"https://example.test","model":"test","declaredLimits":{"contextTokens":32000,"outputTokens":1024}});
    let prepared = dispatch("codec.encode",json!({"protocol":"chat-completions","request":request,"connection":connection,"apiKey":"test"})).unwrap();
    let wire = prepared["body"]["tools"][0]["function"]["name"].clone();
    let context = prepared["context"].clone();
    for (args, valid) in [("{\"path\":\"chapter.md\"}", true), ("{}", false)] {
        let mut stream = ChatStream::new(context.clone());
        let initial = stream.push(json!({"data":{"id":"test","choices":[{"delta":{"tool_calls":[{"index":0,"id":"call","function":{"name":wire,"arguments":args}}]},"finish_reason":"tool_calls"}]}})).unwrap();
        assert!(
            initial
                .iter()
                .all(|event| event["type"] != "response.output_item.done")
        );
        stream.push(json!({"done":true})).unwrap();
        let result = stream.finish();
        if valid {
            let events = result.unwrap();
            assert_eq!(
                events[0]["item"],
                json!({"id":"fc_call","type":"function_call","call_id":"call","name":"read","namespace":"workspace","arguments":args,"status":"completed"})
            );
        } else {
            assert!(result.unwrap_err().contains("path"));
        }
        assert!(stream.finish().is_err());
    }
}

#[test]
fn responses_rejects_conflicting_terminal_call_without_committing() {
    let context = json!({"registry":null,"tools":[{"type":"function","namespace":null,"name":"read","schema":{"type":"object"}}],"toolChoice":"auto"});
    let call = json!({"type":"function_call","call_id":"call","name":"read","arguments":"{}"});
    let mut stream = ResponsesStream::new(context).unwrap();
    assert_eq!(
        stream
            .push(json!({"type":"response.output_item.done","item":call}))
            .unwrap(),
        Vec::<Value>::new()
    );
    let changed = json!({"type":"function_call","call_id":"call","name":"read","arguments":"{\"changed\":true}"});
    assert!(stream.push(json!({"type":"response.completed","response":{"status":"completed","output":[changed]}})).unwrap_err().contains("differs"));
    assert!(stream.finish().is_err());
}

#[test]
fn budget_snapshot_mutation_cannot_restore_spent_capacity() {
    let mut budget =
        TaskBudget::new(json!({"options":{"id":"budget","maxTokens":10},"now":0})).unwrap();
    let reservation = budget
        .apply(
            "budget.reserve",
            json!({"parameters":{"maxOutputTokens":10},"now":0}),
        )
        .unwrap();
    let mut snapshot = budget.inspect();
    snapshot["reservedTokens"] = 0.into();
    assert!(
        budget
            .apply(
                "budget.reserve",
                json!({"parameters":{"maxOutputTokens":1},"now":0,"state":snapshot})
            )
            .is_err()
    );
    let settled = budget
        .apply(
            "budget.settle",
            json!({"id":reservation,"usage":{"total_tokens":10}}),
        )
        .unwrap();
    assert_eq!(
        budget
            .apply(
                "budget.settle",
                json!({"id":reservation,"usage":{"total_tokens":0}})
            )
            .unwrap(),
        settled
    );
}

#[test]
fn cancellation_requires_drain_and_reconciliation_even_after_snapshot_mutation() {
    let mut authority = ExecutionAuthority::default();
    authority
        .apply(
            "issue",
            json!({"lease":{"runId":"run","leaseId":"lease","planId":"plan","epoch":1}}),
        )
        .unwrap();
    let mut revoked = authority
        .apply(
            "revoke",
            json!({"runId":"run","leaseId":"lease","expectedEpoch":1}),
        )
        .unwrap();
    revoked["state"] = "active".into();
    assert!(
        authority
            .apply(
                "validate",
                json!({"runId":"run","leaseId":"lease","epoch":1,"snapshot":revoked})
            )
            .is_err()
    );
    assert!(
        authority
            .apply("terminalize", json!({"runId":"run","outcome":"cancelled"}))
            .is_err()
    );
    authority.apply("drained", json!({"runId":"run"})).unwrap();
    authority
        .apply("reconciled", json!({"runId":"run"}))
        .unwrap();
    let terminal = authority
        .apply("terminalize", json!({"runId":"run","outcome":"cancelled"}))
        .unwrap();
    assert_eq!(
        authority.apply("snapshot", json!({"runId":"run"})).unwrap(),
        terminal
    );
    assert!(
        authority
            .apply("terminalize", json!({"runId":"run","outcome":"completed"}))
            .is_err()
    );
}
