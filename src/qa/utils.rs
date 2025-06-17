use sha2::{Digest, Sha256};

/// 计算问题的 SHA256 哈希值
pub fn get_question_hash(question: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(question.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// 将答案格式化为 HTML 的 blockquote
pub fn format_answer_html(answer: &str) -> String {
    format!("<blockquote>{}</blockquote>", answer)
}
