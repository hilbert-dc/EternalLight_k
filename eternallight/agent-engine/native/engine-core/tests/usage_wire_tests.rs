use eternallight_engine_core::dispatch;
use pretty_assertions::assert_eq;
use serde::Deserialize;
use serde_json::json;

#[derive(Debug, Deserialize, PartialEq)]
struct Usage {
    input_tokens: i64,
    output_tokens: i64,
    total_tokens: i64,
    input_tokens_details: InputDetails,
    output_tokens_details: OutputDetails,
}
#[derive(Debug, Deserialize, PartialEq)]
struct InputDetails {
    cached_tokens: i64,
    cache_write_tokens: i64,
}
#[derive(Debug, Deserialize, PartialEq)]
struct OutputDetails {
    reasoning_tokens: i64,
}

#[test]
fn converted_usage_survives_integer_only_responses_consumers() {
    let response=dispatch("codec.decode",json!({"protocol":"messages","payload":{"id":"wire","stop_reason":"end_turn","content":[{"type":"text","text":"done"}],"usage":{"input_tokens":10,"cache_read_input_tokens":700,"cache_creation_input_tokens":100,"output_tokens":3}},"context":null})).unwrap();
    let encoded = serde_json::to_string(&response["usage"]).unwrap();
    let decoded: Usage = serde_json::from_str(&encoded).unwrap();
    assert_eq!(
        decoded,
        Usage {
            input_tokens: 810,
            output_tokens: 3,
            total_tokens: 813,
            input_tokens_details: InputDetails {
                cached_tokens: 700,
                cache_write_tokens: 100
            },
            output_tokens_details: OutputDetails {
                reasoning_tokens: 0
            }
        }
    );
}
