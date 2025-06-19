use crate::qa::types::QAItem;
use std::cmp::Ordering;

/// 计算两个 f64 切片的余弦相似度
fn cosine_similarity(a: &[f64], b: &[f64]) -> f64 {
    let dot_product: f64 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f64 = a.iter().map(|x| x * x).sum::<f64>().sqrt();
    let norm_b: f64 = b.iter().map(|x| x * x).sum::<f64>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot_product / (norm_a * norm_b)
}

/// 在一组问题词向量中找到与查询词向量最匹配的一个
/// Returns the index and similarity score of the best match, or None if the input is empty.
pub fn find_best_match(
    query_embedding: &[f64],
    question_embeddings: &[Vec<f64>],
) -> Option<(usize, f64)> {
    question_embeddings
        .iter()
        .enumerate()
        .map(|(index, q_embedding)| (index, cosine_similarity(query_embedding, q_embedding)))
        .max_by(|(_, sim_a), (_, sim_b)| sim_a.partial_cmp(sim_b).unwrap_or(Ordering::Equal))
}

/// Searches for QA items where the question text contains the given keywords.
/// The search is case-insensitive and returns up to 10 matches.
pub fn search_by_keyword(qa_data: &[QAItem], keywords: &str) -> Vec<QAItem> {
    if keywords.is_empty() {
        return Vec::new();
    }
    let lower_keywords = keywords.to_lowercase();
    qa_data
        .iter()
        .filter(|item| item.question.text.to_lowercase().contains(&lower_keywords))
        .take(10) // Limit the number of results to avoid overly large responses
        .cloned()
        .collect()
}
