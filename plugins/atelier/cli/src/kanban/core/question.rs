//! An AskUserQuestion call a session has open, as its mod recorded it.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestionOption {
    pub label: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    pub question: String,
    pub header: String,
    pub multi_select: bool,
    pub options: Vec<QuestionOption>,
}

/// The lengths the texts had before the mod cut them, in UTF-16 code units:
/// per question, then per question's option labels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AskLengths {
    pub question: Vec<u64>,
    pub labels: Vec<Vec<u64>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ask {
    pub tool_use_id: String,
    pub questions: Vec<Question>,
    pub at: u64,
    pub orig_len: Option<AskLengths>,
}
