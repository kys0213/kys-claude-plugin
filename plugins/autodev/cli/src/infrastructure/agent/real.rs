use std::path::Path;
use std::time::Instant;

use anyhow::Result;
use async_trait::async_trait;

use super::{Agent, SessionResult};

/// 실제 `claude` CLI를 호출하는 Agent 구현체
pub struct ClaudeAgent;

#[async_trait]
impl Agent for ClaudeAgent {
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

        if let Some(ref sp) = opts.append_system_prompt {
            args.push("--append-system-prompt".to_string());
            args.push(sp.clone());
        }

        tracing::info!(
            "[agent] >>> claude -p \"{}\" in {:?} (args={})",
            truncate(prompt, 80),
            cwd,
            args.len()
        );

        let start = Instant::now();

        let result = tokio::process::Command::new("claude")
            .args(&args)
            .current_dir(cwd)
            .env_remove("CLAUDECODE")
            .output()
            .await?;

        let elapsed = start.elapsed();
        let stdout = String::from_utf8_lossy(&result.stdout).to_string();
        let stderr = String::from_utf8_lossy(&result.stderr).to_string();
        let exit_code = result.status.code().unwrap_or(-1);

        if exit_code == 0 {
            tracing::info!(
                "[agent] <<< OK (exit=0, {}ms, stdout={} bytes, stderr={} bytes)",
                elapsed.as_millis(),
                stdout.len(),
                stderr.len()
            );
        } else {
            tracing::error!(
                "[agent] <<< FAILED (exit={exit_code}, {}ms, stdout={} bytes): {}",
                elapsed.as_millis(),
                stdout.len(),
                truncate(&stderr, 200)
            );
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
