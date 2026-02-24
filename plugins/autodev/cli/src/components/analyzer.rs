use std::path::Path;

use anyhow::Result;

use crate::infrastructure::claude::output;
use crate::infrastructure::claude::Claude;

/// 이슈 분석 결과
pub struct AnalyzerOutput {
    /// 파싱된 분석 결과 (파싱 실패 시 None → 호출측에서 fallback)
    pub analysis: Option<output::AnalysisResult>,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// 이슈 분석 — Claude 세션을 통한 이슈 분석
pub struct Analyzer<'a> {
    claude: &'a dyn Claude,
}

impl<'a> Analyzer<'a> {
    pub fn new(claude: &'a dyn Claude) -> Self {
        Self { claude }
    }

    /// 이슈 분석 실행
    ///
    /// `prompt`로 Claude 세션을 실행하고 JSON 출력을 파싱하여
    /// 구조화된 `AnalyzerOutput`을 반환한다.
    pub async fn analyze(&self, wt_path: &Path, prompt: &str) -> Result<AnalyzerOutput> {
        let result = self
            .claude
            .run_session(wt_path, prompt, Some("json"))
            .await?;

        let analysis = if result.exit_code == 0 {
            output::parse_analysis(&result.stdout)
        } else {
            None
        };

        Ok(AnalyzerOutput {
            analysis,
            stdout: result.stdout,
            stderr: result.stderr,
            exit_code: result.exit_code,
        })
    }
}
