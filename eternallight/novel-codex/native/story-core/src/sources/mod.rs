//! Sources domain: current public contracts and transaction-preserving persistence.

pub(crate) mod context_sources;
pub(crate) mod reading_checkpoint;
pub(crate) mod source_check_evidence;
pub(crate) mod source_evidence;
pub(crate) mod source_payload_audit_schema;

pub use context_sources::{ContextSourceScope, ContextSources};
pub use reading_checkpoint::{AuthorReadingProgress, CheckpointReadingRequest, ReadingTraversal};
pub use source_check_evidence::SourceCheck;

pub use context_sources::ContextSourcesRepository;

pub use reading_checkpoint::ReadingCheckpointRepository;
