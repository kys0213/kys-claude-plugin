use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEntry {
    #[serde(rename = "type")]
    pub entry_type: String,
    pub message: Option<Message>,
    pub name: Option<String>,
    pub input: Option<serde_json::Value>,
    pub output: Option<String>,
    pub timestamp: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub content: Content,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Content {
    Text(String),
    Array(Vec<ContentItem>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentItem {
    #[serde(rename = "type")]
    pub item_type: String,
    pub text: Option<String>,
    pub name: Option<String>,
    pub id: Option<String>,
    pub input: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct ToolUse {
    pub name: String,
    pub timestamp: Option<i64>,
    pub input: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub display: String,
    pub timestamp: i64,
    pub project: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolSequence {
    pub tools: Vec<String>,
    pub count: usize,
    pub sessions: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowAnalysisResult {
    pub total_sequences: usize,
    pub unique_sequences: usize,
    pub top_sequences: Vec<ToolSequence>,
    pub tool_usage_stats: Vec<(String, usize)>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PromptFrequency {
    pub prompt: String,
    pub count: usize,
    pub weighted_count: f64,
    pub last_used: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PromptAnalysisResult {
    pub total: usize,
    pub unique: usize,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub top_prompts: Vec<PromptFrequency>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TacitPattern {
    #[serde(rename = "type")]
    pub pattern_type: String,
    pub pattern: String,
    pub normalized: String,
    pub examples: Vec<String>,
    pub count: usize,
    pub bm25_score: f64,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct TacitAnalysisResult {
    pub total: usize,
    pub patterns: Vec<TacitPattern>,
}

// --- Cache types ---

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheIndex {
    pub project: String,
    pub project_encoded: String,
    pub last_updated: String,
    pub cache_version: String,
    pub sessions: Vec<CacheSessionMeta>,
    pub total_prompts: usize,
    pub total_sessions: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheSessionMeta {
    pub id: String,
    pub file: String,
    pub file_size: u64,
    pub prompt_count: usize,
    pub tool_use_count: usize,
    pub first_timestamp: Option<String>,
    pub last_timestamp: Option<String>,
    pub duration_minutes: Option<i64>,
    pub dominant_tools: Vec<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub id: String,
    pub prompts: Vec<SummaryPrompt>,
    pub tool_use_count: usize,
    pub tool_sequences: Vec<String>,
    pub directives: Vec<String>,
    pub corrections: Vec<String>,
    pub files_mutated: Vec<String>,
    pub static_signals: StaticSignals,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SummaryPrompt {
    pub text: String,
    pub timestamp: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StaticSignals {
    pub has_directive: bool,
    pub has_correction: bool,
    pub prompt_density: String,
    pub workflow_complexity: String,
}
