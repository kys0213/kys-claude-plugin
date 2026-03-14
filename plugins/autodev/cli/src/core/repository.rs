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

pub trait TokenUsageRepository {
    fn usage_insert(&self, usage: &NewTokenUsage) -> Result<()>;
    fn usage_summary(&self, repo: Option<&str>, since: Option<&str>) -> Result<UsageSummary>;
    fn usage_by_issue(&self, repo: &str, issue: i64) -> Result<Vec<UsageByIssue>>;
}

pub trait SpecRepository {
    fn spec_add(&self, spec: &NewSpec) -> Result<String>;
    fn spec_list(&self, repo: Option<&str>) -> Result<Vec<Spec>>;
    fn spec_show(&self, id: &str) -> Result<Option<Spec>>;
    fn spec_update(
        &self,
        id: &str,
        body: &str,
        test_commands: Option<&str>,
        acceptance_criteria: Option<&str>,
    ) -> Result<()>;
    fn spec_set_status(&self, id: &str, status: SpecStatus) -> Result<()>;
    fn spec_issues(&self, spec_id: &str) -> Result<Vec<SpecIssue>>;
    fn spec_link_issue(&self, spec_id: &str, issue_number: i64) -> Result<()>;
    fn spec_unlink_issue(&self, spec_id: &str, issue_number: i64) -> Result<()>;
}

pub trait HitlRepository {
    fn hitl_create(&self, event: &NewHitlEvent) -> Result<String>;
    fn hitl_list(&self, repo: Option<&str>) -> Result<Vec<HitlEvent>>;
    fn hitl_show(&self, id: &str) -> Result<Option<HitlEvent>>;
    fn hitl_respond(&self, response: &NewHitlResponse) -> Result<()>;
    fn hitl_set_status(&self, id: &str, status: HitlStatus) -> Result<()>;
    fn hitl_pending_count(&self, repo: Option<&str>) -> Result<i64>;
    fn hitl_responses(&self, event_id: &str) -> Result<Vec<HitlResponse>>;
}
