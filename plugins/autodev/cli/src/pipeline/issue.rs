use chrono::Utc;
use uuid::Uuid;

use crate::components::analyzer::Analyzer;
use crate::components::notifier::Notifier;
use crate::components::verdict;
use crate::components::workspace::Workspace;
use crate::config;
use crate::config::Env;
use crate::domain::labels;
use crate::domain::models::*;
use crate::infrastructure::agent::output;
use crate::infrastructure::agent::Agent;
use crate::infrastructure::gh::Gh;
use crate::infrastructure::git::Git;
use crate::pipeline::{QueueOp, TaskOutput, AGENT_SYSTEM_PROMPT};
use crate::queue::task_queues::{make_work_id, pr_phase, IssueItem, PrItem};

/// head branch 이름으로 이미 생성된 PR을 조회하여 번호를 반환.
/// extract_pr_number() 파싱 실패 시 fallback으로 사용하여 중복 PR 생성을 방지한다.
async fn find_existing_pr(
    gh: &dyn Gh,
    repo_name: &str,
    head_branch: &str,
    gh_host: Option<&str>,
) -> Option<i64> {
    let params = [("head", head_branch), ("state", "open"), ("per_page", "1")];
    let data = gh
        .api_paginate(repo_name, "pulls", &params, gh_host)
        .await
        .ok()?;
    let prs: Vec<serde_json::Value> = serde_json::from_slice(&data).ok()?;
    prs.first().and_then(|pr| pr["number"].as_i64())
}

// ─── 분석 프롬프트 (JSON 응답 스키마 명시) ───

const ANALYSIS_PROMPT_TEMPLATE: &str = r#"Analyze the following GitHub issue and respond in JSON.

Issue #{number}: {title}

{body}

Respond with this exact JSON schema:
{{
  "verdict": "implement" | "needs_clarification" | "wontfix",
  "confidence": 0.0-1.0,
  "summary": "1-2 sentence summary of the issue",
  "questions": ["question1", ...],
  "reason": "reason if wontfix, null otherwise",
  "report": "full markdown analysis report with: affected files, implementation direction, checkpoints, risks"
}}

Rules:
- verdict "implement": the issue is clear enough to implement
- verdict "needs_clarification": the issue is ambiguous or missing critical details
- verdict "wontfix": the issue should not be implemented (duplicate, out of scope, invalid)
- confidence: how confident you are in the verdict (0.0 = no confidence, 1.0 = fully confident)
- questions: list of clarifying questions (required when verdict is "needs_clarification")
- reason: explanation (required when verdict is "wontfix")
- report: detailed analysis regardless of verdict"#;

// ═══════════════════════════════════════════════════
// Spawnable task functions (event loop용)
// ═══════════════════════════════════════════════════

/// Issue 분석 — spawned task에서 실행.
///
/// Workspace/Notifier를 내부에서 생성하므로 `tokio::spawn` 가능.
/// 큐 조작은 TaskOutput으로 반환하여 main loop에서 처리한다.
pub async fn analyze_one(
    item: IssueItem,
    env: &dyn Env,
    gh: &dyn Gh,
    git: &dyn Git,
    claude: &dyn Agent,
) -> TaskOutput {
    let workspace = Workspace::new(git, env);
    let notifier = Notifier::new(gh);
    let cfg = config::loader::load_merged(env, None);
    let gh_host = item.gh_host.as_deref();
    let analyzer = Analyzer::new(claude);

    let work_id = item.work_id.clone();
    let repo_name = item.repo_name.clone();
    let mut ops = Vec::new();
    let mut logs = Vec::new();

    // Pre-flight: GitHub에서 이슈가 아직 open인지 확인
    if !notifier
        .is_issue_open(&item.repo_name, item.github_number, gh_host)
        .await
    {
        gh.label_remove(&item.repo_name, item.github_number, labels::WIP, gh_host)
            .await;
        gh.label_add(&item.repo_name, item.github_number, labels::DONE, gh_host)
            .await;
        tracing::info!(
            "issue #{} is closed on GitHub, skipping",
            item.github_number
        );
        ops.push(QueueOp::Remove);
        return TaskOutput {
            work_id,
            repo_name,
            queue_ops: ops,
            logs,
        };
    }

    let worker_id = Uuid::new_v4().to_string();
    let task_id = format!("issue-{}", item.github_number);

    if let Err(e) = workspace
        .ensure_cloned(&item.repo_url, &item.repo_name)
        .await
    {
        gh.label_remove(&item.repo_name, item.github_number, labels::WIP, gh_host)
            .await;
        tracing::error!("clone failed for issue #{}: {e}", item.github_number);
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
            gh.label_remove(&item.repo_name, item.github_number, labels::WIP, gh_host)
                .await;
            tracing::error!("worktree failed for issue #{}: {e}", item.github_number);
            ops.push(QueueOp::Remove);
            return TaskOutput {
                work_id,
                repo_name,
                queue_ops: ops,
                logs,
            };
        }
    };

    let body_text = item.body.as_deref().unwrap_or("");
    let prompt = format!(
        "[autodev] analyze: issue #{} - {}\n\n{}",
        item.github_number,
        item.title,
        ANALYSIS_PROMPT_TEMPLATE
            .replace("{number}", &item.github_number.to_string())
            .replace("{title}", &item.title)
            .replace("{body}", body_text),
    );

    let started = Utc::now().to_rfc3339();
    let result = analyzer
        .analyze(&wt_path, &prompt, Some(AGENT_SYSTEM_PROMPT))
        .await;

    match result {
        Ok(res) => {
            let finished = Utc::now().to_rfc3339();
            let duration = chrono::Utc::now()
                .signed_duration_since(chrono::DateTime::parse_from_rfc3339(&started).unwrap())
                .num_milliseconds();

            logs.push(NewConsumerLog {
                repo_id: item.repo_id.clone(),
                queue_type: "issue".to_string(),
                queue_item_id: item.work_id.clone(),
                worker_id: worker_id.clone(),
                command: format!("claude -p \"Analyze issue #{}...\"", item.github_number),
                stdout: res.stdout.clone(),
                stderr: res.stderr.clone(),
                exit_code: res.exit_code,
                started_at: started,
                finished_at: finished,
                duration_ms: duration,
            });

            if res.exit_code != 0 {
                gh.label_remove(&item.repo_name, item.github_number, labels::WIP, gh_host)
                    .await;
                let _ = workspace.remove_worktree(&item.repo_name, &task_id).await;
                ops.push(QueueOp::Remove);
                return TaskOutput {
                    work_id,
                    repo_name,
                    queue_ops: ops,
                    logs,
                };
            }

            match res.analysis {
                Some(ref a) if a.verdict == output::Verdict::Wontfix => {
                    let comment = verdict::format_wontfix_comment(a);
                    notifier
                        .post_issue_comment(&item.repo_name, item.github_number, &comment, gh_host)
                        .await;
                    gh.label_remove(&item.repo_name, item.github_number, labels::WIP, gh_host)
                        .await;
                    gh.label_add(&item.repo_name, item.github_number, labels::SKIP, gh_host)
                        .await;
                    tracing::info!("issue #{} → wontfix (skip)", item.github_number);
                    ops.push(QueueOp::Remove);
                }
                Some(ref a)
                    if a.verdict == output::Verdict::NeedsClarification
                        || a.confidence < cfg.consumer.confidence_threshold =>
                {
                    let comment = verdict::format_clarification_comment(a);
                    notifier
                        .post_issue_comment(&item.repo_name, item.github_number, &comment, gh_host)
                        .await;
                    gh.label_remove(&item.repo_name, item.github_number, labels::WIP, gh_host)
                        .await;
                    gh.label_add(&item.repo_name, item.github_number, labels::SKIP, gh_host)
                        .await;
                    tracing::info!(
                        "issue #{} → skip (verdict={}, confidence={:.2})",
                        item.github_number,
                        a.verdict,
                        a.confidence
                    );
                    ops.push(QueueOp::Remove);
                }
                Some(ref a) => {
                    let comment = verdict::format_analysis_comment(a);
                    notifier
                        .post_issue_comment(&item.repo_name, item.github_number, &comment, gh_host)
                        .await;
                    gh.label_remove(&item.repo_name, item.github_number, labels::WIP, gh_host)
                        .await;
                    gh.label_add(
                        &item.repo_name,
                        item.github_number,
                        labels::ANALYZED,
                        gh_host,
                    )
                    .await;
                    tracing::info!(
                        "issue #{}: Analyzing → analyzed (awaiting human review, confidence={:.2})",
                        item.github_number,
                        a.confidence
                    );
                    ops.push(QueueOp::Remove);
                }
                None => {
                    let report = output::parse_output(&res.stdout);
                    let comment = format!(
                        "<!-- autodev:analysis -->\n\
                         ## Autodev Analysis Report\n\n\
                         {report}\n\n\
                         ---\n\
                         > 이 분석을 승인하려면 `autodev:approved-analysis` 라벨을 추가하세요.\n\
                         > 수정이 필요하면 코멘트로 피드백을 남기고 `autodev:analyzed` 라벨을 제거하세요."
                    );
                    notifier
                        .post_issue_comment(&item.repo_name, item.github_number, &comment, gh_host)
                        .await;
                    gh.label_remove(&item.repo_name, item.github_number, labels::WIP, gh_host)
                        .await;
                    gh.label_add(
                        &item.repo_name,
                        item.github_number,
                        labels::ANALYZED,
                        gh_host,
                    )
                    .await;
                    tracing::warn!(
                        "issue #{}: analysis output not parseable, fallback → analyzed",
                        item.github_number
                    );
                    ops.push(QueueOp::Remove);
                }
            }
        }
        Err(e) => {
            gh.label_remove(&item.repo_name, item.github_number, labels::WIP, gh_host)
                .await;
            tracing::error!("analysis error for issue #{}: {e}", item.github_number);
            ops.push(QueueOp::Remove);
        }
    }

    let _ = workspace.remove_worktree(&item.repo_name, &task_id).await;
    TaskOutput {
        work_id,
        repo_name,
        queue_ops: ops,
        logs,
    }
}

/// Issue 구현 — spawned task에서 실행.
#[allow(clippy::too_many_arguments)]
pub async fn implement_one(
    item: IssueItem,
    env: &dyn Env,
    gh: &dyn Gh,
    git: &dyn Git,
    claude: &dyn Agent,
) -> TaskOutput {
    let workspace = Workspace::new(git, env);
    let notifier = Notifier::new(gh);
    let gh_host = item.gh_host.as_deref();

    let work_id = item.work_id.clone();
    let repo_name = item.repo_name.clone();
    let mut ops = Vec::new();
    let mut logs = Vec::new();

    let worker_id = Uuid::new_v4().to_string();
    let task_id = format!("issue-{}", item.github_number);

    if let Err(e) = workspace
        .ensure_cloned(&item.repo_url, &item.repo_name)
        .await
    {
        gh.label_remove(
            &item.repo_name,
            item.github_number,
            labels::IMPLEMENTING,
            gh_host,
        )
        .await;
        tracing::error!("clone failed for issue #{}: {e}", item.github_number);
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
            gh.label_remove(
                &item.repo_name,
                item.github_number,
                labels::IMPLEMENTING,
                gh_host,
            )
            .await;
            tracing::error!("worktree failed for issue #{}: {e}", item.github_number);
            ops.push(QueueOp::Remove);
            return TaskOutput {
                work_id,
                repo_name,
                queue_ops: ops,
                logs,
            };
        }
    };

    let repo_cfg = config::loader::load_merged(env, Some(&wt_path));
    let workflow = &repo_cfg.workflow.issue;
    let prompt = format!(
        "[autodev] implement: issue #{} in {}",
        item.github_number, item.repo_name
    );
    let system_prompt = format!("{AGENT_SYSTEM_PROMPT}\n\n{workflow}");

    let started = Utc::now().to_rfc3339();
    let result = claude
        .run_session(
            &wt_path,
            &prompt,
            &crate::infrastructure::agent::SessionOptions {
                append_system_prompt: Some(system_prompt),
                ..Default::default()
            },
        )
        .await;

    match result {
        Ok(res) => {
            let finished = Utc::now().to_rfc3339();
            let duration = chrono::Utc::now()
                .signed_duration_since(chrono::DateTime::parse_from_rfc3339(&started).unwrap())
                .num_milliseconds();

            logs.push(NewConsumerLog {
                repo_id: item.repo_id.clone(),
                queue_type: "issue".to_string(),
                queue_item_id: item.work_id.clone(),
                worker_id: worker_id.clone(),
                command: format!(
                    "claude -p \"{workflow} implement issue #{}\"",
                    item.github_number
                ),
                stdout: res.stdout.clone(),
                stderr: res.stderr.clone(),
                exit_code: res.exit_code,
                started_at: started,
                finished_at: finished,
                duration_ms: duration,
            });

            if res.exit_code == 0 {
                let head_branch = format!("autodev/issue-{}", item.github_number);
                let pr_number = match output::extract_pr_number(&res.stdout) {
                    Some(n) => Some(n),
                    None => find_existing_pr(gh, &item.repo_name, &head_branch, gh_host).await,
                };

                match pr_number {
                    Some(pr_num) => {
                        let pr_work_id = make_work_id("pr", &item.repo_name, pr_num);
                        gh.label_add(&item.repo_name, pr_num, labels::WIP, gh_host)
                            .await;

                        let pr_item = PrItem {
                            work_id: pr_work_id,
                            repo_id: item.repo_id.clone(),
                            repo_name: item.repo_name.clone(),
                            repo_url: item.repo_url.clone(),
                            github_number: pr_num,
                            title: format!("PR #{pr_num} (from issue #{})", item.github_number),
                            head_branch: String::new(),
                            base_branch: String::new(),
                            review_comment: None,
                            source_issue_number: Some(item.github_number),
                            review_iteration: 0,
                            gh_host: item.gh_host.clone(),
                        };

                        let pr_comment = format!(
                            "<!-- autodev:pr-link:{pr_num} -->\n\
                             Implementation PR #{pr_num} has been created and is awaiting review."
                        );
                        notifier
                            .post_issue_comment(
                                &item.repo_name,
                                item.github_number,
                                &pr_comment,
                                gh_host,
                            )
                            .await;

                        ops.push(QueueOp::Remove);
                        ops.push(QueueOp::PushPr {
                            phase: pr_phase::PENDING,
                            item: pr_item,
                        });
                        tracing::info!(
                            "issue #{}: PR #{pr_num} created, pushed to PR queue",
                            item.github_number
                        );
                    }
                    None => {
                        gh.label_remove(
                            &item.repo_name,
                            item.github_number,
                            labels::IMPLEMENTING,
                            gh_host,
                        )
                        .await;
                        tracing::warn!(
                            "issue #{}: PR number extraction failed, implementing removed",
                            item.github_number
                        );
                        ops.push(QueueOp::Remove);
                    }
                }
            } else {
                gh.label_remove(
                    &item.repo_name,
                    item.github_number,
                    labels::IMPLEMENTING,
                    gh_host,
                )
                .await;
                tracing::error!(
                    "implementation exited with {} for issue #{}",
                    res.exit_code,
                    item.github_number
                );
                ops.push(QueueOp::Remove);
            }
        }
        Err(e) => {
            gh.label_remove(
                &item.repo_name,
                item.github_number,
                labels::IMPLEMENTING,
                gh_host,
            )
            .await;
            tracing::error!(
                "implementation error for issue #{}: {e}",
                item.github_number
            );
            ops.push(QueueOp::Remove);
        }
    }

    let _ = workspace.remove_worktree(&item.repo_name, &task_id).await;
    TaskOutput {
        work_id,
        repo_name,
        queue_ops: ops,
        logs,
    }
}
