use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

// --- Indexing types ---

pub struct SessionData {
    pub id: String,
    pub file_path: String,
    pub file_size: u64,
    pub file_mtime: i64,
    pub first_ts: Option<i64>,
    pub last_ts: Option<i64>,
    pub prompt_count: usize,
    pub tool_use_count: usize,
    pub first_prompt_snippet: Option<String>,
    pub prompts: Vec<PromptData>,
    pub tool_uses: Vec<ToolUseData>,
    pub file_edits: Vec<FileEditData>,
}

pub struct PromptData {
    pub text: String,
    pub timestamp: i64,
    pub char_count: usize,
}

pub struct ToolUseData {
    pub seq_order: usize,
    pub tool_name: String,
    pub classified_name: String,
    pub timestamp: Option<i64>,
    pub input_json: Option<String>,
}

pub struct FileEditData {
    pub tool_use_seq: usize,
    pub file_path: String,
    pub timestamp: Option<i64>,
}

#[derive(Debug, PartialEq)]
pub enum SessionStatus {
    New,
    Changed,
    Unchanged,
}

// --- Query types ---

#[derive(Debug, Clone)]
pub struct PerspectiveInfo {
    pub name: String,
    pub description: String,
    pub params: Vec<ParamDef>,
    pub sql: String,
}

#[derive(Debug, Clone)]
pub struct ParamDef {
    pub name: String,
    pub param_type: ParamType,
    pub required: bool,
    pub default: Option<String>,
    pub description: String,
}

#[derive(Debug, Clone)]
pub enum ParamType {
    Integer,
    Float,
    Text,
    Date,
}

pub type QueryParams = HashMap<String, String>;

// --- Repository traits ---

#[allow(dead_code)]
pub trait IndexRepository {
    fn initialize(&self) -> Result<()>;
    fn check_session(&self, file_path: &Path, size: u64, mtime: i64) -> Result<SessionStatus>;
    fn upsert_session(&self, session: &SessionData) -> Result<()>;
    fn remove_stale_sessions(&self, existing_paths: &[&Path]) -> Result<u64>;
    fn rebuild_derived_tables(&self) -> Result<()>;
    fn update_meta(&self, key: &str, value: &str) -> Result<()>;
    fn schema_version(&self) -> Result<Option<u32>>;
}

pub trait QueryRepository {
    fn list_perspectives(&self) -> Result<Vec<PerspectiveInfo>>;
    fn query(
        &self,
        perspective: &str,
        params: &QueryParams,
        session_filter: Option<&str>,
    ) -> Result<serde_json::Value>;
    fn execute_sql(&self, sql: &str) -> Result<serde_json::Value>;
}
