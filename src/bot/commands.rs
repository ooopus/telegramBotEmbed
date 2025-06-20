use crate::bot::state::{AppState, PendingQAInfo, QAStatus};
use crate::bot::ui;
use crate::bot::utils::{
    bold, combine_texts, ensure_blockquote, is_admin, is_super_admin, schedule_message_deletion,
};
use crate::config::Config;
use crate::qa::types::{FormattedText, QAItem};
use crate::qa::{QAService, get_question_hash, search};
use chrono::{Duration, Utc};
use std::sync::Arc;
use teloxide::prelude::*;
use teloxide::requests::Requester;
use teloxide::sugar::request::RequestReplyExt;
use teloxide::types::{
    ChatId, InlineKeyboardButton, InlineKeyboardMarkup, Message, MessageEntityKind,
};
use teloxide::utils::command::BotCommands;
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
    #[command(
        description = "暂停机器人自动回复（默认60分钟）。",
        parse_with = "split"
    )]
    Snooze(String),
    #[command(description = "立即恢复机器人自动回复。")]
    Resume,
}

/// Main command handler that dispatches to specific handlers.
pub async fn command_handler(
    bot: Bot,
    message: Message,
    command: Command,
    state: Arc<Mutex<AppState>>,
    qa_service: Arc<Mutex<QAService>>,
) -> Result<(), anyhow::Error> {
    let chat_id = message.chat.id;
    let user_id = message.from.as_ref().map(|u| u.id);

    let config = qa_service.lock().await.config.clone();

    if chat_id.is_user() {
        if let Some(uid) = user_id {
            if !is_super_admin(uid, &config) {
                bot.send_message(chat_id, "您无权在私聊中使用命令。")
                    .await?;
                return Ok(());
            }
        } else {
            return Ok(());
        }
    }

    if !chat_id.is_user()
        && !config.telegram.allowed_group_ids.is_empty()
        && !config.telegram.allowed_group_ids.contains(&chat_id.0)
    {
        log::warn!("Ignoring command from unauthorized group: {}", chat_id);
        return Ok(());
    }

    schedule_message_deletion(bot.clone(), config.clone(), message.clone());

    let is_user_admin = if let Some(uid) = user_id {
        is_admin(&bot, chat_id, uid, &config).await
    } else {
        false
    };

    let admin_only_handler = |bot: Bot, chat_id: ChatId, config: Arc<Config>| async move {
        let sent = bot
            .send_message(chat_id, "只有管理员才能使用此命令。")
            .await?;
        schedule_message_deletion(bot, config, sent);
        Ok(())
    };

    match command {
        Command::Start => handle_start(bot, chat_id, config).await?,
        Command::Answer => handle_answer(bot, message, qa_service, state).await?,
        Command::AddQA => {
            if !is_user_admin {
                return admin_only_handler(bot, chat_id, config).await;
            }
            handle_add_qa(bot, message, state).await?
        }
        Command::ListQA => {
            if !is_user_admin {
                return admin_only_handler(bot, chat_id, config).await;
            }
            handle_list_qa(bot, message, qa_service, state).await?
        }
        Command::SearchQA(keywords) => {
            if !is_user_admin {
                return admin_only_handler(bot, chat_id, config).await;
            }
            handle_search_qa(bot, message, keywords, qa_service, state).await?
        }
        Command::Snooze(minutes_str) => {
            if !is_user_admin {
                return admin_only_handler(bot, chat_id, config).await;
            }
            handle_snooze(bot, chat_id, minutes_str, state, config).await?
        }
        Command::Resume => {
            if !is_user_admin {
                return admin_only_handler(bot, chat_id, config).await;
            }
            handle_resume(bot, chat_id, state, config).await?
        }
    }
    Ok(())
}

async fn check_qa_ready(
    bot: Bot,
    chat_id: ChatId,
    state: Arc<Mutex<AppState>>,
    config: Arc<Config>,
) -> Result<bool, anyhow::Error> {
    if !state.lock().await.is_qa_ready {
        let sent = bot
            .send_message(chat_id, "⌛️ 问答系统正在初始化，请稍后再试...")
            .await?;
        schedule_message_deletion(bot, config, sent);
        return Ok(false);
    }
    Ok(true)
}

fn make_qa_keyboard(list: &[QAItem]) -> InlineKeyboardMarkup {
    let buttons: Vec<Vec<InlineKeyboardButton>> = list
        .iter()
        .map(|item| {
            let question_hash = get_question_hash(&item.question.text);
            let short_question = item.question.text.chars().take(40).collect::<String>();
            let short_hash = &question_hash[..16];
            let callback_data = serde_json::to_string(&crate::bot::types::CallbackData::ViewQa {
                short_hash: short_hash.to_string(),
            })
            .unwrap_or_default();
            vec![InlineKeyboardButton::callback(
                short_question,
                callback_data,
            )]
        })
        .collect();
    InlineKeyboardMarkup::new(buttons)
}

async fn handle_start(bot: Bot, chat_id: ChatId, config: Arc<Config>) -> Result<(), anyhow::Error> {
    let sent_message = bot
        .send_message(chat_id, "您好！我已经准备好回答您的问题了。")
        .await?;
    schedule_message_deletion(bot, config, sent_message);
    Ok(())
}

async fn handle_add_qa(
    bot: Bot,
    message: Message,
    state: Arc<Mutex<AppState>>,
) -> Result<(), anyhow::Error> {
    let replied_to_message = match message.reply_to_message() {
        Some(m) => m,
        None => {
            bot.send_message(
                message.chat.id,
                "请通过回复您想设置为问题的消息来使用此命令。",
            )
            .await?;
            return Ok(());
        }
    };
    let question_text = replied_to_message.text().unwrap_or_default();
    if question_text.is_empty() {
        bot.send_message(message.chat.id, "被回复的消息必须包含文本才能用作问题。")
            .await?;
        return Ok(());
    }

    let question_from_reply = FormattedText {
        text: question_text.to_string(),
        entities: replied_to_message.entities().unwrap_or_default().to_vec(),
    };

    let display_question = ensure_blockquote(
        question_from_reply.clone(),
        MessageEntityKind::ExpandableBlockquote,
    );

    // 使用我们新的格式化工具来构建消息
    let header = bold("❓ 问题已捕获\n\n");
    let footer = FormattedText {
        text: "\n\n管理员现在必须回复此消息以提供相应答案。".to_string(),
        entities: vec![],
    };

    let combined = combine_texts(&[&header, &display_question, &footer]);

    let bot_message = bot
        .send_message(message.chat.id, combined.text)
        .entities(combined.entities)
        .reply_to(replied_to_message.id)
        .reply_markup(ui::simple_cancel_keyboard())
        .await?;

    state.lock().await.pending_qas.insert(
        (bot_message.chat.id, bot_message.id),
        PendingQAInfo {
            status: QAStatus::Answer {
                question: question_from_reply,
            },
        },
    );
    Ok(())
}

async fn handle_list_qa(
    bot: Bot,
    message: Message,
    qa_service: Arc<Mutex<QAService>>,
    state: Arc<Mutex<AppState>>,
) -> Result<(), anyhow::Error> {
    let config = qa_service.lock().await.config.clone();
    if !check_qa_ready(bot.clone(), message.chat.id, state, config).await? {
        return Ok(());
    }
    let service_guard = qa_service.lock().await;
    let all_qas = service_guard.get_all_qa_items();
    if all_qas.is_empty() {
        bot.send_message(message.chat.id, "未找到任何问答对。")
            .await?;
        return Ok(());
    }
    let keyboard = make_qa_keyboard(&all_qas);
    bot.send_message(message.chat.id, "所有问答对。点击进行管理：")
        .reply_markup(keyboard)
        .await?;
    Ok(())
}

async fn handle_search_qa(
    bot: Bot,
    message: Message,
    keywords: String,
    qa_service: Arc<Mutex<QAService>>,
    state: Arc<Mutex<AppState>>,
) -> Result<(), anyhow::Error> {
    let config = qa_service.lock().await.config.clone();
    if !check_qa_ready(bot.clone(), message.chat.id, state, config).await? {
        return Ok(());
    }
    if keywords.is_empty() {
        bot.send_message(message.chat.id, "请输入要搜索的关键字。")
            .await?;
        return Ok(());
    }
    let service_guard = qa_service.lock().await;
    let all_qas = service_guard.get_all_qa_items();
    let matched_qas = search::search_by_keyword(&all_qas, &keywords);

    if matched_qas.is_empty() {
        bot.send_message(
            message.chat.id,
            format!("未找到与“{}”相关的匹配项。", keywords),
        )
        .await?;
    } else {
        let keyboard = make_qa_keyboard(&matched_qas);
        bot.send_message(message.chat.id, "找到以下问答对。点击进行管理：")
            .reply_markup(keyboard)
            .await?;
    }
    Ok(())
}

async fn handle_snooze(
    bot: Bot,
    chat_id: ChatId,
    minutes_str: String,
    state: Arc<Mutex<AppState>>,
    config: Arc<Config>,
) -> Result<(), anyhow::Error> {
    let mins = if minutes_str.is_empty() {
        60
    } else {
        minutes_str.parse::<u64>().unwrap_or(60)
    };

    let snoozed_until = Utc::now() + Duration::minutes(mins as i64);
    state.lock().await.snoozed_until = Some(snoozed_until);
    let sent = bot
        .send_message(chat_id, format!("好的，我将暂停自动回复 {} 分钟。", mins))
        .await?;
    schedule_message_deletion(bot, config, sent);
    Ok(())
}

async fn handle_resume(
    bot: Bot,
    chat_id: ChatId,
    state: Arc<Mutex<AppState>>,
    config: Arc<Config>,
) -> Result<(), anyhow::Error> {
    let mut state_guard = state.lock().await;
    if state_guard.snoozed_until.is_some() {
        state_guard.snoozed_until = None;
        let sent = bot.send_message(chat_id, "好的，自动回复已恢复。").await?;
        schedule_message_deletion(bot, config, sent);
    } else {
        let sent = bot
            .send_message(chat_id, "我当前并未处于暂停状态。")
            .await?;
        schedule_message_deletion(bot, config, sent);
    }
    Ok(())
}

async fn handle_answer(
    bot: Bot,
    message: Message,
    qa_service: Arc<Mutex<QAService>>,
    state: Arc<Mutex<AppState>>,
) -> Result<(), anyhow::Error> {
    let config = qa_service.lock().await.config.clone();
    if !check_qa_ready(bot.clone(), message.chat.id, state, config.clone()).await? {
        return Ok(());
    }
    let replied_to = match message.reply_to_message() {
        Some(m) => m,
        None => {
            let sent = bot
                .send_message(message.chat.id, "请通过回复您想提问的消息来使用此命令。")
                .await?;
            schedule_message_deletion(bot, config, sent);
            return Ok(());
        }
    };
    let question_text = replied_to.text().unwrap_or_default();
    if question_text.is_empty() {
        let sent = bot
            .send_message(message.chat.id, "被回复的消息必须包含文本。")
            .await?;
        schedule_message_deletion(bot, config, sent);
        return Ok(());
    }

    log::info!("Answering for question: {}", question_text);
    let service_guard = qa_service.lock().await;
    match service_guard.find_matching_qa(question_text).await {
        Ok(Some(qa_item)) => {
            let answer = ensure_blockquote(qa_item.answer, MessageEntityKind::Blockquote);

            let sent = bot
                .send_message(replied_to.chat.id, answer.text)
                .entities(answer.entities)
                .reply_to(replied_to.id)
                .await?;
            schedule_message_deletion(bot, config, sent);
        }
        Ok(None) => {
            let sent = bot
                .send_message(replied_to.chat.id, "抱歉，我找不到该问题的答案。")
                .reply_to(replied_to.id)
                .await?;
            schedule_message_deletion(bot, config, sent);
        }
        Err(e) => {
            log::error!("Error finding matching QA: {:?}", e);
            let sent = bot
                .send_message(replied_to.chat.id, "搜索答案时发生错误。")
                .reply_to(replied_to.id)
                .await?;
            schedule_message_deletion(bot, config, sent);
        }
    }
    Ok(())
}
