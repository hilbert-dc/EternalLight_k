use serde_json::{Value, json};

/// The author HTTP DTO includes the complete stored draft. A model write needs
/// only its authoritative receipt; echoing both text representations can exceed
/// the next provider request's item limit after the save has already committed.
pub(crate) fn saved(response: &Value, chapter_id: &Value) -> Result<Value, String> {
    let invalid = || {
        "Saved draft receipt binding is invalid; inspect the stored checkpoint before retrying the write".to_owned()
    };
    if &response["chapterId"] != chapter_id {
        return Err(invalid());
    }
    for field in ["chapterId", "checkpointId", "revisionId"] {
        if response[field]
            .as_str()
            .is_none_or(|id| id.is_empty() || id.len() > 160)
        {
            return Err(invalid());
        }
    }
    if response["version"].as_u64().is_none() {
        return Err(invalid());
    }
    Ok(
        json!({"chapterId":response["chapterId"], "checkpointId":response["checkpointId"],
        "revisionId":response["revisionId"], "version":response["version"],
        "saved":true,"nextAction":"complete_task",
        "instruction":"This checkpoint is committed. Complete the writer task with this receipt. Original input sourceIds from read_context remain frozen input, including the pre-edit draft; their old text does not prove this save failed. To inspect the saved version, discover an authorized savedCheckpoint source in this task's taskWorkspace with read_source_content(view=workspace), then read its sourceId through the existing parsed or raw source tools. Same-Work decisions can discover actual permitted prior outputs as well. These immutable versions are reading/comparison evidence: isCurrentAtListing only observes the candidate head when listed, and neither historical text nor this receipt substitutes for Native independent assessment and final delivery. Do not rewrite or resave merely because an original input still contains old text."}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn committed_receipt_distinguishes_original_inputs_saved_sources_and_acceptance() {
        let receipt=saved(&json!({"chapterId":"chapter","checkpointId":"checkpoint","revisionId":"revision","version":2,"body":"large draft"}),&json!("chapter")).unwrap();
        assert_eq!(receipt["saved"], true);
        assert_eq!(receipt["revisionId"], "revision");
        assert!(receipt.get("body").is_none());
        let text = receipt["instruction"].as_str().unwrap();
        for required in [
            "Original input sourceIds",
            "frozen input",
            "savedCheckpoint",
            "taskWorkspace",
            "Same-Work",
            "isCurrentAtListing",
            "independent assessment",
        ] {
            assert!(text.contains(required), "{required}")
        }
        assert!(!text.contains("read_context/read_source_content remain its frozen input"));
    }
    #[test]
    fn invalid_receipt_is_never_reinterpreted_as_a_saved_checkpoint() {
        assert!(
            saved(
                &json!({"chapterId":"other","checkpointId":"c","revisionId":"r","version":0}),
                &json!("chapter")
            )
            .is_err()
        );
        assert!(
            saved(
                &json!({"chapterId":"chapter","checkpointId":"c","revisionId":"","version":0}),
                &json!("chapter")
            )
            .is_err()
        );
    }
}
