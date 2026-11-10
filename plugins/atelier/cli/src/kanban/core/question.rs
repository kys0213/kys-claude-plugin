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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ask {
    pub tool_use_id: String,
    pub questions: Vec<Question>,
    pub at: u64,
}
