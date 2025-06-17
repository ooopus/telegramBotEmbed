use crate::bot::state::{AppState, PendingQAInfo, QAStatus};
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
#[command(
    rename_rule = "lowercase",
    description = "These commands are supported:"
)]
pub enum Command {
    #[command(description = "Add a new Q&A pair interactively.")]
    AddQA,
    #[command(description = "Reply to a message to find an answer.")]
    Answer,
    #[command(description = "List all Q&A for management.")]
    ListQA,
    #[command(description = "Search Q&A by keyword.", parse_with = "split")]
    SearchQA(String),
}

// 辅助函数，用于为问答列表创建内联键盘
fn make_qa_keyboard(list: &[crate::qa::QAItem]) -> InlineKeyboardMarkup {
    let buttons: Vec<Vec<InlineKeyboardButton>> = list
        .iter()
        .map(|item| {
            let question_hash = get_question_hash(&item.question);
            // 限制问题长度以适应按钮
            let short_question = item.question.chars().take(40).collect::<String>();
            // 使用截断的哈希值以避免超出 Telegram 按钮数据的 64 字节限制
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
    match cmd {
        Command::AddQA => {
            if !(msg.chat.is_group() || msg.chat.is_supergroup()) {
                bot.send_message(msg.chat.id, "This command can only be used in groups.")
                    .await?;
                return Ok(());
            }

            let replied_to_message = match msg.reply_to_message() {
                Some(message) => message,
                None => {
                    bot.send_message(
                        msg.chat.id,
                        "Please use this command by replying to the message you want to set as the question.",
                    )
                    .await?;
                    return Ok(());
                }
            };

            let question = match replied_to_message.text() {
                Some(text) => text.to_string(),
                None => {
                    bot.send_message(
                        msg.chat.id,
                        "The replied-to message must contain text to be used as a question.",
                    )
                    .await?;
                    return Ok(());
                }
            };

            // Reply to the original message that is being set as the question
            let bot_message = bot
                .send_message(
                    replied_to_message.chat.id,
                    format!(
                        "❓ **Question Captured**\n\n> {}\n\nAn administrator must now reply to this message with the corresponding answer\\.",
                        markdown::escape(&question)
                    ),
                )
                .reply_to(replied_to_message.id)
                .parse_mode(teloxide::types::ParseMode::MarkdownV2)
                .reply_markup(InlineKeyboardMarkup::new(vec![vec![
                    InlineKeyboardButton::callback("❌ Cancel", "cancel"),
                ]]))
                .await?;

            let mut state_guard = state.lock().await;
            state_guard.pending_qas.insert(
                (bot_message.chat.id, bot_message.id),
                PendingQAInfo {
                    status: QAStatus::AwaitingAnswer { question },
                },
            );
        }
        Command::Answer => {
            let replied_to = match msg.reply_to_message() {
                Some(m) => m,
                None => {
                    bot.send_message(
                        msg.chat.id,
                        "Please use this command by replying to the message you want to ask about.",
                    )
                    .await?;
                    return Ok(());
                }
            };

            let question_text = match replied_to.text() {
                Some(text) => text,
                None => {
                    bot.send_message(msg.chat.id, "The replied-to message must contain text.")
                        .await?;
                    return Ok(());
                }
            };

            log::info!("Answering for question: {}", question_text);
            let qa_guard = qa_embedding.lock().await;

            // 调用 find_matching_qa 并传入所需依赖
            match qa_guard
                .find_matching_qa(question_text, &config, &key_manager)
                .await
            {
                Ok(Some(qa_item)) => {
                    // 和 message_handler 中一样，格式化并发送答案
                    let formatted_answer = crate::qa::format_answer_html(&qa_item.answer);
                    bot.send_message(replied_to.chat.id, formatted_answer)
                        .reply_to(replied_to.id)
                        .parse_mode(teloxide::types::ParseMode::Html)
                        .await?;
                }
                Ok(None) => {
                    // 如果找不到答案，可以回复一条消息
                    bot.send_message(
                        replied_to.chat.id,
                        "Sorry, I couldn't find an answer to that question.",
                    )
                    .reply_to(replied_to.id)
                    .await?;
                }
                Err(e) => {
                    log::error!("Error finding matching QA: {:?}", e);
                    bot.send_message(
                        replied_to.chat.id,
                        "An error occurred while searching for an answer.",
                    )
                    .reply_to(replied_to.id)
                    .await?;
                }
            }
        }
        Command::ListQA => {
            let qa_guard = qa_embedding.lock().await;
            if qa_guard.qa_data.is_empty() {
                bot.send_message(msg.chat.id, "No Q&A pairs found.").await?;
                return Ok(());
            }

            // 显示所有问题，按添加时间倒序排列
            let all_qas = qa_guard.qa_data.iter().rev().cloned().collect::<Vec<_>>();
            let keyboard = make_qa_keyboard(&all_qas);

            bot.send_message(msg.chat.id, "All Q&A pairs. Click to manage:")
                .reply_markup(keyboard)
                .await?;
        }
        Command::SearchQA(keywords) => {
            if keywords.is_empty() {
                bot.send_message(msg.chat.id, "Please provide keywords to search for.")
                    .await?;
                return Ok(());
            }

            let qa_guard = qa_embedding.lock().await;
            let lower_keywords = keywords.to_lowercase();
            let matched_qas: Vec<_> = qa_guard
                .qa_data
                .iter()
                .filter(|item| item.question.to_lowercase().contains(&lower_keywords))
                .cloned()
                .take(10) // 最多显示10个匹配项
                .collect();

            if matched_qas.is_empty() {
                bot.send_message(msg.chat.id, format!("No matches found for '{}'.", keywords))
                    .await?;
                return Ok(());
            }

            let keyboard = make_qa_keyboard(&matched_qas);
            bot.send_message(msg.chat.id, "Found these Q&A pairs. Click to manage:")
                .reply_markup(keyboard)
                .await?;
        }
    }
    Ok(())
}
