use anyhow::Result;
use chrono::Utc;
use uuid::Uuid;

use crate::queue::models::*;
use crate::queue::repository::*;
use crate::queue::Database;
use crate::session;
use crate::workspace;

/// pending 이슈 처리
pub async fn process_pending(db: &Database) -> Result<()> {
    // concurrency 설정에 따라 처리할 이슈 수 결정
    let items = db.issue_find_pending(5)?;

    for item in items {
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
        match workspace::ensure_cloned(&item.repo_url, &item.repo_name).await {
            Ok(_) => {}
            Err(e) => {
                db.issue_mark_failed(&item.id, &format!("clone failed: {e}"))?;
                continue;
            }
        }

        let wt_path = match workspace::create_worktree(&item.repo_name, &task_id, None).await {
            Ok(p) => p,
            Err(e) => {
                db.issue_mark_failed(&item.id, &format!("worktree failed: {e}"))?;
                continue;
            }
        };

        // 1단계: Multi-LLM 분석
        let body_text = item.body.as_deref().unwrap_or("");
        let prompt = format!(
            "Analyze issue #{}: {}\n\n{body_text}\n\n\
             Provide a structured analysis report with: summary, affected files, \
             implementation direction, checkpoints, and risks.",
            item.github_number, item.title
        );

        let started = Utc::now().to_rfc3339();

        let result = session::run_claude(&wt_path, &prompt, Some("json")).await;

        match result {
            Ok(res) => {
                let finished = Utc::now().to_rfc3339();
                let duration = chrono::Utc::now()
                    .signed_duration_since(chrono::DateTime::parse_from_rfc3339(&started).unwrap())
                    .num_milliseconds();

                // 로그 기록
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

                if res.exit_code == 0 {
                    let report = session::output::parse_output(&res.stdout);
                    db.issue_update_status(
                        &item.id,
                        "ready",
                        &StatusFields {
                            analysis_report: Some(report.clone()),
                            ..Default::default()
                        },
                    )?;
                    tracing::info!("issue #{} analysis complete", item.github_number);

                    // 2단계: 구현
                    process_ready_issue(
                        db,
                        &item.id,
                        &item.repo_name,
                        &item.repo_id,
                        &worker_id,
                        item.github_number,
                        &report,
                        &wt_path,
                    )
                    .await?;
                } else {
                    db.issue_mark_failed(
                        &item.id,
                        &format!("claude exited with {}", res.exit_code),
                    )?;
                }
            }
            Err(e) => {
                db.issue_mark_failed(&item.id, &format!("session error: {e}"))?;
            }
        }
    }

    Ok(())
}

async fn process_ready_issue(
    db: &Database,
    item_id: &str,
    repo_name: &str,
    repo_id: &str,
    worker_id: &str,
    issue_num: i64,
    report: &str,
    wt_path: &std::path::Path,
) -> Result<()> {
    db.issue_update_status(item_id, "processing", &StatusFields::default())?;

    let prompt = format!(
        "/develop implement based on analysis:\n\n{report}\n\nThis is for issue #{issue_num} in {repo_name}."
    );

    let started = Utc::now().to_rfc3339();

    let result = session::run_claude(wt_path, &prompt, None).await;

    match result {
        Ok(res) => {
            let finished = Utc::now().to_rfc3339();
            let duration = chrono::Utc::now()
                .signed_duration_since(chrono::DateTime::parse_from_rfc3339(&started).unwrap())
                .num_milliseconds();

            db.log_insert(&NewConsumerLog {
                repo_id: repo_id.to_string(),
                queue_type: "issue".to_string(),
                queue_item_id: item_id.to_string(),
                worker_id: worker_id.to_string(),
                command: format!("claude -p \"/develop implement issue #{issue_num}\""),
                stdout: res.stdout.clone(),
                stderr: res.stderr.clone(),
                exit_code: res.exit_code,
                started_at: started,
                finished_at: finished,
                duration_ms: duration,
            })?;

            if res.exit_code == 0 {
                db.issue_update_status(item_id, "done", &StatusFields::default())?;
                tracing::info!("issue #{issue_num} implementation complete");
            } else {
                db.issue_mark_failed(
                    item_id,
                    &format!("implementation exited with {}", res.exit_code),
                )?;
            }
        }
        Err(e) => {
            db.issue_mark_failed(item_id, &format!("implementation error: {e}"))?;
        }
    }

    Ok(())
}
