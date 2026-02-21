use anyhow::Result;
use chrono::Utc;
use uuid::Uuid;

use crate::config;
use crate::config::Env;
use crate::queue::models::*;
use crate::queue::repository::*;
use crate::queue::Database;
use crate::session;
use crate::workspace;

/// pending PR 처리
pub async fn process_pending(db: &Database, env: &dyn Env) -> Result<()> {
    let cfg = config::loader::load_merged(env, None);
    let items = db.pr_find_pending(cfg.consumer.pr_concurrency)?;

    for item in items {
        let worker_id = Uuid::new_v4().to_string();

        db.pr_update_status(
            &item.id,
            "reviewing",
            &StatusFields {
                worker_id: Some(worker_id.clone()),
                ..Default::default()
            },
        )?;

        // 워크스페이스 준비
        let task_id = format!("pr-{}", item.github_number);
        if let Err(e) = workspace::ensure_cloned(env, &item.repo_url, &item.repo_name).await {
            db.pr_mark_failed(&item.id, &format!("clone failed: {e}"))?;
            continue;
        }

        let wt_path =
            match workspace::create_worktree(env, &item.repo_name, &task_id, Some(&item.head_branch))
                .await
            {
                Ok(p) => p,
                Err(e) => {
                    db.pr_mark_failed(&item.id, &format!("worktree failed: {e}"))?;
                    continue;
                }
            };

        // YAML 설정 로드 (글로벌 + 레포별 머지)
        let config = config::loader::load_merged(env, Some(&wt_path));
        let pr_workflow = &config.workflow.pr;

        // 1단계: Multi-LLM 리뷰
        let started = Utc::now().to_rfc3339();

        let result = session::run_claude(&wt_path, pr_workflow, Some("json")).await;

        match result {
            Ok(res) => {
                let finished = Utc::now().to_rfc3339();
                let duration = chrono::Utc::now()
                    .signed_duration_since(chrono::DateTime::parse_from_rfc3339(&started).unwrap())
                    .num_milliseconds();

                db.log_insert(&NewConsumerLog {
                    repo_id: item.repo_id.clone(),
                    queue_type: "pr".to_string(),
                    queue_item_id: item.id.clone(),
                    worker_id: worker_id.clone(),
                    command: format!(
                        "claude -p \"{}\" (PR #{})",
                        pr_workflow, item.github_number
                    ),
                    stdout: res.stdout.clone(),
                    stderr: res.stderr.clone(),
                    exit_code: res.exit_code,
                    started_at: started,
                    finished_at: finished,
                    duration_ms: duration,
                })?;

                if res.exit_code == 0 {
                    let review = session::output::parse_output(&res.stdout);
                    db.pr_update_status(
                        &item.id,
                        "review_done",
                        &StatusFields {
                            review_comment: Some(review),
                            ..Default::default()
                        },
                    )?;
                    tracing::info!("PR #{} review complete", item.github_number);
                } else {
                    db.pr_mark_failed(
                        &item.id,
                        &format!("review exited with {}", res.exit_code),
                    )?;
                }
            }
            Err(e) => {
                db.pr_mark_failed(&item.id, &format!("review error: {e}"))?;
            }
        }
    }

    Ok(())
}
