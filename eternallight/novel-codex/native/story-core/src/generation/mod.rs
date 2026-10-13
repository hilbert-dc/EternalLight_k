//! Generation domain: current public contracts and transaction-preserving persistence.

pub(crate) mod authoring_runs;
pub(crate) mod bound_writer_plan;
pub(crate) mod generation_author_source;
pub(crate) mod generation_checkpoint;
pub(crate) mod generation_delivery;
pub(crate) mod generation_execution_scope;
pub(crate) mod generation_handoff;
pub(crate) mod generation_handoff_execution;
pub(crate) mod generation_intent;
pub(crate) mod generation_model_control;
pub(crate) mod writer_baseline;

pub use authoring_runs::{GenerationStartPosition, StoredGenerationRun};
pub use generation_delivery::GenerationChapterDelivery;
pub use generation_handoff::GenerationHandoffRequest;
pub use generation_intent::{
    CreativeIntentRevision, CreativeIntentSource, GenerationFeedbackDecision,
    generation_author_intent,
};
pub use generation_model_control::ModelRunPauseRequest;

pub use authoring_runs::AuthoringRunsRepository;

pub use writer_baseline::WriterBaselineRepository;

pub use generation_model_control::GenerationModelControlRepository;

pub use generation_handoff_execution::GenerationHandoffExecutionRepository;

pub use generation_handoff::GenerationHandoffRepository;

pub use generation_checkpoint::GenerationCheckpointRepository;

pub use bound_writer_plan::BoundWriterPlanRepository;
