use anyhow::Context as _;
use bot::{
    callbacks::callback_handler,
    commands::{Command, command_handler},
    message::message_handler,
    state::AppState,
};
use config::load_user_config;
use gemini::key_manager::GeminiKeyManager;
use qa::types::QAEmbedding;
use std::sync::Arc;
use teloxide::prelude::*;
use tokio::sync::Mutex;

mod bot;
mod config;
mod gemini;
mod qa;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let cfg = Arc::new(load_user_config().context("Failed to load configuration")?);

    let log_level: log::Level = cfg.log_level.clone().into();
    simple_logger::init_with_level(log_level).unwrap();

    let key_manager = Arc::new(GeminiKeyManager::new(
        cfg.embedding.api_keys.clone(),
        cfg.embedding.rpm,
        cfg.embedding.rpd,
    ));
    let app_state = Arc::new(Mutex::new(AppState::new()));

    log::info!("Initializing QAEmbedding and reqwest client...");
    let qa_embedding = Arc::new(Mutex::new(QAEmbedding::new()));
    {
        let mut qa_guard = qa_embedding.lock().await;
        log::info!(
            "Loading and embedding QA data from: {}",
            cfg.qa.qa_json_path
        );
        if let Err(e) = qa_guard.load_and_embed_qa(&cfg, &key_manager).await {
            log::error!("Error loading QA embeddings: {:?}", e);
        } else {
            log::info!("Successfully loaded and processed QA data.");
            log::info!("Number of QA items: {}", qa_guard.qa_data.len());
            log::info!(
                "Number of embeddings: {}",
                qa_guard.question_embeddings.len()
            );
        }
    }

    log::info!("Bot starting with token: {}...", &cfg.telegram.token[..8]);

    let bot = Bot::new(cfg.telegram.token.clone());

    let handler = dptree::entry()
        .branch(
            Update::filter_message()
                .filter_command::<Command>()
                .endpoint(command_handler),
        )
        .branch(Update::filter_callback_query().endpoint(callback_handler))
        .branch(Update::filter_message().endpoint(message_handler));

    Dispatcher::builder(bot, handler)
        .dependencies(dptree::deps![
            qa_embedding,
            cfg.clone(),
            key_manager,
            app_state
        ])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;

    log::info!("Bot stopped.");
    Ok(())
}
