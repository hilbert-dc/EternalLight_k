//! A borrowed verification capability: operations keep the original connection and transaction.
use crate::SqliteStoryRepository;

pub struct QualityStore<'a> {
    pub(super) connection: &'a mut rusqlite::Connection,
    pub(super) recovery_operation: &'a Option<crate::RecoveryOperationBinding>,
}

impl SqliteStoryRepository {
    pub fn quality(&mut self) -> QualityStore<'_> {
        QualityStore {
            connection: &mut self.connection,
            recovery_operation: &self.recovery_operation,
        }
    }
}
