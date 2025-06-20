use crate::config::Config;
use crate::qa::types::FormattedText;
use std::sync::Arc;
use teloxide::{
    prelude::*,
    types::{ChatId, MessageEntity, MessageEntityKind, UserId},
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

/// 确保给定的 FormattedText 具有引用格式实体。
/// 如果文本中既没有 Blockquote 也没有 ExpandableBlockquote，此函数会添加指定的引用格式。
///
/// # Arguments
/// * `ft` - 需要检查并可能修改的 FormattedText。
/// * `kind` - 如果不存在引用格式，则需要添加的引用类型 (应为 Blockquote 或 ExpandableBlockquote)。
///
/// # Returns
/// 返回一个新的 `FormattedText` 实例，可能已包含新增的引用实体。
pub fn ensure_blockquote(mut ft: FormattedText, kind: MessageEntityKind) -> FormattedText {
    // 检查是否已存在任何形式的引用格式。
    let has_blockquote = ft.entities.iter().any(|e| {
        matches!(
            e.kind,
            MessageEntityKind::Blockquote | MessageEntityKind::ExpandableBlockquote
        )
    });

    // 如果不存在引用格式且文本不为空，则添加指定的类型。
    if !has_blockquote && !ft.text.is_empty() {
        // 确保提供的类型是引用格式的一种。如果不是，则回退到默认的 Blockquote。
        let blockquote_kind = match kind {
            MessageEntityKind::Blockquote | MessageEntityKind::ExpandableBlockquote => kind,
            _ => MessageEntityKind::Blockquote, // Fallback
        };

        let blockquote_entity = MessageEntity {
            kind: blockquote_kind,
            offset: 0,
            length: ft.text.encode_utf16().count(),
        };
        // 在开头插入，以确保它能包裹整个文本。
        ft.entities.insert(0, blockquote_entity);
    }

    ft
}

/// 将一个字符串包装成带有加粗格式的 FormattedText。
pub fn bold(text: &str) -> FormattedText {
    if text.is_empty() {
        return FormattedText {
            text: String::new(),
            entities: vec![],
        };
    }
    FormattedText {
        text: text.to_string(),
        entities: vec![MessageEntity {
            kind: MessageEntityKind::Bold,
            offset: 0,
            length: text.encode_utf16().count(),
        }],
    }
}

/// 将一系列 FormattedText 片段合并成单个 FormattedText。
///
/// 此函数会处理所有文本的拼接和实体偏移量的自动调整。
///
/// # Arguments
/// * `parts` - 一个包含要合并的 FormattedText 引用的切片。
///
/// # Returns
/// 一个包含最终文本和正确合并后实体的新 FormattedText 实例。
pub fn combine_texts(parts: &[&FormattedText]) -> FormattedText {
    let mut final_text = String::new();
    let mut final_entities = Vec::new();
    let mut current_offset = 0;

    for part in parts {
        // 调整并追加当前片段的实体
        let mut adjusted_entities = part.entities.clone();
        for entity in &mut adjusted_entities {
            entity.offset += current_offset;
        }
        final_entities.extend(adjusted_entities);

        // 追加文本
        final_text.push_str(&part.text);

        // 为下一个片段更新UTF-16偏移量
        current_offset += part.text.encode_utf16().count();
    }

    FormattedText {
        text: final_text,
        entities: final_entities,
    }
}
