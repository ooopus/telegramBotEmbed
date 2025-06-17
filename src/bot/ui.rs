//! src/bot/ui.rs
//!
//! This module serves as a factory for creating common Telegram UI components,
//! such as inline keyboards and buttons. Centralizing UI creation here ensures
//! a consistent look and feel across the bot and reduces code duplication in the
//! handler modules.

use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};

// --- Single Buttons ---

/// Creates a standard "Cancel" button with the callback data "cancel".
pub fn cancel_button() -> InlineKeyboardButton {
    InlineKeyboardButton::callback("❌ Cancel", "cancel")
}

/// Creates a standard "Confirm" button with the callback data "confirm".
pub fn confirm_button() -> InlineKeyboardButton {
    InlineKeyboardButton::callback("✅ Confirm", "confirm")
}

/// Creates a standard "Re-edit Answer" button with the callback data "reedit".
pub fn reedit_answer_button() -> InlineKeyboardButton {
    InlineKeyboardButton::callback("📝 Re-edit Answer", "reedit")
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
///
/// Contains "Edit Question", "Edit Answer", and "Delete" buttons.
pub fn qa_management_keyboard(short_hash: &str) -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new(vec![
        vec![
            InlineKeyboardButton::callback(
                "📝 Edit Question",
                format!("edit_q_prompt:{}", short_hash),
            ),
            InlineKeyboardButton::callback(
                "📝 Edit Answer",
                format!("edit_a_prompt:{}", short_hash),
            ),
        ],
        vec![InlineKeyboardButton::callback(
            "🗑️ Delete",
            format!("delete_prompt:{}", short_hash),
        )],
    ])
}

/// Creates a keyboard to confirm the deletion of a Q&A item.
///
/// # Arguments
/// * `short_hash` - A unique (but potentially truncated) hash identifying the QA item.
///
/// Contains "Yes, Delete" and "No, Cancel" buttons.
pub fn delete_confirmation_keyboard(short_hash: &str) -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new(vec![vec![
        InlineKeyboardButton::callback("✅ Yes, Delete", format!("delete_confirm:{}", short_hash)),
        InlineKeyboardButton::callback("❌ No, Cancel", format!("view_qa:{}", short_hash)),
    ]])
}

/// Creates a keyboard with a single "Cancel" button that returns to the QA view state.
///
/// # Arguments
/// * `short_hash` - A unique (but potentially truncated) hash identifying the QA item
///   to return to on cancellation.
pub fn cancel_edit_keyboard(short_hash: &str) -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new(vec![vec![InlineKeyboardButton::callback(
        "❌ Cancel",
        format!("view_qa:{}", short_hash),
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
