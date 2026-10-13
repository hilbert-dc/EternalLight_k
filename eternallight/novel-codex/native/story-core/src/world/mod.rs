//! World domain: current public contracts and transaction-preserving persistence.

pub(crate) mod assertion_index_integrity;
pub(crate) mod entity_audit_jobs;
pub(crate) mod entity_identity;
pub(crate) mod entity_index_integrity;
pub(crate) mod epistemic_index_integrity;
pub(crate) mod epistemic_time;
pub(crate) mod evolution;
pub(crate) mod evolution_world;
pub(crate) mod fact_audit_jobs;
pub(crate) mod fact_integrity;
pub(crate) mod hypotheses;
pub(crate) mod identity_decisions;
pub(crate) mod property_conflicts;
pub(crate) mod property_rule_input;
pub(crate) mod property_rules;
pub(crate) mod proposition_sources;
pub(crate) mod relation_audit_jobs;
pub(crate) mod relation_graph;
pub(crate) mod relation_graph_index;
pub(crate) mod relation_index_integrity;
pub(crate) mod relation_types;
pub(crate) mod state_properties;
pub(crate) mod state_selector;
pub(crate) mod state_sources;

pub use assertion_index_integrity::AssertionIndexAudit;
pub use entity_audit_jobs::EntityAuditJob;
pub use entity_index_integrity::EntityIndexAudit;
pub use epistemic_index_integrity::EpistemicIndexAudit;
pub use epistemic_time::epistemic_claim_active_at;
pub use evolution::{EvolutionDimension, EvolutionTraceQuery};
pub use fact_audit_jobs::FactAuditJob;
pub use fact_integrity::FactLedgerAudit;
pub use hypotheses::{CreateHypothesis, HypothesisAssumption};
pub use relation_audit_jobs::RelationAuditJob;
pub use relation_graph::{RelationDirection, RelationGraphQuery, RelationGraphScope};
pub use relation_index_integrity::RelationIndexAudit;

pub use entity_audit_jobs::EntityAuditJobsRepository;

pub use assertion_index_integrity::AssertionIndexIntegrityRepository;

pub use evolution::EvolutionRepository;

pub use relation_audit_jobs::RelationAuditJobsRepository;

pub use entity_identity::EntityIdentityRepository;

pub use property_rule_input::PropertyRuleInputRepository;

pub use entity_index_integrity::EntityIndexIntegrityRepository;

pub use epistemic_index_integrity::EpistemicIndexIntegrityRepository;

pub use fact_integrity::FactIntegrityRepository;

pub use relation_index_integrity::RelationIndexIntegrityRepository;

pub use fact_audit_jobs::FactAuditJobsRepository;

pub use relation_graph::RelationGraphRepository;

pub use hypotheses::HypothesesRepository;
