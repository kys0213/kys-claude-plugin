use super::state_queue::HasWorkId;

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
    pub analysis_report: Option<String>,
    /// GHE hostname (e.g. "git.example.com"). None이면 github.com.
    pub gh_host: Option<String>,
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
    /// v2: 이 PR이 어떤 이슈로부터 생성되었는지 (issue pipeline에서 설정)
    pub source_issue_number: Option<i64>,
    /// 리뷰→수정 반복 횟수 (improve_one에서 +1, re_review_one에서 max_iterations 체크)
    pub review_iteration: u32,
    /// GHE hostname (e.g. "git.example.com"). None이면 github.com.
    pub gh_host: Option<String>,
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
    /// GHE hostname (e.g. "git.example.com"). None이면 github.com.
    pub gh_host: Option<String>,
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
    pub const EXTRACTING: &str = "Extracting";
}

// ─── Merge Phase 상수 ───

pub mod merge_phase {
    pub const PENDING: &str = "Pending";
    pub const MERGING: &str = "Merging";
    pub const CONFLICT: &str = "Conflict";
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::labels;

    #[test]
    fn make_work_id_format() {
        assert_eq!(make_work_id("issue", "org/repo", 42), "issue:org/repo:42");
        assert_eq!(make_work_id("pr", "org/repo", 15), "pr:org/repo:15");
        assert_eq!(make_work_id("merge", "org/repo", 15), "merge:org/repo:15");
    }

    #[test]
    fn label_constants() {
        assert_eq!(labels::WIP, "autodev:wip");
        assert_eq!(labels::DONE, "autodev:done");
        assert_eq!(labels::SKIP, "autodev:skip");

        // v2 라벨
        assert_eq!(labels::ANALYZED, "autodev:analyzed");
        assert_eq!(labels::APPROVED_ANALYSIS, "autodev:approved-analysis");
        assert_eq!(labels::IMPLEMENTING, "autodev:implementing");
    }

    #[test]
    fn iteration_label_format() {
        assert_eq!(labels::iteration_label(1), "autodev:iteration/1");
        assert_eq!(labels::iteration_label(2), "autodev:iteration/2");
        assert_eq!(labels::iteration_label(0), "autodev:iteration/0");
    }

    #[test]
    fn parse_iteration_from_labels() {
        assert_eq!(
            labels::parse_iteration(&["autodev:wip", "autodev:iteration/2"]),
            2
        );
        assert_eq!(labels::parse_iteration(&["autodev:wip"]), 0);
        assert_eq!(labels::parse_iteration(&[]), 0);
        assert_eq!(
            labels::parse_iteration(&["autodev:iteration/3", "autodev:iteration/1"]),
            3, // 첫 번째 매칭 반환
        );
    }

    #[test]
    fn phase_constants_match_design() {
        // Issue: Pending → Analyzing → Ready → Implementing
        assert_eq!(issue_phase::PENDING, "Pending");
        assert_eq!(issue_phase::ANALYZING, "Analyzing");
        assert_eq!(issue_phase::READY, "Ready");
        assert_eq!(issue_phase::IMPLEMENTING, "Implementing");

        // PR: Pending → Reviewing → ReviewDone → Improving → Improved
        assert_eq!(pr_phase::PENDING, "Pending");
        assert_eq!(pr_phase::REVIEWING, "Reviewing");
        assert_eq!(pr_phase::REVIEW_DONE, "ReviewDone");
        assert_eq!(pr_phase::IMPROVING, "Improving");
        assert_eq!(pr_phase::IMPROVED, "Improved");

        // Merge: Pending → Merging → Conflict
        assert_eq!(merge_phase::PENDING, "Pending");
        assert_eq!(merge_phase::MERGING, "Merging");
        assert_eq!(merge_phase::CONFLICT, "Conflict");
    }
}
