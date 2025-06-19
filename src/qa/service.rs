//! src/qa/service.rs
//!
//! This new module introduces the QAService, which acts as a high-level facade
//! for the entire question-answering system. It encapsulates the data store (QASystem),
//! configuration, and external service clients (GeminiKeyManager), providing a clean
//! API for the rest of the application. This improves abstraction and simplifies
//! dependency management. It also contains the optimized CRUD operations.

use super::{
    embedding, persistence, search,
    types::{FormattedText, QAItem, QASystem},
    utils,
};
use crate::{config::Config, gemini::key_manager::GeminiKeyManager};
use anyhow::{Result, anyhow};
use std::sync::Arc;
use tokio::time::Duration;

pub struct QAService {
    system: QASystem,
    pub config: Arc<Config>,
    key_manager: Arc<GeminiKeyManager>,
}

impl QAService {
    /// Creates a new, empty QAService.
    pub fn new(config: Arc<Config>, key_manager: Arc<GeminiKeyManager>) -> Self {
        Self {
            system: QASystem::new(),
            config,
            key_manager,
        }
    }

    /// Loads QA data from persistence and generates embeddings for any uncached items.
    /// This is the main initialization method.
    pub async fn load_and_embed_all(&mut self) -> Result<()> {
        self.system.qa_data = persistence::load_qa_items(&self.config.qa.qa_json_path)?;
        let (cache_path, mut embeddings_cache) = persistence::load_embeddings_cache(&self.config)?;

        let num_keys = self.config.embedding.api_keys.len();
        if num_keys == 0 {
            return Err(anyhow!("No API keys configured for embeddings."));
        }
        let total_rpm = (self.config.embedding.rpm as usize) * num_keys;
        let delay_between_requests = if total_rpm > 0 {
            Duration::from_millis((60_000 / total_rpm) as u64)
        } else {
            Duration::from_secs(3600)
        };

        let mut final_embeddings = Vec::with_capacity(self.system.qa_data.len());
        let mut cache_was_updated = false;

        for qa_item in &self.system.qa_data {
            let question_hash = utils::get_question_hash(&qa_item.question.text);
            if let Some(cached_embedding) = embeddings_cache.get(&question_hash) {
                final_embeddings.push(cached_embedding.clone());
            } else {
                log::info!(
                    "Cache miss for question: '{}'. Generating new embedding.",
                    qa_item.question.text
                );
                tokio::time::sleep(delay_between_requests).await;

                let new_embedding = embedding::generate_embedding_with_retry(
                    &self.config,
                    &self.key_manager,
                    &qa_item.question.text,
                )
                .await?;

                final_embeddings.push(new_embedding.clone());
                embeddings_cache.insert(question_hash, new_embedding);
                cache_was_updated = true;
            }
        }

        if cache_was_updated {
            persistence::save_embeddings_cache(&cache_path, &embeddings_cache)?;
        }

        self.system.question_embeddings = final_embeddings;
        Ok(())
    }

    /// Finds the best matching QA item for a given text query.
    pub async fn find_matching_qa(&self, text: &str) -> Result<Option<QAItem>> {
        if self.system.question_embeddings.is_empty() {
            return Ok(None);
        }

        let query_embedding =
            embedding::generate_embedding_with_retry(&self.config, &self.key_manager, text).await?;

        if let Some((index, similarity)) =
            search::find_best_match(&query_embedding, &self.system.question_embeddings)
        {
            let threshold = self.config.similarity.threshold;
            if similarity >= threshold as f64 {
                log::info!(
                    "Match found for query '{}': Q#{} ('{}') with similarity {:.4}",
                    text,
                    index,
                    self.system.qa_data[index].question.text,
                    similarity
                );
                Ok(Some(self.system.qa_data[index].clone()))
            } else {
                log::info!(
                    "No match above threshold {:.2} for query: '{}'. Best match was Q#{} ('{}') with similarity {:.4}",
                    threshold,
                    text,
                    index,
                    self.system.qa_data[index].question.text,
                    similarity
                );
                Ok(None)
            }
        } else {
            log::info!("No match found for: '{}'", text);
            Ok(None)
        }
    }

    /// Adds a new Q&A item, saves it, and updates the in-memory state and embeddings efficiently.
    pub async fn add_qa(&mut self, question: &FormattedText, answer: &FormattedText) -> Result<()> {
        let new_item = QAItem {
            question: question.clone(),
            answer: answer.clone(),
        };

        // 1. Generate embedding for the new item
        let new_embedding = embedding::generate_embedding_with_retry(
            &self.config,
            &self.key_manager,
            &new_item.question.text,
        )
        .await?;

        // 2. Update in-memory state first
        self.system.qa_data.push(new_item.clone());
        self.system.question_embeddings.push(new_embedding.clone());

        // 3. Persist the new state to JSON and cache
        persistence::save_all_qa_items(&self.config.qa.qa_json_path, &self.system.qa_data)?;
        persistence::add_embedding_to_cache(&self.config, &new_item.question.text, new_embedding)?;

        Ok(())
    }

    /// Deletes a Q&A item by its question hash efficiently.
    pub async fn delete_qa(&mut self, question_hash: &str) -> Result<()> {
        if let Some(index) = self
            .system
            .qa_data
            .iter()
            .position(|item| utils::get_question_hash(&item.question.text) == question_hash)
        {
            // 1. Remove from in-memory state
            self.system.qa_data.remove(index);
            self.system.question_embeddings.remove(index);

            // 2. Persist the new state to JSON
            persistence::save_all_qa_items(&self.config.qa.qa_json_path, &self.system.qa_data)?;
            // Note: We don't remove from the embedding cache, as it might be useful again.
        }
        Ok(())
    }

    /// Updates an existing Q&A item efficiently.
    pub async fn update_qa(
        &mut self,
        old_question_hash: &str,
        new_question: &FormattedText,
        new_answer: &FormattedText,
    ) -> Result<()> {
        if let Some(index) = self
            .system
            .qa_data
            .iter()
            .position(|item| utils::get_question_hash(&item.question.text) == old_question_hash)
        {
            let new_item = QAItem {
                question: new_question.clone(),
                answer: new_answer.clone(),
            };

            // 1. Generate new embedding for the potentially updated question
            let new_embedding = embedding::generate_embedding_with_retry(
                &self.config,
                &self.key_manager,
                &new_item.question.text,
            )
            .await?;

            // 2. Update in-memory state
            self.system.qa_data[index] = new_item.clone();
            self.system.question_embeddings[index] = new_embedding.clone();

            // 3. Persist the new state
            persistence::save_all_qa_items(&self.config.qa.qa_json_path, &self.system.qa_data)?;
            persistence::add_embedding_to_cache(
                &self.config,
                &new_item.question.text,
                new_embedding,
            )?;
        }
        Ok(())
    }

    // --- Accessors for UI/bot logic ---

    /// Gets a snapshot of the current QA data.
    pub fn get_all_qa_items(&self) -> Vec<QAItem> {
        self.system.qa_data.clone()
    }

    /// Finds a QAItem by the truncated beginning of its question's hash.
    /// Returns the item and its full hash to prevent the bot from needing to know hashing logic.
    pub fn find_by_short_hash(&self, short_hash: &str) -> Option<(QAItem, String)> {
        self.system.qa_data.iter().find_map(|item| {
            let full_hash = utils::get_question_hash(&item.question.text);
            if full_hash.starts_with(short_hash) {
                Some((item.clone(), full_hash))
            } else {
                None
            }
        })
    }

    /// Gets the number of QA items.
    pub fn qa_data_len(&self) -> usize {
        self.system.qa_data.len()
    }

    /// Gets the number of embeddings.
    pub fn question_embeddings_len(&self) -> usize {
        self.system.question_embeddings.len()
    }
}
