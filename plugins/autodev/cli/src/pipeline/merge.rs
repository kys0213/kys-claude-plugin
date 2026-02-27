use anyhow::Result;
use chrono::Utc;
use uuid::Uuid;

use crate::components::merger::{MergeOutcome, Merger};
use crate::components::notifier::Notifier;
use crate::components::workspace::Workspace;
use crate::config::Env;
use crate::domain::labels;
use crate::domain::models::*;
use crate::domain::repository::*;
use crate::infrastructure::claude::Claude;
use crate::infrastructure::gh::Gh;
use crate::infrastructure::git::Git;
use crate::pipeline::{QueueOp, TaskOutput};
use crate::queue::task_queues::{merge_phase, MergeItem, TaskQueues};
use crate::queue::Database;

/// MERGING/CONFLICT 상태에서 아이템을 제거하는 헬퍼.
fn remove_from_phase(queues: &mut TaskQueues, work_id: &str) {
    queues.merges.remove(work_id);
}

/// Pending 머지를 pop하여 처리
pub async fn process_pending(
    db: &Database,
    env: &dyn Env,
    workspace: &Workspace<'_>,
    notifier: &Notifier<'_>,
    gh: &dyn Gh,
    claude: &dyn Claude,
    queues: &mut TaskQueues,
) -> Result<()> {
    let cfg = crate::config::loader::load_merged(env, None);
    let concurrency = cfg.consumer.merge_concurrency as usize;
    let merger = Merger::new(claude);

    for _ in 0..concurrency {
        let item = match queues.merges.pop(merge_phase::PENDING) {
            Some(item) => item,
            None => break,
        };
        let gh_host = item.gh_host.as_deref();

        // Pending → Merging 상태 전이 (TUI/status 가시성)
        let work_id = item.work_id.clone();
        queues.merges.push(merge_phase::MERGING, item.clone());
        tracing::debug!("merge PR #{}: Pending → Merging", item.pr_number);

        // Pre-flight: GitHub에서 PR이 아직 머지 가능한 상태인지 확인
        if !notifier
            .is_pr_mergeable(&item.repo_name, item.pr_number, gh_host)
            .await
        {
            remove_from_phase(queues, &work_id);
            gh.label_remove(&item.repo_name, item.pr_number, labels::WIP, gh_host)
                .await;
            gh.label_add(&item.repo_name, item.pr_number, labels::DONE, gh_host)
                .await;
            tracing::info!(
                "PR #{} is closed or already merged, skipping",
                item.pr_number
            );
            continue;
        }

        let worker_id = Uuid::new_v4().to_string();
        let task_id = format!("merge-pr-{}", item.pr_number);

        if let Err(e) = workspace
            .ensure_cloned(&item.repo_url, &item.repo_name)
            .await
        {
            remove_from_phase(queues, &work_id);
            gh.label_remove(&item.repo_name, item.pr_number, labels::WIP, gh_host)
                .await;
            tracing::error!("clone failed for merge PR #{}: {e}", item.pr_number);
            continue;
        }

        let wt_path = match workspace
            .create_worktree(&item.repo_name, &task_id, None)
            .await
        {
            Ok(p) => p,
            Err(e) => {
                remove_from_phase(queues, &work_id);
                gh.label_remove(&item.repo_name, item.pr_number, labels::WIP, gh_host)
                    .await;
                tracing::error!("worktree failed for merge PR #{}: {e}", item.pr_number);
                continue;
            }
        };

        let started = Utc::now().to_rfc3339();
        let merge_output = merger.merge_pr(&wt_path, item.pr_number).await;

        let finished = Utc::now().to_rfc3339();
        let duration = chrono::Utc::now()
            .signed_duration_since(chrono::DateTime::parse_from_rfc3339(&started).unwrap())
            .num_milliseconds();

        let _ = db.log_insert(&NewConsumerLog {
            repo_id: item.repo_id.clone(),
            queue_type: "merge".to_string(),
            queue_item_id: item.work_id.clone(),
            worker_id: worker_id.clone(),
            command: format!("claude -p \"/git-utils:merge-pr {}\"", item.pr_number),
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
                // Merging → done
                remove_from_phase(queues, &work_id);
                gh.label_remove(&item.repo_name, item.pr_number, labels::WIP, gh_host)
                    .await;
                gh.label_add(&item.repo_name, item.pr_number, labels::DONE, gh_host)
                    .await;
                tracing::info!("PR #{}: Merging → done", item.pr_number);
            }
            MergeOutcome::Conflict => {
                // Merging → Conflict 상태 전이
                remove_from_phase(queues, &work_id);
                queues.merges.push(merge_phase::CONFLICT, item.clone());
                tracing::info!("PR #{}: Merging → Conflict", item.pr_number);

                let resolve_output = merger.resolve_conflicts(&wt_path, item.pr_number).await;

                match resolve_output.outcome {
                    MergeOutcome::Success => {
                        // Conflict → done
                        remove_from_phase(queues, &work_id);
                        gh.label_remove(&item.repo_name, item.pr_number, labels::WIP, gh_host)
                            .await;
                        gh.label_add(&item.repo_name, item.pr_number, labels::DONE, gh_host)
                            .await;
                        tracing::info!("PR #{}: Conflict → done (resolved)", item.pr_number);
                    }
                    _ => {
                        remove_from_phase(queues, &work_id);
                        gh.label_remove(&item.repo_name, item.pr_number, labels::WIP, gh_host)
                            .await;
                        tracing::error!("conflict resolution failed for PR #{}", item.pr_number);
                    }
                }
            }
            MergeOutcome::Failed { exit_code } => {
                remove_from_phase(queues, &work_id);
                gh.label_remove(&item.repo_name, item.pr_number, labels::WIP, gh_host)
                    .await;
                tracing::error!("merge exited with {} for PR #{}", exit_code, item.pr_number);
            }
            MergeOutcome::Error(e) => {
                remove_from_phase(queues, &work_id);
                gh.label_remove(&item.repo_name, item.pr_number, labels::WIP, gh_host)
                    .await;
                tracing::error!("merge error for PR #{}: {e}", item.pr_number);
            }
        }

        // 모든 경로에서 worktree 정리 보장
        let _ = workspace.remove_worktree(&item.repo_name, &task_id).await;
    }

    Ok(())
}

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
    claude: &dyn Claude,
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
