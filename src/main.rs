use anyhow::Context as _;
use bot::{
    callbacks::callback_handler,
    commands::{Command, command_handler},
    message::message_handler,
    state::AppState,
};
use config::load_user_config;
use gemini::key_manager::GeminiKeyManager;
use qa::QAService;
use std::sync::Arc;
use teloxide::prelude::*;
use tokio::sync::Mutex;

mod bot;
mod config;
mod gemini;
mod qa;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // --- Configuration and Logging Setup ---
    let config = Arc::new(load_user_config().context("Failed to load configuration")?);
    let log_level: log::Level = config.log_level.clone().into();
    simple_logger::init_with_level(log_level).unwrap();

    // --- Dependency Initialization ---
    let key_manager = Arc::new(GeminiKeyManager::new(
        config.embedding.api_keys.clone(),
        config.embedding.rpm,
        config.embedding.rpd,
    ));
    let app_state = Arc::new(Mutex::new(AppState::new()));
    // Create the new QAService, wrapped for sharing across threads
    let qa_service = Arc::new(Mutex::new(QAService::new(
        config.clone(),
        key_manager.clone(),
    )));

    // --- Asynchronous QA Data Loading ---
    let qa_service_clone = qa_service.clone();
    let app_state_clone = app_state.clone();
    tokio::spawn(async move {
        log::info!(
            "Starting background task to load and embed QA data from: {}",
            qa_service_clone.lock().await.config.qa.qa_json_path // Access config via service
        );
        let mut qa_guard = qa_service_clone.lock().await;
        if let Err(e) = qa_guard.load_and_embed_all().await {
            log::error!("Fatal error during QA data loading and embedding: {:?}", e);
        } else {
            // Set the ready flag upon successful loading
            app_state_clone.lock().await.is_qa_ready = true;
            log::info!("✅ QA data successfully loaded and embedded. System is ready.");
            log::info!("Number of QA items: {}", qa_guard.qa_data_len());
            log::info!(
                "Number of embeddings: {}",
                qa_guard.question_embeddings_len()
            );
        }
    });

    log::info!(
        "Bot starting with token: {}...",
        &config.telegram.token[..8]
    );

    let bot = Bot::new(config.telegram.token.clone());

    // --- Dispatcher Setup ---
    let handler = dptree::entry()
        .branch(
            Update::filter_message()
                .filter_command::<Command>()
                .endpoint(command_handler),
        )
        .branch(Update::filter_callback_query().endpoint(callback_handler))
        .branch(Update::filter_message().endpoint(message_handler));

    Dispatcher::builder(bot, handler)
        // Pass the new qa_service instead of the raw system/config/key_manager
        .dependencies(dptree::deps![qa_service, app_state])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;

    log::info!("Bot stopped.");
    Ok(())
}
