use anyhow::Result;
use chrono::Utc;
use uuid::Uuid;

use crate::components::notifier::Notifier;
use crate::components::verdict;
use crate::components::workspace::Workspace;
use crate::config;
use crate::config::Env;
use crate::infrastructure::claude::output;
use crate::infrastructure::claude::Claude;
use crate::infrastructure::gh::Gh;
use crate::queue::models::*;
use crate::queue::repository::*;
use crate::queue::task_queues::{issue_phase, labels, TaskQueues};
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

    for _ in 0..concurrency {
        let mut item = match queues.issues.pop(issue_phase::PENDING) {
            Some(item) => item,
            None => break,
        };

        // Pre-flight: GitHub에서 이슈가 아직 open인지 확인
        if !notifier
            .is_issue_open(&item.repo_name, item.github_number, gh_host)
            .await
        {
            gh.label_remove(&item.repo_name, item.github_number, labels::WIP, gh_host).await;
            gh.label_add(&item.repo_name, item.github_number, labels::DONE, gh_host).await;
            tracing::info!("issue #{} is closed on GitHub, skipping", item.github_number);
            continue;
        }

        let worker_id = Uuid::new_v4().to_string();
        let task_id = format!("issue-{}", item.github_number);

        if let Err(e) = workspace.ensure_cloned(&item.repo_url, &item.repo_name).await {
            gh.label_remove(&item.repo_name, item.github_number, labels::WIP, gh_host).await;
            tracing::error!("clone failed for issue #{}: {e}", item.github_number);
            continue;
        }

        let wt_path = match workspace.create_worktree(&item.repo_name, &task_id, None).await {
            Ok(p) => p,
            Err(e) => {
                gh.label_remove(&item.repo_name, item.github_number, labels::WIP, gh_host).await;
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
        let result = claude.run_session(&wt_path, &prompt, Some("json")).await;

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
                    gh.label_remove(&item.repo_name, item.github_number, labels::WIP, gh_host).await;
                    let _ = workspace.remove_worktree(&item.repo_name, &task_id).await;
                    continue;
                }

                let analysis = output::parse_analysis(&res.stdout);

                match analysis {
                    Some(ref a) if a.verdict == output::Verdict::Wontfix => {
                        let comment = verdict::format_wontfix_comment(a);
                        notifier.post_issue_comment(&item.repo_name, item.github_number, &comment, gh_host).await;
                        gh.label_remove(&item.repo_name, item.github_number, labels::WIP, gh_host).await;
                        gh.label_add(&item.repo_name, item.github_number, labels::SKIP, gh_host).await;
                        tracing::info!("issue #{} → wontfix (skip)", item.github_number);
                        let _ = workspace.remove_worktree(&item.repo_name, &task_id).await;
                    }
                    Some(ref a)
                        if a.verdict == output::Verdict::NeedsClarification
                            || a.confidence < cfg.consumer.confidence_threshold =>
                    {
                        let comment = verdict::format_clarification_comment(a);
                        notifier.post_issue_comment(&item.repo_name, item.github_number, &comment, gh_host).await;
                        gh.label_remove(&item.repo_name, item.github_number, labels::WIP, gh_host).await;
                        gh.label_add(&item.repo_name, item.github_number, labels::SKIP, gh_host).await;
                        tracing::info!("issue #{} → skip (verdict={}, confidence={:.2})", item.github_number, a.verdict, a.confidence);
                        let _ = workspace.remove_worktree(&item.repo_name, &task_id).await;
                    }
                    Some(ref a) => {
                        item.analysis_report = Some(a.report.clone());
                        queues.issues.push(issue_phase::READY, item);
                        tracing::info!("issue analysis → Ready (confidence={:.2})", a.confidence);
                    }
                    None => {
                        let report = output::parse_output(&res.stdout);
                        item.analysis_report = Some(report);
                        queues.issues.push(issue_phase::READY, item);
                        tracing::warn!("issue analysis output not parseable, fallback → Ready");
                    }
                }
            }
            Err(e) => {
                gh.label_remove(&item.repo_name, item.github_number, labels::WIP, gh_host).await;
                let _ = workspace.remove_worktree(&item.repo_name, &task_id).await;
                tracing::error!("session error for issue #{}: {e}", item.github_number);
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

        let worker_id = Uuid::new_v4().to_string();
        let task_id = format!("issue-{}", item.github_number);

        if let Err(e) = workspace.ensure_cloned(&item.repo_url, &item.repo_name).await {
            gh.label_remove(&item.repo_name, item.github_number, labels::WIP, gh_host).await;
            tracing::error!("clone failed for issue #{}: {e}", item.github_number);
            continue;
        }
        let wt_path = match workspace.create_worktree(&item.repo_name, &task_id, None).await {
            Ok(p) => p,
            Err(e) => {
                gh.label_remove(&item.repo_name, item.github_number, labels::WIP, gh_host).await;
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
        let result = claude.run_session(&wt_path, &prompt, None).await;

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
                    command: format!("claude -p \"{workflow} implement issue #{}\"", item.github_number),
                    stdout: res.stdout.clone(),
                    stderr: res.stderr.clone(),
                    exit_code: res.exit_code,
                    started_at: started,
                    finished_at: finished,
                    duration_ms: duration,
                });

                if res.exit_code == 0 {
                    // Knowledge extraction (best effort — 실패해도 done 전이는 유지)
                    if cfg.consumer.knowledge_extraction {
                        let _ = crate::knowledge::extractor::extract_task_knowledge(
                            claude, gh, &item.repo_name, item.github_number,
                            "issue", &wt_path, gh_host,
                        ).await;
                    }

                    gh.label_remove(&item.repo_name, item.github_number, labels::WIP, gh_host).await;
                    gh.label_add(&item.repo_name, item.github_number, labels::DONE, gh_host).await;
                    tracing::info!("issue #{} → done", item.github_number);
                } else {
                    gh.label_remove(&item.repo_name, item.github_number, labels::WIP, gh_host).await;
                    tracing::error!("implementation exited with {} for issue #{}", res.exit_code, item.github_number);
                }
            }
            Err(e) => {
                gh.label_remove(&item.repo_name, item.github_number, labels::WIP, gh_host).await;
                tracing::error!("implementation error for issue #{}: {e}", item.github_number);
            }
        }

        let _ = workspace.remove_worktree(&item.repo_name, &task_id).await;
    }

    Ok(())
}
