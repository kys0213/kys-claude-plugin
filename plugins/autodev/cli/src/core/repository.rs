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
    /// 로그를 삽입하고 생성된 log_id를 반환한다.
    fn log_insert(&self, log: &NewConsumerLog) -> Result<String>;
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
    /// Fetch all spec-issue mappings in a single query, grouped by spec_id.
    fn spec_issues_all(&self) -> Result<std::collections::HashMap<String, Vec<SpecIssue>>>;
    fn spec_issue_counts(&self) -> Result<std::collections::HashMap<String, usize>>;
    fn spec_link_issue(&self, spec_id: &str, issue_number: i64) -> Result<()>;
    fn spec_unlink_issue(&self, spec_id: &str, issue_number: i64) -> Result<()>;
    fn spec_list_by_status(&self, status: SpecStatus) -> Result<Vec<Spec>>;
    fn spec_set_priority(&self, id: &str, priority: i32) -> Result<()>;
}

pub trait HitlRepository {
    fn hitl_create(&self, event: &NewHitlEvent) -> Result<String>;
    fn hitl_list(&self, repo: Option<&str>) -> Result<Vec<HitlEvent>>;
    fn hitl_show(&self, id: &str) -> Result<Option<HitlEvent>>;
    fn hitl_respond(&self, response: &NewHitlResponse) -> Result<()>;
    fn hitl_set_status(&self, id: &str, status: HitlStatus) -> Result<()>;
    fn hitl_pending_count(&self, repo: Option<&str>) -> Result<i64>;
    fn hitl_total_count(&self, repo: Option<&str>) -> Result<i64>;
    fn hitl_responses(&self, event_id: &str) -> Result<Vec<HitlResponse>>;
    fn hitl_expired_list(&self, timeout_hours: i64) -> Result<Vec<HitlEvent>>;
    /// 특정 spec에 연결된 HITL 이벤트 수를 (total, pending) 튜플로 반환한다.
    fn hitl_count_by_spec(&self, spec_id: &str) -> Result<(i64, i64)>;
}

pub trait QueueRepository {
    /// 큐 아이템의 현재 phase를 조회한다
    fn queue_get_phase(&self, work_id: &str) -> Result<Option<QueuePhase>>;
    /// 큐 아이템을 다음 phase로 전이한다 (pending → ready → running → done)
    fn queue_advance(&self, work_id: &str) -> Result<()>;
    /// 큐 아이템을 skip 처리한다
    fn queue_skip(&self, work_id: &str, reason: Option<&str>) -> Result<()>;
    /// 큐 아이템 목록을 조회한다 (repo별 필터 가능)
    fn queue_list_items(&self, repo: Option<&str>) -> Result<Vec<QueueItemRow>>;
    /// 큐 아이템을 upsert한다 (INSERT OR REPLACE, created_at 보존)
    fn queue_upsert(&self, item: &QueueItemRow) -> Result<()>;
    /// 큐 아이템을 done 상태로 전이한다
    fn queue_remove(&self, work_id: &str) -> Result<()>;
    /// 특정 repo의 활성 큐 아이템을 로드한다 (done/skipped 제외)
    fn queue_load_active(&self, repo_id: &str) -> Result<Vec<QueueItemRow>>;
    /// CAS 방식으로 phase를 전이한다 (from → to). 성공 시 true.
    fn queue_transit(&self, work_id: &str, from: QueuePhase, to: QueuePhase) -> Result<bool>;
    /// 단일 큐 아이템을 work_id로 조회한다
    fn queue_get_item(&self, work_id: &str) -> Result<Option<QueueItemRow>>;
    /// failure_count를 1 증가시키고 새 값을 반환한다
    fn queue_increment_failure(&self, work_id: &str) -> Result<i32>;
    /// 현재 failure_count를 조회한다
    fn queue_get_failure_count(&self, work_id: &str) -> Result<i32>;
}

pub trait ClawDecisionRepository {
    fn decision_add(&self, decision: &NewClawDecision) -> Result<String>;
    fn decision_list(&self, repo: Option<&str>, limit: usize) -> Result<Vec<ClawDecision>>;
    fn decision_show(&self, id: &str) -> Result<Option<ClawDecision>>;
    fn decision_list_by_spec(&self, spec_id: &str, limit: usize) -> Result<Vec<ClawDecision>>;
    fn decision_count(&self, repo: Option<&str>) -> Result<i64>;
}

pub trait FeedbackPatternRepository {
    fn feedback_upsert(&self, pattern: &NewFeedbackPattern) -> Result<String>;
    fn feedback_list(&self, repo_id: &str) -> Result<Vec<FeedbackPattern>>;
    fn feedback_list_actionable(
        &self,
        repo_id: &str,
        min_count: i32,
    ) -> Result<Vec<FeedbackPattern>>;
    fn feedback_set_status(&self, id: &str, status: FeedbackPatternStatus) -> Result<()>;
}

pub trait CronRepository {
    fn cron_add(&self, job: &NewCronJob) -> Result<String>;
    fn cron_list(&self, repo: Option<&str>) -> Result<Vec<CronJob>>;
    fn cron_show(&self, name: &str, repo: Option<&str>) -> Result<Option<CronJob>>;
    fn cron_update_interval(
        &self,
        name: &str,
        repo: Option<&str>,
        interval_secs: u64,
    ) -> Result<()>;
    fn cron_update_schedule(&self, name: &str, repo: Option<&str>, cron_expr: &str) -> Result<()>;
    fn cron_set_status(&self, name: &str, repo: Option<&str>, status: CronStatus) -> Result<()>;
    fn cron_remove(&self, name: &str, repo: Option<&str>) -> Result<()>;
    fn cron_update_last_run(&self, id: &str) -> Result<()>;
    /// Reset last_run_at to NULL so the job is picked up as due on the next tick.
    fn cron_reset_last_run(&self, name: &str, repo: Option<&str>) -> Result<()>;
    fn cron_find_due(&self) -> Result<Vec<CronJob>>;
}
