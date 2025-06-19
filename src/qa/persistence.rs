use crate::config::Config;
use crate::qa::types::QAItem;
use crate::qa::utils;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Loads QA data from a JSON file. Creates the directory and an empty file if they don't exist.
pub fn load_qa_items(qa_json_path: &str) -> Result<Vec<QAItem>> {
    log::info!("Loading QA data from: {}", qa_json_path);
    let path = Path::new(qa_json_path);

    // Create parent directory if it doesn't exist
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory for {}", path.display()))?;
    }

    // Create the file with an empty array if it doesn't exist
    if !path.exists() {
        fs::write(path, "[]")
            .with_context(|| format!("Failed to create empty QA file at: {}", path.display()))?;
    }

    let qa_data_str = fs::read_to_string(path)
        .with_context(|| format!("Failed to read QA JSON file from: {}", qa_json_path))?;

    let items: Vec<QAItem> = serde_json::from_str(&qa_data_str)
        .with_context(|| format!("Failed to deserialize QA JSON from: {}", qa_json_path))?;
    log::info!(
        "Successfully loaded {} QA items from {}",
        items.len(),
        qa_json_path
    );
    Ok(items)
}

/// Saves a slice of QAItems to the JSON file, overwriting its entire contents.
/// This is used by the QAService after any in-memory modification (add, update, delete).
pub fn save_all_qa_items(qa_json_path: &str, items: &[QAItem]) -> Result<()> {
    log::info!("Saving {} QA items to {}", items.len(), qa_json_path);
    let new_content = serde_json::to_string_pretty(items)
        .with_context(|| format!("Failed to serialize {} QA items", items.len()))?;
    fs::write(qa_json_path, new_content)
        .with_context(|| format!("Failed to write QA data to {}", qa_json_path))?;
    log::info!("Successfully saved all QA items to {}", qa_json_path);
    Ok(())
}

/// Gets the canonical path to the embeddings cache file based on config.
fn get_cache_path(config: &Config) -> Result<PathBuf> {
    let model_name_sanitized = config
        .embedding
        .model
        .replace(|c: char| !c.is_alphanumeric(), "_");
    let cache_dir = Path::new(&config.cache.dir);
    fs::create_dir_all(cache_dir)
        .with_context(|| format!("Failed to create cache directory: {:?}", cache_dir))?;

    let embeddings_cache_file_name = format!("embeddings_cache_{}.json", model_name_sanitized);
    Ok(cache_dir.join(embeddings_cache_file_name))
}

/// Loads the embeddings cache from its file.
pub fn load_embeddings_cache(config: &Config) -> Result<(PathBuf, HashMap<String, Vec<f64>>)> {
    let cache_path = get_cache_path(config)?;

    let cache: HashMap<String, Vec<f64>> = if cache_path.exists() {
        log::info!("Loading existing cache from: {}", cache_path.display());
        let file = fs::File::open(&cache_path)?;
        serde_json::from_reader(std::io::BufReader::new(file)).unwrap_or_else(|e| {
            log::warn!(
                "Failed to parse cache file, creating new cache. Error: {}",
                e
            );
            HashMap::new()
        })
    } else {
        log::info!("No cache file found. A new one will be created.");
        HashMap::new()
    };

    Ok((cache_path, cache))
}

/// Saves the entire embeddings cache to its file.
pub fn save_embeddings_cache(cache_path: &Path, cache: &HashMap<String, Vec<f64>>) -> Result<()> {
    log::info!(
        "Saving updated cache with {} total entries to {}...",
        cache.len(),
        cache_path.display()
    );
    let json_string =
        serde_json::to_string_pretty(cache).context("Failed to serialize embeddings cache")?;
    fs::write(cache_path, json_string)
        .with_context(|| format!("Failed to write to cache file: {:?}", cache_path))?;
    log::info!("Successfully saved updated cache.");
    Ok(())
}

/// Adds a single new embedding to the cache file efficiently without rewriting the whole file.
pub fn add_embedding_to_cache(
    config: &Config,
    question_text: &str,
    embedding: Vec<f64>,
) -> Result<()> {
    let (cache_path, mut cache) = load_embeddings_cache(config)?;
    let question_hash = utils::get_question_hash(question_text);
    cache.insert(question_hash, embedding);
    save_embeddings_cache(&cache_path, &cache)?;
    Ok(())
}
