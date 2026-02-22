use anyhow::Result;
use async_trait::async_trait;

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

        let output = tokio::process::Command::new("gh")
            .args(&args)
            .output()
            .await
            .ok()?;

        if output.status.success() {
            Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
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

        let output = tokio::process::Command::new("gh")
            .args(&args)
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("gh api error: {stderr}");
        }

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
            "issue".to_string(),
            "comment".to_string(),
            number.to_string(),
            "--repo".to_string(),
            repo_name.to_string(),
            "--body".to_string(),
            body.to_string(),
        ];

        if let Some(h) = host {
            args.push("--hostname".to_string());
            args.push(h.to_string());
        }

        match tokio::process::Command::new("gh")
            .args(&args)
            .output()
            .await
        {
            Ok(output) => {
                if !output.status.success() {
                    tracing::warn!("gh issue comment failed for {repo_name}#{number}");
                }
                output.status.success()
            }
            Err(e) => {
                tracing::warn!("gh issue comment error: {e}");
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

        match tokio::process::Command::new("gh")
            .args(&args)
            .output()
            .await
        {
            Ok(output) => {
                if !output.status.success() {
                    tracing::warn!("gh label remove failed for {repo_name}#{number} label={label}");
                }
                output.status.success()
            }
            Err(e) => {
                tracing::warn!("gh label remove error: {e}");
                false
            }
        }
    }
}
