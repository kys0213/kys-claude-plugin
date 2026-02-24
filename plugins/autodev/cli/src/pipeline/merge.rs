use anyhow::Result;
use chrono::Utc;
use uuid::Uuid;

use crate::components::merger::{MergeOutcome, Merger};
use crate::components::notifier::Notifier;
use crate::components::workspace::Workspace;
use crate::config::Env;
use crate::infrastructure::claude::Claude;
use crate::infrastructure::gh::Gh;
use crate::queue::models::*;
use crate::queue::repository::*;
use crate::queue::task_queues::{labels, merge_phase, TaskQueues};
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
    let gh_host = cfg.consumer.gh_host.as_deref();
    let merger = Merger::new(claude);

    for _ in 0..concurrency {
        let item = match queues.merges.pop(merge_phase::PENDING) {
            Some(item) => item,
            None => break,
        };

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
                let _ = workspace.remove_worktree(&item.repo_name, &task_id).await;
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
    }

    Ok(())
}
