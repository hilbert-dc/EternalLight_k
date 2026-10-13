//! Manuscript domain: current public contracts and transaction-preserving persistence.

pub(crate) mod acceptance_shared;
pub(crate) mod chapter_deletion;
pub(crate) mod chapter_delivery_review;
pub(crate) mod chapter_history;
pub(crate) mod chapter_structure;
pub(crate) mod chapters;
pub(crate) mod draft_patch;
pub(crate) mod draft_source_projection;
pub(crate) mod manuscript_catalog;
pub(crate) mod planned_scene_mapping;
pub(crate) mod protected_scenes;
pub(crate) mod source_edit_anchors;
pub(crate) mod workflow;
pub(crate) mod workspace_selection;

pub use chapter_deletion::{DeleteChapterRequest, DeletedChapterReceipt};
pub use chapters::AssociateChapterPlanRequest;
pub use chapters::ChapterBinding;
pub use chapters::RebindChapterPlanRequest;
pub use chapters::ReboundChapterPlan;
pub use chapters::{MaterializeStandaloneChapterRequest, StandaloneChapterReceipt};
pub use draft_source_projection::project_draft_source_text;
pub use manuscript_catalog::{
    CatalogChapterDestination, CatalogOperation, CatalogRequest, ManuscriptCatalog, VolumeSource,
};
pub use workflow::AcceptReviewRequest;
pub use workflow::AcceptanceReceipt;
pub use workflow::AcceptanceRecord;
pub use workflow::CreateReviewRequest;
pub use workflow::ReviewRecord;
pub use workflow::UndoAcceptanceRequest;
pub use workspace_selection::ContinuationIntent;
pub use workspace_selection::WorkingBaseline;

pub use workspace_selection::WorkspaceSelectionRepository;

pub use draft_patch::DraftPatchRepository;

pub use chapters::ChapterRebindRepository;

pub use chapter_structure::ChapterStructureRepository;

pub use chapters::ChaptersRepository;

pub use protected_scenes::ProtectedScenesRepository;

pub use workflow::WorkflowRepository;

pub use manuscript_catalog::ManuscriptCatalogRepository;

pub use chapter_deletion::ChapterDeletionRepository;

pub use chapter_history::ChapterHistoryRepository;
