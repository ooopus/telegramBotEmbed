mod embedding;
pub mod persistence;
pub mod search;
pub mod service;
pub mod types;
mod utils;

// Re-export the primary service and key types for easy access from other modules.
pub use service::QAService;
pub use utils::get_question_hash;
