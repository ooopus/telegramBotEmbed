use crate::bot::state::{AppState, QAStatus};
use crate::bot::ui;
use crate::bot::utils::{
    bold, combine_texts, ensure_blockquote, is_admin, schedule_message_deletion,
};
use crate::qa::QAService;
use crate::qa::types::FormattedText;
use std::sync::Arc;
use teloxide::prelude::*;
use teloxide::requests::Requester;
use teloxide::types::{LinkPreviewOptions, Message, MessageEntityKind};
use tokio::sync::Mutex;

// ... message_handler function remains unchanged ...
pub async fn message_handler(
    bot: Bot,
    message: Message,
    qa_service: Arc<Mutex<QAService>>,
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
            return Ok(true);
        }

        let new_formatted_text = FormattedText {
            text: new_text,
            entities: new_entities,
        };

        let current_status = pending_qa.status.clone();
        match current_status {
            QAStatus::Answer { question } => {
                pending_qa.status = QAStatus::Confirmation {
                    question: question.clone(),
                    answer: new_formatted_text.clone(),
                };

                let display_question =
                    ensure_blockquote(question.clone(), MessageEntityKind::ExpandableBlockquote);
                let display_answer = ensure_blockquote(
                    new_formatted_text.clone(),
                    MessageEntityKind::ExpandableBlockquote,
                );

                // 创建带格式的各个部分
                let title = bold("Is this Q&A pair correct?");
                let q_header = bold("\n\nQ:\n");
                let a_header = bold("\n\nA:\n");

                // 使用新的 combine_texts 函数将它们组合起来
                let combined = combine_texts(&[
                    &title,
                    &q_header,
                    &display_question,
                    &a_header,
                    &display_answer,
                ]);

                bot.edit_message_text(pending_qa_key.0, pending_qa_key.1, combined.text)
                    .entities(combined.entities)
                    .reply_markup(ui::confirm_reedit_cancel_keyboard())
                    .await?;
            }
            QAStatus::EditQuestion {
                old_question_hash,
                original_answer,
            } => {
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
                state.lock().await.pending_qas.remove(&pending_qa_key);
            }
            QAStatus::EditAnswer {
                old_question_hash,
                original_question,
            } => {
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
                state.lock().await.pending_qas.remove(&pending_qa_key);
            }
            _ => {}
        }

        if let Err(e) = bot.delete_message(message.chat.id, message.id).await {
            log::warn!("Failed to delete admin's reply message: {:?}", e);
        }
        Ok(true)
    } else {
        Ok(false)
    }
}

/// Handles any message that is not a command or a reply in a QA flow.
pub async fn handle_generic_message(
    bot: Bot,
    message: Message,
    qa_service: Arc<Mutex<QAService>>,
    state: Arc<Mutex<AppState>>,
) -> Result<(), anyhow::Error> {
    if !state.lock().await.is_qa_ready {
        return Ok(());
    }

    let config = qa_service.lock().await.config.clone();

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

    if message.chat.is_private() {
        if let Some(ref user) = message.from {
            if !crate::bot::utils::is_super_admin(user.id, &config) {
                return Ok(());
            }
        } else {
            return Ok(());
        }
    }

    let current_time = chrono::Utc::now().timestamp();
    if (current_time - message.date.timestamp()) > config.message.timeout {
        return Ok(());
    }

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
                let answer =
                    ensure_blockquote(qa_item.answer, MessageEntityKind::ExpandableBlockquote);

                let sent_message = bot
                    .send_message(message.chat.id, answer.text)
                    .entities(answer.entities)
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
