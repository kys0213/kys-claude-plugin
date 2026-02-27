use anyhow::Result;

use super::models::*;

// ─── Repository traits ───

pub trait RepoRepository {
    fn repo_add(&self, url: &str, name: &str) -> Result<String>;
    fn repo_remove(&self, name: &str) -> Result<()>;
    fn repo_list(&self) -> Result<Vec<RepoInfo>>;
    fn repo_find_enabled(&self) -> Result<Vec<EnabledRepo>>;
    fn repo_status_summary(&self) -> Result<Vec<RepoStatusRow>>;
}

pub trait ScanCursorRepository {
    fn cursor_get_last_seen(&self, repo_id: &str, target: &str) -> Result<Option<String>>;
    fn cursor_upsert(&self, repo_id: &str, target: &str, last_seen: &str) -> Result<()>;
    fn cursor_should_scan(&self, repo_id: &str, interval_secs: i64) -> Result<bool>;
}

pub trait ConsumerLogRepository {
    fn log_insert(&self, log: &NewConsumerLog) -> Result<()>;
    fn log_recent(&self, repo_name: Option<&str>, limit: usize) -> Result<Vec<LogEntry>>;
    /// 특정 날짜의 knowledge extraction stdout를 모두 반환
    fn log_knowledge_stdout_by_date(&self, date: &str) -> Result<Vec<String>>;
}
