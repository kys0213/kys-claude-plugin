pub mod issue;
pub mod merge;
pub mod pr;

use anyhow::Result;

use crate::components::notifier::Notifier;
use crate::components::workspace::Workspace;
use crate::config::Env;
use crate::domain::models::NewConsumerLog;
use crate::domain::repository::ConsumerLogRepository;
use crate::infrastructure::claude::Claude;
use crate::infrastructure::gh::Gh;
use crate::infrastructure::suggest_workflow::SuggestWorkflow;
use crate::queue::task_queues::{IssueItem, MergeItem, PrItem, TaskQueues};
use crate::queue::Database;

// ─── System Prompt (공통 행동 지침) ───

/// 모든 pipeline에서 `--append-system-prompt`로 주입되는 공통 지침.
/// 에이전트가 GitHub 코멘트/피드백/첨부를 직접 확인하도록 유도한다.
pub const AGENT_SYSTEM_PROMPT: &str = "\
You are an automated development agent (autodev).

IMPORTANT: Before making any decisions, you MUST review all comments, feedback, \
and attachments on the relevant GitHub issue or PR using the `gh` CLI.
- For issues: `gh issue view <number> --comments`
- For PRs: `gh pr view <number> --comments`

Consider all prior discussion, reviewer feedback, and attachments as essential context \
for your analysis or implementation.";

// ─── Event Loop Task Result Types ───

/// Spawned pipeline task의 결과.
/// Main loop에서 큐 상태 전이 + DB 로그 삽입에 사용한다.
pub struct TaskOutput {
    /// 처리된 아이템의 work_id
    pub work_id: String,
    /// 레포 이름 (InFlightTracker 카운터 감소에 사용)
    pub repo_name: String,
    /// 큐 조작 명령 목록 (main loop에서 순서대로 실행)
    pub queue_ops: Vec<QueueOp>,
    /// DB에 기록할 consumer log 목록
    pub logs: Vec<NewConsumerLog>,
}

/// 큐 조작 명령 — main loop에서만 실행된다 (큐는 main task 소유).
#[allow(dead_code)]
pub enum QueueOp {
    /// 현재 working phase에서 아이템 제거 (done/skip/error)
    Remove,
    /// Issue를 특정 phase에 push
    PushIssue {
        phase: &'static str,
        item: IssueItem,
    },
    /// PR을 특정 phase에 push
    PushPr { phase: &'static str, item: PrItem },
    /// Merge를 특정 phase에 push
    PushMerge {
        phase: &'static str,
        item: MergeItem,
    },
}

/// TaskOutput의 큐 조작을 실행하고 로그를 DB에 기록한다.
///
/// Main loop에서 spawned task 완료 시 호출.
#[allow(dead_code)]
pub fn handle_task_output(queues: &mut TaskQueues, db: &Database, output: TaskOutput) {
    let work_id = output.work_id;

    for op in output.queue_ops {
        match op {
            QueueOp::Remove => {
                queues.issues.remove(&work_id);
                queues.prs.remove(&work_id);
                queues.merges.remove(&work_id);
            }
            QueueOp::PushIssue { phase, item } => {
                queues.issues.push(phase, item);
            }
            QueueOp::PushPr { phase, item } => {
                queues.prs.push(phase, item);
            }
            QueueOp::PushMerge { phase, item } => {
                queues.merges.push(phase, item);
            }
        }
    }

    for log in &output.logs {
        let _ = db.log_insert(log);
    }
}

// ─── Legacy batch processing ───

/// 이벤트 루프 도입 전의 동기 처리 방식.
/// 현재는 daemon event loop가 각 phase를 개별 spawned task로 처리한다.
#[allow(dead_code)]
#[allow(clippy::too_many_arguments)]
pub async fn process_all(
    db: &Database,
    env: &dyn Env,
    workspace: &Workspace<'_>,
    notifier: &Notifier<'_>,
    gh: &dyn Gh,
    claude: &dyn Claude,
    sw: &dyn SuggestWorkflow,
    queues: &mut TaskQueues,
) -> Result<()> {
    issue::process_pending(db, env, workspace, notifier, gh, claude, queues).await?;
    issue::process_ready(db, env, workspace, notifier, gh, claude, sw, queues).await?;
    pr::process_pending(db, env, workspace, notifier, gh, claude, sw, queues).await?;
    pr::process_review_done(db, env, workspace, gh, claude, queues).await?;
    pr::process_improved(db, env, workspace, notifier, gh, claude, sw, queues).await?;
    merge::process_pending(db, env, workspace, notifier, gh, claude, queues).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::queue::task_queues::{issue_phase, make_work_id, pr_phase};

    fn make_test_issue(repo: &str, number: i64) -> IssueItem {
        IssueItem {
            work_id: make_work_id("issue", repo, number),
            repo_id: "r1".to_string(),
            repo_name: repo.to_string(),
            repo_url: format!("https://github.com/{repo}"),
            github_number: number,
            title: format!("Issue #{number}"),
            body: None,
            labels: vec![],
            author: "user".to_string(),
            analysis_report: None,
            gh_host: None,
        }
    }

    fn make_test_pr(repo: &str, number: i64) -> PrItem {
        PrItem {
            work_id: make_work_id("pr", repo, number),
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
            gh_host: None,
        }
    }

    #[test]
    fn handle_task_output_remove_clears_item() {
        let mut queues = TaskQueues::new();
        queues
            .issues
            .push(issue_phase::ANALYZING, make_test_issue("org/repo", 1));

        let output = TaskOutput {
            work_id: "issue:org/repo:1".to_string(),
            repo_name: "org/repo".to_string(),
            queue_ops: vec![QueueOp::Remove],
            logs: vec![],
        };

        let db = Database::open(std::path::Path::new(":memory:")).expect("open db");
        db.initialize().expect("init");

        handle_task_output(&mut queues, &db, output);
        assert!(!queues.contains("issue:org/repo:1"));
    }

    #[test]
    fn handle_task_output_remove_then_push_pr() {
        let mut queues = TaskQueues::new();
        queues
            .issues
            .push(issue_phase::IMPLEMENTING, make_test_issue("org/repo", 1));

        let output = TaskOutput {
            work_id: "issue:org/repo:1".to_string(),
            repo_name: "org/repo".to_string(),
            queue_ops: vec![
                QueueOp::Remove,
                QueueOp::PushPr {
                    phase: pr_phase::PENDING,
                    item: make_test_pr("org/repo", 10),
                },
            ],
            logs: vec![],
        };

        let db = Database::open(std::path::Path::new(":memory:")).expect("open db");
        db.initialize().expect("init");

        handle_task_output(&mut queues, &db, output);
        assert!(!queues.contains("issue:org/repo:1"));
        assert!(queues.contains("pr:org/repo:10"));
    }
}
