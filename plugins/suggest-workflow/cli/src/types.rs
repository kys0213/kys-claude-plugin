use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEntry {
    #[serde(rename = "type")]
    pub entry_type: String,
    #[serde(default)]
    pub message: Option<Message>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub input: Option<serde_json::Value>,
    /// Skipped during deserialization — never accessed, avoids parsing large assistant outputs.
    #[serde(skip_deserializing)]
    pub output: Option<String>,
    #[serde(default)]
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
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
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

// --- Statistical analysis types (rule-free) ---

/// Tool transition: (from_tool, to_tool) → count
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransitionMatrixResult {
    pub transitions: Vec<TransitionEntry>,
    pub total_transitions: usize,
    pub unique_tools: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransitionEntry {
    pub from: String,
    pub to: String,
    pub count: usize,
    pub probability: f64,
}

/// Repetition/loop statistics detected from data distribution
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepetitionResult {
    pub file_edit_outliers: Vec<FileEditOutlier>,
    pub tool_loops: Vec<ToolLoop>,
    pub session_stats: Vec<SessionRepetitionStats>,
    pub global_mean_edits_per_file: f64,
    pub global_std_dev: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEditOutlier {
    pub file: String,
    pub edit_count: usize,
    pub session_id: String,
    pub z_score: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolLoop {
    pub sequence: Vec<String>,
    pub repeat_count: usize,
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRepetitionStats {
    pub session_id: String,
    pub total_tool_uses: usize,
    pub unique_tool_uses: usize,
    pub max_consecutive_same_tool: usize,
    pub most_repeated_tool: Option<String>,
}

/// Weekly trend aggregation
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrendResult {
    pub weeks: Vec<WeeklyBucket>,
    pub tool_trends: Vec<ToolTrend>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeeklyBucket {
    pub week: String,
    pub prompt_count: usize,
    pub session_count: usize,
    pub tool_counts: Vec<(String, usize)>,
    pub unique_files_edited: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolTrend {
    pub tool: String,
    pub weekly_counts: Vec<usize>,
    pub trend_slope: f64,
}

/// File hotspot analysis
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileAnalysisResult {
    pub hot_files: Vec<FileHotspot>,
    pub co_change_groups: Vec<CoChangeGroup>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileHotspot {
    pub path: String,
    pub edit_count: usize,
    pub session_count: usize,
    pub tools_used: Vec<(String, usize)>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoChangeGroup {
    pub files: Vec<String>,
    pub co_occurrence_count: usize,
}

/// Cross-session linking by file overlap
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionLinkResult {
    pub links: Vec<SessionLink>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionLink {
    pub session_a: String,
    pub session_b: String,
    pub shared_files: Vec<String>,
    pub file_overlap_ratio: f64,
    pub time_gap_minutes: Option<i64>,
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
    pub files_mutated: Vec<String>,
    /// Pure quantitative signals — no rule-based classification
    pub stats: SessionStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SummaryPrompt {
    pub text: String,
    pub timestamp: i64,
}

/// Pure quantitative session statistics (no rule-based classification)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStats {
    pub prompt_count: usize,
    pub unique_tool_count: usize,
    pub total_tool_uses: usize,
    pub files_edited_count: usize,
    pub avg_prompt_length: f64,
    pub max_consecutive_same_tool: usize,
    /// Tool transition counts within this session: [(from, to, count)]
    pub tool_transitions: Vec<(String, String, usize)>,
    /// Per-file edit counts within this session
    pub file_edit_counts: Vec<(String, usize)>,
}
