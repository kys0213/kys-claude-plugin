use anyhow::Result;
use chrono::Utc;
use uuid::Uuid;

use crate::active::ActiveItems;
use crate::config;
use crate::config::Env;
use crate::queue::models::*;
use crate::queue::repository::*;
use crate::queue::Database;
use crate::session;
use crate::workspace;

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
// Phase 1: pending → analyzing → ready / waiting_human / done
// ═══════════════════════════════════════════════════

/// pending 이슈를 분석하고 verdict에 따라 분기
pub async fn process_pending(db: &Database, env: &dyn Env, active: &mut ActiveItems) -> Result<()> {
    let cfg = config::loader::load_merged(env, None);
    let items = db.issue_find_pending(cfg.consumer.issue_concurrency)?;

    for item in items {
        // Pre-flight: GitHub에서 이슈가 아직 open인지 확인
        if !super::github::is_issue_open(&item.repo_name, item.github_number, cfg.consumer.gh_host.as_deref()).await {
            db.issue_update_status(&item.id, "done", &StatusFields::default())?;
            active.remove("issue", &item.repo_id, item.github_number);
            tracing::info!("issue #{} is closed on GitHub, skipping", item.github_number);
            continue;
        }

        let worker_id = Uuid::new_v4().to_string();

        // status → analyzing
        db.issue_update_status(
            &item.id,
            "analyzing",
            &StatusFields {
                worker_id: Some(worker_id.clone()),
                ..Default::default()
            },
        )?;

        // 워크스페이스 준비
        let task_id = format!("issue-{}", item.github_number);
        if let Err(e) = workspace::ensure_cloned(env, &item.repo_url, &item.repo_name).await {
            db.issue_mark_failed(&item.id, &format!("clone failed: {e}"))?;
            continue;
        }

        let wt_path = match workspace::create_worktree(env, &item.repo_name, &task_id, None).await {
            Ok(p) => p,
            Err(e) => {
                db.issue_mark_failed(&item.id, &format!("worktree failed: {e}"))?;
                continue;
            }
        };

        // Multi-LLM 분석 실행
        let body_text = item.body.as_deref().unwrap_or("");
        let prompt = ANALYSIS_PROMPT_TEMPLATE
            .replace("{number}", &item.github_number.to_string())
            .replace("{title}", &item.title)
            .replace("{body}", body_text);

        let started = Utc::now().to_rfc3339();
        let result = session::run_claude(&wt_path, &prompt, Some("json")).await;

        match result {
            Ok(res) => {
                let finished = Utc::now().to_rfc3339();
                let duration = chrono::Utc::now()
                    .signed_duration_since(chrono::DateTime::parse_from_rfc3339(&started).unwrap())
                    .num_milliseconds();

                db.log_insert(&NewConsumerLog {
                    repo_id: item.repo_id.clone(),
                    queue_type: "issue".to_string(),
                    queue_item_id: item.id.clone(),
                    worker_id: worker_id.clone(),
                    command: format!("claude -p \"Analyze issue #{}...\"", item.github_number),
                    stdout: res.stdout.clone(),
                    stderr: res.stderr.clone(),
                    exit_code: res.exit_code,
                    started_at: started,
                    finished_at: finished,
                    duration_ms: duration,
                })?;

                if res.exit_code != 0 {
                    db.issue_mark_failed(
                        &item.id,
                        &format!("claude exited with {}", res.exit_code),
                    )?;
                    let _ = workspace::remove_worktree(env, &item.repo_name, &task_id).await;
                    continue;
                }

                // verdict 분기
                let analysis = session::output::parse_analysis(&res.stdout);

                match analysis {
                    Some(ref a) if a.verdict == "wontfix" => {
                        // wontfix → 이슈 댓글 + done
                        let comment = format_wontfix_comment(a);
                        super::github::post_issue_comment(
                            &item.repo_name, item.github_number, &comment,
                            cfg.consumer.gh_host.as_deref(),
                        ).await;
                        db.issue_update_status(&item.id, "done", &StatusFields {
                            analysis_report: Some(a.report.clone()),
                            ..Default::default()
                        })?;
                        active.remove("issue", &item.repo_id, item.github_number);
                        tracing::info!("issue #{} → wontfix", item.github_number);
                        let _ = workspace::remove_worktree(env, &item.repo_name, &task_id).await;
                    }
                    Some(ref a)
                        if a.verdict == "needs_clarification"
                            || a.confidence < cfg.consumer.confidence_threshold =>
                    {
                        // needs_clarification 또는 low confidence → 댓글 + waiting_human
                        let comment = format_clarification_comment(a);
                        super::github::post_issue_comment(
                            &item.repo_name, item.github_number, &comment,
                            cfg.consumer.gh_host.as_deref(),
                        ).await;
                        db.issue_update_status(&item.id, "waiting_human", &StatusFields {
                            analysis_report: Some(a.report.clone()),
                            ..Default::default()
                        })?;
                        tracing::info!(
                            "issue #{} → waiting_human (verdict={}, confidence={:.2})",
                            item.github_number, a.verdict, a.confidence
                        );
                        let _ = workspace::remove_worktree(env, &item.repo_name, &task_id).await;
                    }
                    Some(ref a) => {
                        // implement + high confidence → ready (구현은 process_ready가 처리)
                        db.issue_update_status(&item.id, "ready", &StatusFields {
                            analysis_report: Some(a.report.clone()),
                            ..Default::default()
                        })?;
                        tracing::info!(
                            "issue #{} → ready (confidence={:.2})",
                            item.github_number, a.confidence
                        );
                        // worktree 유지 — process_ready에서 사용
                    }
                    None => {
                        // 파싱 실패 — fallback: 기존 동작 (무조건 ready)
                        let report = session::output::parse_output(&res.stdout);
                        db.issue_update_status(&item.id, "ready", &StatusFields {
                            analysis_report: Some(report),
                            ..Default::default()
                        })?;
                        tracing::warn!(
                            "issue #{} analysis output not parseable, fallback → ready",
                            item.github_number
                        );
                    }
                }
            }
            Err(e) => {
                db.issue_mark_failed(&item.id, &format!("session error: {e}"))?;
                let _ = workspace::remove_worktree(env, &item.repo_name, &task_id).await;
            }
        }
    }

    Ok(())
}

// ═══════════════════════════════════════════════════
// Phase 2: ready → processing → done
// ═══════════════════════════════════════════════════

/// ready 상태 이슈를 구현
pub async fn process_ready(db: &Database, env: &dyn Env, active: &mut ActiveItems) -> Result<()> {
    let cfg = config::loader::load_merged(env, None);
    let items = db.issue_find_ready(cfg.consumer.issue_concurrency)?;

    for item in items {
        // Pre-flight
        if !super::github::is_issue_open(&item.repo_name, item.github_number, cfg.consumer.gh_host.as_deref()).await {
            db.issue_update_status(&item.id, "done", &StatusFields::default())?;
            active.remove("issue", &item.repo_id, item.github_number);
            continue;
        }

        let worker_id = Uuid::new_v4().to_string();
        db.issue_update_status(&item.id, "processing", &StatusFields {
            worker_id: Some(worker_id.clone()),
            ..Default::default()
        })?;

        // worktree 확보 (분석 때 생성된 것 재사용 or 신규)
        let task_id = format!("issue-{}", item.github_number);
        if let Err(e) = workspace::ensure_cloned(env, &item.repo_url, &item.repo_name).await {
            db.issue_mark_failed(&item.id, &format!("clone failed: {e}"))?;
            continue;
        }
        let wt_path = match workspace::create_worktree(env, &item.repo_name, &task_id, None).await {
            Ok(p) => p,
            Err(e) => {
                db.issue_mark_failed(&item.id, &format!("worktree failed: {e}"))?;
                continue;
            }
        };

        // 레포별 설정 로드
        let repo_cfg = config::loader::load_merged(env, Some(&wt_path));
        let workflow = &repo_cfg.workflow.issue;
        let report = item.analysis_report.as_deref().unwrap_or("");
        let prompt = format!(
            "{workflow} implement based on analysis:\n\n{report}\n\n\
             This is for issue #{} in {}.",
            item.github_number, item.repo_name
        );

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
                    queue_type: "issue".to_string(),
                    queue_item_id: item.id.clone(),
                    worker_id: worker_id.clone(),
                    command: format!("claude -p \"{workflow} implement issue #{}\"", item.github_number),
                    stdout: res.stdout.clone(),
                    stderr: res.stderr.clone(),
                    exit_code: res.exit_code,
                    started_at: started,
                    finished_at: finished,
                    duration_ms: duration,
                })?;

                if res.exit_code == 0 {
                    db.issue_update_status(&item.id, "done", &StatusFields::default())?;
                    active.remove("issue", &item.repo_id, item.github_number);
                    tracing::info!("issue #{} implementation complete", item.github_number);
                } else {
                    db.issue_mark_failed(
                        &item.id,
                        &format!("implementation exited with {}", res.exit_code),
                    )?;
                }
            }
            Err(e) => {
                db.issue_mark_failed(&item.id, &format!("implementation error: {e}"))?;
            }
        }

        // 구현 완료 후 worktree 정리
        let _ = workspace::remove_worktree(env, &item.repo_name, &task_id).await;
    }

    Ok(())
}

// ─── 댓글 포맷 ───

fn format_wontfix_comment(a: &session::output::AnalysisResult) -> String {
    let reason = a.reason.as_deref().unwrap_or("No additional details provided.");
    format!(
        "<!-- autodev:wontfix -->\n\
         ## Autodev Analysis\n\n\
         **Verdict**: Won't fix\n\n\
         **Summary**: {}\n\n\
         **Reason**: {reason}",
        a.summary
    )
}

fn format_clarification_comment(a: &session::output::AnalysisResult) -> String {
    let mut comment = format!(
        "<!-- autodev:waiting -->\n\
         ## Autodev Analysis\n\n\
         **Summary**: {}\n\n\
         This issue needs clarification before implementation can proceed.\n\n",
        a.summary
    );

    if !a.questions.is_empty() {
        comment.push_str("**Questions**:\n");
        for (i, q) in a.questions.iter().enumerate() {
            comment.push_str(&format!("{}. {q}\n", i + 1));
        }
    }

    comment
}
