use std::cell::RefCell;

use serde_json::Value;

/// A session-local gate derived only from an acknowledged Runtime receipt.
/// The Runtime remains authoritative after transport failures and restarts.
#[derive(Default)]
pub(super) struct PublicationGate {
    completed_publication: RefCell<Option<[String; 3]>>,
}

impl PublicationGate {
    pub(super) fn allows_tool(&self, name: &str) -> bool {
        self.completed_publication.borrow().is_none() || name == "complete_task"
    }

    pub(super) fn check(
        &self,
        name: &str,
        project: &str,
        task: &str,
        thread: &str,
    ) -> Result<(), String> {
        if let Some(binding) = self.completed_publication.borrow().as_ref() {
            if binding
                .iter()
                .map(String::as_str)
                .ne([project, task, thread])
            {
                return Err("Publication completion belongs to the bound root task".into());
            }
            if name != "complete_task" {
                return Err("Knowledge publication committed; call complete_task now. Other tools are closed.".into());
            }
        }
        Ok(())
    }

    pub(super) fn observe(&self, name: &str, arguments: &Value, response: &(u16, Value)) {
        let (status, receipt) = response;
        if name == "publish_knowledge_batch"
            && (200..300).contains(status)
            && receipt["publicationState"] == "committed"
            && receipt["nextAction"] == "complete_task"
            && receipt["batchId"] == arguments["batchId"]
            && receipt["receiptRef"]
                .as_str()
                .is_some_and(|value| !value.is_empty())
        {
            *self.completed_publication.borrow_mut() = Some([
                arguments["projectId"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
                arguments["taskId"].as_str().unwrap_or_default().to_owned(),
                arguments["threadId"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
            ]);
        }
    }
}
