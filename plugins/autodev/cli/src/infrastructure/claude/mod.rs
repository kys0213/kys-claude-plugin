pub mod mock;
pub mod output;
pub mod real;

use std::path::Path;

use anyhow::Result;
use async_trait::async_trait;

pub use real::RealClaude;

/// claude -p 세션 결과
#[derive(Debug)]
pub struct SessionResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// Claude CLI 세션 옵션
#[derive(Debug, Default)]
pub struct SessionOptions {
    /// --output-format 값 (e.g. "json", "stream-json")
    pub output_format: Option<String>,
    /// --json-schema 값 (JSON schema 문자열)
    pub json_schema: Option<String>,
    /// --append-system-prompt 값 (행동 지침을 system prompt로 주입)
    pub append_system_prompt: Option<String>,
}

/// Claude CLI 추상화
#[async_trait]
pub trait Claude: Send + Sync {
    /// `claude -p "{prompt}" [--output-format {fmt}] [--json-schema {schema}]` in cwd
    async fn run_session(
        &self,
        cwd: &Path,
        prompt: &str,
        opts: &SessionOptions,
    ) -> Result<SessionResult>;
}
