use anyhow::Result;
use chrono::Utc;
use uuid::Uuid;

use crate::queue::models::*;
use crate::queue::repository::*;
use crate::queue::Database;
use crate::session;
use crate::workspace;

/// pending 머지 처리
pub async fn process_pending(db: &Database) -> Result<()> {
    let items = db.merge_find_pending(1)?;

    for item in items {
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
        if let Err(e) = workspace::ensure_cloned(&item.repo_url, &item.repo_name).await {
            db.merge_mark_failed(&item.id, &format!("clone failed: {e}"))?;
            continue;
        }

        let wt_path = match workspace::create_worktree(&item.repo_name, &task_id, None).await {
            Ok(p) => p,
            Err(e) => {
                db.merge_mark_failed(&item.id, &format!("worktree failed: {e}"))?;
                continue;
            }
        };

        // 머지 실행
        let prompt = format!("/git-utils:merge-pr {}", item.pr_number);
        let started = Utc::now().to_rfc3339();

        let result = session::run_claude(&wt_path, &prompt, None).await;

        match result {
            Ok(res) => {
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
                    stdout: res.stdout.clone(),
                    stderr: res.stderr.clone(),
                    exit_code: res.exit_code,
                    started_at: started,
                    finished_at: finished,
                    duration_ms: duration,
                })?;

                if res.exit_code == 0 {
                    db.merge_update_status(&item.id, "done", &StatusFields::default())?;
                    tracing::info!("PR #{} merged successfully", item.pr_number);

                    // worktree 정리
                    let _ = workspace::remove_worktree(&item.repo_name, &task_id).await;
                } else if res.stdout.contains("conflict") || res.stderr.contains("conflict") {
                    // 충돌 발생 - 해결 시도
                    db.merge_update_status(&item.id, "conflict", &StatusFields::default())?;

                    let resolve_result = session::run_claude(
                        &wt_path,
                        &format!("Resolve merge conflicts for PR #{}", item.pr_number),
                        None,
                    )
                    .await;

                    match resolve_result {
                        Ok(rr) if rr.exit_code == 0 => {
                            db.merge_update_status(
                                &item.id,
                                "done",
                                &StatusFields::default(),
                            )?;
                            tracing::info!(
                                "PR #{} conflicts resolved and merged",
                                item.pr_number
                            );
                        }
                        _ => {
                            db.merge_mark_failed(&item.id, "conflict resolution failed")?;
                        }
                    }
                } else {
                    db.merge_mark_failed(
                        &item.id,
                        &format!("merge exited with {}", res.exit_code),
                    )?;
                }
            }
            Err(e) => {
                db.merge_mark_failed(&item.id, &format!("merge error: {e}"))?;
            }
        }
    }

    Ok(())
}
