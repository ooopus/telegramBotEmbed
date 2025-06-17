mod embedding;
pub mod persistence;
mod search;
pub mod types;
mod utils;

pub use persistence::{add_qa_item_to_json, delete_qa_item_by_hash};
pub use types::{QAEmbedding, QAItem};
pub use utils::{format_answer_html, get_question_hash};

use crate::{config::Config, gemini::key_manager::GeminiKeyManager};
use anyhow::{Result, anyhow};
use std::sync::Arc;
use std::time::Duration;

impl QAEmbedding {
    pub fn new() -> Self {
        QAEmbedding {
            qa_data: Vec::new(),
            question_embeddings: Vec::new(),
        }
    }

    /// 加载 QA 数据并生成词向量，协调各个子模块完成任务
    pub async fn load_and_embed_qa(
        &mut self,
        config: &Config,
        key_manager: &Arc<GeminiKeyManager>,
    ) -> Result<(), anyhow::Error> {
        self.qa_data = persistence::load_qa_items(&config.qa.qa_json_path)?;
        let (cache_path, mut embeddings_cache) = persistence::load_embeddings_cache(config)?;

        let num_keys = config.embedding.api_keys.len();
        if num_keys == 0 {
            return Err(anyhow!("No API keys configured for embeddings."));
        }
        let total_rpm = (config.embedding.rpm as usize) * num_keys;

        let delay_between_requests = if total_rpm > 0 {
            Duration::from_millis((60_000 / total_rpm) as u64)
        } else {
            // If RPM is 0, set a very long delay to effectively stop requests.
            Duration::from_secs(3600)
        };

        let mut final_embeddings = Vec::with_capacity(self.qa_data.len());
        let mut cache_was_updated = false;

        for qa_item in &self.qa_data {
            let question_hash = utils::get_question_hash(&qa_item.question);
            if let Some(cached_embedding) = embeddings_cache.get(&question_hash) {
                final_embeddings.push(cached_embedding.clone());
            } else {
                log::info!(
                    "Cache miss for question: '{}'. Generating new embedding.",
                    qa_item.question
                );

                // 为了避免速率限制，在这里加入基于RPM配置的延时
                log::debug!("Rate limit sleep: {:?}", delay_between_requests);
                tokio::time::sleep(delay_between_requests).await;

                let new_embedding = embedding::generate_embedding_with_retry(
                    config,
                    key_manager,
                    &qa_item.question,
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

        self.question_embeddings = final_embeddings;
        log::info!(
            "Finished processing embeddings. Total: {}",
            self.question_embeddings.len()
        );
        Ok(())
    }
    /// 查找匹配的 QA，现在主要负责协调
    pub async fn find_matching_qa(
        &self,
        text: &str,
        config: &Config,
        key_manager: &Arc<GeminiKeyManager>,
    ) -> Result<Option<QAItem>> {
        if self.question_embeddings.is_empty() {
            return Ok(None);
        }

        let query_embedding =
            embedding::generate_embedding_with_retry(config, key_manager, text).await?;

        if let Some((index, similarity)) =
            search::find_best_match(&query_embedding, &self.question_embeddings)
        {
            let threshold = config.similarity.threshold;
            if similarity >= threshold as f64 {
                log::info!(
                    "Match found for query '{}': Q#{} ('{}') with similarity {:.4}",
                    text,
                    index,
                    self.qa_data[index].question,
                    similarity
                );
                Ok(Some(self.qa_data[index].clone()))
            } else {
                log::info!(
                    "No match found above threshold {:.2} for query: '{}'. Best match was Q#{} ('{}') with similarity {:.4}",
                    threshold,
                    text,
                    index,
                    self.qa_data[index].question,
                    similarity
                );
                Ok(None)
            }
        } else {
            // This case should not be reached if question_embeddings is not empty.
            log::info!("No match found for: '{}'", text);
            Ok(None)
        }
    }
}
