//! Verify serialization of the supported tool schema representation.

use super::JsonSchema;
use pretty_assertions::assert_eq;

#[test]
fn json_schema_serializes_encrypted_marker() {
    let schema = JsonSchema::string(Some("Secret value".to_string())).with_encrypted();

    assert_eq!(
        serde_json::to_value(schema).expect("serialize schema"),
        serde_json::json!({
            "type": "string",
            "description": "Secret value",
            "encrypted": true,
        })
    );
}

#[test]
fn json_schema_roundtrips_unknown_constraints_and_raw_conditional_children() {
    let original = serde_json::json!({
        "type":"object","additionalProperties":false,
        "properties":{"id":{"type":"string","pattern":"^[a-f0-9]{64}$","maxLength":64},"count":{"type":"integer","exclusiveMinimum":0,"exclusiveMaximum":100},"items":{"type":"array","maxItems":4,"uniqueItems":true}},
        "dependentRequired":{"id":["count"]},
        "allOf":[{"if":{"properties":{"count":{"const":1}}},"then":{"required":["id"]},"else":{"not":{"required":["id"]}}}],
        "x-annotation":{"version":1}
    });
    let schema: JsonSchema =
        serde_json::from_value(original.clone()).expect("typed plus uninterpreted fields");
    assert_eq!(
        serde_json::to_value(schema).expect("actual serialized schema"),
        original
    );
}
