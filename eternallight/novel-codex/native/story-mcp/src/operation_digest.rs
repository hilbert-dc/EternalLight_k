#[path = "../../../shared/novel-operation-digest.rs"]
mod implementation;

pub(super) use implementation::{host_json, operation_digest};

#[cfg(test)]
#[path = "operation_digest_tests.rs"]
mod tests;
