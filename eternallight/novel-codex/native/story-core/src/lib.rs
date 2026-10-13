#[path = "../../../shared/canonical-scene-body.rs"]
mod scene_body;

/// Maximum chapter count in one explicit author grant; execution remains incremental.
pub const MAX_GENERATION_CHAPTERS: i64 = 100;

mod ids;

mod metrics;

mod read_snapshot;

mod repository;

mod snapshot_state;

mod current_schema;
mod sqlite_repository;

pub use ids::BranchId;
pub use ids::CommitId;
pub use ids::IdError;
pub use ids::IdempotencyKey;
pub use ids::ObjectId;
pub use ids::ProjectId;
pub use ids::RevisionId;
pub use ids::SnapshotId;

pub use repository::CommitReceipt;
pub use repository::CommitRequest;
pub use repository::CommitStatus;
pub use repository::RepositoryError;
pub use repository::RevisionRecord;
pub use repository::Snapshot;
pub use repository::StoryRepository;
pub use repository::UndoRequest;

pub use sqlite_repository::CandidateReceipt;
pub use sqlite_repository::ChapterProjection;
pub use sqlite_repository::ChapterRecord;
pub use sqlite_repository::ObjectHead;
pub use sqlite_repository::ProjectRecord;
pub use sqlite_repository::SaveCandidateRequest;
pub use sqlite_repository::SqliteStoryRepository;
pub use sqlite_repository::TaskOutputBinding;

mod database;
mod schema;
pub use database::StoryDatabase;

mod candidate_use_cases;
pub use candidate_use_cases::{
    ArtifactSubmission, CandidateContext, CandidateStore, CandidateWriteError, DraftContent, DraftSubmission,
};
pub mod conversation;
pub use conversation::*;
mod events;
pub(crate) use events::*;
pub mod generation;
pub use generation::*;
pub mod knowledge;
pub use knowledge::*;
pub mod manuscript;
pub use manuscript::*;
pub mod media;
pub use media::*;
pub mod planning;
pub use planning::*;
pub mod query;
pub use query::*;
pub mod recovery;
pub use recovery::*;
pub mod revision;
pub use revision::*;
pub mod sources;
pub use sources::*;
pub mod task;
pub use task::*;
pub mod verification;
pub use verification::*;
pub mod work;
pub use work::*;
pub mod world;
pub use world::*;

pub use sqlite_repository::ProjectRevisionRepository;

pub use metrics::MetricsRepository;

pub use read_snapshot::ReadSnapshotRepository;
