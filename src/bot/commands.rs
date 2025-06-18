use crate::bot::state::{AppState, PendingQAInfo, QAStatus};
use crate::bot::ui;
use crate::bot::utils::{is_super_admin, schedule_message_deletion};
use crate::config::Config;
use crate::gemini::key_manager::GeminiKeyManager;
use crate::qa::{QAEmbedding, get_question_hash};
use std::sync::Arc;
use teloxide::{
    prelude::*,
    sugar::request::RequestReplyExt,
    types::{InlineKeyboardButton, InlineKeyboardMarkup},
    utils::{command::BotCommands, markdown},
};
use tokio::sync::Mutex;

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase", description = "支持以下命令：")]
pub enum Command {
    #[command(description = "显示欢迎消息。")]
    Start,
    #[command(description = "以交互方式添加新的问答。")]
    AddQA,
    #[command(description = "回复消息以查找答案。")]
    Answer,
    #[command(description = "列出所有问答以进行管理。")]
    ListQA,
    #[command(description = "按关键字搜索问答。", parse_with = "split")]
    SearchQA(String),
}

// Helper function to create an inline keyboard for a list of Q&A items.
fn make_qa_keyboard(list: &[crate::qa::QAItem]) -> InlineKeyboardMarkup {
    let buttons: Vec<Vec<InlineKeyboardButton>> = list
        .iter()
        .map(|item| {
            let question_hash = get_question_hash(&item.question);
            // Limit question length for the button text
            let short_question = item.question.chars().take(40).collect::<String>();
            // Use a truncated hash to avoid exceeding Telegram's 64-byte limit for callback data
            let short_hash = &question_hash[..16];
            vec![InlineKeyboardButton::callback(
                short_question,
                format!("view_qa:{}", short_hash),
            )]
        })
        .collect();
    InlineKeyboardMarkup::new(buttons)
}

pub async fn command_handler(
    bot: Bot,
    msg: Message,
    cmd: Command,
    state: Arc<Mutex<AppState>>,
    qa_embedding: Arc<Mutex<QAEmbedding>>,
    config: Arc<Config>,
    key_manager: Arc<GeminiKeyManager>,
) -> Result<(), anyhow::Error> {
    // Authorization checks
    if msg.chat.is_private() {
        // In private chats, only allow super admins.
        if let Some(ref user) = msg.from {
            if !is_super_admin(user.id, &config) {
                log::warn!(
                    "Ignoring command from non-super-admin in private chat: {}",
                    user.id
                );
                // Optionally inform the user they are not authorized.
                let sent_message = bot
                    .send_message(msg.chat.id, "您无权在私聊中使用命令。")
                    .await?;
                schedule_message_deletion(bot.clone(), config.clone(), sent_message);
                return Ok(());
            }
        } else {
            // No user associated with the message, should be impossible in private chat.
            return Ok(());
        }
    } else if msg.chat.is_group() || msg.chat.is_supergroup() {
        // In groups, only respond in allowed group IDs (if configured).
        if !config.telegram.allowed_group_ids.is_empty()
            && !config.telegram.allowed_group_ids.contains(&msg.chat.id.0)
        {
            log::warn!("Ignoring command from unauthorized group: {}", msg.chat.id);
            return Ok(());
        }
    }

    // Schedule the user's command message for deletion
    schedule_message_deletion(bot.clone(), config.clone(), msg.clone());

    match cmd {
        Command::Start => {
            let sent_message = bot
                .send_message(msg.chat.id, "您好！我已经准备好回答您的问题了。")
                .await?;
            schedule_message_deletion(bot.clone(), config.clone(), sent_message);
        }
        Command::AddQA => {
            if !(msg.chat.is_group() || msg.chat.is_supergroup()) {
                let sent_message = bot
                    .send_message(msg.chat.id, "此命令只能在群组中使用。")
                    .await?;
                schedule_message_deletion(bot.clone(), config.clone(), sent_message);
                return Ok(());
            }

            let replied_to_message = match msg.reply_to_message() {
                Some(message) => message,
                None => {
                    let sent_message = bot
                        .send_message(msg.chat.id, "请通过回复您想设置为问题的消息来使用此命令。")
                        .await?;
                    schedule_message_deletion(bot.clone(), config.clone(), sent_message);
                    return Ok(());
                }
            };

            let question = match replied_to_message.text() {
                Some(text) => text.to_string(),
                None => {
                    let sent_message = bot
                        .send_message(msg.chat.id, "被回复的消息必须包含文本才能用作问题。")
                        .await?;
                    schedule_message_deletion(bot.clone(), config.clone(), sent_message);
                    return Ok(());
                }
            };

            // This is an interactive message and should not be auto-deleted.
            let bot_message = bot
                .send_message(
                    replied_to_message.chat.id,
                    format!(
                        "❓ *问题已捕获*\n\n> {}\n\n管理员现在必须回复此消息以提供相应答案",
                        markdown::escape(&question)
                    ),
                )
                .reply_to(replied_to_message.id)
                .parse_mode(teloxide::types::ParseMode::MarkdownV2)
                .reply_markup(ui::simple_cancel_keyboard())
                .await?;

            let mut state_guard = state.lock().await;
            state_guard.pending_qas.insert(
                (bot_message.chat.id, bot_message.id),
                PendingQAInfo {
                    status: QAStatus::Answer { question },
                },
            );
        }
        Command::Answer => {
            let replied_to = match msg.reply_to_message() {
                Some(m) => m,
                None => {
                    let sent_message = bot
                        .send_message(msg.chat.id, "请通过回复您想提问的消息来使用此命令。")
                        .await?;
                    schedule_message_deletion(bot.clone(), config.clone(), sent_message);
                    return Ok(());
                }
            };

            let question_text = match replied_to.text() {
                Some(text) => text,
                None => {
                    let sent_message = bot
                        .send_message(msg.chat.id, "被回复的消息必须包含文本。")
                        .await?;
                    schedule_message_deletion(bot.clone(), config.clone(), sent_message);
                    return Ok(());
                }
            };

            log::info!("Answering for question: {}", question_text);
            let qa_guard = qa_embedding.lock().await;

            // Call find_matching_qa with dependencies
            match qa_guard
                .find_matching_qa(question_text, &config, &key_manager)
                .await
            {
                Ok(Some(qa_item)) => {
                    let formatted_answer = crate::qa::format_answer_html(&qa_item.answer);
                    let sent_message = bot
                        .send_message(replied_to.chat.id, formatted_answer)
                        .reply_to(replied_to.id)
                        .parse_mode(teloxide::types::ParseMode::Html)
                        .await?;
                    schedule_message_deletion(bot.clone(), config.clone(), sent_message);
                }
                Ok(None) => {
                    let sent_message = bot
                        .send_message(replied_to.chat.id, "抱歉，我找不到该问题的答案。")
                        .reply_to(replied_to.id)
                        .await?;
                    schedule_message_deletion(bot.clone(), config.clone(), sent_message);
                }
                Err(e) => {
                    log::error!("Error finding matching QA: {:?}", e);
                    let sent_message = bot
                        .send_message(replied_to.chat.id, "搜索答案时发生错误。")
                        .reply_to(replied_to.id)
                        .await?;
                    schedule_message_deletion(bot.clone(), config.clone(), sent_message);
                }
            }
        }
        Command::ListQA => {
            let qa_guard = qa_embedding.lock().await;
            if qa_guard.qa_data.is_empty() {
                let sent_message = bot.send_message(msg.chat.id, "未找到任何问答对。").await?;
                schedule_message_deletion(bot.clone(), config.clone(), sent_message);
                return Ok(());
            }

            let all_qas = qa_guard.qa_data.iter().rev().cloned().collect::<Vec<_>>();
            let keyboard = make_qa_keyboard(&all_qas);

            // This is an interactive panel and should not be deleted.
            bot.send_message(msg.chat.id, "所有问答对。点击进行管理：")
                .reply_markup(keyboard)
                .await?;
        }
        Command::SearchQA(keywords) => {
            if keywords.is_empty() {
                let sent_message = bot
                    .send_message(msg.chat.id, "请输入要搜索的关键字。")
                    .await?;
                schedule_message_deletion(bot.clone(), config.clone(), sent_message);
                return Ok(());
            }

            let qa_guard = qa_embedding.lock().await;
            let lower_keywords = keywords.to_lowercase();
            let matched_qas: Vec<_> = qa_guard
                .qa_data
                .iter()
                .filter(|item| item.question.to_lowercase().contains(&lower_keywords))
                .take(10) // Limit to 10 matches
                .cloned()
                .collect();

            if matched_qas.is_empty() {
                let sent_message = bot
                    .send_message(msg.chat.id, format!("未找到与“{}”相关的匹配项。", keywords))
                    .await?;
                schedule_message_deletion(bot.clone(), config.clone(), sent_message);
                return Ok(());
            }

            let keyboard = make_qa_keyboard(&matched_qas);
            // This is an interactive panel and should not be deleted.
            bot.send_message(msg.chat.id, "找到以下问答对。点击进行管理：")
                .reply_markup(keyboard)
                .await?;
        }
    }
    Ok(())
}
