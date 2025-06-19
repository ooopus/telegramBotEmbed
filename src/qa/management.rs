//! src/qa/management.rs
//!
//! This module provides high-level functions for managing the lifecycle of Q&A items.
//! It encapsulates the logic for persistence (modifying the JSON file) and state update
//! (reloading and re-embedding data) to ensure consistency and reduce code
//! duplication in the bot handlers.

use crate::{
    config::Config,
    gemini::key_manager::GeminiKeyManager,
    qa::{
        persistence::{add_qa_item_to_json, delete_qa_item_by_hash, update_qa_item_by_hash},
        types::{FormattedText, QAItem, QASystem},
    },
};
use anyhow::Result;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Adds a new Q&A item, saves it to the JSON file, and triggers a reload of the embeddings.
pub async fn add_qa(
    config: &Config,
    key_manager: &Arc<GeminiKeyManager>,
    qa_system_mutex: &Mutex<QASystem>,
    question: &FormattedText,
    answer: &FormattedText,
) -> Result<()> {
    // Step 1: Create the new item and persist it
    let new_item = QAItem {
        question: question.clone(),
        answer: answer.clone(),
    };
    add_qa_item_to_json(config, &new_item)?;

    // Step 2: Lock and reload the in-memory embeddings
    let mut qa_guard = qa_system_mutex.lock().await;
    qa_guard.load_and_embed_qa(config, key_manager).await?;

    Ok(())
}

/// Deletes a Q&A item by its question hash, saves the change, and triggers a reload.
pub async fn delete_qa(
    config: &Config,
    key_manager: &Arc<GeminiKeyManager>,
    qa_system_mutex: &Mutex<QASystem>,
    question_hash: &str,
) -> Result<()> {
    // Step 1: Persist the deletion
    delete_qa_item_by_hash(config, question_hash)?;

    // Step 2: Lock and reload
    let mut qa_guard = qa_system_mutex.lock().await;
    qa_guard.load_and_embed_qa(config, key_manager).await?;

    Ok(())
}

/// Updates an existing Q&A item by its old question hash and triggers a reload.
pub async fn update_qa(
    config: &Config,
    key_manager: &Arc<GeminiKeyManager>,
    qa_system_mutex: &Mutex<QASystem>,
    old_question_hash: &str,
    new_question: &FormattedText,
    new_answer: &FormattedText,
) -> Result<()> {
    // Step 1: Create the updated item and persist the change
    let new_item = QAItem {
        question: new_question.clone(),
        answer: new_answer.clone(),
    };
    update_qa_item_by_hash(config, old_question_hash, &new_item)?;

    // Step 2: Lock and reload
    let mut qa_guard = qa_system_mutex.lock().await;
    qa_guard.load_and_embed_qa(config, key_manager).await?;

    Ok(())
}
