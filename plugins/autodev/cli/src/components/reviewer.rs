use std::path::Path;

use anyhow::Result;

use crate::infrastructure::claude::output;
use crate::infrastructure::claude::output::ReviewVerdict;
use crate::infrastructure::claude::{Claude, SessionOptions};

/// PR 리뷰 실행 — Claude 세션을 통한 코드 리뷰
pub struct Reviewer<'a> {
    claude: &'a dyn Claude,
}

/// 리뷰 실행 결과
pub struct ReviewOutput {
    /// 파싱된 리뷰 텍스트
    pub review: String,
    /// JSON verdict 파싱 결과 (파싱 실패 시 None → 호출측에서 exit_code 기반 fallback)
    pub verdict: Option<ReviewVerdict>,
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
    ///
    /// verdict 파싱 우선순위:
    /// 1. ReviewResult JSON → verdict + summary
    /// 2. fallback → parse_output (기존 텍스트) + verdict=None
    pub async fn review_pr(
        &self,
        wt_path: &Path,
        prompt: &str,
        system_prompt: Option<&str>,
    ) -> Result<ReviewOutput> {
        let result = self
            .claude
            .run_session(
                wt_path,
                prompt,
                &SessionOptions {
                    output_format: Some("json".into()),
                    json_schema: Some(output::REVIEW_SCHEMA.clone()),
                    append_system_prompt: system_prompt.map(String::from),
                },
            )
            .await?;

        let (review, verdict) = if result.exit_code == 0 {
            match output::parse_review(&result.stdout) {
                Some(r) => (r.summary, Some(r.verdict)),
                None => (output::parse_output(&result.stdout), None),
            }
        } else {
            (String::new(), None)
        };

        Ok(ReviewOutput {
            review,
            verdict,
            stdout: result.stdout,
            stderr: result.stderr,
            exit_code: result.exit_code,
        })
    }
}
