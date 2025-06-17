use crate::bot::state::{AppState, QAStatus};
use crate::qa::format_answer_html;
use std::sync::Arc;
use teloxide::{
    prelude::*,
    types::{InlineKeyboardButton, InlineKeyboardMarkup, LinkPreviewOptions, Message},
    utils::markdown,
};
use tokio::sync::Mutex;

use crate::{config::Config, gemini::key_manager::GeminiKeyManager, qa::QAEmbedding};

async fn is_admin(bot: &Bot, chat_id: ChatId, user_id: UserId) -> bool {
    if !(chat_id.is_group() || chat_id.is_channel_or_supergroup()) {
        return true; // Not a group, no admin check needed
    }
    match bot.get_chat_administrators(chat_id).await {
        Ok(admins) => admins.iter().any(|m| m.user.id == user_id),
        Err(e) => {
            log::error!("Could not get chat administrators: {:?}", e);
            false
        }
    }
}

/// The main message handler, which routes to the appropriate logic.
pub async fn message_handler(
    bot: Bot,
    msg: Message,
    qa_emb: Arc<Mutex<QAEmbedding>>,
    cfg: Arc<Config>,
    key_manager: Arc<GeminiKeyManager>,
    state: Arc<Mutex<AppState>>,
) -> Result<(), anyhow::Error> {
    // Attempt to handle the message as a reply in the QA workflow first.
    let handled = handle_qa_reply(bot.clone(), msg.clone(), state, cfg.clone()).await?;

    // If it was not a QA reply, proceed with the generic message handler.
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
    _config: Arc<Config>,
) -> Result<bool, anyhow::Error> {
    let (reply_to, user, answer_text) = match (msg.reply_to_message(), msg.from.clone(), msg.text())
    {
        (Some(reply_to), Some(user), Some(answer_text)) => (reply_to, user, answer_text),
        _ => return Ok(false), // Not a text reply to a message
    };

    let key = (msg.chat.id, reply_to.id);
    let mut state_guard = state.lock().await;

    if let Some(pending_qa) = state_guard.pending_qas.get_mut(&key) {
        if !is_admin(&bot, msg.chat.id, user.id).await {
            // Ignore replies from non-admins
            return Ok(true); // 'true' because we've identified it's a reply in our flow, but did nothing.
        }

        if let QAStatus::AwaitingAnswer { question } = pending_qa.status.clone() {
            let question = question.clone();
            let answer = answer_text.to_string();

            pending_qa.status = QAStatus::AwaitingConfirmation {
                question: question.clone(),
                answer: answer.clone(),
            };

            let buttons = InlineKeyboardMarkup::new(vec![vec![
                InlineKeyboardButton::callback("✅ Confirm", "confirm"),
                InlineKeyboardButton::callback("📝 Re-edit Answer", "reedit"),
                InlineKeyboardButton::callback("❌ Cancel", "cancel"),
            ]]);

            let text = format!(
                "**Is this Q&A pair correct?**\n\n**Q:** {}\n\n**A:** {}",
                markdown::escape(&question),
                markdown::escape(&answer)
            );

            bot.edit_message_text(key.0, key.1, text)
                .parse_mode(teloxide::types::ParseMode::MarkdownV2)
                .reply_markup(buttons)
                .await?;

            // We can optionally delete the admin's reply message to keep the chat clean.
            if let Err(e) = bot.delete_message(msg.chat.id, msg.id).await {
                log::warn!("Failed to delete admin's answer message: {:?}", e);
            }
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

                if msg.chat.is_group() || msg.chat.is_supergroup() {
                    tokio::spawn(async move {
                        tokio::time::sleep(tokio::time::Duration::from_secs(
                            cfg.message.delete_delay,
                        ))
                        .await;
                        if let Err(e) = bot
                            .delete_message(sent_message.chat.id, sent_message.id)
                            .await
                        {
                            log::error!("Failed to delete message: {:?}", e);
                        }
                    });
                }
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
