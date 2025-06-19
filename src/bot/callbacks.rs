use crate::{
    bot::{
        state::{AppState, PendingQAInfo, QAStatus},
        types::CallbackData,
        ui,
        utils::is_admin,
    },
    qa::QAService, // Use the new QAService
};
use std::sync::Arc;
use teloxide::{prelude::*, requests::Requester};
use tokio::sync::Mutex;

/// Handles all callback queries from inline keyboards.
pub async fn callback_handler(
    bot: Bot,
    callback_query: CallbackQuery,
    state: Arc<Mutex<AppState>>,
    qa_service: Arc<Mutex<QAService>>, // Now uses the service
) -> Result<(), anyhow::Error> {
    // Extract necessary data and a clone of the config from the service
    let (user, message, data, config) = {
        let service_guard = qa_service.lock().await;
        let config_clone = service_guard.config.clone();
        match (
            callback_query.from,
            callback_query.message,
            callback_query.data,
        ) {
            (user, Some(message), Some(data)) => (user, message, data, config_clone),
            _ => return Ok(()),
        }
    };

    // --- Authorization Check ---
    if !is_admin(&bot, message.chat().id, user.id, &config).await {
        bot.answer_callback_query(callback_query.id)
            .text("Only administrators can perform this action.")
            .show_alert(true)
            .await?;
        return Ok(());
    }

    // --- Deserialize Callback Data ---
    let callback_data: CallbackData = match serde_json::from_str(&data) {
        Ok(data) => data,
        Err(e) => {
            log::error!("Failed to deserialize callback data: {}. Data: {}", e, data);
            return Ok(());
        }
    };

    let pending_qa_key = (message.chat().id, message.id());

    // --- Actions that don't depend on pending_qas state ---
    // These actions are self-contained and can be handled immediately.
    match callback_data.clone() {
        CallbackData::ViewQa { short_hash } => {
            state.lock().await.pending_qas.remove(&pending_qa_key);
            let service_guard = qa_service.lock().await;
            if let Some((item, _)) = service_guard.find_by_short_hash(&short_hash) {
                // --- Blockquote display logic ---
                let mut display_question = item.question.clone();
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

                let mut display_answer = item.answer.clone();
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

                let header = "Q:\n";
                let separator = "\n\nA:\n";
                let final_text = format!(
                    "{}{}{}{}",
                    header, display_question.text, separator, display_answer.text
                );

                let mut final_entities = display_question.entities.clone();
                let q_offset = header.encode_utf16().count();
                for entity in &mut final_entities {
                    entity.offset += q_offset;
                }

                let mut answer_entities = display_answer.entities.clone();
                let a_offset = (header.to_string() + &display_question.text + separator)
                    .encode_utf16()
                    .count();
                for entity in &mut answer_entities {
                    entity.offset += a_offset;
                }
                final_entities.extend(answer_entities);

                let keyboard = ui::qa_management_keyboard(&short_hash);
                bot.edit_message_text(message.chat().id, message.id(), final_text)
                    .entities(final_entities)
                    .reply_markup(keyboard)
                    .await?;
            }
            return Ok(());
        }
        CallbackData::DeletePrompt { short_hash } => {
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
        CallbackData::DeleteConfirm { short_hash } => {
            let mut service_guard = qa_service.lock().await;
            // Get the full hash from the service to ensure we delete the correct item
            if let Some((_, full_hash)) = service_guard.find_by_short_hash(&short_hash) {
                match service_guard.delete_qa(&full_hash).await {
                    Ok(_) => {
                        bot.edit_message_text(
                            message.chat().id,
                            message.id(),
                            "✅ QA pair deleted successfully!",
                        )
                        .await?;
                    }
                    Err(e) => {
                        log::error!("Failed to delete QA: {:?}", e);
                        bot.edit_message_text(
                            message.chat().id,
                            message.id(),
                            format!("Error during deletion: {}", e),
                        )
                        .await?;
                    }
                }
            }
            return Ok(());
        }
        CallbackData::EditQuestionPrompt { short_hash }
        | CallbackData::EditAnswerPrompt { short_hash } => {
            let service_guard = qa_service.lock().await;
            // Get the item and its full hash from the service
            if let Some((item, full_hash)) = service_guard.find_by_short_hash(&short_hash) {
                let mut state_guard = state.lock().await;
                let (new_status, prompt_text) =
                    if matches!(callback_data, CallbackData::EditQuestionPrompt { .. }) {
                        (
                            QAStatus::EditQuestion {
                                old_question_hash: full_hash, // Use the full hash
                                original_answer: item.answer.clone(),
                            },
                            "Please reply to this message with the **new question**.",
                        )
                    } else {
                        (
                            QAStatus::EditAnswer {
                                old_question_hash: full_hash, // Use the full hash
                                original_question: item.question.clone(),
                            },
                            "Please reply to this message with the **new answer**.",
                        )
                    };

                state_guard
                    .pending_qas
                    .insert(pending_qa_key, PendingQAInfo { status: new_status });

                let keyboard = ui::cancel_edit_keyboard(&short_hash);
                bot.edit_message_text(message.chat().id, message.id(), prompt_text)
                    .parse_mode(teloxide::types::ParseMode::MarkdownV2)
                    .reply_markup(keyboard)
                    .await?;
            }
            return Ok(());
        }
        _ => {}
    }

    // --- Actions that DO depend on pending_qas state ---
    // These actions are part of a multi-step conversation flow.
    let mut state_guard = state.lock().await;
    let pending_qa = match state_guard.pending_qas.get_mut(&pending_qa_key) {
        Some(info) => info,
        None => {
            bot.answer_callback_query(callback_query.id).await?;
            bot.edit_message_text(message.chat().id, message.id(), "This action has expired.")
                .await?;
            return Ok(());
        }
    };

    match callback_data {
        CallbackData::Cancel => {
            bot.answer_callback_query(callback_query.id).await?;
            bot.edit_message_text(message.chat().id, message.id(), "❌ Action Cancelled.")
                .await?;
            state_guard.pending_qas.remove(&pending_qa_key);
        }
        CallbackData::Reedit => {
            if let QAStatus::Confirmation { question, .. } = pending_qa.status.clone() {
                pending_qa.status = QAStatus::Answer {
                    question: question.clone(),
                };
                bot.answer_callback_query(callback_query.id).await?;

                let mut display_question = question.clone();
                let has_blockquote = display_question
                    .entities
                    .iter()
                    .any(|e| matches!(e.kind, teloxide::types::MessageEntityKind::Blockquote));

                if !has_blockquote && !display_question.text.is_empty() {
                    display_question.entities.insert(
                        0,
                        teloxide::types::MessageEntity {
                            kind: teloxide::types::MessageEntityKind::Blockquote,
                            offset: 0,
                            length: display_question.text.encode_utf16().count(),
                        },
                    );
                }

                let header = "❓ **Question**\n\n";
                let footer = "\n\nPlease reply to this message with the new answer.";
                let final_text = format!("{}{}{}", header, display_question.text, footer);

                let mut final_entities = display_question.entities.clone();
                let offset = header.encode_utf16().count();
                for entity in &mut final_entities {
                    entity.offset += offset;
                }

                bot.edit_message_text(message.chat().id, message.id(), final_text)
                    .entities(final_entities)
                    .reply_markup(ui::reedit_keyboard())
                    .await?;
            }
        }
        CallbackData::Confirm => {
            if let QAStatus::Confirmation { question, answer } = pending_qa.status.clone() {
                bot.answer_callback_query(callback_query.id)
                    .text("Saving...")
                    .await?;

                // Release the lock on state before awaiting the service call.
                drop(state_guard);

                let mut service_guard = qa_service.lock().await;
                match service_guard.add_qa(&question, &answer).await {
                    Ok(_) => {
                        bot.edit_message_text(
                            message.chat().id,
                            message.id(),
                            "✅ QA pair added successfully!",
                        )
                        .await?;
                    }
                    Err(e) => {
                        log::error!("Failed to add QA: {:?}", e);
                        bot.edit_message_text(
                            message.chat().id,
                            message.id(),
                            format!("Error saving QA: {}", e),
                        )
                        .await?;
                    }
                }
                // Re-acquire lock to remove the pending QA
                state.lock().await.pending_qas.remove(&pending_qa_key);
            }
        }
        // Other cases were handled above or are not applicable here.
        _ => {}
    }

    Ok(())
}
