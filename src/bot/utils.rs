use crate::config::Config;
use std::sync::Arc;
use teloxide::{
    prelude::*,
    types::{ChatId, UserId},
};

/// Checks if a user is a super admin based on the configuration.
pub fn is_super_admin(user_id: UserId, config: &Config) -> bool {
    config.telegram.super_admins.contains(&(user_id.0 as i64))
}

pub async fn is_admin(bot: &Bot, chat_id: ChatId, user_id: UserId, config: &Config) -> bool {
    // 1. Super admin check: Super admins are admins everywhere.
    if is_super_admin(user_id, config) {
        return true;
    }

    // 2. Group admin check: If in a group, check for administrator privileges.
    if chat_id.is_group() || chat_id.is_channel_or_supergroup() {
        if let Ok(admins) = bot.get_chat_administrators(chat_id).await {
            return admins.iter().any(|m| m.user.id == user_id);
        }
    }

    false
}

/// Schedules a message to be deleted after a configured delay in group chats.
pub fn schedule_message_deletion(bot: Bot, config: Arc<Config>, message: Message) {
    // Only schedule deletion in group or supergroup chats
    if message.chat.is_group() || message.chat.is_supergroup() {
        let delete_delay = config.message.delete_delay;
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_secs(delete_delay)).await;
            if let Err(e) = bot.delete_message(message.chat.id, message.id).await {
                // It's common for a message to be already deleted by an admin,
                // so we specifically check for the "message to delete not found" error
                // and avoid logging it as a critical error.
                if !e.to_string().contains("message to delete not found") {
                    log::error!("Failed to delete scheduled message: {:?}", e);
                }
            } else {
                log::info!(
                    "Successfully deleted scheduled message {} in chat {}",
                    message.id,
                    message.chat.id
                );
            }
        });
    }
}
