use eternallight_engine_core::{AppServerTurn, RpcPending};
use serde_json::json;

#[test]
fn late_start_keeps_cancellation_and_rejects_turn_identity_changes() {
    let mut turn = AppServerTurn::default();
    turn.cancel();
    turn.bind_thread(1, "root".into()).unwrap();
    assert!(turn.bind_started(Some("turn".into())).unwrap());
    assert!(turn.bind_started(Some("different".into())).is_err());
    assert_eq!(
        turn.snapshot(),
        json!({"generation":1,"threadId":"root","turnId":"turn","cancellationRequested":true,"terminal":false})
    );
    turn.finish();
    assert!(!turn.bind_started(Some("other".into())).unwrap());
}

#[test]
fn turn_discards_foreign_generation_and_child_output() {
    let mut turn = AppServerTurn::default();
    turn.bind_thread(2, "root".into()).unwrap();
    turn.bind_started(Some("turn".into())).unwrap();
    let mut delta = json!({"method":"item/agentMessage/delta","params":{"threadId":"root","turnId":"turn","itemId":"message","delta":"root response"}});
    turn.observe(2, &delta).unwrap();
    delta["params"]["delta"] = json!("old generation");
    turn.observe(1, &delta).unwrap();
    delta["params"]["threadId"] = json!("child");
    turn.observe(2, &delta).unwrap();
    let result = turn.observe(2,&json!({"method":"turn/completed","params":{"threadId":"root","turn":{"id":"turn","status":"completed"}}})).unwrap();
    assert_eq!(
        result,
        json!({"interrupt":false,"completion":{"result":{"mode":"agent-engine","message":"root response","turn":{"id":"turn","status":"completed"}}}})
    );
}

#[test]
fn bounded_unicode_reply_is_not_split_inside_a_character() {
    let mut turn = AppServerTurn::default();
    turn.bind_thread(1, "root".into()).unwrap();
    turn.bind_started(Some("turn".into())).unwrap();
    turn.observe(1,&json!({"method":"item/agentMessage/delta","params":{"threadId":"root","turnId":"turn","itemId":"message","delta":"🌙".repeat(40_000)}})).unwrap();
    let result = turn.observe(1,&json!({"method":"turn/completed","params":{"threadId":"root","turn":{"id":"turn","status":"completed"}}})).unwrap();
    assert_eq!(
        result["completion"]["result"]["message"],
        json!("🌙".repeat(32_768))
    );
}

#[test]
fn rpc_timeouts_and_generation_checks_cannot_settle_a_different_request() {
    let mut rpc = RpcPending::default();
    let id = rpc
        .apply(
            "register",
            &json!({"method":"initialize","generation":1,"now":10,"timeoutMs":20}),
        )
        .unwrap();
    assert_eq!(
        rpc.apply(
            "response",
            &json!({"generation":2,"message":{"id":id,"result":"wrong generation"}})
        )
        .unwrap(),
        json!(null)
    );
    assert_eq!(rpc.apply("expire", &json!({"now":29})).unwrap(), json!([]));
    assert_eq!(
        rpc.apply("expire", &json!({"now":30})).unwrap(),
        json!([{"id":id,"error":"Codex App Server request timed out: initialize"}])
    );
    assert_eq!(
        rpc.apply(
            "response",
            &json!({"generation":1,"message":{"id":id,"result":"late"}})
        )
        .unwrap(),
        json!(null)
    );
    let next = rpc
        .apply(
            "register",
            &json!({"method":"thread/start","generation":2,"now":31,"timeoutMs":20}),
        )
        .unwrap();
    assert_ne!(next, id);
    assert_eq!(
        rpc.apply(
            "response",
            &json!({"generation":2,"message":{"id":next,"method":"approval","params":{}}})
        )
        .unwrap(),
        json!(null)
    );
    assert_eq!(
        rpc.apply(
            "response",
            &json!({"generation":2,"message":{"id":next,"result":{"thread":{"id":"root"}}}})
        )
        .unwrap(),
        json!({"id":next,"result":{"thread":{"id":"root"}}})
    );
}
