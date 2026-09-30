use serde::{Deserialize, Serialize};

use crate::RepositoryError;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum CreativeIntentSource {
    AuthorMessage {
        author_message_id: String,
        created_at: i64,
    },
    FeedbackDecision {
        decision_id: String,
    },
}

/// An explicit author decision, never a model diagnosis or an invented author message.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GenerationFeedbackDecision {
    pub id: String,
    pub author_message_id: Option<String>,
    pub chapter_id: String,
    pub chapter_number: i64,
    pub assessment_revision_id: String,
    pub choice: String,
    pub instruction: String,
    pub source_digest: String,
    pub decided_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreativeIntentRevision {
    pub id: String,
    pub generation: i64,
    pub instruction: String,
    pub effective_from_chapter: i64,
    pub source: CreativeIntentSource,
}

/// Renders the bounded author history applicable to a chapter, never future instructions.
/// Both authoring actions and persisted task inputs use this contract.
pub fn generation_author_intent(
    history: &[CreativeIntentRevision],
    chapter: i64,
) -> Result<String, RepositoryError> {
    if history.is_empty() || history.len() > 16 || chapter < 1 {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let mut text = String::new();
    for (index, intent) in history.iter().enumerate() {
        if intent.generation != index as i64
            || intent.id.is_empty()
            || intent.id.len() > 160
            || intent.instruction.chars().count() > 4_000
            || intent.effective_from_chapter < 1
            || history[..index].iter().any(|prior| prior.id == intent.id)
        {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        let (applies, rendered) = match &intent.source {
            CreativeIntentSource::AuthorMessage {
                author_message_id,
                created_at,
            } => {
                if !crate::author_message::valid_id(author_message_id)
                    || *created_at < 0
                    || intent.instruction.trim().is_empty()
                {
                    return Err(RepositoryError::TaskBindingInvalid);
                }
                (
                    intent.effective_from_chapter <= chapter,
                    format!("[要求版本 {}] {}", intent.generation, intent.instruction),
                )
            }
            CreativeIntentSource::FeedbackDecision { decision_id } => {
                if index == 0 || !crate::author_message::valid_id(decision_id) {
                    return Err(RepositoryError::TaskBindingInvalid);
                }
                let note = if intent.instruction.trim().is_empty() {
                    String::new()
                } else {
                    format!("；作者补充原话：{}", intent.instruction)
                };
                (
                    intent.effective_from_chapter == chapter,
                    format!(
                        "[作者决定版本 {}] 按本章场景评估修订工作稿{note}",
                        intent.generation
                    ),
                )
            }
        };
        if applies {
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(&rendered);
        }
    }
    if text.is_empty() || text.len() > crate::task_author_intent::MAX_AUTHOR_HISTORY_BYTES {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    Ok(text)
}
