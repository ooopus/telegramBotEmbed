use crate::qa::format_answer_html;
use std::sync::Arc;
use teloxide::{
    prelude::*,
    types::{LinkPreviewOptions, Message},
};

use crate::{config::Config, gemini::key_manager::GeminiKeyManager, qa::QAEmbedding};

pub async fn message_handler(
    bot: Bot,
    msg: Message,
    qa_emb: Arc<QAEmbedding>,
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

        match qa_emb.find_matching_qa(text, &cfg, &key_manager).await {
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
                    let bot_clone = bot.clone();
                    let chat_id = msg.chat.id;
                    let message_id_to_delete = sent_message.id;
                    let delete_delay = cfg.message.delete_delay;

                    tokio::spawn(async move {
                        tokio::time::sleep(tokio::time::Duration::from_secs(delete_delay)).await;
                        match bot_clone
                            .delete_message(chat_id, message_id_to_delete)
                            .await
                        {
                            Ok(_) => log::info!(
                                "Successfully deleted message {} in chat {}",
                                message_id_to_delete,
                                chat_id
                            ),
                            Err(e) => log::error!(
                                "Failed to delete message {} in chat {}: {:?}",
                                message_id_to_delete,
                                chat_id,
                                e
                            ),
                        }
                    });
                }
            }
            Ok(None) => {
                log::info!("No match found for: {}", text);
                bot.send_message(msg.chat.id, "I couldn't find a relevant answer to that.")
                    .await?;
            }
            Err(e) => {
                log::error!("Error finding matching QA: {:?}", e);
                bot.send_message(
                    msg.chat.id,
                    "Sorry, I encountered an error trying to understand that.",
                )
                .await?;
            }
        }
    }
    Ok(())
}
