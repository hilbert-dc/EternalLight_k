use super::*;
use pretty_assertions::assert_eq;
use serde_json::json;

#[test]
fn tagged_union_reports_a_repairable_declared_field_without_echoing_values() {
    let schema = json!({"type":"object","properties":{"assertions":{"type":"array","items":{"anyOf":[
        {"type":"object","required":["kind","owner"],"properties":{"kind":{"enum":["fact"]},"owner":{"type":"string"}}},
        {"type":"object","required":["kind","participants"],"properties":{"kind":{"enum":["event"]},"participants":{"type":"array"}}}
    ]}}}});
    let message = validate(&json!({"assertions":[{"kind":"event","secret":"never echo"}]}), &schema, "publish").unwrap_err();
    let diagnostic: Value = serde_json::from_str(message.strip_prefix(PREFIX).unwrap()).unwrap();
    assert_eq!(diagnostic["reason"], "required");
    assert_eq!(diagnostic["missingField"], "participants");
    assert_eq!(diagnostic["requiredFields"], json!(["kind", "participants"]));
    assert_eq!(diagnostic["schemaPath"], "/properties/assertions/items/anyOf/1/required");
    assert!(!message.contains("secret"));
    assert!(!message.contains("never echo"));
    assert_eq!(diagnostic_message(&message), Some(message));
    let unknown = validate(&json!({"assertions":[{"kind":"other"}]}), &schema, "publish").unwrap_err();
    assert!(unknown.contains("constraint"));
}

#[test]
fn accepted_arguments_are_preserved_and_rejections_use_declared_locations() {
    let schema = json!({"type":"object","required":["payload"],"properties":{
        "payload":{"type":"object","required":["text"],"properties":{"text":{"type":"string","enum":["allowed"]}},"additionalProperties":false}}});
    let valid = json!({"payload":{"text":"allowed"}});
    assert_eq!(validate(&valid, &schema, "save"), Ok(valid));
    for (value, expected) in [
        (
            json!({"payload":{}}),
            json!({"tool":"save","reason":"required","schemaPath":"/properties/payload/required","missingField":"text","requiredFields":["text"]}),
        ),
        (
            json!({"payload":123}),
            json!({"tool":"save","reason":"type","schemaPath":"/properties/payload/type"}),
        ),
        (
            json!({"payload":{"text":"private-input"}}),
            json!({"tool":"save","reason":"enum","schemaPath":"/properties/payload/properties/text/enum"}),
        ),
        (
            json!({"payload":{"text":"allowed","private-key":"private-value"}}),
            json!({"tool":"save","reason":"additionalProperties","schemaPath":"/properties/payload/additionalProperties"}),
        ),
    ] {
        let message = validate(&value, &schema, "save").unwrap_err();
        assert_eq!(diagnostic_message(&message), Some(message.clone()));
        assert_eq!(
            serde_json::from_str::<Value>(message.strip_prefix(PREFIX).unwrap()).unwrap(),
            expected
        );
        assert!(!message.contains("private"));
    }
}

#[test]
fn diagnostic_is_hard_bounded_without_partial_field_or_value_echo() {
    let field = "declared".repeat(30);
    let schema = json!({"type":"object","properties":{field.clone():{"type":"string"}}});
    let message = validate(&json!({field.clone():false}), &schema, &"tool".repeat(30)).unwrap_err();
    assert_eq!(
        serde_json::from_str::<Value>(message.strip_prefix(PREFIX).unwrap()).unwrap(),
        json!({"tool":"unknown","reason":"type","schemaPath":"unknown"})
    );
    assert!(message.len() <= MAX_MESSAGE_BYTES);
    let required = "private-required\n".repeat(20);
    let message = validate(&json!({}), &json!({"required":[required]}), "save").unwrap_err();
    assert_eq!(
        serde_json::from_str::<Value>(message.strip_prefix(PREFIX).unwrap()).unwrap(),
        json!({"tool":"save","reason":"required","schemaPath":"/required"})
    );
    assert!(message.len() <= MAX_MESSAGE_BYTES);
    let spoof = format!(
        "{PREFIX}{}",
        json!({"tool":"save","reason":"required","schemaPath":"/required","private":true})
    );
    assert_eq!(diagnostic_message(&spoof), None);
}
