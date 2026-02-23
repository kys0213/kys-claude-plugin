use anyhow::{Context, Result};
use async_trait::async_trait;

use crate::knowledge::models::{RepetitionEntry, SessionEntry, ToolFrequencyEntry};

use super::SuggestWorkflow;

/// 실제 `suggest-workflow` CLI를 호출하는 구현체
pub struct RealSuggestWorkflow;

#[async_trait]
impl SuggestWorkflow for RealSuggestWorkflow {
    async fn query_tool_frequency(
        &self,
        session_filter: Option<&str>,
    ) -> Result<Vec<ToolFrequencyEntry>> {
        let mut args = vec![
            "query".to_string(),
            "--perspective".to_string(),
            "tool-frequency".to_string(),
        ];
        if let Some(sf) = session_filter {
            args.push("--session-filter".to_string());
            args.push(sf.to_string());
        }

        let stdout = run_suggest_workflow(&args).await?;
        let entries: Vec<ToolFrequencyEntry> =
            serde_json::from_str(&stdout).context("failed to parse tool-frequency response")?;
        Ok(entries)
    }

    async fn query_filtered_sessions(
        &self,
        prompt_pattern: &str,
        since: Option<&str>,
        top: Option<u32>,
    ) -> Result<Vec<SessionEntry>> {
        let mut args = vec![
            "query".to_string(),
            "--perspective".to_string(),
            "filtered-sessions".to_string(),
            "--param".to_string(),
            format!("prompt_pattern={prompt_pattern}"),
        ];
        if let Some(s) = since {
            args.push("--param".to_string());
            args.push(format!("since={s}"));
        }
        if let Some(t) = top {
            args.push("--param".to_string());
            args.push(format!("top={t}"));
        }

        let stdout = run_suggest_workflow(&args).await?;
        let entries: Vec<SessionEntry> =
            serde_json::from_str(&stdout).context("failed to parse filtered-sessions response")?;
        Ok(entries)
    }

    async fn query_repetition(
        &self,
        session_filter: Option<&str>,
    ) -> Result<Vec<RepetitionEntry>> {
        let mut args = vec![
            "query".to_string(),
            "--perspective".to_string(),
            "repetition".to_string(),
        ];
        if let Some(sf) = session_filter {
            args.push("--session-filter".to_string());
            args.push(sf.to_string());
        }

        let stdout = run_suggest_workflow(&args).await?;
        let entries: Vec<RepetitionEntry> =
            serde_json::from_str(&stdout).context("failed to parse repetition response")?;
        Ok(entries)
    }
}

/// suggest-workflow CLI 실행 공통 헬퍼
async fn run_suggest_workflow(args: &[String]) -> Result<String> {
    tracing::info!(
        "running: suggest-workflow {}",
        args.iter()
            .map(|a| if a.contains(' ') {
                format!("\"{a}\"")
            } else {
                a.clone()
            })
            .collect::<Vec<_>>()
            .join(" ")
    );

    let output = tokio::process::Command::new("suggest-workflow")
        .args(args)
        .output()
        .await
        .context("failed to execute suggest-workflow")?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        let code = output.status.code().unwrap_or(-1);
        tracing::warn!("suggest-workflow exited with {code}: {stderr}");
        anyhow::bail!("suggest-workflow exited with code {code}: {stderr}");
    }

    Ok(stdout)
}
