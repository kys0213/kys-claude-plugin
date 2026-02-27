use chrono::Utc;
use uuid::Uuid;

use crate::components::merger::{MergeOutcome, Merger};
use crate::components::notifier::Notifier;
use crate::components::workspace::Workspace;
use crate::config::Env;
use crate::domain::labels;
use crate::domain::models::*;
use crate::infrastructure::agent::Agent;
use crate::infrastructure::gh::Gh;
use crate::infrastructure::git::Git;
use crate::pipeline::{QueueOp, TaskOutput};
use crate::queue::task_queues::MergeItem;

// ═══════════════════════════════════════════════════
// Spawnable task functions (event loop용)
// ═══════════════════════════════════════════════════

/// PR 머지 — spawned task에서 실행.
///
/// 머지 시도 → 충돌 시 해결 시도 → 결과를 TaskOutput으로 반환.
pub async fn merge_one(
    item: MergeItem,
    env: &dyn Env,
    gh: &dyn Gh,
    git: &dyn Git,
    claude: &dyn Agent,
) -> TaskOutput {
    let workspace = Workspace::new(git, env);
    let notifier = Notifier::new(gh);
    let gh_host = item.gh_host.as_deref();
    let merger = Merger::new(claude);

    let work_id = item.work_id.clone();
    let repo_name = item.repo_name.clone();
    let pr_number = item.pr_number;
    let mut ops = Vec::new();
    let mut logs = Vec::new();

    // Pre-flight: GitHub에서 PR이 아직 머지 가능한 상태인지 확인
    if !notifier
        .is_pr_mergeable(&item.repo_name, pr_number, gh_host)
        .await
    {
        gh.label_remove(&item.repo_name, pr_number, labels::WIP, gh_host)
            .await;
        gh.label_add(&item.repo_name, pr_number, labels::DONE, gh_host)
            .await;
        tracing::info!("PR #{pr_number} is closed or already merged, skipping");
        ops.push(QueueOp::Remove);
        return TaskOutput {
            work_id,
            repo_name,
            queue_ops: ops,
            logs,
        };
    }

    let worker_id = Uuid::new_v4().to_string();
    let task_id = format!("merge-pr-{pr_number}");

    if let Err(e) = workspace
        .ensure_cloned(&item.repo_url, &item.repo_name)
        .await
    {
        gh.label_remove(&repo_name, pr_number, labels::WIP, gh_host)
            .await;
        tracing::error!("clone failed for merge PR #{pr_number}: {e}");
        ops.push(QueueOp::Remove);
        return TaskOutput {
            work_id,
            repo_name,
            queue_ops: ops,
            logs,
        };
    }

    let wt_path = match workspace
        .create_worktree(&item.repo_name, &task_id, None)
        .await
    {
        Ok(p) => p,
        Err(e) => {
            gh.label_remove(&repo_name, pr_number, labels::WIP, gh_host)
                .await;
            tracing::error!("worktree failed for merge PR #{pr_number}: {e}");
            ops.push(QueueOp::Remove);
            return TaskOutput {
                work_id,
                repo_name,
                queue_ops: ops,
                logs,
            };
        }
    };

    let started = Utc::now().to_rfc3339();
    let merge_output = merger.merge_pr(&wt_path, pr_number).await;

    let finished = Utc::now().to_rfc3339();
    let duration = chrono::Utc::now()
        .signed_duration_since(chrono::DateTime::parse_from_rfc3339(&started).unwrap())
        .num_milliseconds();

    logs.push(NewConsumerLog {
        repo_id: item.repo_id.clone(),
        queue_type: "merge".to_string(),
        queue_item_id: item.work_id.clone(),
        worker_id: worker_id.clone(),
        command: format!("claude -p \"/git-utils:merge-pr {pr_number}\""),
        stdout: merge_output.stdout.clone(),
        stderr: merge_output.stderr.clone(),
        exit_code: match &merge_output.outcome {
            MergeOutcome::Success => 0,
            MergeOutcome::Conflict | MergeOutcome::Failed { .. } => 1,
            MergeOutcome::Error(_) => -1,
        },
        started_at: started,
        finished_at: finished,
        duration_ms: duration,
    });

    match merge_output.outcome {
        MergeOutcome::Success => {
            gh.label_remove(&repo_name, pr_number, labels::WIP, gh_host)
                .await;
            gh.label_add(&repo_name, pr_number, labels::DONE, gh_host)
                .await;
            tracing::info!("PR #{pr_number}: Merging → done");
            ops.push(QueueOp::Remove);
            let _ = workspace.remove_worktree(&repo_name, &task_id).await;
        }
        MergeOutcome::Conflict => {
            tracing::info!("PR #{pr_number}: Merging → Conflict, attempting resolve");

            let resolve_output = merger.resolve_conflicts(&wt_path, pr_number).await;

            match resolve_output.outcome {
                MergeOutcome::Success => {
                    gh.label_remove(&repo_name, pr_number, labels::WIP, gh_host)
                        .await;
                    gh.label_add(&repo_name, pr_number, labels::DONE, gh_host)
                        .await;
                    tracing::info!("PR #{pr_number}: Conflict → done (resolved)");
                    ops.push(QueueOp::Remove);
                }
                _ => {
                    gh.label_remove(&repo_name, pr_number, labels::WIP, gh_host)
                        .await;
                    tracing::error!("conflict resolution failed for PR #{pr_number}");
                    ops.push(QueueOp::Remove);
                }
            }
        }
        MergeOutcome::Failed { exit_code } => {
            gh.label_remove(&repo_name, pr_number, labels::WIP, gh_host)
                .await;
            tracing::error!("merge exited with {exit_code} for PR #{pr_number}");
            ops.push(QueueOp::Remove);
        }
        MergeOutcome::Error(e) => {
            gh.label_remove(&repo_name, pr_number, labels::WIP, gh_host)
                .await;
            tracing::error!("merge error for PR #{pr_number}: {e}");
            ops.push(QueueOp::Remove);
        }
    }

    let _ = workspace.remove_worktree(&repo_name, &task_id).await;
    TaskOutput {
        work_id,
        repo_name,
        queue_ops: ops,
        logs,
    }
}
