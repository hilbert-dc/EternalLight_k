use std::cell::RefCell;
use serde_json::{Value, json};

// Only an acknowledged Runtime completion is cached. This does not reopen the
// revoked task capability, read current state, or execute a post-completion call.
#[derive(Default)]
pub(super) struct CompletionReceipt(RefCell<Option<(Value, Value)>>);
fn binding(args: &Value) -> Value {
    json!([args["projectId"], args["taskId"], args["threadId"], args["expectedRevision"]])
}
impl CompletionReceipt {
    pub(super) fn recall(&self, name: &str, args: &Value, meta: &Value) -> Result<Option<(u16, Value)>, String> {
        let saved = self.0.borrow();
        let Some((bound, receipt)) = saved.as_ref() else { return Ok(None); };
        if args["projectId"] != bound[0] || args["taskId"] != bound[1]
            || args["threadId"] != bound[2] || meta["callerThreadId"] != args["threadId"]
            || meta["rootThreadId"] != args["threadId"] || !meta["parentThreadId"].is_null() {
            return Err("Completion receipt belongs to its exact root task binding".into());
        }
        if name == "complete_task" && args["expectedRevision"] == bound[3] {
            return Ok(Some((200, receipt.clone())));
        }
        Ok(Some((409, json!({"code":"task_already_completed","requestedOperationExecuted":false,
            "completion":receipt,"instruction":"This task has already completed. The acknowledged receipt below remains valid after context compaction. Do not restart, switch task IDs, request authorization or repeat tools. Give the author a final reply using the saved receipt and previously verified results; do not reinterpret a closed capability as evidence that earlier actions failed."}))))
    }
    pub(super) fn observe(&self, args: &Value, response: &mut (u16, Value)) -> Result<(), String> {
        if !(200..300).contains(&response.0) { return Ok(()); }
        let task = &response.1;
        if task["id"] != args["taskId"] || task["projectId"] != args["projectId"]
            || task["threadId"] != args["threadId"] || task["status"] != "completed"
            || !task["resultSummary"].is_string() {
            return Err("Task completion receipt binding is invalid; saved state may need reconciliation".into());
        }
        let summary = task["resultSummary"].as_str().unwrap();
        let receipt = json!({"taskId":task["id"],"projectId":task["projectId"],"threadId":task["threadId"],
            "status":"completed","deliveryStatus":task["deliveryStatus"],"summary":summary.chars().take(800).collect::<String>(),
            "summaryTruncated":summary.chars().count()>800,"authority":"acknowledgedRuntimeCompletion",
            "nextAction":"finalReply","instruction":"The bound task is complete and its execution capability is closed. Give the final reply in the author's language using already verified results. Do not call more tools or reopen the task."});
        *self.0.borrow_mut() = Some((binding(args), receipt.clone()));
        response.1 = receipt;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn acknowledged_completion_is_bounded_and_does_not_reexecute_after_compaction() {
        let cache = CompletionReceipt::default();
        let args = json!({"projectId":"p","taskId":"t","threadId":"root","expectedRevision":1});
        let meta = json!({"rootThreadId":"root","callerThreadId":"root"});
        let mut response = (200, json!({"id":"t","projectId":"p","threadId":"root","status":"completed","resultSummary":"已保存计划","inputManifest":{"large":"文".repeat(10000)}}));
        cache.observe(&args, &mut response).unwrap();
        assert!(response.1.to_string().len()<1500);
        assert_eq!(cache.recall("complete_task", &args, &meta).unwrap(), Some(response));
        for tool in ["read_context","read_work_plan","update_work_plan","request_work_action"] {
            let (code, receipt) = cache.recall(tool, &args, &meta).unwrap().unwrap();
            assert_eq!(code,409);
            assert_eq!(receipt["requestedOperationExecuted"],false);
            assert_eq!(receipt["completion"]["summary"],"已保存计划");
        }
        // Context compaction can lose the CAS version, not the trusted identity.
        // Return the terminal fact without executing even a completion retry.
        for revision in [json!(0), json!(3), Value::Null] {
            let mut stale = args.clone(); stale["expectedRevision"] = revision;
            for tool in ["read_work_status", "complete_task", "request_work_action"] {
                let (code, closed) = cache.recall(tool, &stale, &meta).unwrap().unwrap();
                assert_eq!(code, 409);
                assert_eq!(closed["requestedOperationExecuted"], false);
                assert_eq!(closed["completion"]["status"], "completed");
            }
        }
        let mut foreign = args.clone(); foreign["taskId"] = json!("other");
        assert!(cache.recall("read_context", &foreign, &meta).is_err());
        let child = json!({"rootThreadId":"root","callerThreadId":"child","parentThreadId":"root"});
        assert!(cache.recall("read_context", &args, &child).is_err());
    }
    #[test]
    fn failed_unknown_or_foreign_completions_never_become_receipts() {
        let args = json!({"projectId":"p","taskId":"t","threadId":"root","expectedRevision":1});
        for response in [(503,json!({"error":"unknown"})),(200,json!({"id":"other","status":"completed"}))] {
            let cache = CompletionReceipt::default();
            let mut response = response;
            let _ = cache.observe(&args,&mut response);
            assert!(cache.recall("read_context", &args, &Value::Null).unwrap().is_none());
        }
    }
}
