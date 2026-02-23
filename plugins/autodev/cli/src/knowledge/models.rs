use serde::{Deserialize, Serialize};

/// Knowledge suggestion 타입
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionType {
    Rule,
    ClaudeMd,
    Hook,
    Skill,
    Subagent,
}

/// 단일 지식 제안
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Suggestion {
    #[serde(rename = "type")]
    pub suggestion_type: SuggestionType,
    pub target_file: String,
    pub content: String,
    pub reason: String,
}

/// per-task 또는 daily에서 생성되는 제안 목록
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeSuggestion {
    pub suggestions: Vec<Suggestion>,
}

/// 일간 리포트 요약 통계
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailySummary {
    pub issues_done: u32,
    pub prs_done: u32,
    pub failed: u32,
    pub skipped: u32,
    pub avg_duration_ms: u64,
}

/// 교차 작업 패턴 유형
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatternType {
    RepeatedFailure,
    ReviewCycle,
    TestLoop,
    Hotfile,
}

/// 교차 작업 패턴 분석 결과
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pattern {
    #[serde(rename = "type")]
    pub pattern_type: PatternType,
    pub description: String,
    pub occurrences: u32,
    pub affected_tasks: Vec<String>,
}

/// 일간 리포트 전체 구조
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyReport {
    pub date: String,
    pub summary: DailySummary,
    pub patterns: Vec<Pattern>,
    pub suggestions: Vec<Suggestion>,
    /// suggest-workflow 교차 분석 데이터 (M-03)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cross_analysis: Option<CrossAnalysis>,
}

// ─── suggest-workflow 연동 모델 (M-03) ───

/// suggest-workflow tool-frequency perspective 결과 항목
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolFrequencyEntry {
    pub tool: String,
    pub frequency: u64,
    pub sessions: u64,
}

/// suggest-workflow filtered-sessions perspective 결과 항목
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEntry {
    pub id: String,
    pub prompt_count: u64,
    pub tool_use_count: u64,
    pub first_prompt: String,
    pub started_at: Option<String>,
    pub duration_minutes: Option<f64>,
}

/// suggest-workflow repetition perspective 결과 항목
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepetitionEntry {
    pub session_id: String,
    pub tool: String,
    pub cnt: u64,
    pub deviation_score: f64,
}

/// daemon.log + suggest-workflow 교차 분석 데이터
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossAnalysis {
    pub tool_frequencies: Vec<ToolFrequencyEntry>,
    pub anomalies: Vec<RepetitionEntry>,
    pub sessions: Vec<SessionEntry>,
}
