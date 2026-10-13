use eternallight_engine_core::BridgePolicy;
use serde_json::json;

#[test]
fn child_policy_is_isolated_resettable_and_only_advances_on_valid_requests() {
    let mut policy = BridgePolicy::default();
    let mut input = json!({"body":{"tools":[{"type":"function","name":"first"},{"type":"namespace","name":"ns","tools":[{"name":"second"}]}],"max_output_tokens":200},"outputBudget":100,"identity":{"threadId":"child","parentThreadId":"root"},"policy":{"childFirstAllowed":["first"],"childSubsequentAllowed":["ns.second"]}});
    input["body"]["tool_choice"] = json!({"name":"forbidden"});
    assert!(policy.apply("prepare", &input).is_err());
    input["body"].as_object_mut().unwrap().remove("tool_choice");
    assert_eq!(
        policy.apply("prepare", &input).unwrap(),
        json!({"tools":[{"type":"function","name":"first"}],"max_output_tokens":100})
    );
    assert_eq!(
        policy.apply("prepare", &input).unwrap(),
        json!({"tools":[{"type":"namespace","name":"ns","tools":[{"name":"second"}]}],"max_output_tokens":100})
    );
    input["identity"]["threadId"] = json!("another");
    assert_eq!(
        policy.apply("prepare", &input).unwrap()["tools"],
        json!([{"type":"function","name":"first"}])
    );
    policy.apply("reset", &json!(null)).unwrap();
    input["identity"]["threadId"] = json!("child");
    assert_eq!(
        policy.apply("prepare", &input).unwrap()["tools"],
        json!([{"type":"function","name":"first"}])
    );
    input["childComplete"] = json!(true);
    assert_eq!(
        policy.apply("prepare", &input).unwrap(),
        json!({"tools":[],"max_output_tokens":100,"tool_choice":"none"})
    );
}

#[test]
fn routing_requires_valid_metadata_and_an_explicit_model_route() {
    let mut policy = BridgePolicy::default();
    assert!(
        policy
            .apply(
                "identity",
                &json!("{\"thread_id\":\"r\",\"parent_thread_id\":3}")
            )
            .is_err()
    );
    assert_eq!(
        policy
            .apply("identity", &json!("{\"thread_id\":\"r\"}"))
            .unwrap(),
        json!({"threadId":"r","parentThreadId":null,"turnId":null,"rootTurnId":null,"sessionId":null})
    );
    let mut input = json!({"body":{},"sessionPlan":{"defaultModel":"a","routes":[{"logicalModel":"a","connectionId":"one"}]}});
    assert_eq!(
        policy.apply("route", &input).unwrap(),
        json!({"logicalModel":"a","connectionId":"one"})
    );
    input["body"]["model"] = json!("b");
    assert!(policy.apply("route", &input).is_err());
}
