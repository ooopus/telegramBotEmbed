//! src/bot/types.rs
//!
//! Defines shared types for the bot module, particularly for handling
//! type-safe callback queries.

use serde::{Deserialize, Serialize};

/// Represents the various actions that can be triggered from an inline keyboard.
///
/// This enum is serialized into a JSON string for callback data, providing a
/// type-safe way to handle different user interactions, instead of relying on
/// fragile string parsing.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum CallbackData {
    /// View the details of a specific QA item. Payload is the short hash.
    ViewQa { short_hash: String },
    /// Show the confirmation prompt for deleting a QA item.
    DeletePrompt { short_hash: String },
    /// Confirm and execute the deletion of a QA item.
    DeleteConfirm { short_hash: String },
    /// Prompt the user to enter a new question.
    EditQuestionPrompt { short_hash: String },
    /// Prompt the user to enter a new answer.
    EditAnswerPrompt { short_hash: String },
    /// Confirm the addition of a new QA pair.
    Confirm,
    /// Go back to the answer-editing step for a new QA pair.
    Reedit,
    /// Cancel the current multi-step operation (e.g., add/edit QA).
    Cancel,
}
