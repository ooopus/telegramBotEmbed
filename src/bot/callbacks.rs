use crate::{
    bot::{
        state::{AppState, PendingQAInfo, QAStatus},
        ui,
        utils::is_admin,
    },
    config::Config,
    gemini::key_manager::GeminiKeyManager,
    qa::{QAEmbedding, QAItem, get_question_hash, management},
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
                drop(qa_guard); // 在调用管理函数前释放锁

                // 使用新的 management 模块来处理删除和重新加载
                match management::delete_qa(&config, &key_manager, &qa_embedding, &full_hash).await
                {
                    Ok(_) => {
                        bot.edit_message_text(
                            message.chat().id,
                            message.id(),
                            "✅ QA pair deleted successfully!",
                        )
                        .await?;
                    }
                    Err(e) => {
                        log::error!("Failed to delete and reload QA: {:?}", e);
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
        "edit_q_prompt" | "edit_a_prompt" => {
            let short_hash = payload.to_string();
            let qa_guard = qa_embedding.lock().await;
            if let Some(item) = find_qa_by_short_hash(&qa_guard, &short_hash) {
                let full_hash = get_question_hash(&item.question);
                let mut state_guard = state.lock().await;
                let (new_status, prompt_text) = if action == "edit_q_prompt" {
                    (
                        QAStatus::EditQuestion {
                            old_question_hash: full_hash,
                            original_answer: item.answer,
                        },
                        "Please reply to this message with the **new question**.",
                    )
                } else {
                    // edit_a_prompt
                    (
                        QAStatus::EditAnswer {
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
            if let QAStatus::Confirmation { question, .. } = pending_qa.status.clone() {
                pending_qa.status = QAStatus::Answer {
                    question: question.clone(),
                };
                bot.answer_callback_query(q.id).await?;
                bot.edit_message_text(
                    message.chat().id,
                    message.id(),
                    format!(
                        "❓ **Question**\n\n{}\n\nPlease reply to this message with the new answer\\.",
                        markdown::escape(&question)
                    ),
                )
                .parse_mode(teloxide::types::ParseMode::MarkdownV2)
                .reply_markup(ui::reedit_keyboard())
                .await?;
            }
        }
        "confirm" => {
            if let QAStatus::Confirmation { question, answer } = pending_qa.status.clone() {
                bot.answer_callback_query(q.id).text("Saving...").await?;
                let new_item = QAItem { question, answer };

                drop(state_guard); // 释放 state 上的锁

                // 使用新的 management 模块来处理添加和重新加载
                match management::add_qa(&config, &key_manager, &qa_embedding, &new_item).await {
                    Ok(_) => {
                        bot.edit_message_text(
                            message.chat().id,
                            message.id(),
                            "✅ QA pair added successfully!",
                        )
                        .await?;
                    }
                    Err(e) => {
                        log::error!("Failed to add and reload QA: {:?}", e);
                        bot.edit_message_text(
                            message.chat().id,
                            message.id(),
                            format!("Error saving QA: {}", e),
                        )
                        .await?;
                    }
                }

                // 重新获取锁以移除
                state.lock().await.pending_qas.remove(&key);
            }
        }
        _ => {}
    }

    Ok(())
}
