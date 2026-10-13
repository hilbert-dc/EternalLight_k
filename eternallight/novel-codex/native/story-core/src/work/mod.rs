//! Work domain: current public contracts and transaction-preserving persistence.

pub(crate) mod initial_work_budget;
pub(crate) mod model_work_plan;
pub(crate) mod ordinary_work_budget;
pub(crate) mod provider_budget;
pub(crate) mod work_action_proposal;
pub(crate) mod work_budget_authorization;
pub(crate) mod work_budget_storage;
pub(crate) mod work_control_commands;
pub(crate) mod work_decision;
pub(crate) mod work_decision_actions;
pub(crate) mod work_decision_delivery;
pub(crate) mod work_decision_generation;
pub(crate) mod work_decision_page;
pub(crate) mod work_decision_recovery;
pub(crate) mod work_decision_status;
pub(crate) mod work_delivery;
pub(crate) mod work_delivery_review;
pub(crate) mod work_execution_events;
#[cfg(test)]
pub(crate) mod work_execution_events_tests;
pub(crate) mod work_interludes;
pub(crate) mod work_issues;
pub(crate) mod work_modeling_recovery;
#[cfg(test)]
pub(crate) mod work_modeling_recovery_tests;
pub(crate) mod work_pause_barrier;
pub(crate) mod work_tasks;

pub use initial_work_budget::InitialWorkBudget;
pub use model_work_plan::{
    GoalContinuation, GoalEvidence, ModelGoal, ModelWorkPlan, ModelWorkPlanInput,
    UpdateModelWorkPlan,
};
pub use ordinary_work_budget::PendingProviderUsage;
pub use provider_budget::{
    ProviderBudget, ProviderUsageCharge, UsageChargeOutcome, deserialize_execution_limit,
};
pub use work_action_proposal::{ProposalTarget, WorkActionProposal};
pub use work_budget_authorization::WorkBudgetAuthorization;
pub use work_control_commands::{WorkControlCommand, WorkControlOutcome, WorkControlReceipt};
pub use work_decision::{WorkDecisionContinuation, WorkDecisionRequest};
pub use work_decision_actions::WorkActionRequest;
pub use work_delivery::{WorkDeliveryEvidence, WorkDeliveryReport, WorkEvidenceState};
pub use work_execution_events::{
    WorkExecutionAckRequest, WorkExecutionClaimRequest, WorkExecutionLeaseRequest,
    WorkExecutionOwnerRequest, WorkExecutionReleaseRequest,
};
pub use work_interludes::{BeginWorkInterlude, ReturnWorkInterlude, WorkInterlude};
pub use work_modeling_recovery::WorkResumeMode;
pub use work_pause_barrier::WorkPauseAfterStep;
pub use work_tasks::{WorkStep, WorkTask, WorkTaskPage};

pub use work_execution_events::WorkExecutionStore;

pub use model_work_plan::ModelWorkPlanRepository;

pub use work_budget_authorization::WorkBudgetAuthorizationRepository;

pub use work_decision::WorkDecisionRepository;

pub use work_interludes::WorkInterludesRepository;

pub use ordinary_work_budget::OrdinaryWorkBudgetRepository;

pub use work_decision_generation::WorkDecisionGenerationRepository;

pub use work_issues::WorkIssuesRepository;

pub use work_decision_actions::WorkDecisionActionsRepository;

pub use work_pause_barrier::WorkPauseBarrierRepository;

pub use work_tasks::WorkTasksRepository;

pub use work_control_commands::WorkControlCommandsRepository;

pub use initial_work_budget::InitialWorkBudgetRepository;
