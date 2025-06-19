use crate::qa::types::FormattedText;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use teloxide::types::{ChatId, MessageId};

/// Represents the current state of a pending QA addition.
#[derive(Clone, Debug)]
pub enum QAStatus {
    /// The bot is waiting for an administrator to reply with an answer.
    Answer { question: FormattedText },
    /// The bot has received an answer and is waiting for confirmation.
    Confirmation {
        question: FormattedText,
        answer: FormattedText,
    },
    /// Waiting for an admin to reply with the new question text.
    EditQuestion {
        old_question_hash: String,
        original_answer: FormattedText,
    },
    /// Waiting for an admin to reply with the new answer text.
    EditAnswer {
        old_question_hash: String,
        original_question: FormattedText,
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
    /// A map for interactive QA processes.
    pub pending_qas: HashMap<(ChatId, MessageId), PendingQAInfo>,
    /// If Some, the bot will ignore generic messages until the specified time.
    pub snoozed_until: Option<DateTime<Utc>>,
    /// A flag to indicate if the QA system has finished its initial loading.
    pub is_qa_ready: bool,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            pending_qas: HashMap::new(),
            snoozed_until: None,
            is_qa_ready: false, // Initial state is not ready.
        }
    }
}
