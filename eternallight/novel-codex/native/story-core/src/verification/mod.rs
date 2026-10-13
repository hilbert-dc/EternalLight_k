//! Verification domain: current public contracts and transaction-preserving persistence.

pub(crate) mod condition_evaluation_jobs;
pub(crate) mod direct_verification;
pub(crate) mod proposal_application;
pub(crate) mod proposal_selection;
pub(crate) mod proposal_sets;
pub(crate) mod quality_notebook;
pub(crate) mod scene_assessment;
pub(crate) mod scene_commitment_context;
pub(crate) mod scene_revision;

pub use condition_evaluation_jobs::{
    ConditionEvaluationControl, ConditionEvaluationJob, ConditionEvaluationStatus,
    ConditionEvaluationTarget, ConditionExplanationPage,
};
pub use proposal_application::{ApplyProposalSet, UndoProposalSet};
pub use proposal_sets::{CreateProposalSet, ProposedReview};
pub use quality_notebook::{
    QualityAction, QualityEvidence, QualityMutation, QualityNote, QualityPacketSource,
};
pub use scene_assessment::StoredSceneAssessment;

mod quality_store;
pub use quality_store::QualityStore;

pub use condition_evaluation_jobs::ConditionExplanationsRepository;

pub use proposal_application::ProposalApplicationRepository;

pub use proposal_sets::ProposalSetsRepository;

pub use proposal_selection::ProposalSelectionRepository;

pub use condition_evaluation_jobs::ConditionEvaluationWorkerRepository;

pub use condition_evaluation_jobs::ConditionEvaluationJobsRepository;

pub use scene_assessment::SceneAssessmentRepository;

pub use condition_evaluation_jobs::ConditionEvaluationExecutionRepository;
