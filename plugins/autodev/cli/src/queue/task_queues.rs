use super::state_queue::{HasWorkId, StateQueue};

// ─── In-Memory Work Items ───

/// Issue 작업 아이템 (인메모리)
#[derive(Debug, Clone)]
pub struct IssueItem {
    pub work_id: String,
    pub repo_id: String,
    pub repo_name: String,
    pub repo_url: String,
    pub github_number: i64,
    pub title: String,
    pub body: Option<String>,
    pub labels: Vec<String>,
    pub author: String,
    /// Phase 1(분석) 완료 후 Phase 2(구현)에서 사용
    pub analysis_report: Option<String>,
}

impl HasWorkId for IssueItem {
    fn work_id(&self) -> &str {
        &self.work_id
    }
}

/// PR 리뷰 작업 아이템 (인메모리)
#[derive(Debug, Clone)]
pub struct PrItem {
    pub work_id: String,
    pub repo_id: String,
    pub repo_name: String,
    pub repo_url: String,
    pub github_number: i64,
    pub title: String,
    pub head_branch: String,
    pub base_branch: String,
    /// 리뷰 결과 (피드백 루프에서 사용)
    pub review_comment: Option<String>,
}

impl HasWorkId for PrItem {
    fn work_id(&self) -> &str {
        &self.work_id
    }
}

/// Merge 작업 아이템 (인메모리)
#[derive(Debug, Clone)]
pub struct MergeItem {
    pub work_id: String,
    pub repo_id: String,
    pub repo_name: String,
    pub repo_url: String,
    pub pr_number: i64,
    pub title: String,
    pub head_branch: String,
    pub base_branch: String,
}

impl HasWorkId for MergeItem {
    fn work_id(&self) -> &str {
        &self.work_id
    }
}

// ─── Work ID 생성 헬퍼 ───

/// work_id 형식: "{type}:{repo_name}:{number}"
pub fn make_work_id(queue_type: &str, repo_name: &str, number: i64) -> String {
    format!("{queue_type}:{repo_name}:{number}")
}

// ─── Issue Phase 상수 ───

pub mod issue_phase {
    pub const PENDING: &str = "Pending";
    pub const ANALYZING: &str = "Analyzing";
    pub const READY: &str = "Ready";
    pub const IMPLEMENTING: &str = "Implementing";
}

// ─── PR Phase 상수 ───

pub mod pr_phase {
    pub const PENDING: &str = "Pending";
    pub const REVIEWING: &str = "Reviewing";
    pub const REVIEW_DONE: &str = "ReviewDone";
    pub const IMPROVING: &str = "Improving";
    pub const IMPROVED: &str = "Improved";
}

// ─── Merge Phase 상수 ───

pub mod merge_phase {
    pub const PENDING: &str = "Pending";
    pub const MERGING: &str = "Merging";
    pub const CONFLICT: &str = "Conflict";
}

// ─── GitHub Label 상수 ───

pub mod labels {
    pub const WIP: &str = "autodev:wip";
    pub const DONE: &str = "autodev:done";
    pub const SKIP: &str = "autodev:skip";
}

// ─── TaskQueues: 전체 작업 큐 ───

/// 3종 큐(issue, pr, merge)를 관리하며 work_id 기반 O(1) dedup을 제공한다.
pub struct TaskQueues {
    pub issues: StateQueue<IssueItem>,
    pub prs: StateQueue<PrItem>,
    pub merges: StateQueue<MergeItem>,
}

impl Default for TaskQueues {
    fn default() -> Self {
        Self {
            issues: StateQueue::new(),
            prs: StateQueue::new(),
            merges: StateQueue::new(),
        }
    }
}

impl TaskQueues {
    pub fn new() -> Self {
        Self::default()
    }

    /// 어떤 큐든 해당 work_id가 존재하는지 확인
    pub fn contains(&self, work_id: &str) -> bool {
        self.issues.contains(work_id)
            || self.prs.contains(work_id)
            || self.merges.contains(work_id)
    }

    /// 전체 아이템 수
    pub fn total(&self) -> usize {
        self.issues.total() + self.prs.total() + self.merges.total()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn issue(repo: &str, number: i64) -> IssueItem {
        IssueItem {
            work_id: make_work_id("issue", repo, number),
            repo_id: "repo-id-1".to_string(),
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

    fn pr(repo: &str, number: i64) -> PrItem {
        PrItem {
            work_id: make_work_id("pr", repo, number),
            repo_id: "repo-id-1".to_string(),
            repo_name: repo.to_string(),
            repo_url: format!("https://github.com/{repo}"),
            github_number: number,
            title: format!("PR #{number}"),
            head_branch: "feature".to_string(),
            base_branch: "main".to_string(),
            review_comment: None,
        }
    }

    fn merge(repo: &str, number: i64) -> MergeItem {
        MergeItem {
            work_id: make_work_id("merge", repo, number),
            repo_id: "repo-id-1".to_string(),
            repo_name: repo.to_string(),
            repo_url: format!("https://github.com/{repo}"),
            pr_number: number,
            title: format!("Merge PR #{number}"),
            head_branch: "feature".to_string(),
            base_branch: "main".to_string(),
        }
    }

    #[test]
    fn make_work_id_format() {
        assert_eq!(
            make_work_id("issue", "org/repo", 42),
            "issue:org/repo:42"
        );
        assert_eq!(make_work_id("pr", "org/repo", 15), "pr:org/repo:15");
        assert_eq!(
            make_work_id("merge", "org/repo", 15),
            "merge:org/repo:15"
        );
    }

    #[test]
    fn task_queues_contains_across_queues() {
        let mut tq = TaskQueues::new();

        let i = issue("org/repo", 42);
        let p = pr("org/repo", 10);
        let m = merge("org/repo", 5);

        tq.issues.push(issue_phase::PENDING, i);
        tq.prs.push(pr_phase::PENDING, p);
        tq.merges.push(merge_phase::PENDING, m);

        assert!(tq.contains("issue:org/repo:42"));
        assert!(tq.contains("pr:org/repo:10"));
        assert!(tq.contains("merge:org/repo:5"));
        assert!(!tq.contains("issue:org/repo:99"));
    }

    #[test]
    fn task_queues_total() {
        let mut tq = TaskQueues::new();
        assert_eq!(tq.total(), 0);

        tq.issues.push(issue_phase::PENDING, issue("org/repo", 1));
        tq.issues.push(issue_phase::PENDING, issue("org/repo", 2));
        tq.prs.push(pr_phase::PENDING, pr("org/repo", 3));

        assert_eq!(tq.total(), 3);
    }

    #[test]
    fn issue_lifecycle_pending_to_done() {
        let mut tq = TaskQueues::new();

        // scan → Pending
        let i = issue("org/repo", 42);
        assert!(tq.issues.push(issue_phase::PENDING, i));
        assert_eq!(tq.issues.len(issue_phase::PENDING), 1);

        // consume → pop Pending, start analyzing
        let mut item = tq.issues.pop(issue_phase::PENDING).unwrap();
        assert_eq!(item.github_number, 42);

        // analyze 완료 → Ready에 push (analysis_report 첨부)
        item.analysis_report = Some("report...".to_string());
        tq.issues.push(issue_phase::READY, item);
        assert_eq!(tq.issues.len(issue_phase::READY), 1);

        // implement → pop Ready
        let item = tq.issues.pop(issue_phase::READY).unwrap();
        assert_eq!(item.analysis_report.as_deref(), Some("report..."));

        // done → remove (pop 시 이미 제거됨)
        assert_eq!(tq.total(), 0);
    }

    #[test]
    fn pr_review_feedback_loop() {
        let mut tq = TaskQueues::new();

        // scan → Pending
        tq.prs.push(pr_phase::PENDING, pr("org/repo", 10));

        // review → pop Pending
        let mut item = tq.prs.pop(pr_phase::PENDING).unwrap();

        // request_changes → ReviewDone에 push
        item.review_comment = Some("fix null check".to_string());
        tq.prs.push(pr_phase::REVIEW_DONE, item);
        assert_eq!(tq.prs.len(pr_phase::REVIEW_DONE), 1);

        // improve → pop ReviewDone
        let item = tq.prs.pop(pr_phase::REVIEW_DONE).unwrap();
        assert_eq!(item.review_comment.as_deref(), Some("fix null check"));

        // improved → Improved에 push
        tq.prs.push(pr_phase::IMPROVED, item);

        // re-review → pop Improved, re-review
        let item = tq.prs.pop(pr_phase::IMPROVED).unwrap();

        // approve → done (pop 시 이미 제거됨)
        assert_eq!(tq.total(), 0);
        drop(item);
    }

    #[test]
    fn dedup_across_lifecycle() {
        let mut tq = TaskQueues::new();

        let i = issue("org/repo", 42);
        let wid = i.work_id.clone();
        tq.issues.push(issue_phase::PENDING, i);

        // 같은 work_id로 중복 push 불가
        let dup = issue("org/repo", 42);
        assert!(!tq.issues.push(issue_phase::PENDING, dup));

        // pop 후에는 다시 push 가능
        tq.issues.pop(issue_phase::PENDING);
        assert!(!tq.contains(&wid));

        let reinsert = issue("org/repo", 42);
        assert!(tq.issues.push(issue_phase::PENDING, reinsert));
    }

    #[test]
    fn label_constants() {
        assert_eq!(labels::WIP, "autodev:wip");
        assert_eq!(labels::DONE, "autodev:done");
        assert_eq!(labels::SKIP, "autodev:skip");
    }
}
