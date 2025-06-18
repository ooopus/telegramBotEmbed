use sha2::{Digest, Sha256};

/// 计算问题的 SHA256 哈希值
pub fn get_question_hash(question: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(question.as_bytes());
    format!("{:x}", hasher.finalize())
}
