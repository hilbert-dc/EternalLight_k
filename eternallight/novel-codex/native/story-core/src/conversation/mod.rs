//! Conversation domain: current public contracts and transaction-preserving persistence.

pub(crate) mod author_intent_message;
pub(crate) mod author_message;
pub(crate) mod conversation_messages;
pub(crate) mod conversation_receipts;
pub(crate) mod conversations;
pub(crate) mod thread_author_messages;

pub use conversation_messages::{
    ChangeConversationMessage, ConversationMessageInput, MessageTrigger, SubmitConversationMessage,
};
pub use conversations::{ConversationRecord, CreateConversationRequest, UpdateConversationRequest};

mod store;
pub use store::ConversationStore;
