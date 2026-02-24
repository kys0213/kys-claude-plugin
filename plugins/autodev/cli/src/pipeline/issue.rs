use anyhow::Result;
use chrono::Utc;
use uuid::Uuid;

use crate::components::analyzer::Analyzer;
use crate::components::notifier::Notifier;
use crate::components::verdict;
use crate::components::workspace::Workspace;
use crate::config;
use crate::config::Env;
use crate::infrastructure::claude::output;
use crate::infrastructure::claude::Claude;
use crate::infrastructure::gh::Gh;
use crate::infrastructure::suggest_workflow::SuggestWorkflow;
use crate::queue::models::*;
use crate::queue::repository::*;
use crate::queue::task_queues::{issue_phase, labels, make_work_id, pr_phase, PrItem, TaskQueues};

/// ANALYZING/IMPLEMENTING 상태에서 아이템을 제거하는 헬퍼.
/// 모든 early-return 경로에서 호출하여 아이템이 중간 상태에 stuck되는 것을 방지한다.
fn remove_from_phase(queues: &mut TaskQueues, work_id: &str) {
    queues.issues.remove(work_id);
}
use crate::queue::Database;

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
// Phase 1: Pending → Analyzing → Ready / skip
// ═══════════════════════════════════════════════════

/// Pending 이슈를 pop하여 분석하고 verdict에 따라 분기
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
    let concurrency = cfg.consumer.issue_concurrency as usize;
    let gh_host = cfg.consumer.gh_host.as_deref();
    let analyzer = Analyzer::new(claude);

    for _ in 0..concurrency {
        let item = match queues.issues.pop(issue_phase::PENDING) {
            Some(item) => item,
            None => break,
        };

        // Pending → Analyzing 상태 전이 (TUI/status 가시성)
        let work_id = item.work_id.clone();
        queues.issues.push(issue_phase::ANALYZING, item.clone());
        tracing::debug!("issue #{}: Pending → Analyzing", item.github_number);

        // Pre-flight: GitHub에서 이슈가 아직 open인지 확인
        if !notifier
            .is_issue_open(&item.repo_name, item.github_number, gh_host)
            .await
        {
            remove_from_phase(queues, &work_id);
            gh.label_remove(&item.repo_name, item.github_number, labels::WIP, gh_host)
                .await;
            gh.label_add(&item.repo_name, item.github_number, labels::DONE, gh_host)
                .await;
            tracing::info!(
                "issue #{} is closed on GitHub, skipping",
                item.github_number
            );
            continue;
        }

        let worker_id = Uuid::new_v4().to_string();
        let task_id = format!("issue-{}", item.github_number);

        if let Err(e) = workspace
            .ensure_cloned(&item.repo_url, &item.repo_name)
            .await
        {
            remove_from_phase(queues, &work_id);
            gh.label_remove(&item.repo_name, item.github_number, labels::WIP, gh_host)
                .await;
            tracing::error!("clone failed for issue #{}: {e}", item.github_number);
            continue;
        }

        let wt_path = match workspace
            .create_worktree(&item.repo_name, &task_id, None)
            .await
        {
            Ok(p) => p,
            Err(e) => {
                remove_from_phase(queues, &work_id);
                gh.label_remove(&item.repo_name, item.github_number, labels::WIP, gh_host)
                    .await;
                tracing::error!("worktree failed for issue #{}: {e}", item.github_number);
                continue;
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
        let result = analyzer.analyze(&wt_path, &prompt).await;

        match result {
            Ok(res) => {
                let finished = Utc::now().to_rfc3339();
                let duration = chrono::Utc::now()
                    .signed_duration_since(chrono::DateTime::parse_from_rfc3339(&started).unwrap())
                    .num_milliseconds();

                let _ = db.log_insert(&NewConsumerLog {
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
                    remove_from_phase(queues, &work_id);
                    gh.label_remove(&item.repo_name, item.github_number, labels::WIP, gh_host)
                        .await;
                    let _ = workspace.remove_worktree(&item.repo_name, &task_id).await;
                    continue;
                }

                match res.analysis {
                    Some(ref a) if a.verdict == output::Verdict::Wontfix => {
                        remove_from_phase(queues, &work_id);
                        let comment = verdict::format_wontfix_comment(a);
                        notifier
                            .post_issue_comment(
                                &item.repo_name,
                                item.github_number,
                                &comment,
                                gh_host,
                            )
                            .await;
                        gh.label_remove(&item.repo_name, item.github_number, labels::WIP, gh_host)
                            .await;
                        gh.label_add(&item.repo_name, item.github_number, labels::SKIP, gh_host)
                            .await;
                        tracing::info!("issue #{} → wontfix (skip)", item.github_number);
                        let _ = workspace.remove_worktree(&item.repo_name, &task_id).await;
                    }
                    Some(ref a)
                        if a.verdict == output::Verdict::NeedsClarification
                            || a.confidence < cfg.consumer.confidence_threshold =>
                    {
                        remove_from_phase(queues, &work_id);
                        let comment = verdict::format_clarification_comment(a);
                        notifier
                            .post_issue_comment(
                                &item.repo_name,
                                item.github_number,
                                &comment,
                                gh_host,
                            )
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
                        let _ = workspace.remove_worktree(&item.repo_name, &task_id).await;
                    }
                    Some(ref a) => {
                        // v2: Analyzing → analyzed 라벨 + 코멘트 + queue 이탈 (HITL 게이트)
                        let comment = verdict::format_analysis_comment(a);
                        notifier
                            .post_issue_comment(
                                &item.repo_name,
                                item.github_number,
                                &comment,
                                gh_host,
                            )
                            .await;
                        remove_from_phase(queues, &work_id);
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
                        let _ = workspace.remove_worktree(&item.repo_name, &task_id).await;
                    }
                    None => {
                        // v2: 파싱 실패 fallback도 analyzed 라벨 + queue 이탈
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
                            .post_issue_comment(
                                &item.repo_name,
                                item.github_number,
                                &comment,
                                gh_host,
                            )
                            .await;
                        remove_from_phase(queues, &work_id);
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
                        let _ = workspace.remove_worktree(&item.repo_name, &task_id).await;
                    }
                }
            }
            Err(e) => {
                remove_from_phase(queues, &work_id);
                gh.label_remove(&item.repo_name, item.github_number, labels::WIP, gh_host)
                    .await;
                let _ = workspace.remove_worktree(&item.repo_name, &task_id).await;
                tracing::error!("analysis error for issue #{}: {e}", item.github_number);
            }
        }
    }

    Ok(())
}

// ═══════════════════════════════════════════════════
// Phase 2: Ready → done
// ═══════════════════════════════════════════════════

/// Ready 상태 이슈를 pop하여 구현
pub async fn process_ready(
    db: &Database,
    env: &dyn Env,
    workspace: &Workspace<'_>,
    gh: &dyn Gh,
    claude: &dyn Claude,
    _sw: &dyn SuggestWorkflow,
    queues: &mut TaskQueues,
) -> Result<()> {
    let cfg = config::loader::load_merged(env, None);
    let concurrency = cfg.consumer.issue_concurrency as usize;
    let gh_host = cfg.consumer.gh_host.as_deref();

    for _ in 0..concurrency {
        let item = match queues.issues.pop(issue_phase::READY) {
            Some(item) => item,
            None => break,
        };

        // Ready → Implementing 상태 전이 (TUI/status 가시성)
        let work_id = item.work_id.clone();
        queues.issues.push(issue_phase::IMPLEMENTING, item.clone());
        tracing::debug!("issue #{}: Ready → Implementing", item.github_number);

        let worker_id = Uuid::new_v4().to_string();
        let task_id = format!("issue-{}", item.github_number);

        if let Err(e) = workspace
            .ensure_cloned(&item.repo_url, &item.repo_name)
            .await
        {
            remove_from_phase(queues, &work_id);
            gh.label_remove(
                &item.repo_name,
                item.github_number,
                labels::IMPLEMENTING,
                gh_host,
            )
            .await;
            tracing::error!("clone failed for issue #{}: {e}", item.github_number);
            continue;
        }
        let wt_path = match workspace
            .create_worktree(&item.repo_name, &task_id, None)
            .await
        {
            Ok(p) => p,
            Err(e) => {
                remove_from_phase(queues, &work_id);
                gh.label_remove(
                    &item.repo_name,
                    item.github_number,
                    labels::IMPLEMENTING,
                    gh_host,
                )
                .await;
                tracing::error!("worktree failed for issue #{}: {e}", item.github_number);
                continue;
            }
        };

        let repo_cfg = config::loader::load_merged(env, Some(&wt_path));
        let workflow = &repo_cfg.workflow.issue;
        let report = item.analysis_report.as_deref().unwrap_or("");
        let prompt = format!(
            "[autodev] implement: issue #{}\n\n\
             {workflow} implement based on analysis:\n\n{report}\n\n\
             This is for issue #{} in {}.",
            item.github_number, item.github_number, item.repo_name
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
                    // v2: PR 번호 추출 → PR queue에 push (Issue-PR 연동)
                    let pr_number = output::extract_pr_number(&res.stdout);

                    match pr_number {
                        Some(pr_num) => {
                            // PR queue에 push (source_issue_number 설정)
                            let pr_work_id = make_work_id("pr", &item.repo_name, pr_num);
                            if !queues.contains(&pr_work_id) {
                                let pr_item = PrItem {
                                    work_id: pr_work_id,
                                    repo_id: item.repo_id.clone(),
                                    repo_name: item.repo_name.clone(),
                                    repo_url: item.repo_url.clone(),
                                    github_number: pr_num,
                                    title: format!(
                                        "PR #{pr_num} (from issue #{})",
                                        item.github_number
                                    ),
                                    head_branch: String::new(), // PR scan에서 채워짐
                                    base_branch: String::new(),
                                    review_comment: None,
                                    source_issue_number: Some(item.github_number),
                                };
                                gh.label_add(&item.repo_name, pr_num, labels::WIP, gh_host)
                                    .await;
                                queues.prs.push(pr_phase::PENDING, pr_item);
                                tracing::info!(
                                    "issue #{}: PR #{pr_num} created, pushed to PR queue",
                                    item.github_number
                                );
                            }

                            // Issue: Implementing → done (PR pipeline이 이어서 처리)
                            remove_from_phase(queues, &work_id);
                            gh.label_remove(
                                &item.repo_name,
                                item.github_number,
                                labels::IMPLEMENTING,
                                gh_host,
                            )
                            .await;
                            gh.label_add(
                                &item.repo_name,
                                item.github_number,
                                labels::DONE,
                                gh_host,
                            )
                            .await;
                            tracing::info!(
                                "issue #{}: Implementing → done (PR #{pr_num} linked)",
                                item.github_number
                            );
                        }
                        None => {
                            // PR 번호 추출 실패 → issue done + 경고
                            remove_from_phase(queues, &work_id);
                            gh.label_remove(
                                &item.repo_name,
                                item.github_number,
                                labels::IMPLEMENTING,
                                gh_host,
                            )
                            .await;
                            gh.label_add(
                                &item.repo_name,
                                item.github_number,
                                labels::DONE,
                                gh_host,
                            )
                            .await;
                            tracing::warn!(
                                "issue #{}: Implementing → done (no PR number extracted)",
                                item.github_number
                            );
                        }
                    }
                } else {
                    remove_from_phase(queues, &work_id);
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
                }
            }
            Err(e) => {
                remove_from_phase(queues, &work_id);
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
            }
        }

        let _ = workspace.remove_worktree(&item.repo_name, &task_id).await;
    }

    Ok(())
}
