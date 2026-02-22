use anyhow::Result;
use chrono::Utc;
use uuid::Uuid;

use crate::active::ActiveItems;
use crate::components::merger::{MergeOutcome, Merger};
use crate::components::notifier::Notifier;
use crate::components::workspace::Workspace;
use crate::config::Env;
use crate::infrastructure::claude::Claude;
use crate::queue::models::*;
use crate::queue::repository::*;
use crate::queue::Database;

/// pending 머지 처리
pub async fn process_pending(
    db: &Database,
    env: &dyn Env,
    workspace: &Workspace<'_>,
    notifier: &Notifier<'_>,
    claude: &dyn Claude,
    active: &mut ActiveItems,
) -> Result<()> {
    let cfg = crate::config::loader::load_merged(env, None);
    let items = db.merge_find_pending(cfg.consumer.merge_concurrency)?;
    let merger = Merger::new(claude);

    for item in items {
        // Pre-flight: GitHub에서 PR이 아직 머지 가능한 상태인지 확인
        if !notifier
            .is_pr_mergeable(
                &item.repo_name,
                item.pr_number,
                cfg.consumer.gh_host.as_deref(),
            )
            .await
        {
            db.merge_update_status(&item.id, "done", &StatusFields::default())?;
            active.remove("merge", &item.repo_id, item.pr_number);
            tracing::info!(
                "PR #{} is closed or already merged, skipping",
                item.pr_number
            );
            continue;
        }

        let worker_id = Uuid::new_v4().to_string();

        db.merge_update_status(
            &item.id,
            "merging",
            &StatusFields {
                worker_id: Some(worker_id.clone()),
                ..Default::default()
            },
        )?;

        // 워크스페이스 준비
        let task_id = format!("merge-pr-{}", item.pr_number);
        if let Err(e) = workspace
            .ensure_cloned(&item.repo_url, &item.repo_name)
            .await
        {
            db.merge_mark_failed(&item.id, &format!("clone failed: {e}"))?;
            continue;
        }

        let wt_path = match workspace
            .create_worktree(&item.repo_name, &task_id, None)
            .await
        {
            Ok(p) => p,
            Err(e) => {
                db.merge_mark_failed(&item.id, &format!("worktree failed: {e}"))?;
                continue;
            }
        };

        // 머지 실행 (Merger 컴포넌트 위임)
        let started = Utc::now().to_rfc3339();
        let merge_output = merger.merge_pr(&wt_path, item.pr_number).await;

        let finished = Utc::now().to_rfc3339();
        let duration = chrono::Utc::now()
            .signed_duration_since(chrono::DateTime::parse_from_rfc3339(&started).unwrap())
            .num_milliseconds();

        db.log_insert(&NewConsumerLog {
            repo_id: item.repo_id.clone(),
            queue_type: "merge".to_string(),
            queue_item_id: item.id.clone(),
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
        })?;

        match merge_output.outcome {
            MergeOutcome::Success => {
                db.merge_update_status(&item.id, "done", &StatusFields::default())?;
                active.remove("merge", &item.repo_id, item.pr_number);
                tracing::info!("PR #{} merged successfully", item.pr_number);

                let _ = workspace
                    .remove_worktree(&item.repo_name, &task_id)
                    .await;
            }
            MergeOutcome::Conflict => {
                db.merge_update_status(&item.id, "conflict", &StatusFields::default())?;

                let resolve_output =
                    merger.resolve_conflicts(&wt_path, item.pr_number).await;

                match resolve_output.outcome {
                    MergeOutcome::Success => {
                        db.merge_update_status(
                            &item.id,
                            "done",
                            &StatusFields::default(),
                        )?;
                        active.remove("merge", &item.repo_id, item.pr_number);
                        tracing::info!(
                            "PR #{} conflicts resolved and merged",
                            item.pr_number
                        );
                    }
                    _ => {
                        db.merge_mark_failed(&item.id, "conflict resolution failed")?;
                    }
                }
            }
            MergeOutcome::Failed { exit_code } => {
                db.merge_mark_failed(
                    &item.id,
                    &format!("merge exited with {}", exit_code),
                )?;
            }
            MergeOutcome::Error(e) => {
                db.merge_mark_failed(&item.id, &format!("merge error: {e}"))?;
            }
        }
    }

    Ok(())
}
