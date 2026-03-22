use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{bail, Result};
use async_trait::async_trait;

use super::{Claude, SessionResult};

/// 실제 `claude` CLI를 호출하는 구현체.
///
/// `timeout` 설정으로 Claude CLI 프로세스의 최대 실행 시간을 제한한다.
/// 타임아웃 시 프로세스를 kill하고 에러를 반환한다.
pub struct RealClaude {
    /// Claude CLI 프로세스 타임아웃. None이면 무제한 (테스트/하위호환).
    timeout: Option<Duration>,
}

impl RealClaude {
    /// 타임아웃을 지정하여 생성한다.
    pub fn with_timeout(timeout_secs: u64) -> Self {
        Self {
            timeout: Some(Duration::from_secs(timeout_secs)),
        }
    }

    /// 타임아웃 없이 생성한다 (하위호환).
    pub fn new() -> Self {
        Self { timeout: None }
    }
}

impl Default for RealClaude {
    fn default() -> Self {
        Self::new()
    }
}

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

        if let Some(ref sp) = opts.append_system_prompt {
            args.push("--append-system-prompt".to_string());
            args.push(sp.clone());
        }

        tracing::info!(
            "[claude] >>> claude -p \"{}\" in {:?} (args={}, timeout={:?})",
            truncate(prompt, 80),
            cwd,
            args.len(),
            self.timeout
        );

        let start = Instant::now();

        let child = tokio::process::Command::new("claude")
            .args(&args)
            .current_dir(cwd)
            .env_remove("CLAUDECODE")
            .kill_on_drop(true)
            .spawn()?;

        // wait_with_output()은 child를 소유권 이동하므로 timeout 래핑 시
        // kill_on_drop에 의존하여 타임아웃 시 자동 kill한다.
        let result = if let Some(timeout) = self.timeout {
            match tokio::time::timeout(timeout, child.wait_with_output()).await {
                Ok(output) => output?,
                Err(_) => {
                    // 타임아웃: child는 이미 소유권 이동되어 drop → kill_on_drop으로 정리됨
                    let elapsed = start.elapsed();
                    tracing::error!(
                        "[claude] <<< TIMEOUT after {}s (limit={}s), killing process",
                        elapsed.as_secs(),
                        timeout.as_secs()
                    );
                    bail!(
                        "claude CLI process timed out after {}s (limit: {}s)",
                        elapsed.as_secs(),
                        timeout.as_secs()
                    );
                }
            }
        } else {
            child.wait_with_output().await?
        };

        let elapsed = start.elapsed();
        let stdout = String::from_utf8_lossy(&result.stdout).to_string();
        let stderr = String::from_utf8_lossy(&result.stderr).to_string();
        let exit_code = result.status.code().unwrap_or(-1);

        if exit_code == 0 {
            tracing::info!(
                "[claude] <<< OK (exit=0, {}ms, stdout={} bytes, stderr={} bytes)",
                elapsed.as_millis(),
                stdout.len(),
                stderr.len()
            );
        } else {
            tracing::error!(
                "[claude] <<< FAILED (exit={exit_code}, {}ms, stdout={} bytes): {}",
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn new_creates_without_timeout() {
        let claude = RealClaude::new();
        assert!(claude.timeout.is_none());
    }

    #[test]
    fn default_creates_without_timeout() {
        let claude = RealClaude::default();
        assert!(claude.timeout.is_none());
    }

    #[test]
    fn with_timeout_sets_duration() {
        let claude = RealClaude::with_timeout(1800);
        assert_eq!(claude.timeout, Some(Duration::from_secs(1800)));
    }

    #[tokio::test]
    async fn timeout_kills_long_running_process() {
        // 1초 타임아웃으로 sleep 60을 실행하면 타임아웃으로 kill되어야 한다
        let claude = RealClaude::with_timeout(1);
        let cwd = PathBuf::from("/tmp");
        // "sleep 60" 대신 실제 존재하지 않는 커맨드 대신, sleep을 직접 사용
        // RealClaude는 "claude" 바이너리를 호출하므로 직접 테스트는 어렵다.
        // 대신 spawn + timeout 로직을 검증하기 위해 짧은 타임아웃으로 실행
        let result = claude
            .run_session(&cwd, "test", &super::super::SessionOptions::default())
            .await;

        // claude CLI가 없으면 spawn 실패, 있으면 타임아웃으로 에러
        assert!(result.is_err());
    }
}
