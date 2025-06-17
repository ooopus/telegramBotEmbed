use crate::{config::Config, gemini::key_manager::GeminiKeyManager};
use anyhow::{Result, anyhow};
use rig::{
    client::EmbeddingsClient, embeddings::builder::EmbeddingsBuilder,
    providers::gemini::Client as GeminiClient,
};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

/// 使用 rig 和 Gemini API 生成单个文本的词向量
async fn generate_single_embedding(api_key: &str, config: &Config, text: &str) -> Result<Vec<f64>> {
    let gemini_client = GeminiClient::new(api_key);
    let model =
        gemini_client.embedding_model_with_ndims(&config.embedding.model, config.embedding.ndims);
    let builder = EmbeddingsBuilder::new(model.clone()).document(text.to_string())?;
    let embeddings = builder.build().await?;
    let embedding = embeddings
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("No embedding generated"))?
        .1
        .first()
        .vec;
    Ok(embedding)
}

/// 生成单个词向量，包含重试和 API Key 管理逻辑
pub async fn generate_embedding_with_retry(
    config: &Config,
    key_manager: &Arc<GeminiKeyManager>,
    text: &str,
) -> Result<Vec<f64>> {
    const MAX_ATTEMPTS: u32 = 10;
    let mut attempts = 0;

    loop {
        if attempts >= MAX_ATTEMPTS {
            return Err(anyhow!(
                "Failed to generate embedding after {} attempts.",
                MAX_ATTEMPTS
            ));
        }
        attempts += 1;

        let api_key = match key_manager.get_key() {
            Ok(key) => key,
            Err(e) => {
                log::error!("Could not get API key: {}. Retrying in 60s.", e);
                sleep(Duration::from_secs(60)).await;
                continue;
            }
        };

        match generate_single_embedding(&api_key, config, text).await {
            Ok(embedding) => return Ok(embedding),
            Err(e) => {
                let error_string = e.to_string().to_lowercase();
                if error_string.contains("429")
                    || error_string.contains("resource has been exhausted")
                {
                    log::warn!("API key rate-limited. Disabling it. Error: {}", e);
                    key_manager.disable_key(&api_key);
                    continue; // Immediately try with the next key
                } else {
                    log::error!("Failed to generate embedding: {}. Retrying in 5s...", e);
                    sleep(Duration::from_secs(5)).await;
                    continue;
                }
            }
        }
    }
}
