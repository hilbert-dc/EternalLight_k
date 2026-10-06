use super::{MAX_ITEM_BYTES, bounded, bounded_output_item};
use pretty_assertions::assert_eq;
use serde_json::json;

#[test]
fn low_token_items_are_not_rejected_by_a_byte_proxy() {
    let text = "abcdefghijklmnopqrstuvwxyz".repeat(2_000);
    let item = json!({"type":"reasoning","summary":[],
        "content":[{"type":"reasoning_text","text":text}],"encrypted_content":"retain-exactly"});
    assert_eq!(bounded_output_item(&item), Ok(()));
    assert_eq!(bounded(&item.to_string().into(), "input item 0"), Ok(()));
}

#[test]
fn token_and_byte_limits_remain_distinct() {
    assert_eq!(
        bounded(&"界".repeat(12_000).into(), "input item 0"),
        Err("input item 0 exceeds the 10K-token conservative item limit".into())
    );
    assert_eq!(
        bounded(&"a".repeat(MAX_ITEM_BYTES + 1).into(), "input item 0"),
        Err("input item 0 exceeds the 256KiB item byte limit".into())
    );
}
