use crate::bot::state::{AppState, QAStatus};
use crate::bot::ui;
use crate::bot::utils::{is_admin, schedule_message_deletion};
use crate::qa::QAService; // Use QAService
use crate::qa::types::FormattedText;
use std::sync::Arc;
use teloxide::prelude::*;
use teloxide::requests::Requester;
use teloxide::types::{LinkPreviewOptions, Message};
use tokio::sync::Mutex;

/// The main message handler, which routes to the appropriate logic.
pub async fn message_handler(
    bot: Bot,
    message: Message,
    qa_service: Arc<Mutex<QAService>>, // Use QAService
    state: Arc<Mutex<AppState>>,
) -> Result<(), anyhow::Error> {
    // Clone config from service at the beginning
    let config = qa_service.lock().await.config.clone();

    if (message.chat.is_group() || message.chat.is_supergroup())
        && !config.telegram.allowed_group_ids.is_empty()
        && !config
            .telegram
            .allowed_group_ids
            .contains(&message.chat.id.0)
    {
        log::warn!(
            "Ignoring message from unauthorized group: {}",
            message.chat.id
        );
        return Ok(());
    }

    // First, try to handle it as a reply in a QA flow.
    let handled_as_reply = handle_qa_reply(
        bot.clone(),
        message.clone(),
        state.clone(),
        qa_service.clone(),
    )
    .await?;

    // If it was not a reply in a QA flow, handle it as a generic message.
    if !handled_as_reply {
        handle_generic_message(bot, message, qa_service, state).await?;
    }

    Ok(())
}

/// Handles replies that are part of the interactive QA-adding/editing process.
async fn handle_qa_reply(
    bot: Bot,
    message: Message,
    state: Arc<Mutex<AppState>>,
    qa_service: Arc<Mutex<QAService>>,
) -> Result<bool, anyhow::Error> {
    // Extract necessary info from the message
    let (reply_to, user, new_text, new_entities) = match (
        message.reply_to_message(),
        &message.from,
        message.text(),
        message.entities(),
    ) {
        (Some(reply_to), Some(user), Some(text), entities) => (
            reply_to,
            user.clone(),
            text.to_string(),
            entities.unwrap_or_default().to_vec(),
        ),
        _ => return Ok(false), // Not a valid reply in a QA flow
    };

    let pending_qa_key = (message.chat.id, reply_to.id);
    let mut state_guard = state.lock().await;

    if let Some(pending_qa) = state_guard.pending_qas.get_mut(&pending_qa_key) {
        // Must clone config here to release the lock on qa_service quickly
        let config_clone = qa_service.lock().await.config.clone();
        if !is_admin(&bot, message.chat.id, user.id, &config_clone).await {
            // If a non-admin replies to the bot's prompt, ignore it but consider it "handled"
            // to prevent it from being processed by the generic message handler.
            return Ok(true);
        }

        let new_formatted_text = FormattedText {
            text: new_text,
            entities: new_entities,
        };

        // Clone the status to work with it, then update the original `pending_qa`
        let current_status = pending_qa.status.clone();
        match current_status {
            QAStatus::Answer { question } => {
                pending_qa.status = QAStatus::Confirmation {
                    question: question.clone(),
                    answer: new_formatted_text.clone(),
                };

                // --- Blockquote display logic ---
                let mut display_question = question.clone();
                if !display_question.text.is_empty()
                    && !display_question
                        .entities
                        .iter()
                        .any(|e| matches!(e.kind, teloxide::types::MessageEntityKind::Blockquote))
                {
                    display_question.entities.insert(
                        0,
                        teloxide::types::MessageEntity {
                            kind: teloxide::types::MessageEntityKind::Blockquote,
                            offset: 0,
                            length: display_question.text.encode_utf16().count(),
                        },
                    );
                }

                let mut display_answer = new_formatted_text.clone();
                if !display_answer.text.is_empty()
                    && !display_answer
                        .entities
                        .iter()
                        .any(|e| matches!(e.kind, teloxide::types::MessageEntityKind::Blockquote))
                {
                    display_answer.entities.insert(
                        0,
                        teloxide::types::MessageEntity {
                            kind: teloxide::types::MessageEntityKind::Blockquote,
                            offset: 0,
                            length: display_answer.text.encode_utf16().count(),
                        },
                    );
                }
                // --- End blockquote logic ---

                let header = "Is this Q&A pair correct?**\n\nQ:\n";
                let separator = "\n\nA:\n";
                let final_text = format!(
                    "{}{}{}{}",
                    header, display_question.text, separator, display_answer.text
                );

                // Combine and offset entities for the confirmation message
                let mut final_entities = display_question.entities.clone();
                let q_offset = header.encode_utf16().count();
                final_entities.iter_mut().for_each(|e| e.offset += q_offset);

                let mut answer_entities = display_answer.entities.clone();
                let a_offset = (header.to_string() + &display_question.text + separator)
                    .encode_utf16()
                    .count();
                answer_entities
                    .iter_mut()
                    .for_each(|e| e.offset += a_offset);

                final_entities.extend(answer_entities);

                bot.edit_message_text(pending_qa_key.0, pending_qa_key.1, final_text)
                    .entities(final_entities)
                    .reply_markup(ui::confirm_reedit_cancel_keyboard())
                    .await?;
            }
            QAStatus::EditQuestion {
                old_question_hash,
                original_answer,
            } => {
                // Release state lock before async service call
                drop(state_guard);
                let mut service_guard = qa_service.lock().await;
                service_guard
                    .update_qa(&old_question_hash, &new_formatted_text, &original_answer)
                    .await?;
                bot.edit_message_text(
                    pending_qa_key.0,
                    pending_qa_key.1,
                    "✅ QA pair updated successfully!",
                )
                .await?;
                // Re-acquire lock to remove pending state
                state.lock().await.pending_qas.remove(&pending_qa_key);
            }
            QAStatus::EditAnswer {
                old_question_hash,
                original_question,
            } => {
                // Release state lock before async service call
                drop(state_guard);
                let mut service_guard = qa_service.lock().await;
                service_guard
                    .update_qa(&old_question_hash, &original_question, &new_formatted_text)
                    .await?;
                bot.edit_message_text(
                    pending_qa_key.0,
                    pending_qa_key.1,
                    "✅ QA pair updated successfully!",
                )
                .await?;
                // Re-acquire lock to remove pending state
                state.lock().await.pending_qas.remove(&pending_qa_key);
            }
            _ => {} // Not expecting other statuses here
        }

        // Clean up the admin's reply message
        if let Err(e) = bot.delete_message(message.chat.id, message.id).await {
            log::warn!("Failed to delete admin's reply message: {:?}", e);
        }
        Ok(true) // Message was handled as part of a QA flow
    } else {
        Ok(false) // Not a reply to a pending QA message
    }
}

/// Handles any message that is not a command or a reply in a QA flow.
async fn handle_generic_message(
    bot: Bot,
    message: Message,
    qa_service: Arc<Mutex<QAService>>,
    state: Arc<Mutex<AppState>>,
) -> Result<(), anyhow::Error> {
    // 0. Check if the QA system is ready. If not, do nothing.
    if !state.lock().await.is_qa_ready {
        return Ok(());
    }

    let config = qa_service.lock().await.config.clone();

    // 1. Check if the bot is snoozed
    {
        let mut state_guard = state.lock().await;
        if let Some(snoozed_until) = state_guard.snoozed_until {
            if chrono::Utc::now() < snoozed_until {
                log::info!(
                    "Bot is snoozed. Ignoring generic message from chat {}.",
                    message.chat.id
                );
                return Ok(());
            } else {
                log::info!("Snooze period expired. Resuming normal operations.");
                state_guard.snoozed_until = None;
            }
        }
    }

    // 2. Private chat check: Only super admins can trigger generic responses
    if message.chat.is_private() {
        if let Some(ref user) = message.from {
            if !crate::bot::utils::is_super_admin(user.id, &config) {
                return Ok(());
            }
        } else {
            return Ok(());
        }
    }

    // 3. Message freshness check
    let current_time = chrono::Utc::now().timestamp();
    if (current_time - message.date.timestamp()) > config.message.timeout {
        return Ok(());
    }

    // 4. Process the message text
    if let Some(text) = message.text() {
        if message.chat.is_group() || message.chat.is_supergroup() {
            schedule_message_deletion(bot.clone(), config.clone(), message.clone());
        }

        log::info!(
            "Chat ID: {}, Received generic message: {}",
            message.chat.id,
            text
        );
        let service_guard = qa_service.lock().await;

        match service_guard.find_matching_qa(text).await {
            Ok(Some(qa_item)) => {
                log::info!("Found matching QA: {:?}", qa_item);
                let answer_text = qa_item.answer.text.clone();
                let mut answer_entities = qa_item.answer.entities.clone();

                let has_blockquote = answer_entities
                    .iter()
                    .any(|e| matches!(e.kind, teloxide::types::MessageEntityKind::Blockquote));

                if !has_blockquote && !answer_text.is_empty() {
                    let blockquote_entity = teloxide::types::MessageEntity {
                        kind: teloxide::types::MessageEntityKind::Blockquote,
                        offset: 0,
                        length: answer_text.encode_utf16().count(),
                    };
                    answer_entities.insert(0, blockquote_entity);
                }

                let sent_message = bot
                    .send_message(message.chat.id, answer_text)
                    .entities(answer_entities)
                    .link_preview_options(LinkPreviewOptions {
                        is_disabled: true,
                        url: None,
                        prefer_small_media: false,
                        prefer_large_media: false,
                        show_above_text: false,
                    })
                    .await?;
                schedule_message_deletion(bot, config, sent_message);
            }
            Ok(None) => {
                log::info!("No match found for: {}", text);
            }
            Err(e) => {
                log::error!("Error finding matching QA: {:?}", e);
            }
        }
    }
    Ok(())
}
