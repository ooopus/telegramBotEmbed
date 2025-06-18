use std::collections::HashMap;
use teloxide::types::{ChatId, MessageId};

/// Represents the current state of a pending QA addition.
#[derive(Clone, Debug)]
pub enum QAStatus {
    /// The bot is waiting for an administrator to reply with an answer.
    Answer { question: String },
    /// The bot has received an answer and is waiting for confirmation.
    Confirmation { question: String, answer: String },
    /// 等待管理员回复以提供新的问题文本
    EditQuestion {
        old_question_hash: String,
        original_answer: String,
    },
    /// 等待管理员回复以提供新的答案文本
    EditAnswer {
        old_question_hash: String,
        original_question: String,
    },
}

/// Contains all information about a single pending QA process.
#[derive(Clone, Debug)]
pub struct PendingQAInfo {
    /// The current status of the process.
    pub status: QAStatus,
}

/// The overall application state, shared across handlers.
pub struct AppState {
    /// A map where the key is the (chat_id, message_id) of the bot's interactive message,
    /// and the value is the state of the QA process tied to that message.
    pub pending_qas: HashMap<(ChatId, MessageId), PendingQAInfo>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            pending_qas: HashMap::new(),
        }
    }
}
