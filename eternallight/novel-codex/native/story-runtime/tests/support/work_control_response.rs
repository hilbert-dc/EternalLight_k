/// Explicitly consume the new command outcome in tests of other Work behavior.
/// There is no old bare-Work response fallback: an applied receipt is required.
pub fn work_control_response(
    uri: &str,
    status: axum::http::StatusCode,
    mut value: serde_json::Value,
) -> serde_json::Value {
    if uri.contains("/work-tasks/") && uri.ends_with("/control") && status.is_success() {
        assert_eq!(value["receipt"]["status"], "applied", "{value}");
        assert_eq!(value["receipt"]["workTaskId"], value["work"]["id"]);
        assert!(value["replayed"].is_boolean());
        value["work"].take()
    } else {
        value
    }
}
