use crate::bot::state::{AppState, PendingQAInfo, QAStatus};
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
}

pub async fn command_handler(
    bot: Bot,
    msg: Message,
    cmd: Command,
    state: Arc<Mutex<AppState>>,
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
    }
    Ok(())
}
