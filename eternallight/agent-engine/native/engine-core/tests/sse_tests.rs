use eternallight_engine_core::SseDecoder;
use pretty_assertions::assert_eq;
use serde_json::json;

#[test]
fn byte_boundaries_preserve_unicode_crlf_and_multiline_data() {
    let input = "data: {\r\ndata: \"text\":\"你好🌙\"}\r\n\r\ndata: [DONE]";
    for width in 1..=input.len() {
        let mut decoder = SseDecoder::new(4096).unwrap();
        let mut events = Vec::new();
        for chunk in input.as_bytes().chunks(width) {
            events.extend(decoder.push(chunk).unwrap());
        }
        events.extend(decoder.finish().unwrap());
        assert_eq!(
            events,
            vec![json!({"data":{"text":"你好🌙"}}), json!({"done":true})]
        );
    }
}

#[test]
fn overflow_invalid_frames_and_close_are_terminal() {
    for input in [b"data: nope\n\n".as_slice(), b"data: \xff\n\n"] {
        let mut decoder = SseDecoder::new(4096).unwrap();
        let error = decoder.push(input).unwrap_err();
        assert!(!error.contains("private"));
        assert_eq!(decoder.finish().unwrap_err(), "SSE decoder closed");
    }
    let mut decoder = SseDecoder::new(4).unwrap();
    assert_eq!(
        decoder.push(b"12345").unwrap_err(),
        "streaming response exceeds codec limit"
    );
    assert_eq!(decoder.push(b"").unwrap_err(), "SSE decoder closed");
}

#[test]
fn framing_preserves_error_metadata_for_semantic_terminal_handling() {
    let mut decoder = SseDecoder::new(4096).unwrap();
    let event = json!({"type":"error","error":{"code":"server_error"}});
    assert_eq!(
        decoder
            .push(format!("data: {event}\n\n").as_bytes())
            .unwrap(),
        vec![json!({"data":event})]
    );
}
