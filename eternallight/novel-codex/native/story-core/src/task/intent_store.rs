//! A borrowed task capability: operations keep the original connection and transaction.
use crate::SqliteStoryRepository;

pub struct TaskIntentStore<'a> {
    pub(super) connection: &'a mut rusqlite::Connection,
}

impl SqliteStoryRepository {
    pub fn task_intent_store(&mut self) -> TaskIntentStore<'_> {
        TaskIntentStore {
            connection: &mut self.connection,
        }
    }
}
