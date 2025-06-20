use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct Config {
    pub telegram: TelegramConfig,
    pub embedding: EmbeddingConfig,
    pub cache: CacheConfig,
    pub similarity: SimilarityConfig,
    pub message: MessageConfig,
    pub log_level: LogLevel,
    pub qa: QaConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct TelegramConfig {
    pub token: String,
    pub super_admins: Vec<i64>,
    pub allowed_group_ids: Vec<i64>,
}

impl Default for TelegramConfig {
    fn default() -> Self {
        Self {
            token: "YOUR_TELEGRAM_BOT_TOKEN".to_string(),
            super_admins: vec![],
            allowed_group_ids: vec![],
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct EmbeddingConfig {
    pub api_keys: Vec<String>,
    pub model: String,
    pub ndims: usize,
    pub rpm: u32,
    pub rpd: u32,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            api_keys: vec!["YOUR_API_KEY".to_string()],
            model: "gemini-embedding-exp-03-07".to_string(),
            ndims: 3072,
            rpm: 5,
            rpd: 100,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct CacheConfig {
    pub dir: String,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            dir: "cache".to_string(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct SimilarityConfig {
    pub threshold: f32,
}

impl Default for SimilarityConfig {
    fn default() -> Self {
        Self { threshold: 0.85 }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct MessageConfig {
    pub delete_delay: u64,
    pub timeout: i64,
}

impl Default for MessageConfig {
    fn default() -> Self {
        Self {
            delete_delay: 10,
            timeout: 60,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl Default for LogLevel {
    fn default() -> Self {
        Self::Info
    }
}

impl From<LogLevel> for log::Level {
    fn from(level: LogLevel) -> Self {
        match level {
            LogLevel::Trace => log::Level::Trace,
            LogLevel::Debug => log::Level::Debug,
            LogLevel::Info => log::Level::Info,
            LogLevel::Warn => log::Level::Warn,
            LogLevel::Error => log::Level::Error,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct QaConfig {
    pub qa_json_path: String,
}

impl Default for QaConfig {
    fn default() -> Self {
        Self {
            qa_json_path: "data/QA.json".to_string(),
        }
    }
}
