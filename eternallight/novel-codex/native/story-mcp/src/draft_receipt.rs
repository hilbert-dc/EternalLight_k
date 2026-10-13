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
    let completeness = response["completeness"].as_str().ok_or_else(invalid)?;
    let source = &response["savedSource"];
    let saved_source = if source["status"] == "available"
        && source["revisionId"] == response["revisionId"]
        && source["sourceId"].as_str().is_some_and(|id| !id.is_empty() && id.len() <= 160) {
        json!({"status":"available","sourceId":source["sourceId"],"revisionId":source["revisionId"],"contentRead":false})
    } else { json!({"status":"unavailable","contentRead":false}) };
    Ok(
        json!({"chapterId":response["chapterId"], "checkpointId":response["checkpointId"],
        "revisionId":response["revisionId"], "version":response["version"],
        "saved":true,"completeness":completeness,"savedSource":saved_source,
        "nextAction":if completeness == "chapterComplete" { "complete_task" } else { "continue_writing" },
        "instruction":"Committed. Only chapterComplete satisfies the writer output requirement. For partial prose, finish the authorized request and save actual completeness; do not expand scope or relabel unfinished work. Original input sourceIds are frozen input (pre-edit draft), not proof this save failed. To inspect this savedCheckpoint, use savedSource.read directly when available; taskWorkspace via read_source_content(view=workspace) remains the directory for other versions or unavailable navigation. Same-Work decisions may read permitted prior outputs. isCurrentAtListing observes the head only when listed; historical text and this receipt do not replace required independent assessment or final delivery. Do not resave because frozen input is old."}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn committed_receipt_distinguishes_original_inputs_saved_sources_and_acceptance() {
        let receipt=saved(&json!({"chapterId":"chapter","checkpointId":"checkpoint","revisionId":"revision","version":2,"completeness":"chapterComplete","body":"large draft"}),&json!("chapter")).unwrap();
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
    fn direct_navigation_must_match_the_committed_revision() {
        let mut response = json!({"chapterId":"chapter","checkpointId":"checkpoint","revisionId":"revision","version":2,"completeness":"chapterComplete",
            "savedSource":{"status":"available","sourceId":"source","revisionId":"revision","contentRead":true,"body":"must not echo"}});
        let receipt = saved(&response, &json!("chapter")).unwrap();
        assert_eq!(receipt["savedSource"], json!({"status":"available","sourceId":"source","revisionId":"revision","contentRead":false}));
        response["savedSource"]["revisionId"] = json!("other");
        let receipt = saved(&response, &json!("chapter")).unwrap();
        assert_eq!(receipt["saved"], true);
        assert_eq!(receipt["savedSource"]["status"], "unavailable");
    }
    #[test]
    fn partial_checkpoint_is_saved_but_does_not_direct_task_completion() {
        let mut response=json!({"chapterId":"chapter","checkpointId":"checkpoint","revisionId":"revision","version":1,"completeness":"chapterPartial"});
        let receipt=saved(&response,&json!("chapter")).unwrap();
        assert_eq!(receipt["saved"],true);
        assert_eq!(receipt["completeness"],"chapterPartial");
        assert_eq!(receipt["nextAction"],"continue_writing");
        response.as_object_mut().unwrap().remove("completeness");
        assert!(saved(&response,&json!("chapter")).is_err());
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
