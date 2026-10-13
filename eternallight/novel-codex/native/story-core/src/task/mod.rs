//! Task domain: current public contracts and transaction-preserving persistence.
mod author_task_submission;
pub use author_task_submission::AuthorTaskSubmission;

pub(crate) mod delegated_candidates;
pub(crate) mod execution_scope;
pub(crate) mod execution_scope_storage;
pub(crate) mod task_action_binding;
pub(crate) mod task_action_permit;
pub(crate) mod task_action_queue;
pub(crate) mod task_actions;
pub(crate) mod task_attempts;
pub(crate) mod task_author_intent;
pub(crate) mod task_capability_renewal;
pub(crate) mod task_chapter_structure;
pub(crate) mod task_delegation_targets;
pub(crate) mod task_evidence;
pub(crate) mod task_final_reply;
pub(crate) mod task_followup;
pub(crate) mod task_hypothesis;
pub(crate) mod task_impact;
pub(crate) mod task_input;
pub(crate) mod task_intent_messages;
pub(crate) mod task_intents;
pub(crate) mod task_knowledge_search;
pub(crate) mod task_planning_review;
pub(crate) mod task_provider_budget;
pub(crate) mod task_scope;
pub(crate) mod task_sources;
pub(crate) mod task_workspace;
pub(crate) mod tasks;

pub use delegated_candidates::{DelegatedDraftCaller, DelegatedDraftWrite};
pub use execution_scope::{ExecutionScopePreview, ExecutionScopeRequest};
pub use task_actions::TaskActionRequest;
pub use task_capability_renewal::RenewTaskCapability;
pub use task_chapter_structure::{ChapterStructureSourceRequest, ChapterStructureSourceRole};
pub use task_delegation_targets::DelegationTargetSelection;
pub use task_final_reply::{FinalReplyState, RecordTaskFinalReply, TaskFinalReply};
pub use task_followup::TaskFollowup;
pub use task_intents::TaskIntentRecord;
pub use task_knowledge_search::WriterKnowledgeScope;
pub use task_planning_review::TaskPlanningReviewRequest;
pub use task_provider_budget::TaskProviderBudget;
pub use task_scope::reader_scene_limit;
pub use task_scope::writer_scene_limit;
pub use tasks::CreateTaskRequest;
pub use tasks::TaskRecord;
pub use tasks::TaskUpdate;

mod intent_store;
pub use intent_store::TaskIntentStore;

// Public response budget shared with Runtime contract verification.
pub use task_sources::SOURCE_CONTENT_PAGE_BYTES;

pub use task_hypothesis::TaskHypothesisRepository;

pub use task_final_reply::TaskFinalReplyRepository;

pub use task_knowledge_search::TaskKnowledgeSearchRepository;

pub use task_provider_budget::TaskProviderBudgetRepository;

pub use task_actions::TaskActionsRepository;

pub use tasks::TasksRepository;

pub use task_workspace::TaskWorkspaceRepository;

pub use delegated_candidates::DelegatedCandidatesRepository;

pub use task_planning_review::TaskPlanningReviewRepository;

pub use task_chapter_structure::TaskChapterStructureRepository;

pub use task_followup::TaskFollowupRepository;

pub use task_sources::TaskSourcesRepository;

pub use task_evidence::TaskEvidenceRepository;

pub use task_capability_renewal::TaskCapabilityRenewalRepository;

pub use task_delegation_targets::TaskDelegationTargetsRepository;

pub use task_impact::TaskImpactRepository;

pub use task_input::TaskInputRepository;

pub use task_action_permit::TaskActionPermitRepository;

pub use execution_scope::ExecutionScopeRepository;
