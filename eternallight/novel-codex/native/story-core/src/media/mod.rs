//! Media domain: current public contracts and transaction-preserving persistence.

pub(crate) mod multimedia;
pub(crate) mod multimedia_paging;
pub(crate) mod multimedia_types;
pub(crate) mod multimedia_validation;

pub use multimedia::{
    CanonCategory, CanonDocument, CanonEntry, CanonWorkspace, MediaOperationOutputs,
    MediaOperationSeal, MediaTranscriptInput, PrepareMediaOperation, RequestMediaAdaptation,
    SaveCanonDocument,
};
pub use multimedia_paging::MediaWorkspacePageRequest;
pub use multimedia_types::*;

pub use multimedia::MultimediaOperationsRepository;

pub use multimedia::MultimediaSourcesRepository;

pub use multimedia::MultimediaAdaptationRepository;

pub use multimedia::MultimediaCatalogRepository;

pub use multimedia::MultimediaRepository;

pub use multimedia::MultimediaCanonRepository;

pub use multimedia::MultimediaImpactRepository;
