use crate::{
    bot::{
        state::{AppState, PendingQAInfo, QAStatus},
        types::CallbackData,
        ui,
        utils::{bold, combine_texts, ensure_blockquote, is_admin},
    },
    qa::{QAService, types::FormattedText},
};
use std::sync::Arc;
use teloxide::{
    prelude::*,
    requests::Requester,
    types::{MessageEntityKind, ParseMode},
};
use tokio::sync::Mutex;

/// Handles all callback queries from inline keyboards.
pub async fn callback_handler(
    bot: Bot,
    callback_query: CallbackQuery,
    state: Arc<Mutex<AppState>>,
    qa_service: Arc<Mutex<QAService>>,
) -> Result<(), anyhow::Error> {
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

    if !is_admin(&bot, message.chat().id, user.id, &config).await {
        bot.answer_callback_query(callback_query.id)
            .text("Only administrators can perform this action.")
            .show_alert(true)
            .await?;
        return Ok(());
    }

    let callback_data: CallbackData = match serde_json::from_str(&data) {
        Ok(data) => data,
        Err(e) => {
            log::error!("Failed to deserialize callback data: {}. Data: {}", e, data);
            return Ok(());
        }
    };

    let pending_qa_key = (message.chat().id, message.id());

    match callback_data.clone() {
        CallbackData::ViewQa { short_hash } => {
            state.lock().await.pending_qas.remove(&pending_qa_key);
            let service_guard = qa_service.lock().await;
            if let Some((item, _)) = service_guard.find_by_short_hash(&short_hash) {
                let display_question = ensure_blockquote(
                    item.question.clone(),
                    MessageEntityKind::ExpandableBlockquote,
                );
                let display_answer =
                    ensure_blockquote(item.answer.clone(), MessageEntityKind::ExpandableBlockquote);

                let q_header = bold("Q:\n");
                let a_header = bold("\n\nA:\n");

                let combined =
                    combine_texts(&[&q_header, &display_question, &a_header, &display_answer]);

                let keyboard = ui::qa_management_keyboard(&short_hash);
                bot.edit_message_text(message.chat().id, message.id(), combined.text)
                    .entities(combined.entities)
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
            if let Some((item, full_hash)) = service_guard.find_by_short_hash(&short_hash) {
                let mut state_guard = state.lock().await;
                let (new_status, prompt_text) =
                    if matches!(callback_data, CallbackData::EditQuestionPrompt { .. }) {
                        (
                            QAStatus::EditQuestion {
                                old_question_hash: full_hash,
                                original_answer: item.answer.clone(),
                            },
                            "Please reply to this message with the **new question**.",
                        )
                    } else {
                        (
                            QAStatus::EditAnswer {
                                old_question_hash: full_hash,
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
                    .parse_mode(ParseMode::MarkdownV2)
                    .reply_markup(keyboard)
                    .await?;
            }
            return Ok(());
        }
        _ => {}
    }

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

                let display_question =
                    ensure_blockquote(question.clone(), MessageEntityKind::ExpandableBlockquote);

                let header = bold("❓ Question\n\n");
                let footer = FormattedText {
                    text: "\n\nPlease reply to this message with the new answer.".to_string(),
                    entities: vec![],
                };

                let combined = combine_texts(&[&header, &display_question, &footer]);

                bot.edit_message_text(message.chat().id, message.id(), combined.text)
                    .entities(combined.entities)
                    .reply_markup(ui::reedit_keyboard())
                    .await?;
            }
        }
        CallbackData::Confirm => {
            if let QAStatus::Confirmation { question, answer } = pending_qa.status.clone() {
                bot.answer_callback_query(callback_query.id)
                    .text("Saving...")
                    .await?;

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
                state.lock().await.pending_qas.remove(&pending_qa_key);
            }
        }
        _ => {}
    }

    Ok(())
}
