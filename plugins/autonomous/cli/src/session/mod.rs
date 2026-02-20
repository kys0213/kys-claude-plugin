pub mod output;

use std::path::Path;

use anyhow::Result;

/// claude -p 세션 실행
pub async fn run_claude(
    cwd: &Path,
    prompt: &str,
    output_format: Option<&str>,
) -> Result<SessionResult> {
    let mut args = vec!["-p".to_string(), prompt.to_string()];

    if let Some(fmt) = output_format {
        args.push("--output-format".to_string());
        args.push(fmt.to_string());
    }

    tracing::info!("running: claude -p \"{}\" in {:?}", truncate(prompt, 80), cwd);

    let result = tokio::process::Command::new("claude")
        .args(&args)
        .current_dir(cwd)
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

#[derive(Debug)]
pub struct SessionResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max {
        s
    } else {
        &s[..max]
    }
}
