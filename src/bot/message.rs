use crate::bot::state::{AppState, QAStatus};
use crate::bot::ui;
use crate::bot::utils::{is_admin, schedule_message_deletion};
use crate::qa::persistence::update_qa_item_by_hash;
use crate::qa::{QAItem, format_answer_html};
use std::sync::Arc;
use teloxide::types::MessageId;
use teloxide::{
    prelude::*,
    types::{LinkPreviewOptions, Message},
    utils::markdown,
};
use tokio::sync::Mutex;

use crate::{config::Config, gemini::key_manager::GeminiKeyManager, qa::QAEmbedding};

/// The main message handler, which routes to the appropriate logic.
pub async fn message_handler(
    bot: Bot,
    msg: Message,
    qa_emb: Arc<Mutex<QAEmbedding>>,
    cfg: Arc<Config>,
    key_manager: Arc<GeminiKeyManager>,
    state: Arc<Mutex<AppState>>,
) -> Result<(), anyhow::Error> {
    // If the chat is a group chat, check if it's in the allowed list.
    if msg.chat.is_group() || msg.chat.is_supergroup() {
        if !cfg.telegram.allowed_group_ids.is_empty()
            && !cfg.telegram.allowed_group_ids.contains(&msg.chat.id.0)
        {
            log::warn!("Ignoring message from unauthorized group: {}", msg.chat.id);
            return Ok(());
        }
    }

    let handled = handle_qa_reply(
        bot.clone(),
        msg.clone(),
        state,
        cfg.clone(),
        qa_emb.clone(),
        key_manager.clone(),
    )
    .await?;

    if !handled {
        handle_generic_message(bot, msg, qa_emb, cfg, key_manager).await?;
    }

    Ok(())
}

/// Handles replies that are part of the interactive QA-adding process.
async fn handle_qa_reply(
    bot: Bot,
    msg: Message,
    state: Arc<Mutex<AppState>>,
    config: Arc<Config>,
    qa_embedding: Arc<Mutex<QAEmbedding>>,
    key_manager: Arc<GeminiKeyManager>,
) -> Result<bool, anyhow::Error> {
    let (reply_to, user, new_text) = match (msg.reply_to_message(), msg.from.clone(), msg.text()) {
        (Some(reply_to), Some(user), Some(new_text)) => (reply_to, user, new_text),
        _ => return Ok(false), // Not a text reply to a message
    };

    let key = (msg.chat.id, reply_to.id);
    let mut state_guard = state.lock().await;

    if let Some(pending_qa) = state_guard.pending_qas.get_mut(&key) {
        if !is_admin(&bot, msg.chat.id, user.id, &config).await {
            // Ignore replies from non-admins
            return Ok(true); // 'true' because we've identified it's a reply in our flow, but did nothing.
        }
        // Clone status to avoid borrow conflicts
        let current_status = pending_qa.status.clone();
        match current_status {
            QAStatus::AwaitingAnswer { question } => {
                let question = question.clone();
                let answer = new_text.to_string();

                pending_qa.status = QAStatus::AwaitingConfirmation {
                    question: question.clone(),
                    answer: answer.clone(),
                };

                let text = format!(
                    "**Is this Q&A pair correct?**\n\n**Q:** {}\n\n**A:** {}",
                    markdown::escape(&question),
                    markdown::escape(&answer)
                );

                // This is an interactive message, do not auto-delete
                bot.edit_message_text(key.0, key.1, text)
                    .parse_mode(teloxide::types::ParseMode::MarkdownV2)
                    .reply_markup(ui::confirm_reedit_cancel_keyboard())
                    .await?;
            }
            QAStatus::AwaitingEditQuestion {
                old_question_hash,
                original_answer,
            } => {
                let new_item = QAItem {
                    question: new_text.to_string(),
                    answer: original_answer,
                };
                update_and_reload(
                    &bot,
                    &msg,
                    &key,
                    &config,
                    &qa_embedding,
                    &key_manager,
                    &old_question_hash,
                    &new_item,
                )
                .await?;
                state_guard.pending_qas.remove(&key);
            }
            QAStatus::AwaitingEditAnswer {
                old_question_hash,
                original_question,
            } => {
                let new_item = QAItem {
                    question: original_question,
                    answer: new_text.to_string(),
                };
                update_and_reload(
                    &bot,
                    &msg,
                    &key,
                    &config,
                    &qa_embedding,
                    &key_manager,
                    &old_question_hash,
                    &new_item,
                )
                .await?;
                state_guard.pending_qas.remove(&key);
            }
            _ => { /* Other states like AwaitingConfirmation are not handled by text replies */ }
        }
        // Delete the admin's reply message to keep the chat clean.
        if let Err(e) = bot.delete_message(msg.chat.id, msg.id).await {
            log::warn!("Failed to delete admin's reply message: {:?}", e);
        }
        Ok(true) // Handled
    } else {
        Ok(false) // Not a reply to a pending QA message
    }
}
async fn update_and_reload(
    bot: &Bot,
    msg: &Message,
    key: &(ChatId, MessageId),
    config: &Config,
    qa_embedding: &Arc<Mutex<QAEmbedding>>,
    key_manager: &Arc<GeminiKeyManager>,
    old_hash: &str,
    new_item: &QAItem,
) -> Result<(), anyhow::Error> {
    let _ = msg;
    if let Err(e) = update_qa_item_by_hash(config, old_hash, new_item) {
        log::error!("Failed to update QA in JSON: {:?}", e);
        // Do not auto-delete error messages on interactive panels
        bot.edit_message_text(key.0, key.1, format!("Error saving QA: {}", e))
            .await?;
        return Ok(());
    }

    let mut qa_guard = qa_embedding.lock().await;
    if let Err(e) = qa_guard.load_and_embed_qa(config, key_manager).await {
        log::error!("Failed to reload and embed QA data: {:?}", e);
        bot.edit_message_text(key.0, key.1, format!("Error reloading embeddings: {}", e))
            .await?;
    } else {
        bot.edit_message_text(key.0, key.1, "✅ QA pair updated successfully!")
            .await?;
    }

    Ok(())
}
/// The original message handler logic for answering questions.
async fn handle_generic_message(
    bot: Bot,
    msg: Message,
    qa_emb: Arc<Mutex<QAEmbedding>>,
    cfg: Arc<Config>,
    key_manager: Arc<GeminiKeyManager>,
) -> Result<(), anyhow::Error> {
    // If the chat is private, only allow super admins to interact.
    if msg.chat.is_private() {
        if let Some(ref user) = msg.from {
            if !crate::bot::utils::is_super_admin(user.id, &cfg) {
                log::warn!(
                    "Ignoring private message from non-super-admin user: {}",
                    user.id
                );
                return Ok(()); // Silently ignore
            }
        } else {
            // Should not happen in private chats, but as a safeguard.
            log::warn!("Ignoring private message with no sender information.");
            return Ok(());
        }
    }

    // Message freshness check
    let current_time = chrono::Utc::now().timestamp();
    if (current_time - msg.date.timestamp()) > cfg.message.timeout {
        log::info!(
            "Ignoring old message ({}s old) from chat {}: {}",
            current_time - msg.date.timestamp(),
            msg.chat.id,
            msg.text().unwrap_or_default()
        );
        return Ok(());
    }

    if let Some(text) = msg.text() {
        // In groups, schedule deletion of the user's message that triggered the bot
        if msg.chat.is_group() || msg.chat.is_supergroup() {
            schedule_message_deletion(bot.clone(), cfg.clone(), msg.clone());
        }

        log::info!("Chat ID: {}, Received message: {}", msg.chat.id, text);

        let qa_guard = qa_emb.lock().await;

        match qa_guard.find_matching_qa(text, &cfg, &key_manager).await {
            Ok(Some(qa_item)) => {
                log::info!("Found matching QA: {:?}", qa_item);
                let formatted_answer = format_answer_html(&qa_item.answer);
                let sent_message = bot
                    .send_message(msg.chat.id, formatted_answer)
                    .link_preview_options(LinkPreviewOptions {
                        is_disabled: true,
                        url: None,
                        prefer_small_media: false,
                        prefer_large_media: false,
                        show_above_text: false,
                    })
                    .parse_mode(teloxide::types::ParseMode::Html)
                    .await?;

                // Use the centralized deletion scheduler
                schedule_message_deletion(bot, cfg, sent_message);
            }
            Ok(None) => {
                // Do nothing if no match is found, to avoid spamming "I don't know"
                log::info!("No match found for: {}", text);
            }
            Err(e) => {
                log::error!("Error finding matching QA: {:?}", e);
                // Avoid sending error messages in the chat
            }
        }
    }
    Ok(())
}
