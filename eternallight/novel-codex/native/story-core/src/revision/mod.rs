//! Revision domain: current public contracts and transaction-preserving persistence.

pub(crate) mod impact_analysis;
pub(crate) mod impact_text;
pub(crate) mod impacts;
pub(crate) mod review_use_cases;
pub(crate) mod revision_continuation;
pub(crate) mod revision_delta;
pub(crate) mod revision_outcome;
pub(crate) mod revision_outcome_context;
pub(crate) mod revision_outcome_decision;
pub(crate) mod revision_outcome_validation;
pub(crate) mod revision_repair;

pub use impact_analysis::{ImpactAnalysisQuery, ImpactAnalysisScope, ImpactRevisionPair};
pub use impacts::ImpactPage;
pub use impacts::ResolveImpactReceipt;
pub use impacts::ResolvePlanningImpact;
pub use revision_continuation::RevisionPredecessor;
pub use revision_outcome::{DesiredOutcome, StoredRevisionOutcome};
pub use revision_outcome_decision::{
    RevisionOutcomeChoice, RevisionOutcomeDecision, RevisionOutcomeDecisionRequest,
};

pub use review_use_cases::{
    ReviewAcceptance, ReviewCreation, ReviewStore, ReviewSubmission, ReviewUndo,
};

pub use impact_analysis::ImpactAnalysisRepository;

pub use impacts::ImpactsRepository;

pub use revision_outcome_decision::RevisionOutcomeDecisionRepository;

pub use revision_outcome::RevisionOutcomeRepository;

pub use revision_outcome_context::RevisionOutcomeContextRepository;

pub use revision_repair::RevisionRepairRepository;

pub use revision_delta::RevisionDeltaRepository;

pub use revision_outcome_validation::RevisionOutcomeValidationRepository;
