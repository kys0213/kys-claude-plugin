use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::queue::task_queues::TaskQueues;

// ─── Status file models ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonStatus {
    pub updated_at: String,
    pub uptime_secs: u64,
    pub active_items: Vec<StatusItem>,
    pub counters: StatusCounters,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusItem {
    pub work_id: String,
    pub queue_type: String,
    pub repo_name: String,
    pub number: i64,
    pub title: String,
    pub phase: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StatusCounters {
    pub wip: i64,
    pub done: i64,
    pub skip: i64,
    pub failed: i64,
}

// ─── Build / Write / Read ───

/// TaskQueues의 인메모리 상태를 DaemonStatus로 변환
pub fn build_status(
    queues: &TaskQueues,
    counters: &StatusCounters,
    start_time: std::time::Instant,
) -> DaemonStatus {
    let mut items = Vec::new();

    for (phase, issue) in queues.issues.iter_all() {
        items.push(StatusItem {
            work_id: issue.work_id.clone(),
            queue_type: "issue".to_string(),
            repo_name: issue.repo_name.clone(),
            number: issue.github_number,
            title: issue.title.clone(),
            phase: phase.to_string(),
        });
    }

    for (phase, pr) in queues.prs.iter_all() {
        items.push(StatusItem {
            work_id: pr.work_id.clone(),
            queue_type: "pr".to_string(),
            repo_name: pr.repo_name.clone(),
            number: pr.github_number,
            title: pr.title.clone(),
            phase: phase.to_string(),
        });
    }

    for (phase, merge) in queues.merges.iter_all() {
        items.push(StatusItem {
            work_id: merge.work_id.clone(),
            queue_type: "merge".to_string(),
            repo_name: merge.repo_name.clone(),
            number: merge.pr_number,
            title: merge.title.clone(),
            phase: phase.to_string(),
        });
    }

    let wip = items.len() as i64;

    DaemonStatus {
        updated_at: chrono::Local::now().to_rfc3339(),
        uptime_secs: start_time.elapsed().as_secs(),
        active_items: items,
        counters: StatusCounters {
            wip,
            done: counters.done,
            skip: counters.skip,
            failed: counters.failed,
        },
    }
}

/// Atomic write: tmp → rename
pub fn write_status(path: &Path, status: &DaemonStatus) {
    let json = match serde_json::to_string_pretty(status) {
        Ok(j) => j,
        Err(e) => {
            tracing::warn!("failed to serialize status: {e}");
            return;
        }
    };

    let tmp = path.with_extension("tmp");
    if let Err(e) = std::fs::write(&tmp, &json) {
        tracing::warn!("failed to write status tmp file: {e}");
        return;
    }
    if let Err(e) = std::fs::rename(&tmp, path) {
        tracing::warn!("failed to rename status file: {e}");
    }
}

/// status file 읽기 (없거나 파싱 실패 시 None)
pub fn read_status(path: &Path) -> Option<DaemonStatus> {
    let data = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

/// 데몬 종료 시 status file 삭제
pub fn remove_status(path: &Path) {
    let _ = std::fs::remove_file(path);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::queue::task_queues::{issue_phase, pr_phase, IssueItem, PrItem, TaskQueues};

    fn make_issue(repo: &str, number: i64) -> IssueItem {
        IssueItem {
            work_id: format!("issue:{repo}:{number}"),
            repo_id: "r1".to_string(),
            repo_name: repo.to_string(),
            repo_url: format!("https://github.com/{repo}"),
            github_number: number,
            title: format!("Issue #{number}"),
            body: None,
            labels: vec![],
            author: "user".to_string(),
            analysis_report: None,
        }
    }

    fn make_pr(repo: &str, number: i64) -> PrItem {
        PrItem {
            work_id: format!("pr:{repo}:{number}"),
            repo_id: "r1".to_string(),
            repo_name: repo.to_string(),
            repo_url: format!("https://github.com/{repo}"),
            github_number: number,
            title: format!("PR #{number}"),
            head_branch: "feat".to_string(),
            base_branch: "main".to_string(),
            review_comment: None,
            source_issue_number: None,
            review_iteration: 0,
        }
    }

    #[test]
    fn build_status_collects_all_queue_items() {
        let mut queues = TaskQueues::new();
        queues
            .issues
            .push(issue_phase::PENDING, make_issue("org/repo", 1));
        queues
            .issues
            .push(issue_phase::READY, make_issue("org/repo", 2));
        queues.prs.push(pr_phase::PENDING, make_pr("org/repo", 10));

        let counters = StatusCounters {
            wip: 0,
            done: 5,
            skip: 1,
            failed: 0,
        };
        let status = build_status(&queues, &counters, std::time::Instant::now());

        assert_eq!(status.active_items.len(), 3);
        assert_eq!(status.counters.wip, 3); // overridden by actual queue count
        assert_eq!(status.counters.done, 5);
        assert_eq!(status.counters.skip, 1);
    }

    #[test]
    fn build_status_empty_queues() {
        let queues = TaskQueues::new();
        let counters = StatusCounters::default();
        let status = build_status(&queues, &counters, std::time::Instant::now());

        assert!(status.active_items.is_empty());
        assert_eq!(status.counters.wip, 0);
    }

    #[test]
    fn write_and_read_status_roundtrip() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("daemon.status.json");

        let status = DaemonStatus {
            updated_at: "2026-02-23T14:00:00+09:00".to_string(),
            uptime_secs: 3600,
            active_items: vec![StatusItem {
                work_id: "issue:org/repo:42".to_string(),
                queue_type: "issue".to_string(),
                repo_name: "org/repo".to_string(),
                number: 42,
                title: "Fix bug".to_string(),
                phase: "Pending".to_string(),
            }],
            counters: StatusCounters {
                wip: 1,
                done: 10,
                skip: 2,
                failed: 0,
            },
        };

        write_status(&path, &status);
        let loaded = read_status(&path).expect("should read back");

        assert_eq!(loaded.active_items.len(), 1);
        assert_eq!(loaded.active_items[0].work_id, "issue:org/repo:42");
        assert_eq!(loaded.counters.wip, 1);
        assert_eq!(loaded.counters.done, 10);
    }

    #[test]
    fn read_status_missing_file_returns_none() {
        let result = read_status(Path::new("/tmp/nonexistent-status.json"));
        assert!(result.is_none());
    }

    #[test]
    fn read_status_invalid_json_returns_none() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("bad.json");
        std::fs::write(&path, "not json").unwrap();

        assert!(read_status(&path).is_none());
    }

    #[test]
    fn remove_status_cleans_up() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("daemon.status.json");
        std::fs::write(&path, "{}").unwrap();
        assert!(path.exists());

        remove_status(&path);
        assert!(!path.exists());
    }

    #[test]
    fn status_file_reflects_queue_changes_across_ticks() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("daemon.status.json");
        let start = std::time::Instant::now();

        let mut queues = TaskQueues::new();
        let mut counters = StatusCounters::default();

        // ── Tick 1: issue 1개 적재 ──
        queues
            .issues
            .push(issue_phase::PENDING, make_issue("org/repo", 1));

        let s1 = build_status(&queues, &counters, start);
        write_status(&path, &s1);

        let loaded1 = read_status(&path).unwrap();
        assert_eq!(loaded1.active_items.len(), 1);
        assert_eq!(loaded1.active_items[0].number, 1);
        assert_eq!(loaded1.counters.wip, 1);
        assert_eq!(loaded1.counters.done, 0);

        // ── Tick 2: PR 추가, issue phase 변경 ──
        queues.prs.push(pr_phase::PENDING, make_pr("org/repo", 10));
        let issue = queues.issues.pop(issue_phase::PENDING).unwrap();
        queues.issues.push(issue_phase::READY, issue);

        let s2 = build_status(&queues, &counters, start);
        write_status(&path, &s2);

        let loaded2 = read_status(&path).unwrap();
        assert_eq!(loaded2.active_items.len(), 2);
        assert_eq!(loaded2.counters.wip, 2);

        // issue의 phase가 Ready로 변경되었는지 확인
        let issue_item = loaded2
            .active_items
            .iter()
            .find(|i| i.queue_type == "issue")
            .unwrap();
        assert_eq!(issue_item.phase, "Ready");

        // ── Tick 3: issue 처리 완료 (pop) → done 카운터 증가 ──
        queues.issues.pop(issue_phase::READY);
        counters.done += 1;

        let s3 = build_status(&queues, &counters, start);
        write_status(&path, &s3);

        let loaded3 = read_status(&path).unwrap();
        assert_eq!(loaded3.active_items.len(), 1, "issue popped, only PR left");
        assert_eq!(loaded3.active_items[0].queue_type, "pr");
        assert_eq!(loaded3.counters.wip, 1);
        assert_eq!(loaded3.counters.done, 1);

        // ── Tick 4: PR도 처리 완료 → 큐 비어짐 ──
        queues.prs.pop(pr_phase::PENDING);
        counters.done += 1;

        let s4 = build_status(&queues, &counters, start);
        write_status(&path, &s4);

        let loaded4 = read_status(&path).unwrap();
        assert!(loaded4.active_items.is_empty());
        assert_eq!(loaded4.counters.wip, 0);
        assert_eq!(loaded4.counters.done, 2);

        // uptime은 tick 1보다 크거나 같아야 함
        assert!(loaded4.uptime_secs >= loaded1.uptime_secs);
    }
}
