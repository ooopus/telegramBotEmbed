use crate::{
    bot::{
        state::{AppState, PendingQAInfo, QAStatus},
        ui,
        utils::is_admin,
    },
    config::Config,
    gemini::key_manager::GeminiKeyManager,
    qa::{QAEmbedding, QAItem, add_qa_item_to_json, delete_qa_item_by_hash, get_question_hash},
};
use std::sync::Arc;
use teloxide::{prelude::*, utils::markdown};
use tokio::sync::Mutex;

// Find a QA item by its short hash.
fn find_qa_by_short_hash(qa_embedding: &QAEmbedding, short_hash: &str) -> Option<QAItem> {
    qa_embedding
        .qa_data
        .iter()
        .find(|item| get_question_hash(&item.question).starts_with(short_hash))
        .cloned()
}

pub async fn callback_handler(
    bot: Bot,
    q: CallbackQuery,
    state: Arc<Mutex<AppState>>,
    qa_embedding: Arc<Mutex<QAEmbedding>>,
    key_manager: Arc<GeminiKeyManager>,
    config: Arc<Config>,
) -> Result<(), anyhow::Error> {
    let (user, message, data) = match (q.from, q.message, q.data) {
        (user, Some(message), Some(data)) => (user, message, data),
        _ => return Ok(()),
    };

    if !is_admin(&bot, message.chat().id, user.id, &config).await {
        bot.answer_callback_query(q.id)
            .text("Only administrators can perform this action.")
            .show_alert(true)
            .await?;
        return Ok(());
    }

    let data_parts: Vec<&str> = data.splitn(2, ':').collect();
    let (action, payload) = (data_parts[0], data_parts.get(1).cloned().unwrap_or(""));

    let key = (message.chat().id, message.id());

    // Handle actions that do not require state first
    match action {
        "view_qa" => {
            state.lock().await.pending_qas.remove(&key);
            let short_hash = payload.to_string();
            let qa_guard = qa_embedding.lock().await;
            if let Some(item) = find_qa_by_short_hash(&qa_guard, &short_hash) {
                let text = format!(
                    "**Q:** {}\n\n**A:** {}",
                    markdown::escape(&item.question),
                    markdown::escape(&item.answer)
                );
                let keyboard = ui::qa_management_keyboard(&short_hash);
                bot.edit_message_text(message.chat().id, message.id(), text)
                    .parse_mode(teloxide::types::ParseMode::MarkdownV2)
                    .reply_markup(keyboard)
                    .await?;
            }
            return Ok(());
        }
        "delete_prompt" => {
            let short_hash = payload.to_string();
            let keyboard = ui::delete_confirmation_keyboard(&short_hash);
            bot.edit_message_text(
                message.chat().id,
                message.id(),
                "Are you sure you want to delete this Q&A?",
            )
            .reply_markup(keyboard)
            .await?;
            return Ok(());
        }
        "delete_confirm" => {
            let short_hash = payload.to_string();
            let qa_guard = qa_embedding.lock().await;
            if let Some(item) = find_qa_by_short_hash(&qa_guard, &short_hash) {
                let full_hash = get_question_hash(&item.question);
                if let Err(e) = delete_qa_item_by_hash(&config, &full_hash) {
                    log::error!("Failed to delete QA from JSON: {:?}", e);
                    bot.edit_message_text(
                        message.chat().id,
                        message.id(),
                        format!("Error deleting QA: {}", e),
                    )
                    .await?;
                } else {
                    drop(qa_guard); // Release lock to allow reloading
                    let mut qa_guard_mut = qa_embedding.lock().await;
                    if let Err(e) = qa_guard_mut.load_and_embed_qa(&config, &key_manager).await {
                        log::error!("Failed to reload and embed QA data after deletion: {:?}", e);
                        bot.edit_message_text(
                            message.chat().id,
                            message.id(),
                            format!("QA deleted, but failed to reload embeddings: {}", e),
                        )
                        .await?;
                    } else {
                        bot.edit_message_text(
                            message.chat().id,
                            message.id(),
                            "✅ QA pair deleted successfully!",
                        )
                        .await?;
                    }
                }
            }
            return Ok(());
        }
        "edit_q_prompt" | "edit_a_prompt" => {
            let short_hash = payload.to_string();
            let qa_guard = qa_embedding.lock().await;
            if let Some(item) = find_qa_by_short_hash(&qa_guard, &short_hash) {
                let full_hash = get_question_hash(&item.question);
                let mut state_guard = state.lock().await;
                let (new_status, prompt_text) = if action == "edit_q_prompt" {
                    (
                        QAStatus::AwaitingEditQuestion {
                            old_question_hash: full_hash,
                            original_answer: item.answer,
                        },
                        "Please reply to this message with the **new question**.",
                    )
                } else {
                    // edit_a_prompt
                    (
                        QAStatus::AwaitingEditAnswer {
                            old_question_hash: full_hash,
                            original_question: item.question,
                        },
                        "Please reply to this message with the **new answer**.",
                    )
                };

                state_guard
                    .pending_qas
                    .insert(key, PendingQAInfo { status: new_status });

                let keyboard = ui::cancel_edit_keyboard(&short_hash);
                bot.edit_message_text(message.chat().id, message.id(), prompt_text)
                    .reply_markup(keyboard)
                    .await?;
            }
            return Ok(());
        }
        _ => {}
    }

    // Handle actions that require state
    let mut state_guard = state.lock().await;
    let pending_qa = match state_guard.pending_qas.get_mut(&key) {
        Some(info) => info,
        None => {
            bot.answer_callback_query(q.id).await?;
            if !matches!(
                action,
                "view_qa" | "delete_prompt" | "delete_confirm" | "edit_q_prompt" | "edit_a_prompt"
            ) {
                bot.edit_message_text(message.chat().id, message.id(), "This action has expired.")
                    .await?;
            }
            return Ok(());
        }
    };

    match action {
        "cancel" => {
            bot.answer_callback_query(q.id).await?;
            bot.edit_message_text(message.chat().id, message.id(), "❌ Action Cancelled.")
                .await?;
            state_guard.pending_qas.remove(&key);
        }
        "reedit" => {
            if let QAStatus::AwaitingConfirmation { question, .. } = pending_qa.status.clone() {
                pending_qa.status = QAStatus::AwaitingAnswer {
                    question: question.clone(),
                };
                bot.answer_callback_query(q.id).await?;
                bot.edit_message_text(
                    message.chat().id,
                    message.id(),
                    format!(
                        "❓ **Question**\n\n> {}\n\nPlease reply to this message with the new answer\\.",
                        markdown::escape(&question)
                    ),
                )
                .parse_mode(teloxide::types::ParseMode::MarkdownV2)
                .reply_markup(ui::reedit_keyboard())
                .await?;
            }
        }
        "confirm" => {
            if let QAStatus::AwaitingConfirmation { question, answer } = pending_qa.status.clone() {
                bot.answer_callback_query(q.id).text("Saving...").await?;
                let new_item = QAItem { question, answer };

                if let Err(e) = add_qa_item_to_json(&config, &new_item) {
                    log::error!("Failed to save new QA to JSON: {:?}", e);
                    bot.edit_message_text(
                        message.chat().id,
                        message.id(),
                        format!("Error saving QA: {}", e),
                    )
                    .await?;
                    state_guard.pending_qas.remove(&key);
                    return Ok(());
                }

                drop(state_guard); // Release lock on state

                // Reload embeddings
                let mut qa_guard = qa_embedding.lock().await;
                if let Err(e) = qa_guard.load_and_embed_qa(&config, &key_manager).await {
                    log::error!("Failed to reload and embed QA data: {:?}", e);
                    bot.edit_message_text(
                        message.chat().id,
                        message.id(),
                        format!("Error reloading embeddings: {}", e),
                    )
                    .await?;
                } else {
                    bot.edit_message_text(
                        message.chat().id,
                        message.id(),
                        "✅ QA pair added successfully!",
                    )
                    .await?;
                }

                // Re-acquire lock to remove
                state.lock().await.pending_qas.remove(&key);
            }
        }
        _ => {}
    }

    Ok(())
}
