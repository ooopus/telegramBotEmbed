use serde::{Deserialize, Serialize};
use std::fs;
// use std::io::{Read, Write}; // Read/Write not directly needed for fs::read_to_string and fs::write with strings
use std::path::Path;
use anyhow::{Context, anyhow}; // anyhow::Context is already imported here
use sha2::{Digest, Sha256};
// Removed: use std::net::TcpStream; // No longer using manual TCP client

// Struct for reqwest-based get_embedding
#[derive(Deserialize, Debug)]
struct EmbeddingData { // As per subtask item 2
    embedding: Vec<f32>,
}

#[derive(Deserialize, Debug)]
struct EmbeddingResponse { // As per subtask item 2
    data: Vec<EmbeddingData>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct QAItem {
    pub question: String,
    pub answer: String,
}

#[derive(Debug)]
pub struct QAEmbedding {
    pub qa_data: Vec<QAItem>,
    pub question_embeddings: Vec<Vec<f32>>,
}

impl QAEmbedding {
    pub fn new() -> Self {
        QAEmbedding {
            qa_data: Vec::new(),
            question_embeddings: Vec::new(),
        }
    }

    // Updated to be async and use new get_embedding
    pub async fn load_and_embed_qa(
        &mut self,
        config: &crate::Config, // Assuming Config is in crate root (main.rs)
        qa_json_path: &str,
        client: &reqwest::Client, // Pass reqwest client
    ) -> Result<(), anyhow::Error> {
        log::info!("Loading QA data from: {}", qa_json_path);
        let qa_data_str = fs::read_to_string(qa_json_path)
            .with_context(|| format!("Failed to read QA JSON file from: {}", qa_json_path))?;
        self.qa_data = serde_json::from_str(&qa_data_str)
            .with_context(|| format!("Failed to deserialize QA JSON from: {}", qa_json_path))?;
        log::info!("Successfully loaded {} QA items from {}", self.qa_data.len(), qa_json_path);

        let current_qa_hash = calculate_qa_hash(&self.qa_data)
            .context("Failed to calculate QA hash")?;
        log::debug!("Calculated QA hash for '{}': {}", qa_json_path, current_qa_hash);

        let model_name_sanitized = config.embed_model.replace(|c: char| !c.is_alphanumeric(), "_");
        let cache_dir = Path::new(&config.cache_dir);

        fs::create_dir_all(cache_dir).with_context(|| format!("Failed to create cache directory: {:?}", cache_dir))?;

        let embeddings_cache_file_name = format!("embeddings_cache_{}.json", model_name_sanitized);
        let embeddings_cache_file_path = cache_dir.join(embeddings_cache_file_name);

        match load_cached_embeddings(&embeddings_cache_file_path, &current_qa_hash)? {
            Some(cached_embeddings) => {
                self.question_embeddings = cached_embeddings;
                log::info!("Successfully loaded {} embeddings from cache: {:?}", self.question_embeddings.len(), embeddings_cache_file_path);
                return Ok(());
            }
            None => {
                log::info!("No valid cache found or cache is stale for {}. Generating new embeddings...", embeddings_cache_file_path.display());
            }
        }

        log::info!("Generating {} new embeddings for {} items...", self.qa_data.len(), qa_json_path);
        let mut new_embeddings = Vec::new();
        for (index, qa_item) in self.qa_data.iter().enumerate() {
            // Old: print!("Fetching embedding for Q{}: {}... ", index + 1, qa_item.question.chars().take(50).collect::<String>());
            log::info!("Fetching embedding for Q{}/{}: '{}'...", index + 1, self.qa_data.len(), qa_item.question.chars().take(70).collect::<String>());
            match get_embedding( // Use new async get_embedding
                &qa_item.question,
                &config.api_key,
                &config.embed_api_url,
                &config.embed_model,
                client, // Pass client
            )
            .await // await the async call
            {
                Ok(embedding) => {
                    println!("Ok ({} dims)", embedding.len());
                    new_embeddings.push(embedding);
                }
                Err(e) => {
                    // Log the specific error and the question that failed.
                    log::error!("Failed to get embedding for Q{}: '{}'. Error: {:?}", index + 1, qa_item.question, e);
                    // Current behavior: fail entire process if one embedding fails. Acceptable for now.
                    return Err(e).context(format!(
                        "Failed to get embedding for question Q{}: '{}'",
                        index + 1, qa_item.question
                    ));
                }
            }
        }
        self.question_embeddings = new_embeddings;
        log::info!("Successfully generated {} new embeddings.", self.question_embeddings.len());

        save_embeddings_cache(
            &embeddings_cache_file_path,
            &current_qa_hash,
            &self.question_embeddings,
        )
        .with_context(|| format!("Failed to save new embeddings to cache: {:?}", embeddings_cache_file_path))?;

        Ok(())
    }

    pub async fn find_matching_qa(
        &self,
        text: &str,
        config: &crate::Config, // Assuming Config is in crate root (main.rs)
        client: &reqwest::Client,
    ) -> Result<Option<QAItem>, anyhow::Error> {
        if self.question_embeddings.is_empty() || self.qa_data.is_empty() {
            log::warn!("find_matching_qa called with no embeddings or QA data loaded.");
            return Ok(None);
        }

        log::debug!("Getting embedding for query text: '{}'", text.chars().take(70).collect::<String>());
        let query_embedding = match get_embedding(
            text,
            &config.api_key,
            &config.embed_api_url,
            &config.embed_model,
            client,
        )
        .await
        {
            Ok(embedding) => embedding,
            Err(e) => {
                log::error!("Failed to get embedding for query text '{}': {:?}", text.chars().take(70).collect::<String>(), e);
                return Err(e).context(format!("Failed to get embedding for query text: {}", text.chars().take(70).collect::<String>()));
            }
        };

        let mut best_match_index = None;
        let mut max_similarity = -1.0f32; // Cosine similarity ranges from -1 to 1

        for (index, q_embedding) in self.question_embeddings.iter().enumerate() {
            let similarity = cosine_similarity(&query_embedding, q_embedding);
            log::trace!("Similarity with Q{}/{} ('{}'): {:.4}", index + 1, self.question_embeddings.len(), self.qa_data[index].question.chars().take(30).collect::<String>(), similarity);
            if similarity > max_similarity {
                max_similarity = similarity;
                best_match_index = Some(index);
            }
        }

        log::info!("Query: '{}', Max similarity: {:.4}", text.chars().take(70).collect::<String>(), max_similarity);

        if let Some(index) = best_match_index {
            if max_similarity >= config.similarity_threshold {
                log::info!("Match found for query '{}': Q{}/{} ('{}') with similarity {:.4}", text.chars().take(70).collect::<String>(), index + 1, self.qa_data.len(), self.qa_data[index].question.chars().take(70).collect::<String>(), max_similarity);
                Ok(Some(self.qa_data[index].clone()))
            } else {
                log::info!("Max similarity {:.4} is below threshold {:.4} for query: '{}'", max_similarity, config.similarity_threshold, text.chars().take(70).collect::<String>());
                Ok(None)
            }
        } else {
            // This case should ideally not be reached if self.question_embeddings is not empty.
            log::warn!("No best_match_index found for query '{}', though embeddings were present.", text.chars().take(70).collect::<String>());
            Ok(None)
        }
    }
}

pub fn calculate_qa_hash(qa_data: &Vec<QAItem>) -> Result<String, anyhow::Error> {
    let json_string = serde_json::to_string(qa_data)
        .context("Failed to serialize QAData for hashing")?;
    let mut hasher = Sha256::new();
    hasher.update(json_string.as_bytes());
    let result = hasher.finalize();
    Ok(format!("{:x}", result)) // Wrapped in Ok()
}

pub fn load_cached_embeddings(
    cache_file_path: &Path,
    expected_qa_hash: &str,
) -> Result<Option<Vec<Vec<f32>>>, anyhow::Error> {
    let hash_file_path = cache_file_path.with_extension("hash");
    if !cache_file_path.exists() || !hash_file_path.exists() {
        log::info!("Cache file or hash file not found for: {:?}", cache_file_path.display());
        return Ok(None);
    }
    let cached_hash = fs::read_to_string(&hash_file_path)
        .with_context(|| format!("Failed to read hash file: {:?}", hash_file_path.display()))?;
    if cached_hash.trim() != expected_qa_hash {
        log::info!("Cache is stale (hash mismatch) for {:?}. Expected: {}, Found: {}", cache_file_path.display(), expected_qa_hash, cached_hash.trim());
        // Optionally: Delete stale cache files
        // fs::remove_file(cache_file_path).ok();
        // fs::remove_file(hash_file_path).ok();
        return Ok(None);
    }
    let file = fs::File::open(cache_file_path)
        .with_context(|| format!("Failed to open cache file: {:?}", cache_file_path.display()))?;
    let embeddings: Vec<Vec<f32>> = serde_json::from_reader(std::io::BufReader::new(file))
        .with_context(|| format!("Failed to deserialize embeddings from cache file: {:?}", cache_file_path.display()))?;
    // log::info! is now in the caller `load_and_embed_qa`
    Ok(Some(embeddings))
}

pub fn save_embeddings_cache(
    cache_file_path: &Path,
    qa_hash: &str,
    embeddings: &Vec<Vec<f32>>,
) -> Result<(), anyhow::Error> {
    if let Some(parent_dir) = cache_file_path.parent() {
        fs::create_dir_all(parent_dir).with_context(|| format!("Failed to create cache directory: {:?}", parent_dir.display()))?;
    }
    let json_string = serde_json::to_string_pretty(embeddings)
        .context("Failed to serialize embeddings to JSON for saving to cache")?;
    fs::write(cache_file_path, json_string)
        .with_context(|| format!("Failed to write embeddings to cache file: {:?}", cache_file_path.display()))?;
    let hash_file_path = cache_file_path.with_extension("hash");
    fs::write(&hash_file_path, qa_hash)
        .with_context(|| format!("Failed to write hash to file: {:?}", hash_file_path.display()))?;
    log::info!("Saved {} embeddings to cache: {:?}", embeddings.len(), cache_file_path.display());
    Ok(())
}

// New async get_embedding using reqwest
pub async fn get_embedding(
    text: &str,
    api_key: &str,
    embed_api_url: &str,
    embed_model: &str,
    client: &reqwest::Client,
) -> Result<Vec<f32>, anyhow::Error> {
    log::debug!("Requesting embedding for text: '{}' with model: {}", text.chars().take(70).collect::<String>(), embed_model);
    let payload = serde_json::json!({
        "model": embed_model,
        "input": text
    });
    log::trace!("Embedding request payload: {:?}", payload);

    let response = client
        .post(embed_api_url)
        .bearer_auth(api_key)
        .json(&payload)
        .send()
        .await
        .with_context(|| format!("Failed to send request to embedding API URL: {}", embed_api_url))?;

    if !response.status().is_success() {
        let status = response.status();
        let error_body = response.text().await.unwrap_or_else(|e| format!("Could not read error body: {}", e));
        log::error!("Embedding API request failed with status {}. Response body: {}", status, error_body);
        return Err(anyhow!(
            "Embedding API request failed with status {}: {}",
            status,
            error_body
        ));
    }

    let parsed_response: EmbeddingResponse = response
        .json()
        .await
        .context("Failed to deserialize JSON response from embedding API")?;

    log::trace!("Received embedding API response: {:?}", parsed_response);

    if let Some(first_data) = parsed_response.data.into_iter().next() {
        Ok(first_data.embedding)
    } else {
        Err(anyhow!("No embedding data found in the API response structure"))
    }
}

pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.is_empty() || b.is_empty() || a.len() != b.len() {
        return 0.0; // Or handle error: dimensions mismatch or empty vectors
    }

    let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();

    let magnitude_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let magnitude_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if magnitude_a == 0.0 || magnitude_b == 0.0 {
        return 0.0; // Avoid division by zero
    }

    dot_product / (magnitude_a * magnitude_b)
}

pub fn format_answer_html(answer: &str) -> String {
    let escaped_answer = teloxide::utils::html::escape(answer);
    format!("<blockquote>{}</blockquote>", escaped_answer)
}

// Removed get_embedding_simplified (TCP client) and its url_parse helper.
// If needed for comparison or specific tests later, it can be retrieved from version history.

#[cfg(test)]
mod tests {
    use super::*; // Imports items from the parent module (qa.rs)
    use std::fs; // `File` and `Write` are not directly used by name in tests after review
    use std::path::Path;

    // Helper function to create a dummy Config for tests if needed
    // For functions like load_cached_embeddings or save_embeddings_cache,
    // file paths are constructed, so actual Config might not be strictly needed
    // if paths are directly provided or mocked.
    // For `find_matching_qa` or `load_and_embed_qa` if they were to be unit tested
    // without full network/FS mocking, a simplified Config would be useful.
    // For now, tests will focus on functions not heavily reliant on a complex Config.

    #[test]
    fn test_cosine_similarity_identical() {
        let v1 = vec![1.0, 2.0, 3.0];
        let v2 = vec![1.0, 2.0, 3.0];
        assert!((cosine_similarity(&v1, &v2) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let v1 = vec![1.0, 0.0];
        let v2 = vec![0.0, 1.0];
        assert!((cosine_similarity(&v1, &v2) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let v1 = vec![1.0, 2.0];
        let v2 = vec![-1.0, -2.0];
        assert!((cosine_similarity(&v1, &v2) - (-1.0)).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_zero_magnitude() {
        let v1 = vec![0.0, 0.0];
        let v2 = vec![1.0, 2.0];
        assert!((cosine_similarity(&v1, &v2) - 0.0).abs() < 1e-6, "Similarity with zero vector v1");
        assert!((cosine_similarity(&v2, &v1) - 0.0).abs() < 1e-6, "Similarity with zero vector v2");
        let v3 = vec![0.0, 0.0];
        assert!((cosine_similarity(&v1, &v3) - 0.0).abs() < 1e-6, "Similarity between two zero vectors");
    }

    #[test]
    fn test_cosine_similarity_empty_vectors() {
        let v1 = vec![];
        let v2 = vec![1.0, 2.0];
        assert!((cosine_similarity(&v1, &v2) - 0.0).abs() < 1e-6);
        assert!((cosine_similarity(&v2, &v1) - 0.0).abs() < 1e-6);
        let v3 = vec![];
        assert!((cosine_similarity(&v1, &v3) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_calculate_qa_hash_consistency() {
        let qa_items = vec![
            QAItem { question: "q1".to_string(), answer: "a1".to_string() },
            QAItem { question: "q2".to_string(), answer: "a2".to_string() },
        ];
        let hash1 = calculate_qa_hash(&qa_items).unwrap();
        let hash2 = calculate_qa_hash(&qa_items).unwrap();
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_calculate_qa_hash_sensitivity() {
        let qa_items1 = vec![QAItem { question: "q1".to_string(), answer: "a1".to_string() }];
        let qa_items2 = vec![QAItem { question: "q2".to_string(), answer: "a1".to_string() }]; // Different question
        let qa_items3 = vec![QAItem { question: "q1".to_string(), answer: "a2".to_string() }]; // Different answer

        let hash1 = calculate_qa_hash(&qa_items1).unwrap();
        let hash2 = calculate_qa_hash(&qa_items2).unwrap();
        let hash3 = calculate_qa_hash(&qa_items3).unwrap();

        assert_ne!(hash1, hash2);
        assert_ne!(hash1, hash3);
        assert_ne!(hash2, hash3); // Also ensure these are different
    }

    // Helper to ensure test cache directory exists and is clean
    fn setup_test_cache_dir(dir_path_str: &str) -> std::io::Result<()> {
        let dir_path = Path::new(dir_path_str);
        if dir_path.exists() {
            std::fs::remove_dir_all(dir_path)?;
        }
        std::fs::create_dir_all(dir_path)?;
        Ok(())
    }

    #[test]
    fn test_embedding_cache_save_and_load() {
        let cache_dir_base = "target/test_cache"; // Base for all cache tests
        let specific_test_cache_dir = Path::new(cache_dir_base).join("embeddings_cache_test");
        setup_test_cache_dir(specific_test_cache_dir.to_str().unwrap()).expect("Failed to set up test cache directory");

        let model_name = "test_model_cache";
        // Construct path consistent with how load_and_embed_qa does it.
        let cache_file_name = format!("embeddings_cache_{}.json", model_name);
        let cache_file_path = specific_test_cache_dir.join(cache_file_name);

        let embeddings: Vec<Vec<f32>> = vec![vec![1.0, 2.0, 0.5], vec![3.0, 4.0, 1.5]];
        let qa_hash = "test_hash_abc_123";

        // Test saving
        save_embeddings_cache(&cache_file_path, qa_hash, &embeddings).unwrap();
        assert!(cache_file_path.exists(), "Cache file should be created");

        let hash_file_path = cache_file_path.with_extension("hash");
        assert!(hash_file_path.exists(), "Hash file should be created");
        let saved_hash = std::fs::read_to_string(hash_file_path).unwrap();
        assert_eq!(saved_hash, qa_hash, "Saved hash should match original hash");

        // Test loading with correct hash
        let loaded_embeddings = load_cached_embeddings(&cache_file_path, qa_hash)
            .unwrap()
            .expect("Should load embeddings with correct hash");
        assert_eq!(loaded_embeddings, embeddings, "Loaded embeddings should match saved ones");

        // Test loading with incorrect hash
        let incorrect_hash = "incorrect_hash_xyz_789";
        let result_incorrect_hash = load_cached_embeddings(&cache_file_path, incorrect_hash).unwrap();
        assert!(result_incorrect_hash.is_none(), "Should return None for incorrect hash");

        // Test loading when cache file is missing (after deleting it)
        std::fs::remove_file(&cache_file_path).unwrap();
        let result_missing_file = load_cached_embeddings(&cache_file_path, qa_hash).unwrap();
        assert!(result_missing_file.is_none(), "Should return None if cache file is missing");

        // Clean up: remove the specific test_cache_dir.
        // If using a common base like "target/test_cache", be careful if other tests use it.
        // For isolated test, removing specific_test_cache_dir is fine.
        std::fs::remove_dir_all(specific_test_cache_dir).expect("Failed to clean up test cache directory");
    }
}
