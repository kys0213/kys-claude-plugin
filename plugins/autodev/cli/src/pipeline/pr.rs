use anyhow::Result;
use chrono::Utc;
use uuid::Uuid;

use crate::components::notifier::Notifier;
use crate::components::reviewer::Reviewer;
use crate::components::workspace::Workspace;
use crate::config;
use crate::config::Env;
use crate::infrastructure::claude::Claude;
use crate::infrastructure::gh::Gh;
use crate::queue::models::*;
use crate::queue::repository::*;
use crate::queue::task_queues::{labels, pr_phase, TaskQueues};
use crate::queue::Database;

/// PR 리뷰 결과를 GitHub 댓글로 포맷
fn format_review_comment(review: &str, pr_number: i64) -> String {
    format!(
        "<!-- autodev:review -->\n\
         ## Autodev Code Review (PR #{})\n\n\
         {review}",
        pr_number
    )
}

/// Pending PR을 pop하여 리뷰
pub async fn process_pending(
    db: &Database,
    env: &dyn Env,
    workspace: &Workspace<'_>,
    notifier: &Notifier<'_>,
    gh: &dyn Gh,
    claude: &dyn Claude,
    queues: &mut TaskQueues,
) -> Result<()> {
    let cfg = config::loader::load_merged(env, None);
    let concurrency = cfg.consumer.pr_concurrency as usize;
    let gh_host = cfg.consumer.gh_host.as_deref();
    let reviewer = Reviewer::new(claude);

    for _ in 0..concurrency {
        let mut item = match queues.prs.pop(pr_phase::PENDING) {
            Some(item) => item,
            None => break,
        };

        // Pre-flight: GitHub에서 PR이 리뷰 대상인지 확인
        if !notifier
            .is_pr_reviewable(&item.repo_name, item.github_number, gh_host)
            .await
        {
            gh.label_remove(&item.repo_name, item.github_number, labels::WIP, gh_host).await;
            gh.label_add(&item.repo_name, item.github_number, labels::DONE, gh_host).await;
            tracing::info!("PR #{} is closed or already approved, skipping", item.github_number);
            continue;
        }

        let worker_id = Uuid::new_v4().to_string();
        let task_id = format!("pr-{}", item.github_number);

        if let Err(e) = workspace.ensure_cloned(&item.repo_url, &item.repo_name).await {
            gh.label_remove(&item.repo_name, item.github_number, labels::WIP, gh_host).await;
            tracing::error!("clone failed for PR #{}: {e}", item.github_number);
            continue;
        }

        let wt_path = match workspace
            .create_worktree(&item.repo_name, &task_id, Some(&item.head_branch))
            .await
        {
            Ok(p) => p,
            Err(e) => {
                gh.label_remove(&item.repo_name, item.github_number, labels::WIP, gh_host).await;
                tracing::error!("worktree failed for PR #{}: {e}", item.github_number);
                continue;
            }
        };

        let repo_cfg = config::loader::load_merged(env, Some(&wt_path));
        let pr_workflow = &repo_cfg.workflow.pr;

        let started = Utc::now().to_rfc3339();

        match reviewer.review_pr(&wt_path, pr_workflow).await {
            Ok(output) => {
                let finished = Utc::now().to_rfc3339();
                let duration = chrono::Utc::now()
                    .signed_duration_since(chrono::DateTime::parse_from_rfc3339(&started).unwrap())
                    .num_milliseconds();

                let _ = db.log_insert(&NewConsumerLog {
                    repo_id: item.repo_id.clone(),
                    queue_type: "pr".to_string(),
                    queue_item_id: item.work_id.clone(),
                    worker_id: worker_id.clone(),
                    command: format!("claude -p \"{}\" (PR #{})", pr_workflow, item.github_number),
                    stdout: output.stdout.clone(),
                    stderr: output.stderr.clone(),
                    exit_code: output.exit_code,
                    started_at: started,
                    finished_at: finished,
                    duration_ms: duration,
                });

                if output.exit_code == 0 {
                    let pr_num = item.github_number;

                    // 리뷰 결과를 GitHub PR 댓글로 게시
                    let comment = format_review_comment(&output.review, pr_num);
                    notifier.post_issue_comment(&item.repo_name, pr_num, &comment, gh_host).await;

                    // 리뷰 결과 저장 → ReviewDone에 push (피드백 루프 진입)
                    item.review_comment = Some(output.review);
                    queues.prs.push(pr_phase::REVIEW_DONE, item);
                    tracing::info!("PR #{} review complete → ReviewDone", pr_num);
                } else {
                    gh.label_remove(&item.repo_name, item.github_number, labels::WIP, gh_host).await;
                    tracing::error!("review exited with {} for PR #{}", output.exit_code, item.github_number);
                }
            }
            Err(e) => {
                gh.label_remove(&item.repo_name, item.github_number, labels::WIP, gh_host).await;
                tracing::error!("review error for PR #{}: {e}", item.github_number);
            }
        }
    }

    Ok(())
}

/// ReviewDone PR을 pop하여 피드백 반영 구현
pub async fn process_review_done(
    db: &Database,
    env: &dyn Env,
    workspace: &Workspace<'_>,
    gh: &dyn Gh,
    claude: &dyn Claude,
    queues: &mut TaskQueues,
) -> Result<()> {
    let cfg = config::loader::load_merged(env, None);
    let gh_host = cfg.consumer.gh_host.as_deref();

    while let Some(item) = queues.prs.pop(pr_phase::REVIEW_DONE) {
        let worker_id = Uuid::new_v4().to_string();
        let task_id = format!("pr-{}", item.github_number);

        if let Err(e) = workspace.ensure_cloned(&item.repo_url, &item.repo_name).await {
            gh.label_remove(&item.repo_name, item.github_number, labels::WIP, gh_host).await;
            tracing::error!("clone failed for PR #{}: {e}", item.github_number);
            continue;
        }

        let wt_path = match workspace
            .create_worktree(&item.repo_name, &task_id, Some(&item.head_branch))
            .await
        {
            Ok(p) => p,
            Err(e) => {
                gh.label_remove(&item.repo_name, item.github_number, labels::WIP, gh_host).await;
                tracing::error!("worktree failed for PR #{}: {e}", item.github_number);
                continue;
            }
        };

        let review = item.review_comment.as_deref().unwrap_or("");
        let prompt = format!(
            "Implement the following review feedback for PR #{}:\n\n{review}",
            item.github_number
        );

        let started = Utc::now().to_rfc3339();
        let result = claude.run_session(&wt_path, &prompt, None).await;

        match result {
            Ok(res) => {
                let finished = Utc::now().to_rfc3339();
                let duration = chrono::Utc::now()
                    .signed_duration_since(chrono::DateTime::parse_from_rfc3339(&started).unwrap())
                    .num_milliseconds();

                let _ = db.log_insert(&NewConsumerLog {
                    repo_id: item.repo_id.clone(),
                    queue_type: "pr".to_string(),
                    queue_item_id: item.work_id.clone(),
                    worker_id: worker_id.clone(),
                    command: format!("implement review feedback PR #{}", item.github_number),
                    stdout: res.stdout.clone(),
                    stderr: res.stderr.clone(),
                    exit_code: res.exit_code,
                    started_at: started,
                    finished_at: finished,
                    duration_ms: duration,
                });

                if res.exit_code == 0 {
                    // 개선 완료 → Improved에 push (재리뷰 대기)
                    queues.prs.push(pr_phase::IMPROVED, item);
                    tracing::info!("PR feedback implemented → Improved");
                } else {
                    gh.label_remove(&item.repo_name, item.github_number, labels::WIP, gh_host).await;
                    tracing::error!("feedback implementation failed for PR #{}", item.github_number);
                }
            }
            Err(e) => {
                gh.label_remove(&item.repo_name, item.github_number, labels::WIP, gh_host).await;
                tracing::error!("feedback implementation error for PR #{}: {e}", item.github_number);
            }
        }
    }

    Ok(())
}

/// Improved PR을 pop하여 재리뷰 → approve면 done, request_changes면 ReviewDone 재진입
pub async fn process_improved(
    db: &Database,
    env: &dyn Env,
    workspace: &Workspace<'_>,
    _notifier: &Notifier<'_>,
    gh: &dyn Gh,
    claude: &dyn Claude,
    queues: &mut TaskQueues,
) -> Result<()> {
    let cfg = config::loader::load_merged(env, None);
    let gh_host = cfg.consumer.gh_host.as_deref();
    let reviewer = Reviewer::new(claude);

    while let Some(mut item) = queues.prs.pop(pr_phase::IMPROVED) {
        let worker_id = Uuid::new_v4().to_string();
        let task_id = format!("pr-{}", item.github_number);

        if let Err(e) = workspace.ensure_cloned(&item.repo_url, &item.repo_name).await {
            gh.label_remove(&item.repo_name, item.github_number, labels::WIP, gh_host).await;
            tracing::error!("clone failed for PR #{}: {e}", item.github_number);
            continue;
        }

        let wt_path = match workspace
            .create_worktree(&item.repo_name, &task_id, Some(&item.head_branch))
            .await
        {
            Ok(p) => p,
            Err(e) => {
                gh.label_remove(&item.repo_name, item.github_number, labels::WIP, gh_host).await;
                tracing::error!("worktree failed for PR #{}: {e}", item.github_number);
                continue;
            }
        };

        let repo_cfg = config::loader::load_merged(env, Some(&wt_path));
        let pr_workflow = &repo_cfg.workflow.pr;

        let started = Utc::now().to_rfc3339();

        match reviewer.review_pr(&wt_path, pr_workflow).await {
            Ok(output) => {
                let finished = Utc::now().to_rfc3339();
                let duration = chrono::Utc::now()
                    .signed_duration_since(chrono::DateTime::parse_from_rfc3339(&started).unwrap())
                    .num_milliseconds();

                let _ = db.log_insert(&NewConsumerLog {
                    repo_id: item.repo_id.clone(),
                    queue_type: "pr".to_string(),
                    queue_item_id: item.work_id.clone(),
                    worker_id: worker_id.clone(),
                    command: format!("re-review PR #{}", item.github_number),
                    stdout: output.stdout.clone(),
                    stderr: output.stderr.clone(),
                    exit_code: output.exit_code,
                    started_at: started,
                    finished_at: finished,
                    duration_ms: duration,
                });

                if output.exit_code == 0 {
                    // Knowledge extraction (best effort)
                    if cfg.consumer.knowledge_extraction {
                        let _ = crate::knowledge::extractor::extract_task_knowledge(
                            claude, gh, &item.repo_name, item.github_number,
                            "pr", &wt_path, gh_host,
                        ).await;
                    }

                    gh.label_remove(&item.repo_name, item.github_number, labels::WIP, gh_host).await;
                    gh.label_add(&item.repo_name, item.github_number, labels::DONE, gh_host).await;
                    tracing::info!("PR #{} re-review → done (approved)", item.github_number);
                } else {
                    // 재리뷰 실패 → request_changes → ReviewDone 재진입
                    item.review_comment = Some(output.review);
                    queues.prs.push(pr_phase::REVIEW_DONE, item);
                    tracing::info!("PR re-review → ReviewDone (request_changes)");
                }
            }
            Err(e) => {
                gh.label_remove(&item.repo_name, item.github_number, labels::WIP, gh_host).await;
                tracing::error!("re-review error for PR #{}: {e}", item.github_number);
            }
        }
    }

    Ok(())
}
