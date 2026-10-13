//! Query domain: current public contracts and transaction-preserving persistence.

pub(crate) mod query_bound;
pub(crate) mod query_evidence;
pub(crate) mod query_passages;
pub(crate) mod query_plan;
pub(crate) mod query_ranking;
pub(crate) mod query_report;
pub(crate) mod query_scope;
pub(crate) mod query_semantic;
pub(crate) mod search_audit_jobs;
pub(crate) mod search_index;
pub(crate) mod search_rebuild;

pub use query_plan::{QueryDirection, QueryOperation, QueryPerspective, QueryStep, StoryQueryPlan};
pub use query_semantic::{ModelRelevanceJudgment, QueryRanking};
pub use search_audit_jobs::SearchAuditJob;
pub use search_index::SearchQuery;
pub use search_index::SearchScope;
pub use search_rebuild::SearchIndexAudit;

pub use query_plan::QueryPlanRepository;

pub use search_index::SearchIndexRepository;

pub use search_rebuild::SearchRebuildRepository;

pub use search_audit_jobs::SearchAuditJobsRepository;

pub use query_scope::QueryScopeRepository;
