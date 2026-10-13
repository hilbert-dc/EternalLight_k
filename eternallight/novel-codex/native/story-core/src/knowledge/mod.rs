//! Knowledge domain: current public contracts and transaction-preserving persistence.

pub(crate) mod dependency_rules;
pub(crate) mod knowledge_audit_jobs;
pub(crate) mod knowledge_conflicts;
pub(crate) mod knowledge_head_integrity;
pub(crate) mod knowledge_projection;
pub(crate) mod knowledge_search;
pub(crate) mod model_delta;
pub(crate) mod model_dependencies;
pub(crate) mod model_sources;
pub(crate) mod modeling;
pub(crate) mod modeling_action;
#[cfg(test)]
pub(crate) mod modeling_input_tests;
pub(crate) mod modeling_operation;
pub(crate) mod modeling_outbox;
pub(crate) mod modeling_outbox_schema;
pub(crate) mod modeling_review;
pub(crate) mod snapshot_knowledge;
pub(crate) mod snapshot_knowledge_audit_jobs;
pub(crate) mod snapshot_knowledge_integrity;

pub use knowledge_audit_jobs::{KnowledgeAudit, KnowledgeAuditJob};
pub use knowledge_head_integrity::KnowledgeHeadAudit;
pub use knowledge_projection::{KnowledgePerspective, KnowledgeStateQuery};
pub use modeling::BeginModelingBatch;
pub use modeling::ModelingBatch;
pub use modeling::PublishModelingBatch;
pub use modeling_outbox::ModelingOutboxQuery;
pub use snapshot_knowledge_audit_jobs::SnapshotKnowledgeAuditJob;
pub use snapshot_knowledge_integrity::SnapshotKnowledgeAudit;

pub use knowledge_head_integrity::KnowledgeHeadIntegrityRepository;

pub use snapshot_knowledge_audit_jobs::SnapshotKnowledgeAuditJobsRepository;

pub use snapshot_knowledge_integrity::SnapshotKnowledgeIntegrityRepository;

pub use knowledge_audit_jobs::KnowledgeAuditJobsRepository;

pub use snapshot_knowledge::SnapshotKnowledgeRepository;

pub use knowledge_audit_jobs::KnowledgeAuditSchedulingRepository;

pub use modeling_operation::ModelingOperationRepository;

pub use knowledge_projection::KnowledgeProjectionRepository;

pub use modeling::ModelingRepository;

pub use modeling_outbox::ModelingOutboxRepository;

pub use knowledge_search::KnowledgeSearchRepository;
