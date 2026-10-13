//! Native cursors become directly callable navigation; no calculated offsets.
use serde_json::{Value, json};
fn call(args: Value) -> Value {
    json!({"tool":"read_source_content","arguments":args})
}
fn base(args: &Value, source: &Value) -> Value {
    json!({"expectedRevision":args["expectedRevision"],"sourceId":source})
}
pub(super) fn present(name: &str, args: &Value, result: &mut Value) {
    if name == "save_draft_checkpoint" && result["savedSource"]["status"] == "available" {
        result["savedSource"]["read"] = call(base(args, &result["savedSource"]["sourceId"]));
    }
    if name == "read_source_content" && result.is_object() {
        result["nextRead"] = result
            .get("nextCursor")
            .filter(|v| !v.is_null())
            .map(|cursor| {
                let mut next = args.clone();
                for field in super::tool_binding::FIELDS {
                    next.as_object_mut().unwrap().remove(field);
                }
                next["cursor"] = cursor.clone();
                call(next)
            })
            .unwrap_or(Value::Null);
        // A continuation is a complete call, not a cursor to splice into new filters.
        result.as_object_mut().unwrap().remove("nextCursor");
        if let Some(items) = result.get_mut("items").and_then(Value::as_array_mut) {
            for item in items {
                if let Some(cursor) = item.get("readCursor").cloned() {
                    let mut request = base(args, &args["sourceId"]);
                    request["cursor"] = cursor;
                    item["read"] = call(request);
                    item.as_object_mut().unwrap().remove("readCursor");
                } else if let Some(source) = item.get("sourceId").filter(|v| v.is_string()).cloned()
                {
                    item["read"] = call(base(args, &source));
                }
            }
        }
        if result["representation"] == "sourceIndex" {
            result["instruction"]="Use each item's read request and follow nextRead until null. Reading stays within that record; index coverage is not content reading.".into();
        }
    }
    if matches!(name, "read_context" | "read_revision_outcome") {
        if let Some(entries) = result
            .get_mut("taskSources")
            .and_then(|sources| sources.get_mut("entries"))
            .and_then(Value::as_array_mut)
        {
            for item in entries {
                item["read"] = if item["sectionPath"] == "/taskWorkspace" {
                    call(json!({"expectedRevision":args["expectedRevision"],"view":"workspace"}))
                } else { call(base(args, &item["sourceId"])) };
            }
        }
        if result["taskWorkspace"]["sourceId"].is_string() {
            let mut request = json!({"expectedRevision":args["expectedRevision"]});
            request["view"] = "workspace".into();
            result["taskWorkspace"]["read"] = call(request);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn navigation_preserves_source_view_filters_and_version_without_identity() {
        let args = json!({"projectId":"p","taskId":"t","threadId":"root","expectedRevision":7,"sourceId":"source","view":"workspace","query":"雨"});
        let mut result =
            json!({"nextCursor":"opaque:12","items":[{"sourceId":"child","contentRead":false}]});
        present("read_source_content", &args, &mut result);
        let next = &result["nextRead"]["arguments"];
        assert_eq!(next["cursor"], "opaque:12");
        assert_eq!(next["query"], "雨");
        assert_eq!(next["view"], "workspace");
        assert!(result.get("nextCursor").is_none());
        assert!(next.get("taskId").is_none());
        assert_eq!(
            result["items"][0]["read"]["arguments"],
            json!({"expectedRevision":7,"sourceId":"child"})
        );
        let mut context = json!({"taskWorkspace":{"sourceId":"old-task-directory"}});
        present("read_context", &args, &mut context);
        assert_eq!(context["taskWorkspace"]["read"]["arguments"], json!({"view":"workspace","expectedRevision":7}));
        let mut saved = json!({"savedSource":{"status":"available","sourceId":"saved-source","contentRead":false}});
        present("save_draft_checkpoint", &args, &mut saved);
        assert_eq!(saved["savedSource"]["read"]["arguments"], json!({"expectedRevision":7,"sourceId":"saved-source"}));
        assert_eq!(saved["savedSource"]["contentRead"], false);
        let mut index = json!({"representation":"sourceIndex","items":[{"readCursor":"exact:3","entryRange":{"end":7}}],"nextCursor":null});
        present("read_source_content", &args, &mut index);
        assert_eq!(index["nextRead"], Value::Null);
        assert_eq!(index["items"][0]["read"]["arguments"]["cursor"], "exact:3");
        assert!(index["items"][0].get("readCursor").is_none());
    }
}
