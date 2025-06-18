use crate::bot::state::{AppState, QAStatus};
use crate::bot::ui;
use crate::bot::utils::{is_admin, schedule_message_deletion};
use crate::qa::QAItem;
use crate::qa::management;
use std::sync::Arc;
use teloxide::prelude::*;
use teloxide::types::{LinkPreviewOptions, Message};
use teloxide::utils::markdown::{self, expandable_blockquote};
use teloxide::utils::render::RenderMessageTextHelper;
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
    if (msg.chat.is_group() || msg.chat.is_supergroup())
        && !cfg.telegram.allowed_group_ids.is_empty()
        && !cfg.telegram.allowed_group_ids.contains(&msg.chat.id.0)
    {
        log::warn!("Ignoring message from unauthorized group: {}", msg.chat.id);
        return Ok(());
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
    let (reply_to, user, new_text) = match (
        msg.reply_to_message(),
        msg.from.clone(),
        msg.markdown_text(),
    ) {
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
            QAStatus::Answer { question } => {
                let question = question.clone();
                let answer = new_text;

                pending_qa.status = QAStatus::Confirmation {
                    question: question.clone(),
                    answer: answer.clone(),
                };

                let text = format!(
                    "**Is this Q&A pair correct?**\n\n**Q:**\n{}\n\n**A:**\n{}",
                    markdown::blockquote(&question),
                    markdown::blockquote(&answer)
                );

                // This is an interactive message, do not auto-delete
                bot.edit_message_text(key.0, key.1, text)
                    .parse_mode(teloxide::types::ParseMode::MarkdownV2)
                    .reply_markup(ui::confirm_reedit_cancel_keyboard())
                    .await?;
            }
            QAStatus::EditQuestion {
                old_question_hash,
                original_answer,
            } => {
                let new_item = QAItem {
                    question: new_text,
                    answer: original_answer,
                };

                match management::update_qa(
                    &config,
                    &key_manager,
                    &qa_embedding,
                    &old_question_hash,
                    &new_item,
                )
                .await
                {
                    Ok(_) => {
                        bot.edit_message_text(key.0, key.1, "✅ QA pair updated successfully!")
                            .await?;
                    }
                    Err(e) => {
                        log::error!("Failed to update and reload QA: {:?}", e);
                        bot.edit_message_text(key.0, key.1, format!("Error saving QA: {}", e))
                            .await?;
                    }
                }
                state_guard.pending_qas.remove(&key);
            }
            QAStatus::EditAnswer {
                old_question_hash,
                original_question,
            } => {
                let new_item = QAItem {
                    question: original_question,
                    answer: new_text,
                };

                match management::update_qa(
                    &config,
                    &key_manager,
                    &qa_embedding,
                    &old_question_hash,
                    &new_item,
                )
                .await
                {
                    Ok(_) => {
                        bot.edit_message_text(key.0, key.1, "✅ QA pair updated successfully!")
                            .await?;
                    }
                    Err(e) => {
                        log::error!("Failed to update and reload QA: {:?}", e);
                        bot.edit_message_text(key.0, key.1, format!("Error updating QA: {}", e))
                            .await?;
                    }
                }
                state_guard.pending_qas.remove(&key);
            }
            _ => { /* Other states like Confirmation are not handled by text replies */ }
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
            msg.markdown_text().unwrap_or_default()
        );
        return Ok(());
    }

    if let Some(text) = msg.markdown_text() {
        // In groups, schedule deletion of the user's message that triggered the bot
        if msg.chat.is_group() || msg.chat.is_supergroup() {
            schedule_message_deletion(bot.clone(), cfg.clone(), msg.clone());
        }

        log::info!("Chat ID: {}, Received message: {}", msg.chat.id, text);

        let qa_guard = qa_emb.lock().await;

        match qa_guard.find_matching_qa(&text, &cfg, &key_manager).await {
            Ok(Some(qa_item)) => {
                log::info!("Found matching QA: {:?}", qa_item);
                let formatted_answer = expandable_blockquote(&qa_item.answer);
                let sent_message = bot
                    .send_message(msg.chat.id, formatted_answer)
                    .link_preview_options(LinkPreviewOptions {
                        is_disabled: true,
                        url: None,
                        prefer_small_media: false,
                        prefer_large_media: false,
                        show_above_text: false,
                    })
                    .parse_mode(teloxide::types::ParseMode::MarkdownV2)
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
