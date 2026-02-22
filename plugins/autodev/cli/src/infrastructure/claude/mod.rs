pub mod mock;
pub mod output;
pub mod real;

use std::path::Path;

use anyhow::Result;
use async_trait::async_trait;

pub use mock::MockClaude;
pub use real::RealClaude;

/// claude -p 세션 결과
#[derive(Debug)]
pub struct SessionResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// Claude CLI 추상화
#[async_trait]
pub trait Claude: Send + Sync {
    /// `claude -p "{prompt}" [--output-format {fmt}]` in cwd
    async fn run_session(
        &self,
        cwd: &Path,
        prompt: &str,
        output_format: Option<&str>,
    ) -> Result<SessionResult>;
}
