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
}
