//! src/qa/management.rs
//!
//! 该模块提供了用于管理 Q&A 项目生命周期的高级函数。
//! 它封装了持久化（修改 JSON 文件）和状态更新（重新加载和重新嵌入数据）的逻辑，
//! 以确保一致性并减少机器人处理器中的代码重复。

use crate::{
    config::Config,
    gemini::key_manager::GeminiKeyManager,
    qa::{
        persistence::{add_qa_item_to_json, delete_qa_item_by_hash, update_qa_item_by_hash},
        types::{QAEmbedding, QAItem},
    },
};
use anyhow::Result;
use std::sync::Arc;
use tokio::sync::Mutex;

/// 添加一个新的 Q&A 项目，将其保存到 JSON 文件，并触发词向量的重新加载。
pub async fn add_qa(
    config: &Config,
    key_manager: &Arc<GeminiKeyManager>,
    qa_embedding_mutex: &Mutex<QAEmbedding>,
    new_item: &QAItem,
) -> Result<()> {
    // 步骤 1: 持久化新项目
    add_qa_item_to_json(config, new_item)?;

    // 步骤 2: 锁定并重新加载内存中的词向量
    let mut qa_guard = qa_embedding_mutex.lock().await;
    qa_guard.load_and_embed_qa(config, key_manager).await?;

    Ok(())
}

/// 根据问题的哈希值删除一个 Q&A 项目，保存更改，并触发重新加载。
pub async fn delete_qa(
    config: &Config,
    key_manager: &Arc<GeminiKeyManager>,
    qa_embedding_mutex: &Mutex<QAEmbedding>,
    question_hash: &str,
) -> Result<()> {
    // 步骤 1: 持久化删除操作
    delete_qa_item_by_hash(config, question_hash)?;

    // 步骤 2: 锁定并重新加载
    let mut qa_guard = qa_embedding_mutex.lock().await;
    qa_guard.load_and_embed_qa(config, key_manager).await?;

    Ok(())
}

/// 根据旧问题的哈希值更新一个现有的 Q&A 项目，并触发重新加载。
pub async fn update_qa(
    config: &Config,
    key_manager: &Arc<GeminiKeyManager>,
    qa_embedding_mutex: &Mutex<QAEmbedding>,
    old_question_hash: &str,
    new_item: &QAItem,
) -> Result<()> {
    // 步骤 1: 持久化更新操作
    update_qa_item_by_hash(config, old_question_hash, new_item)?;

    // 步骤 2: 锁定并重新加载
    let mut qa_guard = qa_embedding_mutex.lock().await;
    qa_guard.load_and_embed_qa(config, key_manager).await?;

    Ok(())
}
