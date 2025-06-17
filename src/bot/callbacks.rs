use crate::{
    bot::state::{AppState, QAStatus},
    config::Config,
    gemini::key_manager::GeminiKeyManager,
    qa::{QAEmbedding, QAItem, add_qa_item_to_json},
};
use std::sync::Arc;
use teloxide::{
    prelude::*,
    types::{InlineKeyboardButton, InlineKeyboardMarkup},
    utils::markdown,
};
use tokio::sync::Mutex;

async fn is_admin(bot: &Bot, chat_id: ChatId, user_id: UserId) -> bool {
    match bot.get_chat_administrators(chat_id).await {
        Ok(admins) => admins.iter().any(|m| m.user.id == user_id),
        Err(_) => false,
    }
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

    if !is_admin(&bot, message.chat().id, user.id).await {
        bot.answer_callback_query(q.id)
            .text("Only administrators can perform this action.")
            .show_alert(true)
            .await?;
        return Ok(());
    }

    let key = (message.chat().id, message.id());
    let mut state_guard = state.lock().await;

    let pending_qa = match state_guard.pending_qas.get_mut(&key) {
        Some(info) => info,
        None => {
            bot.answer_callback_query(q.id).await?;
            bot.edit_message_text(message.chat().id, message.id(), "This action has expired.")
                .await?;
            return Ok(());
        }
    };

    match data.as_str() {
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
                .reply_markup(InlineKeyboardMarkup::new(vec![vec![
                    InlineKeyboardButton::callback("❌ Cancel", "cancel"),
                ]]))
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

                // Reload embeddings
                let mut qa_guard = qa_embedding.lock().await;
                if let Err(e) = qa_guard
                    .load_and_embed_qa(&config, &config.qa.qa_json_path, &key_manager)
                    .await
                {
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

                state_guard.pending_qas.remove(&key);
            }
        }
        _ => {}
    }

    Ok(())
}
