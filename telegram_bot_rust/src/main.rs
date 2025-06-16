use std::fs;
use std::path::Path;
use std::sync::Arc;
use anyhow::Context as _; // Import the Context trait for .context() method

use teloxide::prelude::*;
use teloxide::utils::command::BotCommands;

mod qa;

// Assuming Config is defined here for now.
// It might be better in its own module (e.g., config.rs) or in qa.rs if closely tied.
#[derive(Debug, Clone)]
pub struct Config {
    pub api_key: String,
    pub embed_api_url: String,
    pub embed_model: String,
    pub cache_dir: String,
    pub token: String, // Telegram Bot Token
    pub similarity_threshold: f32,
    pub delete_delay: u64, // Added
    pub message_timeout: u64, // Added
}

// Basic command for testing
#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase", description = "These commands are supported:")]
enum Command {
    #[command(description = "display this text.")]
    Help,
    #[command(description = "handle a text message.")]
    Message(String), // To allow direct interaction for testing find_matching_qa
}

async fn message_handler(
    bot: Bot,
    msg: Message,
    qa_emb: Arc<qa::QAEmbedding>,
    cfg: Arc<Config>,
    client: Arc<reqwest::Client>,
    // cmd: Command, // Using text directly for now
) -> Result<(), anyhow::Error> {
    // Message freshness check
    let current_time = chrono::Utc::now().timestamp();
    if (current_time - msg.date.timestamp()) as u64 > cfg.message_timeout {
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

        match qa_emb.find_matching_qa(text, &cfg, &client).await {
            Ok(Some(qa_item)) => {
                log::info!("Found matching QA: {:?}", qa_item);
                let formatted_answer = qa::format_answer_html(&qa_item.answer);
                let sent_message = bot
                    .send_message(msg.chat.id, formatted_answer)
                    .parse_mode(teloxide::types::ParseMode::Html)
                    .await?;

                if msg.chat.is_group() || msg.chat.is_supergroup() {
                    let bot_clone = bot.clone();
                    let chat_id = msg.chat.id;
                    let message_id_to_delete = sent_message.id;
                    let delete_delay = cfg.delete_delay;

                    tokio::spawn(async move {
                        tokio::time::sleep(tokio::time::Duration::from_secs(delete_delay)).await;
                        match bot_clone.delete_message(chat_id, message_id_to_delete).await {
                            Ok(_) => log::info!("Successfully deleted message {} in chat {}", message_id_to_delete, chat_id),
                            Err(e) => log::error!("Failed to delete message {} in chat {}: {:?}", message_id_to_delete, chat_id, e),
                        }
                    });
                }
            }
            Ok(None) => {
                log::info!("No match found for: {}", text);
                // No need to delete "no match" messages, or make it configurable
                bot.send_message(msg.chat.id, "I couldn't find a relevant answer to that.").await?;
            }
            Err(e) => {
                log::error!("Error finding matching QA: {:?}", e);
                bot.send_message(msg.chat.id, "Sorry, I encountered an error trying to understand that.").await?;
            }
        }
    }
    Ok(())
}


#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    env_logger::init(); // Initialize logger

    // Config: values from env vars or defaults
    log::info!("Loading configuration...");
    let token = std::env::var("TELEGRAM_BOT_TOKEN").context("TELEGRAM_BOT_TOKEN must be set")?;
    log::info!("TELEGRAM_BOT_TOKEN loaded.");

    let api_key = std::env::var("EMBED_API_KEY").unwrap_or_else(|_| {
        log::warn!("EMBED_API_KEY not set, using default dummy key.");
        "YOUR_DUMMY_API_KEY".to_string()
    });
    let embed_api_url = std::env::var("EMBED_API_URL").unwrap_or_else(|_| {
        log::warn!("EMBED_API_URL not set, using default dummy URL.");
        "http://localhost:12347/v1/embeddings".to_string()
    });
     let delete_delay_str = std::env::var("DELETE_DELAY_SECS").unwrap_or_else(|_| "60".to_string());
    let delete_delay = delete_delay_str.parse().with_context(|| format!("Failed to parse DELETE_DELAY_SECS: '{}'", delete_delay_str))?;

    let message_timeout_str = std::env::var("MESSAGE_TIMEOUT_SECS").unwrap_or_else(|_| "120".to_string());
    let message_timeout = message_timeout_str.parse().with_context(|| format!("Failed to parse MESSAGE_TIMEOUT_SECS: '{}'", message_timeout_str))?;

    let config = Arc::new(Config {
        api_key,
        embed_api_url,
        embed_model: std::env::var("EMBED_MODEL").unwrap_or_else(|_| "test-model/dummy-model-2".to_string()),
        cache_dir: std::env::var("CACHE_DIR").unwrap_or_else(|_| "cache_data_v2".to_string()),
        token,
        similarity_threshold: 0.75f32, // Could also be from env var
        delete_delay,
        message_timeout,
    });

    log::info!("Configuration loaded successfully: {:?}", config); // Be careful logging sensitive parts of config like API keys or full tokens.

    // Ensure docs and cache directories exist
    let docs_dir = Path::new("docs");
    if !docs_dir.exists() {
        fs::create_dir_all(docs_dir).with_context(|| format!("Failed to create docs directory at {:?}", docs_dir.display()))?;
        log::info!("Created directory: {:?}", docs_dir.display());
    }
    let qa_json_path = "docs/QA.json";
    if !Path::new(qa_json_path).exists() {
        let dummy_qa_content = r#"[
            {"question": "What is Rust?", "answer": "A systems programming language."},
            {"question": "What is Cargo?", "answer": "The Rust package manager."}
        ]"#;
        fs::write(qa_json_path, dummy_qa_content).with_context(|| format!("Failed to write dummy QA.json to {}", qa_json_path))?;
        log::info!("Created dummy {} for testing.", qa_json_path);
    }

    let cache_dir_path = Path::new(&config.cache_dir);
    if !cache_dir_path.exists() {
        fs::create_dir_all(cache_dir_path).with_context(|| format!("Failed to create cache directory at {:?}", cache_dir_path.display()))?;
        log::info!("Created directory: {:?}", cache_dir_path.display());
    }

    log::info!("Initializing QAEmbedding and reqwest client...");
    let mut qa_embedding = qa::QAEmbedding::new();
    let reqwest_client = Arc::new(reqwest::Client::new());

    log::info!("Loading and embedding QA data from: {}", qa_json_path);
    if let Err(e) = qa_embedding.load_and_embed_qa(&config, qa_json_path, &reqwest_client).await {
        log::error!("Error loading QA embeddings: {:?}", e);
        // Depending on strictness, might exit or run with no QA capability
    } else {
        log::info!("Successfully loaded and processed QA data.");
        log::info!("Number of QA items: {}", qa_embedding.qa_data.len());
        log::info!("Number of embeddings: {}", qa_embedding.question_embeddings.len());
    }

    let qa_arc = Arc::new(qa_embedding);

    log::info!("Bot starting with token: {}...", &config.token[..8]); // Log only part of token

    let bot = Bot::new(config.token.clone());

    let handler = Update::filter_message()
        // .filter_command::<Command>() // If using commands
        .endpoint(message_handler);

    Dispatcher::builder(bot, handler)
        .dependencies(dptree::deps![qa_arc, config.clone(), reqwest_client.clone()])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;

    log::info!("Bot stopped.");
    Ok(())
}
