use serde_json::Value;
use std::collections::HashMap;

pub(crate) struct SnapshotStateSources {
    pub sources: Vec<Value>,
    pub chapters: HashMap<String, i64>,
    pub total_sources: i64,
    pub pending_sources: i64,
    pub unknown_chapter_sources: i64,
}
