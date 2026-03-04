use anyhow::Result;
use async_trait::async_trait;
use std::time::Instant;

use super::Gh;

/// 실제 `gh` CLI를 호출하는 구현체
pub struct RealGh;

#[async_trait]
impl Gh for RealGh {
    async fn api_get_field(
        &self,
        repo_name: &str,
        path: &str,
        jq: &str,
        host: Option<&str>,
    ) -> Option<String> {
        let mut args = vec![
            "api".to_string(),
            format!("repos/{repo_name}/{path}"),
            "--jq".to_string(),
            jq.to_string(),
        ];

        if let Some(h) = host {
            args.push("--hostname".to_string());
            args.push(h.to_string());
        }

        tracing::debug!("[gh:api_get_field] >>> gh {}", args.join(" "));
        let start = Instant::now();

        let output = tokio::process::Command::new("gh")
            .args(&args)
            .output()
            .await
            .ok()?;

        let elapsed = start.elapsed();

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            tracing::debug!(
                "[gh:api_get_field] <<< OK ({}ms, {} bytes)",
                elapsed.as_millis(),
                stdout.len()
            );
            Some(stdout)
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            tracing::warn!(
                "[gh:api_get_field] <<< FAILED (exit={}, {}ms): {}",
                output.status.code().unwrap_or(-1),
                elapsed.as_millis(),
                stderr.trim()
            );
            None
        }
    }

    async fn api_paginate(
        &self,
        repo_name: &str,
        endpoint: &str,
        params: &[(&str, &str)],
        host: Option<&str>,
    ) -> Result<Vec<u8>> {
        let mut args = vec![
            "api".to_string(),
            format!("repos/{repo_name}/{endpoint}"),
            "--paginate".to_string(),
            "--method".to_string(),
            "GET".to_string(),
        ];

        for (key, val) in params {
            args.push("-f".to_string());
            args.push(format!("{key}={val}"));
        }

        if let Some(h) = host {
            args.push("--hostname".to_string());
            args.push(h.to_string());
        }

        tracing::debug!("[gh:api_paginate] >>> gh {}", args.join(" "));
        let start = Instant::now();

        let output = tokio::process::Command::new("gh")
            .args(&args)
            .output()
            .await?;

        let elapsed = start.elapsed();

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            tracing::warn!(
                "[gh:api_paginate] <<< FAILED (exit={}, {}ms): {}",
                output.status.code().unwrap_or(-1),
                elapsed.as_millis(),
                stderr.trim()
            );
            anyhow::bail!("gh api error ({}ms): {stderr}", elapsed.as_millis());
        }

        tracing::debug!(
            "[gh:api_paginate] <<< OK ({}ms, {} bytes)",
            elapsed.as_millis(),
            output.stdout.len()
        );
        Ok(output.stdout)
    }

    async fn issue_comment(
        &self,
        repo_name: &str,
        number: i64,
        body: &str,
        host: Option<&str>,
    ) -> bool {
        let mut args = vec![
            "api".to_string(),
            format!("repos/{repo_name}/issues/{number}/comments"),
            "--method".to_string(),
            "POST".to_string(),
            "--silent".to_string(),
            "-f".to_string(),
            format!("body={body}"),
        ];

        if let Some(h) = host {
            args.push("--hostname".to_string());
            args.push(h.to_string());
        }

        tracing::debug!(
            "[gh:issue_comment] >>> gh api repos/{repo_name}/issues/{number}/comments (body={} bytes)",
            body.len()
        );
        let start = Instant::now();

        match tokio::process::Command::new("gh")
            .args(&args)
            .output()
            .await
        {
            Ok(output) => {
                let elapsed = start.elapsed();
                if output.status.success() {
                    tracing::debug!("[gh:issue_comment] <<< OK ({}ms)", elapsed.as_millis());
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    tracing::warn!(
                        "[gh:issue_comment] <<< FAILED (exit={}, {}ms): {}",
                        output.status.code().unwrap_or(-1),
                        elapsed.as_millis(),
                        stderr.trim()
                    );
                }
                output.status.success()
            }
            Err(e) => {
                tracing::warn!(
                    "[gh:issue_comment] <<< ERROR ({}ms): {e}",
                    start.elapsed().as_millis()
                );
                false
            }
        }
    }

    async fn label_remove(
        &self,
        repo_name: &str,
        number: i64,
        label: &str,
        host: Option<&str>,
    ) -> bool {
        let mut args = vec![
            "api".to_string(),
            format!("repos/{repo_name}/issues/{number}/labels/{label}"),
            "--method".to_string(),
            "DELETE".to_string(),
            "--silent".to_string(),
        ];

        if let Some(h) = host {
            args.push("--hostname".to_string());
            args.push(h.to_string());
        }

        tracing::debug!("[gh:label_remove] >>> {repo_name}#{number} -{label}");
        let start = Instant::now();

        match tokio::process::Command::new("gh")
            .args(&args)
            .output()
            .await
        {
            Ok(output) => {
                let elapsed = start.elapsed();
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    let stderr_trimmed = stderr.trim();
                    // 404 = label already removed → treat as success
                    if stderr_trimmed.contains("HTTP 404") || stderr_trimmed.contains("Not Found") {
                        tracing::debug!(
                            "[gh:label_remove] label already removed ({}ms): {stderr_trimmed}",
                            elapsed.as_millis()
                        );
                        return true;
                    }
                    tracing::warn!(
                        "[gh:label_remove] <<< FAILED (exit={}, {}ms): {stderr_trimmed}",
                        output.status.code().unwrap_or(-1),
                        elapsed.as_millis(),
                    );
                }
                output.status.success()
            }
            Err(e) => {
                tracing::warn!(
                    "[gh:label_remove] <<< ERROR ({}ms): {e}",
                    start.elapsed().as_millis()
                );
                false
            }
        }
    }

    async fn label_add(
        &self,
        repo_name: &str,
        number: i64,
        label: &str,
        host: Option<&str>,
    ) -> bool {
        let mut args = vec![
            "api".to_string(),
            format!("repos/{repo_name}/issues/{number}/labels"),
            "--method".to_string(),
            "POST".to_string(),
            "--silent".to_string(),
            "-f".to_string(),
            format!("labels[]={label}"),
        ];

        if let Some(h) = host {
            args.push("--hostname".to_string());
            args.push(h.to_string());
        }

        tracing::debug!("[gh:label_add] >>> {repo_name}#{number} +{label}");
        let start = Instant::now();

        match tokio::process::Command::new("gh")
            .args(&args)
            .output()
            .await
        {
            Ok(output) => {
                let elapsed = start.elapsed();
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    tracing::warn!(
                        "[gh:label_add] <<< FAILED (exit={}, {}ms): {}",
                        output.status.code().unwrap_or(-1),
                        elapsed.as_millis(),
                        stderr.trim()
                    );
                }
                output.status.success()
            }
            Err(e) => {
                tracing::warn!(
                    "[gh:label_add] <<< ERROR ({}ms): {e}",
                    start.elapsed().as_millis()
                );
                false
            }
        }
    }

    async fn create_issue(
        &self,
        repo_name: &str,
        title: &str,
        body: &str,
        host: Option<&str>,
    ) -> bool {
        let mut args = vec![
            "api".to_string(),
            format!("repos/{repo_name}/issues"),
            "--method".to_string(),
            "POST".to_string(),
            "-f".to_string(),
            format!("title={title}"),
            "-f".to_string(),
            format!("body={body}"),
        ];

        if let Some(h) = host {
            args.push("--hostname".to_string());
            args.push(h.to_string());
        }

        tracing::debug!("[gh:create_issue] >>> {repo_name} title={title}");
        let start = Instant::now();

        match tokio::process::Command::new("gh")
            .args(&args)
            .output()
            .await
        {
            Ok(output) => {
                let elapsed = start.elapsed();
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    tracing::warn!(
                        "[gh:create_issue] <<< FAILED (exit={}, {}ms): {}",
                        output.status.code().unwrap_or(-1),
                        elapsed.as_millis(),
                        stderr.trim()
                    );
                }
                output.status.success()
            }
            Err(e) => {
                tracing::warn!(
                    "[gh:create_issue] <<< ERROR ({}ms): {e}",
                    start.elapsed().as_millis()
                );
                false
            }
        }
    }

    async fn pr_review(
        &self,
        repo_name: &str,
        number: i64,
        event: &str,
        body: &str,
        host: Option<&str>,
    ) -> bool {
        let review_body = match event {
            "REQUEST_CHANGES" if body.is_empty() => "Changes requested",
            _ => body,
        };

        let mut args = vec![
            "api".to_string(),
            format!("repos/{repo_name}/pulls/{number}/reviews"),
            "--method".to_string(),
            "POST".to_string(),
            "--silent".to_string(),
            "-f".to_string(),
            format!("event={event}"),
        ];

        if !review_body.is_empty() {
            args.push("-f".to_string());
            args.push(format!("body={review_body}"));
        }

        if let Some(h) = host {
            args.push("--hostname".to_string());
            args.push(h.to_string());
        }

        tracing::debug!("[gh:pr_review] >>> {repo_name}#{number} event={event}");
        let start = Instant::now();

        match tokio::process::Command::new("gh")
            .args(&args)
            .output()
            .await
        {
            Ok(output) => {
                let elapsed = start.elapsed();
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    tracing::warn!(
                        "[gh:pr_review] <<< FAILED (exit={}, {}ms): {}",
                        output.status.code().unwrap_or(-1),
                        elapsed.as_millis(),
                        stderr.trim()
                    );
                }
                output.status.success()
            }
            Err(e) => {
                tracing::warn!(
                    "[gh:pr_review] <<< ERROR ({}ms): {e}",
                    start.elapsed().as_millis()
                );
                false
            }
        }
    }

    async fn create_pr(
        &self,
        repo_name: &str,
        head: &str,
        base: &str,
        title: &str,
        body: &str,
        host: Option<&str>,
    ) -> Option<i64> {
        let mut args = vec![
            "api".to_string(),
            format!("repos/{repo_name}/pulls"),
            "--method".to_string(),
            "POST".to_string(),
            "-f".to_string(),
            format!("head={head}"),
            "-f".to_string(),
            format!("base={base}"),
            "-f".to_string(),
            format!("title={title}"),
            "-f".to_string(),
            format!("body={body}"),
            "--jq".to_string(),
            ".number".to_string(),
        ];

        if let Some(h) = host {
            args.push("--hostname".to_string());
            args.push(h.to_string());
        }

        tracing::debug!("[gh:create_pr] >>> {repo_name} {head} -> {base}");
        let start = Instant::now();

        match tokio::process::Command::new("gh")
            .args(&args)
            .output()
            .await
        {
            Ok(output) => {
                let elapsed = start.elapsed();
                if output.status.success() {
                    let num_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    tracing::debug!(
                        "[gh:create_pr] <<< OK ({}ms, pr={})",
                        elapsed.as_millis(),
                        num_str
                    );
                    num_str.parse::<i64>().ok()
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    tracing::warn!(
                        "[gh:create_pr] <<< FAILED (exit={}, {}ms): {}",
                        output.status.code().unwrap_or(-1),
                        elapsed.as_millis(),
                        stderr.trim()
                    );
                    None
                }
            }
            Err(e) => {
                tracing::warn!(
                    "[gh:create_pr] <<< ERROR ({}ms): {e}",
                    start.elapsed().as_millis()
                );
                None
            }
        }
    }
}
