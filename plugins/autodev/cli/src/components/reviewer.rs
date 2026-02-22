use std::path::Path;

use anyhow::Result;

use crate::infrastructure::claude::output;
use crate::infrastructure::claude::Claude;

/// PR 리뷰 실행 — Claude 세션을 통한 코드 리뷰
pub struct Reviewer<'a> {
    claude: &'a dyn Claude,
}

/// 리뷰 실행 결과
pub struct ReviewOutput {
    /// 파싱된 리뷰 텍스트
    pub review: String,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

impl<'a> Reviewer<'a> {
    pub fn new(claude: &'a dyn Claude) -> Self {
        Self { claude }
    }

    /// PR 리뷰 실행
    ///
    /// `prompt`로 Claude 세션을 실행하고 JSON 출력을 파싱하여
    /// 구조화된 `ReviewOutput`을 반환한다.
    pub async fn review_pr(&self, wt_path: &Path, prompt: &str) -> Result<ReviewOutput> {
        let result = self
            .claude
            .run_session(wt_path, prompt, Some("json"))
            .await?;

        let review = if result.exit_code == 0 {
            output::parse_output(&result.stdout)
        } else {
            String::new()
        };

        Ok(ReviewOutput {
            review,
            stdout: result.stdout,
            stderr: result.stderr,
            exit_code: result.exit_code,
        })
    }
}
