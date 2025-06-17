use crate::{config::Config, gemini::key_manager::GeminiKeyManager};
use anyhow::Context;
use rig::{
    client::EmbeddingsClient, embeddings::builder::EmbeddingsBuilder,
    providers::gemini::Client as GeminiClient,
};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
pub use types::{QAEmbedding, QAItem};
pub mod types;

impl QAEmbedding {
    pub fn new() -> Self {
        QAEmbedding {
            qa_data: Vec::new(),
            question_embeddings: Vec::new(),
        }
    }

    pub async fn load_and_embed_qa(
        &mut self,
        config: &Config,
        qa_json_path: &str,
        key_manager: &Arc<GeminiKeyManager>,
    ) -> Result<(), anyhow::Error> {
        log::info!("Loading QA data from: {}", qa_json_path);
        let qa_data_str = fs::read_to_string(qa_json_path)
            .with_context(|| format!("Failed to read QA JSON file from: {}", qa_json_path))?;
        self.qa_data = serde_json::from_str(&qa_data_str)
            .with_context(|| format!("Failed to deserialize QA JSON from: {}", qa_json_path))?;
        log::info!(
            "Successfully loaded {} QA items from {}",
            self.qa_data.len(),
            qa_json_path
        );

        let current_qa_hash =
            calculate_qa_hash(&self.qa_data).context("Failed to calculate QA hash")?;
        log::debug!(
            "Calculated QA hash for '{}': {}",
            qa_json_path,
            current_qa_hash
        );

        let model_name_sanitized = config
            .embedding
            .model
            .replace(|c: char| !c.is_alphanumeric(), "_");
        let cache_dir = Path::new(&config.cache.dir);

        fs::create_dir_all(cache_dir)
            .with_context(|| format!("Failed to create cache directory: {:?}", cache_dir))?;

        let embeddings_cache_file_name = format!("embeddings_cache_{}.json", model_name_sanitized);
        let embeddings_cache_file_path = cache_dir.join(embeddings_cache_file_name);

        match load_cached_embeddings(&embeddings_cache_file_path, &current_qa_hash)? {
            Some(cached_embeddings) => {
                self.question_embeddings = cached_embeddings;
                log::info!(
                    "Successfully loaded {} embeddings from cache: {:?}",
                    self.question_embeddings.len(),
                    embeddings_cache_file_path
                );
                return Ok(());
            }
            None => {
                log::info!(
                    "No valid cache found or cache is stale for {}. Generating new embeddings...",
                    embeddings_cache_file_path.display()
                );
            }
        }

        log::info!(
            "Generating {} new embeddings for {} items...",
            self.qa_data.len(),
            qa_json_path
        );

        self.question_embeddings = self
            .generate_embeddings_individually(config, &key_manager)
            .await?;

        log::info!(
            "Successfully generated {} new embeddings.",
            self.question_embeddings.len()
        );

        save_embeddings_cache(
            &embeddings_cache_file_path,
            &current_qa_hash,
            &self.question_embeddings,
        )
        .with_context(|| {
            format!(
                "Failed to save new embeddings to cache: {:?}",
                embeddings_cache_file_path
            )
        })?;

        Ok(())
    }

    pub async fn find_matching_qa(
        &self,
        text: &str,
        config: &Config,
        key_manager: &Arc<GeminiKeyManager>,
    ) -> Result<Option<QAItem>, anyhow::Error> {
        if self.question_embeddings.is_empty() || self.qa_data.is_empty() {
            log::warn!("find_matching_qa called with no embeddings or QA data loaded.");
            return Ok(None);
        }

        log::debug!(
            "Getting embedding for query text: '{}'",
            text.chars().take(70).collect::<String>()
        );

        // 使用重试机制生成查询embedding
        let query_embedding = self
            .generate_single_embedding_with_retry(config, key_manager, text)
            .await?;

        let mut best_match_index = None;
        let mut max_similarity = -1.0f64;

        for (index, q_embedding) in self.question_embeddings.iter().enumerate() {
            let similarity = cosine_similarity(&query_embedding, q_embedding);
            log::trace!(
                "Similarity with Q{}/{} ('{}'): {:.4}",
                index + 1,
                self.question_embeddings.len(),
                self.qa_data[index]
                    .question
                    .chars()
                    .take(30)
                    .collect::<String>(),
                similarity
            );
            if similarity > max_similarity {
                max_similarity = similarity;
                best_match_index = Some(index);
            }
        }

        log::info!(
            "Query: '{}', Max similarity: {:.4}",
            text.chars().take(70).collect::<String>(),
            max_similarity
        );

        if let Some(index) = best_match_index {
            if max_similarity >= config.similarity.threshold as f64 {
                log::info!(
                    "Match found for query '{}': Q{}/{} ('{}') with similarity {:.4}",
                    text.chars().take(70).collect::<String>(),
                    index + 1,
                    self.qa_data.len(),
                    self.qa_data[index]
                        .question
                        .chars()
                        .take(70)
                        .collect::<String>(),
                    max_similarity
                );
                Ok(Some(self.qa_data[index].clone()))
            } else {
                log::info!(
                    "Max similarity {:.4} is below threshold {:.4} for query: '{}'",
                    max_similarity,
                    config.similarity.threshold,
                    text.chars().take(70).collect::<String>()
                );
                Ok(None)
            }
        } else {
            log::warn!(
                "No best_match_index found for query '{}', though embeddings were present.",
                text.chars().take(70).collect::<String>()
            );
            Ok(None)
        }
    }

    async fn generate_embeddings_individually(
        &self,
        config: &Config,
        key_manager: &Arc<GeminiKeyManager>,
    ) -> Result<Vec<Vec<f64>>, anyhow::Error> {
        let mut embeddings = Vec::new();
        let total = self.qa_data.len();

        for (idx, qa_item) in self.qa_data.iter().enumerate() {
            log::info!(
                "Processing item {}/{}: {}",
                idx + 1,
                total,
                qa_item.question.chars().take(50).collect::<String>()
            );

            let embedding = self
                .generate_single_embedding_with_retry(config, &key_manager, &qa_item.question)
                .await?;
            embeddings.push(embedding);

            // 每个请求之间的延迟（12秒确保不超过5 RPM）
            if idx < total - 1 {
                let delay = Duration::from_secs(12);
                log::debug!("Waiting {:?} before next item...", delay);
                sleep(delay).await;
            }
        }

        Ok(embeddings)
    }

    // 带重试机制的单个embedding生成
    async fn generate_single_embedding_with_retry(
        &self,
        config: &Config,
        key_manager: &Arc<GeminiKeyManager>,
        text: &str,
    ) -> Result<Vec<f64>, anyhow::Error> {
        const MAX_ATTEMPTS: u32 = 10;
        let mut attempts = 0;

        loop {
            if attempts >= MAX_ATTEMPTS {
                return Err(anyhow::anyhow!(
                    "Failed to generate embedding after {} attempts. All keys might be exhausted.",
                    MAX_ATTEMPTS
                ));
            }
            attempts += 1;

            let api_key = match key_manager.get_key() {
                Ok(key) => key,
                Err(e) => {
                    log::error!(
                        "Could not retrieve a valid API key: {}. Retrying in 60s.",
                        e
                    );
                    sleep(Duration::from_secs(60)).await;
                    continue;
                }
            };

            match self.generate_single_embedding(&api_key, config, text).await {
                Ok(embedding) => return Ok(embedding),
                Err(e) => {
                    let error_string = e.to_string().to_lowercase();
                    if error_string.contains("429")
                        || error_string.contains("resource has been exhausted")
                    {
                        log::warn!(
                            "API key ending in ...{} is rate-limited. Disabling it for today. Error: {}",
                            &api_key.chars().rev().take(4).collect::<String>(),
                            e
                        );
                        key_manager.disable_key(&api_key);
                        continue; // Immediately try again with the next key.
                    } else {
                        log::error!(
                            "Failed to generate embedding with key ending in ...{}: {}. Retrying in 5s...",
                            &api_key.chars().rev().take(4).collect::<String>(),
                            e
                        );
                        sleep(Duration::from_secs(5)).await;
                        continue;
                    }
                }
            }
        }
    }
    async fn generate_single_embedding(
        &self,
        api_key: &str,
        config: &Config,
        text: &str,
    ) -> Result<Vec<f64>, anyhow::Error> {
        let gemini_client = GeminiClient::new(api_key);
        let model = gemini_client
            .embedding_model_with_ndims(&config.embedding.model, config.embedding.ndims);

        let builder = EmbeddingsBuilder::new(model.clone()).document(text.to_string())?;
        let embeddings = builder.build().await?;

        let embedding = embeddings
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("No embedding generated"))?
            .1
            .first()
            .vec;

        Ok(embedding)
    }
}

pub fn calculate_qa_hash(qa_data: &Vec<QAItem>) -> Result<String, anyhow::Error> {
    let json_string =
        serde_json::to_string(qa_data).context("Failed to serialize QAData for hashing")?;
    let mut hasher = Sha256::new();
    hasher.update(json_string.as_bytes());
    let result = hasher.finalize();
    Ok(format!("{:x}", result))
}

pub fn load_cached_embeddings(
    cache_file_path: &Path,
    expected_qa_hash: &str,
) -> Result<Option<Vec<Vec<f64>>>, anyhow::Error> {
    let hash_file_path: std::path::PathBuf = cache_file_path.with_extension("hash");
    if !cache_file_path.exists() || !hash_file_path.exists() {
        log::info!(
            "Cache file or hash file not found for: {:?}",
            cache_file_path.display()
        );
        return Ok(None);
    }
    let cached_hash = fs::read_to_string(&hash_file_path)
        .with_context(|| format!("Failed to read hash file: {:?}", hash_file_path.display()))?;
    if cached_hash.trim() != expected_qa_hash {
        log::info!(
            "Cache is stale (hash mismatch) for {:?}. Expected: {}, Found: {}",
            cache_file_path.display(),
            expected_qa_hash,
            cached_hash.trim()
        );
        return Ok(None);
    }
    let file = fs::File::open(cache_file_path)
        .with_context(|| format!("Failed to open cache file: {:?}", cache_file_path.display()))?;
    let embeddings: Vec<Vec<f64>> = serde_json::from_reader(std::io::BufReader::new(file))
        .with_context(|| {
            format!(
                "Failed to deserialize embeddings from cache file: {:?}",
                cache_file_path.display()
            )
        })?;
    Ok(Some(embeddings))
}

pub fn save_embeddings_cache(
    cache_file_path: &Path,
    qa_hash: &str,
    embeddings: &Vec<Vec<f64>>,
) -> Result<(), anyhow::Error> {
    if let Some(parent_dir) = cache_file_path.parent() {
        fs::create_dir_all(parent_dir).with_context(|| {
            format!(
                "Failed to create cache directory: {:?}",
                parent_dir.display()
            )
        })?;
    }
    let json_string = serde_json::to_string_pretty(embeddings)
        .context("Failed to serialize embeddings to JSON for saving to cache")?;
    fs::write(cache_file_path, json_string).with_context(|| {
        format!(
            "Failed to write embeddings to cache file: {:?}",
            cache_file_path.display()
        )
    })?;
    let hash_file_path = cache_file_path.with_extension("hash");
    fs::write(&hash_file_path, qa_hash).with_context(|| {
        format!(
            "Failed to write hash to file: {:?}",
            hash_file_path.display()
        )
    })?;
    log::info!(
        "Saved {} embeddings to cache: {:?}",
        embeddings.len(),
        cache_file_path.display()
    );
    Ok(())
}

pub fn format_answer_html(answer: &str) -> String {
    format!("<blockquote>{}</blockquote>", answer)
}

fn cosine_similarity(a: &[f64], b: &[f64]) -> f64 {
    let dot_product: f64 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f64 = a.iter().map(|x| x * x).sum::<f64>().sqrt();
    let norm_b: f64 = b.iter().map(|x| x * x).sum::<f64>().sqrt();
    dot_product / (norm_a * norm_b)
}
