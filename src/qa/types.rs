use serde::{Deserialize, Serialize};
use teloxide::types::MessageEntity;

// --- Step 3: Implement the From trait ---
// This defines how to convert our temporary representation into our final struct.
impl From<FormattedTextRepr> for FormattedText {
    fn from(repr: FormattedTextRepr) -> Self {
        match repr {
            // If the representation was a string, create a FormattedText with it.
            FormattedTextRepr::String(s) => FormattedText {
                text: s,
                entities: Vec::new(),
            },
            // If it was the full struct, just pass the values through.
            FormattedTextRepr::Struct { text, entities } => FormattedText { text, entities },
        }
    }
}

// --- Step 1: Define the "Representation" Enum ---
// This enum describes the possible formats in the JSON data.
#[derive(Deserialize)]
#[serde(untagged)] // Tells serde to try matching variants without a tag.
enum FormattedTextRepr {
    String(String),
    Struct {
        text: String,
        #[serde(default)]
        entities: Vec<MessageEntity>,
    },
}

/// Represents text that may contain formatting.
// --- Step 4: Annotate the final struct ---
// We use `from` for deserialization and derive `Serialize` as usual.
#[derive(Serialize, Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(from = "FormattedTextRepr")]
pub struct FormattedText {
    pub text: String,
    #[serde(default)]
    pub entities: Vec<MessageEntity>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct QAItem {
    pub question: FormattedText,
    pub answer: FormattedText,
}

/// Represents the core data of the Question-Answering system.
/// It holds the QA data and the corresponding question embeddings.
#[derive(Debug, Default)]
pub struct QASystem {
    pub qa_data: Vec<QAItem>,
    pub question_embeddings: Vec<Vec<f64>>,
}

impl QASystem {
    /// Creates a new, empty QASystem.
    pub fn new() -> Self {
        Self::default()
    }
}
