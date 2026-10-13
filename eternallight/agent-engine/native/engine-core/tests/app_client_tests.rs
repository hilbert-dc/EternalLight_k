use eternallight_engine_core::AppClient;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};

fn client() -> AppClient {
    AppClient::new(json!({"model":"fixture","cwd":"/fixture","requestTimeoutMs":1000,"turnTimeoutMs":5000,"clientInfo":{"name":"fixture","version":"1"}})).unwrap()
}
fn command(client: &mut AppClient, id: u64, method: &str, params: Value) -> Value {
    client
        .apply(
            "command",
            &json!({"id":id,"method":method,"params":params,"now":1}),
        )
        .unwrap()
}
fn message(client: &mut AppClient, value: Value) -> Value {
    client
        .apply(
            "transport",
            &json!({"generation":1,"event":{"kind":"message","value":value}}),
        )
        .unwrap()
}
fn initialize(client: &mut AppClient) {
    command(client, 1, "ensureSession", json!({}));
    message(client, json!({"id":1,"result":{}}));
    message(client, json!({"id":2,"result":{"thread":{"id":"root"}}}));
}

#[test]
fn resume_remains_exclusive_until_its_rpc_returns() {
    let mut client = client();
    initialize(&mut client);
    command(
        &mut client,
        2,
        "resumeSession",
        json!({"threadId":"restored"}),
    );
    assert_eq!(
        command(
            &mut client,
            3,
            "sendTurn",
            json!({"input":[{"type":"text","text":"hello"}]})
        ),
        json!([{"kind":"reject","id":3,"error":"Codex App Server already has an active operation","code":null}])
    );
    assert_eq!(
        command(&mut client, 4, "ensureSession", json!({})),
        json!([])
    );
    let effects = message(
        &mut client,
        json!({"id":3,"result":{"thread":{"id":"restored"}}}),
    );
    assert_eq!(
        effects,
        json!([{"kind":"resolve","id":2,"result":"restored"},{"kind":"resolve","id":4,"result":"restored"},{"kind":"event","message":{"id":3,"result":{"thread":{"id":"restored"}}}}])
    );
}

#[test]
fn late_start_response_cannot_bind_the_next_turn() {
    let mut client = client();
    initialize(&mut client);
    command(
        &mut client,
        2,
        "sendTurn",
        json!({"input":[{"type":"text","text":"first"}]}),
    );
    message(
        &mut client,
        json!({"method":"turn/started","params":{"threadId":"root","turn":{"id":"old"}}}),
    );
    message(
        &mut client,
        json!({"method":"turn/completed","params":{"threadId":"root","turn":{"id":"old","status":"completed"}}}),
    );
    command(
        &mut client,
        3,
        "sendTurn",
        json!({"input":[{"type":"text","text":"second"}]}),
    );
    message(&mut client, json!({"id":3,"result":{"turn":{"id":"old"}}}));
    assert_eq!(
        client.apply("inspect", &json!({})).unwrap(),
        json!({"generation":1,"threadId":"root","turnId":null,"activeLease":null,"executionRevoked":false})
    );
    message(&mut client, json!({"id":4,"result":{"turn":{"id":"new"}}}));
    assert_eq!(
        client.apply("inspect", &json!({})).unwrap(),
        json!({"generation":1,"threadId":"root","turnId":"new","activeLease":null,"executionRevoked":false})
    );
}

#[test]
fn close_completion_is_fenced_by_the_native_process_generation() {
    let mut client = client();
    initialize(&mut client);
    assert_eq!(
        command(&mut client, 2, "close", json!({"timeoutMs":25})),
        json!([{"kind":"retire","generation":1,"timeoutMs":25}])
    );
    assert_eq!(
        client.apply("retired", &json!({"generation":0})).unwrap(),
        json!([])
    );
    assert_eq!(
        client.apply("retired", &json!({"generation":1})).unwrap(),
        json!([{"kind":"resolve","id":2,"result":null}])
    );
    command(&mut client, 3, "ensureSession", json!({}));
    assert_eq!(
        client
            .apply(
                "transport",
                &json!({"generation":1,"event":{"kind":"message","value":{"id":4,"result":{}}}})
            )
            .unwrap(),
        json!([])
    );
}

#[test]
fn revoked_execution_requires_a_new_lease_and_a_monotonic_epoch() {
    let mut client = client();
    command(
        &mut client,
        1,
        "bindExecution",
        json!({"lease":{"runId":"run","leaseId":"lease","epoch":2}}),
    );
    assert_eq!(
        command(&mut client, 2, "revokeExecution", json!({"revokedEpoch":2})),
        json!([{"kind":"reject","id":2,"error":"App Server revoked epoch must advance","code":null}])
    );
    assert_eq!(
        command(&mut client, 3, "revokeExecution", json!({})),
        json!([{"kind":"resolve","id":3,"result":{"revokedEpoch":3}}])
    );
    assert_eq!(
        command(
            &mut client,
            4,
            "sendTurn",
            json!({"input":[{"type":"text","text":"hello"}]})
        ),
        json!([{"kind":"reject","id":4,"error":"Codex execution has been revoked","code":"TASK_CANCELLED"}])
    );
}
