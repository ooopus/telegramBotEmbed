use crate::config::Config;
use crate::qa::types::QAItem;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// 从 JSON 文件加载 QA 数据
pub fn load_qa_items(qa_json_path: &str) -> Result<Vec<QAItem>> {
    log::info!("Loading QA data from: {}", qa_json_path);
    let qa_data_str = fs::read_to_string(qa_json_path)
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

/// 将新的 QAItem 添加到 JSON 文件
pub fn add_qa_item_to_json(config: &Config, item: &QAItem) -> Result<()> {
    let qa_path = Path::new(&config.qa.qa_json_path);
    log::info!("Adding new QA item to {}", qa_path.display());

    if let Some(parent) = qa_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory for {}", qa_path.display()))?;
    }

    let mut qa_data: Vec<QAItem> = if qa_path.exists() {
        let file_content = fs::read_to_string(qa_path)?;
        if file_content.trim().is_empty() {
            Vec::new()
        } else {
            serde_json::from_str(&file_content)?
        }
    } else {
        Vec::new()
    };

    qa_data.push(item.clone());
    let new_content = serde_json::to_string_pretty(&qa_data)?;
    fs::write(qa_path, new_content)?;

    log::info!(
        "Successfully added new QA and saved to {}",
        qa_path.display()
    );
    Ok(())
}

/// 加载词向量缓存
pub fn load_embeddings_cache(
    config: &Config,
) -> Result<(std::path::PathBuf, HashMap<String, Vec<f64>>)> {
    let model_name_sanitized = config
        .embedding
        .model
        .replace(|c: char| !c.is_alphanumeric(), "_");
    let cache_dir = Path::new(&config.cache.dir);
    fs::create_dir_all(cache_dir)
        .with_context(|| format!("Failed to create cache directory: {:?}", cache_dir))?;

    let embeddings_cache_file_name = format!("embeddings_cache_{}.json", model_name_sanitized);
    let cache_path = cache_dir.join(embeddings_cache_file_name);

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

/// 保存词向量缓存
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

/// 根据问题的哈希值从 JSON 文件中删除一个 QAItem
pub fn delete_qa_item_by_hash(config: &Config, question_hash_to_delete: &str) -> Result<()> {
    let qa_path = Path::new(&config.qa.qa_json_path);
    log::info!(
        "Attempting to delete QA item with hash {} from {}",
        question_hash_to_delete,
        qa_path.display()
    );

    let mut qa_data = load_qa_items(&config.qa.qa_json_path)?;
    let initial_len = qa_data.len();

    qa_data.retain(|item| {
        crate::qa::utils::get_question_hash(&item.question) != question_hash_to_delete
    });

    if qa_data.len() < initial_len {
        let new_content = serde_json::to_string_pretty(&qa_data)?;
        fs::write(qa_path, new_content)?;
        log::info!(
            "Successfully deleted QA item and saved to {}",
            qa_path.display()
        );
    } else {
        log::warn!(
            "Could not find QA item with hash {} to delete.",
            question_hash_to_delete
        );
    }

    Ok(())
}

/// 根据旧问题的哈希值，用新的 QAItem 更新 JSON 文件
pub fn update_qa_item_by_hash(
    config: &Config,
    old_question_hash: &str,
    new_item: &QAItem,
) -> Result<()> {
    let qa_path = Path::new(&config.qa.qa_json_path);
    log::info!(
        "Attempting to update QA item with old hash {} in {}",
        old_question_hash,
        qa_path.display()
    );

    let mut qa_data = load_qa_items(&config.qa.qa_json_path)?;
    let mut item_updated = false;

    if let Some(item_to_update) = qa_data
        .iter_mut()
        .find(|item| crate::qa::utils::get_question_hash(&item.question) == old_question_hash)
    {
        *item_to_update = new_item.clone();
        item_updated = true;
    }

    if item_updated {
        let new_content = serde_json::to_string_pretty(&qa_data)?;
        fs::write(qa_path, new_content)?;
        log::info!(
            "Successfully updated QA item and saved to {}",
            qa_path.display()
        );
    } else {
        log::warn!(
            "Could not find QA item with hash {} to update.",
            old_question_hash
        );
    }

    Ok(())
}
