use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{cell::RefCell, collections::VecDeque};

/// Advisory feedback only: every read still reaches Runtime for authorization
/// and freshness checks. Never suppress writes or unknown-outcome recovery.
#[derive(Default)]
pub(crate) struct ReadProgress {
    recent: RefCell<VecDeque<(String, String, u16, [u8; 32], usize)>>,
}

impl ReadProgress {
    pub(crate) fn observe(&self, name: &str, hash: &str, response: &mut (u16, Value)) {
        if !matches!(
            name,
            "read_task_source" | "resolve_story_evidence" | "query_story" | "analyze_story_impact"
        ) || !response.1.is_object()
        {
            return;
        }
        let fingerprint: [u8; 32] = Sha256::digest(response.1.to_string().as_bytes()).into();
        let mut recent = self.recent.borrow_mut();
        let position = recent
            .iter()
            .position(|(tool, key, ..)| tool == name && key == hash);
        let count = position
            .and_then(|i| recent.remove(i))
            .filter(|(_, _, status, value, _)| *status == response.0 && *value == fingerprint)
            .map_or(1, |(_, _, _, _, count)| count.saturating_add(1));
        // Store only fingerprints, not potentially large source payloads.
        // The bounded window is scoped to this MCP process / task.
        recent.push_back((name.into(), hash.into(), response.0, fingerprint, count));
        while recent.len() > 8 {
            recent.pop_front();
        }
        if count >= 3 {
            response.1["progressHint"] = json!({"kind":"repeatedUnchangedRead","count":count,
                "instruction":"This exact read has returned unchanged results repeatedly. Reuse the already delivered source/evidence and advance to reasoning or the next returned cursor. If a concrete information gap remains, change the query to address it. Do not repeat an invalid request unchanged. Missing evidence is uncertainty, not proof of absence. This hint grants no write permission."});
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repeated_reads_are_advisory_and_changed_results_reset_progress() {
        let monitor = ReadProgress::default();
        for count in 1..=3 {
            let mut reply = (200, json!({"matches":[1]}));
            monitor.observe("resolve_story_evidence", "source-bound-args", &mut reply);
            assert_eq!(reply.0, 200);
            assert_eq!(reply.1["matches"], json!([1]));
            assert_eq!(reply.1.get("progressHint").is_some(), count == 3);
        }
        let mut changed = (200, json!({"matches":[2]}));
        monitor.observe("resolve_story_evidence", "source-bound-args", &mut changed);
        assert!(changed.1.get("progressHint").is_none());
        for _ in 0..4 {
            let mut write = (409, json!({"error":"unknown outcome"}));
            monitor.observe("publish_knowledge_batch", "same-write", &mut write);
            assert_eq!(write, (409, json!({"error":"unknown outcome"})));
        }
        let mut page = (200, json!({"matches":[1]}));
        monitor.observe("read_task_source", "different-cursor", &mut page);
        assert!(page.1.get("progressHint").is_none());
    }
}
