use anyhow::Result;
use chrono::Utc;
use uuid::Uuid;

use crate::components::notifier::Notifier;
use crate::components::reviewer::Reviewer;
use crate::components::workspace::Workspace;
use crate::config;
use crate::config::Env;
use crate::infrastructure::claude::output::ReviewVerdict;
use crate::infrastructure::claude::Claude;
use crate::infrastructure::gh::Gh;
use crate::infrastructure::suggest_workflow::SuggestWorkflow;
use crate::queue::models::*;
use crate::queue::repository::*;
use crate::queue::task_queues::{labels, pr_phase, TaskQueues};
use crate::queue::Database;

/// REVIEWING/IMPROVING 상태에서 아이템을 제거하는 헬퍼.
fn remove_from_phase(queues: &mut TaskQueues, work_id: &str) {
    queues.prs.remove(work_id);
}

/// PR 리뷰 결과를 GitHub 댓글로 포맷
fn format_review_comment(review: &str, pr_number: i64, verdict: Option<&ReviewVerdict>) -> String {
    let verdict_label = match verdict {
        Some(ReviewVerdict::Approve) => " — **Approved**",
        Some(ReviewVerdict::RequestChanges) => " — **Changes Requested**",
        None => "",
    };
    format!(
        "<!-- autodev:review -->\n\
         ## Autodev Code Review (PR #{pr_number}){verdict_label}\n\n\
         {review}"
    )
}

/// Pending PR을 pop하여 리뷰
#[allow(clippy::too_many_arguments)]
pub async fn process_pending(
    db: &Database,
    env: &dyn Env,
    workspace: &Workspace<'_>,
    notifier: &Notifier<'_>,
    gh: &dyn Gh,
    claude: &dyn Claude,
    sw: &dyn SuggestWorkflow,
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

        // Pending → Reviewing 상태 전이 (TUI/status 가시성)
        let work_id = item.work_id.clone();
        queues.prs.push(pr_phase::REVIEWING, item.clone());
        tracing::debug!("PR #{}: Pending → Reviewing", item.github_number);

        // Pre-flight: GitHub에서 PR이 리뷰 대상인지 확인
        if !notifier
            .is_pr_reviewable(&item.repo_name, item.github_number, gh_host)
            .await
        {
            remove_from_phase(queues, &work_id);
            gh.label_remove(&item.repo_name, item.github_number, labels::WIP, gh_host)
                .await;
            gh.label_add(&item.repo_name, item.github_number, labels::DONE, gh_host)
                .await;
            tracing::info!(
                "PR #{} is closed or already approved, skipping",
                item.github_number
            );
            continue;
        }

        let worker_id = Uuid::new_v4().to_string();
        let task_id = format!("pr-{}", item.github_number);

        if let Err(e) = workspace
            .ensure_cloned(&item.repo_url, &item.repo_name)
            .await
        {
            remove_from_phase(queues, &work_id);
            gh.label_remove(&item.repo_name, item.github_number, labels::WIP, gh_host)
                .await;
            tracing::error!("clone failed for PR #{}: {e}", item.github_number);
            continue;
        }

        let wt_path = match workspace
            .create_worktree(&item.repo_name, &task_id, Some(&item.head_branch))
            .await
        {
            Ok(p) => p,
            Err(e) => {
                remove_from_phase(queues, &work_id);
                gh.label_remove(&item.repo_name, item.github_number, labels::WIP, gh_host)
                    .await;
                tracing::error!("worktree failed for PR #{}: {e}", item.github_number);
                continue;
            }
        };

        let repo_cfg = config::loader::load_merged(env, Some(&wt_path));
        let pr_prompt = format!(
            "[autodev] review: PR #{}\n\n{}",
            item.github_number, repo_cfg.workflow.pr
        );

        let started = Utc::now().to_rfc3339();

        match reviewer.review_pr(&wt_path, &pr_prompt).await {
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
                    command: format!("[autodev] review: PR #{}", item.github_number),
                    stdout: output.stdout.clone(),
                    stderr: output.stderr.clone(),
                    exit_code: output.exit_code,
                    started_at: started,
                    finished_at: finished,
                    duration_ms: duration,
                });

                if output.exit_code == 0 {
                    let pr_num = item.github_number;

                    match output.verdict {
                        Some(ReviewVerdict::Approve) => {
                            // approve → GitHub Review API + 댓글 게시 + 즉시 done
                            gh.pr_review(
                                &item.repo_name,
                                pr_num,
                                "APPROVE",
                                &output.review,
                                gh_host,
                            )
                            .await;
                            let comment = format_review_comment(
                                &output.review,
                                pr_num,
                                Some(&ReviewVerdict::Approve),
                            );
                            notifier
                                .post_issue_comment(&item.repo_name, pr_num, &comment, gh_host)
                                .await;

                            // Knowledge extraction (best effort)
                            if cfg.consumer.knowledge_extraction {
                                let _ = crate::knowledge::extractor::extract_task_knowledge(
                                    claude,
                                    gh,
                                    sw,
                                    &item.repo_name,
                                    item.github_number,
                                    "pr",
                                    &wt_path,
                                    gh_host,
                                )
                                .await;
                            }

                            // Reviewing → done
                            remove_from_phase(queues, &work_id);
                            gh.label_remove(&item.repo_name, pr_num, labels::WIP, gh_host)
                                .await;
                            gh.label_add(&item.repo_name, pr_num, labels::DONE, gh_host)
                                .await;
                            tracing::info!("PR #{pr_num}: Reviewing → done (approved)");
                        }
                        Some(ReviewVerdict::RequestChanges) | None => {
                            // Reviewing → ReviewDone (피드백 루프 진입)
                            remove_from_phase(queues, &work_id);
                            if matches!(output.verdict, Some(ReviewVerdict::RequestChanges)) {
                                gh.pr_review(
                                    &item.repo_name,
                                    pr_num,
                                    "REQUEST_CHANGES",
                                    &output.review,
                                    gh_host,
                                )
                                .await;
                            }
                            let comment = format_review_comment(
                                &output.review,
                                pr_num,
                                output.verdict.as_ref(),
                            );
                            notifier
                                .post_issue_comment(&item.repo_name, pr_num, &comment, gh_host)
                                .await;

                            item.review_comment = Some(output.review);
                            queues.prs.push(pr_phase::REVIEW_DONE, item);
                            tracing::info!("PR #{pr_num}: Reviewing → ReviewDone");
                        }
                    }
                } else {
                    remove_from_phase(queues, &work_id);
                    gh.label_remove(&item.repo_name, item.github_number, labels::WIP, gh_host)
                        .await;
                    tracing::error!(
                        "review exited with {} for PR #{}",
                        output.exit_code,
                        item.github_number
                    );
                }
            }
            Err(e) => {
                remove_from_phase(queues, &work_id);
                gh.label_remove(&item.repo_name, item.github_number, labels::WIP, gh_host)
                    .await;
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
        // ReviewDone → Improving 상태 전이 (TUI/status 가시성)
        let work_id = item.work_id.clone();
        queues.prs.push(pr_phase::IMPROVING, item.clone());
        tracing::debug!("PR #{}: ReviewDone → Improving", item.github_number);

        let worker_id = Uuid::new_v4().to_string();
        let task_id = format!("pr-{}", item.github_number);

        if let Err(e) = workspace
            .ensure_cloned(&item.repo_url, &item.repo_name)
            .await
        {
            remove_from_phase(queues, &work_id);
            gh.label_remove(&item.repo_name, item.github_number, labels::WIP, gh_host)
                .await;
            tracing::error!("clone failed for PR #{}: {e}", item.github_number);
            continue;
        }

        let wt_path = match workspace
            .create_worktree(&item.repo_name, &task_id, Some(&item.head_branch))
            .await
        {
            Ok(p) => p,
            Err(e) => {
                remove_from_phase(queues, &work_id);
                gh.label_remove(&item.repo_name, item.github_number, labels::WIP, gh_host)
                    .await;
                tracing::error!("worktree failed for PR #{}: {e}", item.github_number);
                continue;
            }
        };

        let review = item.review_comment.as_deref().unwrap_or("");
        let prompt = format!(
            "[autodev] improve: PR #{}\n\n\
             Implement the following review feedback for PR #{}:\n\n{review}",
            item.github_number, item.github_number
        );

        let started = Utc::now().to_rfc3339();
        let result = claude
            .run_session(&wt_path, &prompt, &Default::default())
            .await;

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
                    // Improving → Improved (재리뷰 대기)
                    remove_from_phase(queues, &work_id);
                    let pr_num = item.github_number;
                    queues.prs.push(pr_phase::IMPROVED, item);
                    tracing::info!("PR #{pr_num}: Improving → Improved");
                } else {
                    remove_from_phase(queues, &work_id);
                    gh.label_remove(&item.repo_name, item.github_number, labels::WIP, gh_host)
                        .await;
                    tracing::error!(
                        "feedback implementation failed for PR #{}",
                        item.github_number
                    );
                }
            }
            Err(e) => {
                remove_from_phase(queues, &work_id);
                gh.label_remove(&item.repo_name, item.github_number, labels::WIP, gh_host)
                    .await;
                tracing::error!(
                    "feedback implementation error for PR #{}: {e}",
                    item.github_number
                );
            }
        }
    }

    Ok(())
}

/// Improved PR을 pop하여 재리뷰 → approve면 done, request_changes면 ReviewDone 재진입
#[allow(clippy::too_many_arguments)]
pub async fn process_improved(
    db: &Database,
    env: &dyn Env,
    workspace: &Workspace<'_>,
    _notifier: &Notifier<'_>,
    gh: &dyn Gh,
    claude: &dyn Claude,
    sw: &dyn SuggestWorkflow,
    queues: &mut TaskQueues,
) -> Result<()> {
    let cfg = config::loader::load_merged(env, None);
    let gh_host = cfg.consumer.gh_host.as_deref();
    let reviewer = Reviewer::new(claude);

    while let Some(mut item) = queues.prs.pop(pr_phase::IMPROVED) {
        // Improved → Reviewing 상태 전이 (재리뷰, TUI/status 가시성)
        let work_id = item.work_id.clone();
        queues.prs.push(pr_phase::REVIEWING, item.clone());
        tracing::debug!(
            "PR #{}: Improved → Reviewing (re-review)",
            item.github_number
        );

        let worker_id = Uuid::new_v4().to_string();
        let task_id = format!("pr-{}", item.github_number);

        if let Err(e) = workspace
            .ensure_cloned(&item.repo_url, &item.repo_name)
            .await
        {
            remove_from_phase(queues, &work_id);
            gh.label_remove(&item.repo_name, item.github_number, labels::WIP, gh_host)
                .await;
            tracing::error!("clone failed for PR #{}: {e}", item.github_number);
            continue;
        }

        let wt_path = match workspace
            .create_worktree(&item.repo_name, &task_id, Some(&item.head_branch))
            .await
        {
            Ok(p) => p,
            Err(e) => {
                remove_from_phase(queues, &work_id);
                gh.label_remove(&item.repo_name, item.github_number, labels::WIP, gh_host)
                    .await;
                tracing::error!("worktree failed for PR #{}: {e}", item.github_number);
                continue;
            }
        };

        let repo_cfg = config::loader::load_merged(env, Some(&wt_path));
        let pr_prompt = format!(
            "[autodev] review: PR #{}\n\n{}",
            item.github_number, repo_cfg.workflow.pr
        );

        let started = Utc::now().to_rfc3339();

        match reviewer.review_pr(&wt_path, &pr_prompt).await {
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
                    command: format!("[autodev] re-review: PR #{}", item.github_number),
                    stdout: output.stdout.clone(),
                    stderr: output.stderr.clone(),
                    exit_code: output.exit_code,
                    started_at: started,
                    finished_at: finished,
                    duration_ms: duration,
                });

                if output.exit_code != 0 {
                    remove_from_phase(queues, &work_id);
                    gh.label_remove(&item.repo_name, item.github_number, labels::WIP, gh_host)
                        .await;
                    tracing::error!(
                        "re-review exited with {} for PR #{}",
                        output.exit_code,
                        item.github_number
                    );
                    continue;
                }

                match output.verdict {
                    Some(ReviewVerdict::Approve) => {
                        // GitHub Review API: approve
                        gh.pr_review(&item.repo_name, item.github_number, "APPROVE", "", gh_host)
                            .await;

                        // Knowledge extraction (best effort)
                        if cfg.consumer.knowledge_extraction {
                            let _ = crate::knowledge::extractor::extract_task_knowledge(
                                claude,
                                gh,
                                sw,
                                &item.repo_name,
                                item.github_number,
                                "pr",
                                &wt_path,
                                gh_host,
                            )
                            .await;
                        }

                        // Reviewing → done (re-review approved)
                        remove_from_phase(queues, &work_id);
                        gh.label_remove(&item.repo_name, item.github_number, labels::WIP, gh_host)
                            .await;
                        gh.label_add(&item.repo_name, item.github_number, labels::DONE, gh_host)
                            .await;
                        tracing::info!(
                            "PR #{}: Reviewing → done (re-review approved)",
                            item.github_number
                        );
                    }
                    Some(ReviewVerdict::RequestChanges) | None => {
                        // Reviewing → ReviewDone (재진입)
                        remove_from_phase(queues, &work_id);
                        if matches!(output.verdict, Some(ReviewVerdict::RequestChanges)) {
                            gh.pr_review(
                                &item.repo_name,
                                item.github_number,
                                "REQUEST_CHANGES",
                                &output.review,
                                gh_host,
                            )
                            .await;
                        }
                        item.review_comment = Some(output.review);
                        queues.prs.push(pr_phase::REVIEW_DONE, item);
                        tracing::info!("PR re-review: Reviewing → ReviewDone (request_changes)");
                    }
                }
            }
            Err(e) => {
                remove_from_phase(queues, &work_id);
                gh.label_remove(&item.repo_name, item.github_number, labels::WIP, gh_host)
                    .await;
                tracing::error!("re-review error for PR #{}: {e}", item.github_number);
            }
        }
    }

    Ok(())
}
