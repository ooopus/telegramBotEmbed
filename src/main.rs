use anyhow::Context as _;
use bot::message::message_handler;
use config::load_user_config;
use gemini::key_manager::GeminiKeyManager;
use qa::types::QAEmbedding;
use std::sync::Arc;
use teloxide::prelude::*;

mod bot;
mod config;
mod gemini;
mod qa;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let cfg = Arc::new(load_user_config().context("Failed to load configuration")?);

    let log_level: log::Level = cfg.log_level.clone().into();
    simple_logger::init_with_level(log_level).unwrap();

    let key_manager = Arc::new(GeminiKeyManager::new(cfg.embedding.api_keys.clone()));

    log::info!("Initializing QAEmbedding and reqwest client...");
    let mut qa_embedding = QAEmbedding::new();

    log::info!(
        "Loading and embedding QA data from: {}",
        cfg.qa.qa_json_path
    );
    if let Err(e) = qa_embedding
        .load_and_embed_qa(&cfg, cfg.qa.qa_json_path.as_str(), &key_manager)
        .await
    {
        log::error!("Error loading QA embeddings: {:?}", e);
        // Depending on strictness, might exit or run with no QA capability
    } else {
        log::info!("Successfully loaded and processed QA data.");
        log::info!("Number of QA items: {}", qa_embedding.qa_data.len());
        log::info!(
            "Number of embeddings: {}",
            qa_embedding.question_embeddings.len()
        );
    }

    let qa_arc = Arc::new(qa_embedding);

    log::info!("Bot starting with token: {}...", &cfg.telegram.token[..8]); // Log only part of token

    let bot = Bot::new(cfg.telegram.token.clone());

    let handler = Update::filter_message()
        // .filter_command::<Command>() // If using commands
        .endpoint(message_handler);

    Dispatcher::builder(bot, handler)
        .dependencies(dptree::deps![qa_arc, cfg.clone(), key_manager])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;

    log::info!("Bot stopped.");
    Ok(())
}
