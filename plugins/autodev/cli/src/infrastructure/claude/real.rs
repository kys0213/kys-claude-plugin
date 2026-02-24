use std::path::Path;

use anyhow::Result;
use async_trait::async_trait;

use super::{Claude, SessionResult};

/// 실제 `claude` CLI를 호출하는 구현체
pub struct RealClaude;

#[async_trait]
impl Claude for RealClaude {
    async fn run_session(
        &self,
        cwd: &Path,
        prompt: &str,
        opts: &super::SessionOptions,
    ) -> Result<SessionResult> {
        let mut args = vec!["-p".to_string(), prompt.to_string()];

        if let Some(ref fmt) = opts.output_format {
            args.push("--output-format".to_string());
            args.push(fmt.clone());
        }

        if let Some(ref schema) = opts.json_schema {
            args.push("--json-schema".to_string());
            args.push(schema.clone());
        }

        tracing::info!(
            "running: claude -p \"{}\" in {:?}",
            truncate(prompt, 80),
            cwd
        );

        let result = tokio::process::Command::new("claude")
            .args(&args)
            .current_dir(cwd)
            .env_remove("CLAUDECODE")
            .output()
            .await?;

        let stdout = String::from_utf8_lossy(&result.stdout).to_string();
        let stderr = String::from_utf8_lossy(&result.stderr).to_string();
        let exit_code = result.status.code().unwrap_or(-1);

        if exit_code != 0 {
            tracing::warn!("claude session exited with code {exit_code}: {stderr}");
        }

        Ok(SessionResult {
            stdout,
            stderr,
            exit_code,
        })
    }
}

fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max {
        s
    } else {
        let mut end = max;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        &s[..end]
    }
}
