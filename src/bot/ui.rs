//! src/bot/ui.rs
//!
//! This module serves as a factory for creating common Telegram UI components,
//! such as inline keyboards and buttons. Centralizing UI creation here ensures
//! a consistent look and feel across the bot and reduces code duplication in the
//! handler modules.

use crate::bot::types::CallbackData;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};

/// Serializes a CallbackData enum into a JSON string for use in an InlineKeyboardButton.
/// Panics on failure, as serialization of the internal enum should never fail.
fn create_callback_data(data: CallbackData) -> String {
    serde_json::to_string(&data).expect("Failed to serialize callback data")
}

// --- Single Buttons ---

/// Creates a standard "Cancel" button with the callback data "cancel".
pub fn cancel_button() -> InlineKeyboardButton {
    InlineKeyboardButton::callback("❌ Cancel", create_callback_data(CallbackData::Cancel))
}

/// Creates a standard "Confirm" button with the callback data "confirm".
pub fn confirm_button() -> InlineKeyboardButton {
    InlineKeyboardButton::callback("✅ Confirm", create_callback_data(CallbackData::Confirm))
}

/// Creates a standard "Re-edit Answer" button with the callback data "reedit".
pub fn reedit_answer_button() -> InlineKeyboardButton {
    InlineKeyboardButton::callback(
        "📝 Re-edit Answer",
        create_callback_data(CallbackData::Reedit),
    )
}

// --- Keyboards ---

/// Creates a keyboard for the confirmation step of adding a new Q&A.
/// Contains "Confirm", "Re-edit Answer", and "Cancel" buttons.
pub fn confirm_reedit_cancel_keyboard() -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new(vec![vec![
        confirm_button(),
        reedit_answer_button(),
        cancel_button(),
    ]])
}

/// Creates the main management keyboard for a single Q&A item.
///
/// # Arguments
/// * `short_hash` - A unique (but potentially truncated) hash identifying the QA item.
pub fn qa_management_keyboard(short_hash: &str) -> InlineKeyboardMarkup {
    let edit_q_data = create_callback_data(CallbackData::EditQuestionPrompt {
        short_hash: short_hash.to_string(),
    });
    let edit_a_data = create_callback_data(CallbackData::EditAnswerPrompt {
        short_hash: short_hash.to_string(),
    });
    let delete_data = create_callback_data(CallbackData::DeletePrompt {
        short_hash: short_hash.to_string(),
    });

    InlineKeyboardMarkup::new(vec![
        vec![
            InlineKeyboardButton::callback("📝 Edit Question", edit_q_data),
            InlineKeyboardButton::callback("📝 Edit Answer", edit_a_data),
        ],
        vec![InlineKeyboardButton::callback("🗑️ Delete", delete_data)],
    ])
}

/// Creates a keyboard to confirm the deletion of a Q&A item.
///
/// # Arguments
/// * `short_hash` - A unique (but potentially truncated) hash identifying the QA item.
pub fn delete_confirmation_keyboard(short_hash: &str) -> InlineKeyboardMarkup {
    let confirm_data = create_callback_data(CallbackData::DeleteConfirm {
        short_hash: short_hash.to_string(),
    });
    let cancel_data = create_callback_data(CallbackData::ViewQa {
        short_hash: short_hash.to_string(),
    });

    InlineKeyboardMarkup::new(vec![vec![
        InlineKeyboardButton::callback("✅ Yes, Delete", confirm_data),
        InlineKeyboardButton::callback("❌ No, Cancel", cancel_data),
    ]])
}

/// Creates a keyboard with a single "Cancel" button that returns to the QA view state.
///
/// # Arguments
/// * `short_hash` - A unique (but potentially truncated) hash identifying the QA item
///   to return to on cancellation.
pub fn cancel_edit_keyboard(short_hash: &str) -> InlineKeyboardMarkup {
    let cancel_data = create_callback_data(CallbackData::ViewQa {
        short_hash: short_hash.to_string(),
    });
    InlineKeyboardMarkup::new(vec![vec![InlineKeyboardButton::callback(
        "❌ Cancel",
        cancel_data,
    )]])
}

/// Creates a keyboard with just a single, simple "Cancel" button.
/// Used in the initial step of adding a new Q&A.
pub fn simple_cancel_keyboard() -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new(vec![vec![cancel_button()]])
}

/// Creates a keyboard for the `reedit` flow.
/// Contains a single "Cancel" button.
pub fn reedit_keyboard() -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new(vec![vec![cancel_button()]])
}
