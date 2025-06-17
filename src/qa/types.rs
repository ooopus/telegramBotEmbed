use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct QAItem {
    pub question: String,
    pub answer: String,
}

#[derive(Debug)]
pub struct QAEmbedding {
    pub qa_data: Vec<QAItem>,
    pub question_embeddings: Vec<Vec<f64>>,
}
