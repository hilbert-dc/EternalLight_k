//! A borrowed conversation capability: operations keep the original connection and transaction.
use crate::SqliteStoryRepository;

pub struct ConversationStore<'a> {
    pub(super) connection: &'a mut rusqlite::Connection,
}

impl SqliteStoryRepository {
    pub fn conversation_store(&mut self) -> ConversationStore<'_> {
        ConversationStore {
            connection: &mut self.connection,
        }
    }
}
