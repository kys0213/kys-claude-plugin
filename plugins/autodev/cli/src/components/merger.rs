use std::path::Path;

use crate::infrastructure::claude::Claude;

/// 머지 실행 결과의 분류
pub enum MergeOutcome {
    /// 머지 성공
    Success,
    /// 충돌 발생 (stdout/stderr에 "conflict" 포함)
    Conflict,
    /// 머지 실패 (충돌 아닌 다른 이유)
    Failed { exit_code: i32 },
    /// Claude 세션 자체 실행 에러
    Error(String),
}

/// 머지 실행 결과
pub struct MergeOutput {
    pub outcome: MergeOutcome,
    pub stdout: String,
    pub stderr: String,
}

fn contains_conflict(s: &str) -> bool {
    s.to_ascii_lowercase().contains("conflict")
}

/// PR 머지 + 충돌 해결 — Claude 세션을 통한 머지 자동화
pub struct Merger<'a> {
    claude: &'a dyn Claude,
}

impl<'a> Merger<'a> {
    pub fn new(claude: &'a dyn Claude) -> Self {
        Self { claude }
    }

    /// PR 머지 실행
    ///
    /// `/git-utils:merge-pr {pr_number}` 커맨드로 머지를 시도하고
    /// 결과를 `MergeOutcome`으로 분류하여 반환한다.
    pub async fn merge_pr(&self, wt_path: &Path, pr_number: i64) -> MergeOutput {
        let prompt = format!("/git-utils:merge-pr {}", pr_number);

        match self.claude.run_session(wt_path, &prompt, None).await {
            Ok(res) => {
                let outcome = if res.exit_code == 0 {
                    MergeOutcome::Success
                } else if contains_conflict(&res.stdout) || contains_conflict(&res.stderr) {
                    MergeOutcome::Conflict
                } else {
                    MergeOutcome::Failed {
                        exit_code: res.exit_code,
                    }
                };

                MergeOutput {
                    outcome,
                    stdout: res.stdout,
                    stderr: res.stderr,
                }
            }
            Err(e) => MergeOutput {
                outcome: MergeOutcome::Error(e.to_string()),
                stdout: String::new(),
                stderr: String::new(),
            },
        }
    }

    /// 충돌 해결 시도
    ///
    /// 충돌 발생 후 Claude에게 해결을 요청한다.
    /// 성공 시 `MergeOutcome::Success`, 실패 시 `MergeOutcome::Failed` 반환.
    pub async fn resolve_conflicts(&self, wt_path: &Path, pr_number: i64) -> MergeOutput {
        let prompt = format!(
            "Resolve all merge conflicts for PR #{pr_number}. \
             Steps: 1) Run `git status` to find conflicting files. \
             2) For each file with conflict markers (<<<<<<< / ======= / >>>>>>>), \
             resolve by choosing the correct version or combining both changes. \
             3) `git add` each resolved file. \
             4) Run the project's tests to verify the resolution is correct. \
             5) `git commit` the merge resolution."
        );

        match self.claude.run_session(wt_path, &prompt, None).await {
            Ok(res) => {
                let outcome = if res.exit_code == 0 {
                    MergeOutcome::Success
                } else {
                    MergeOutcome::Failed {
                        exit_code: res.exit_code,
                    }
                };

                MergeOutput {
                    outcome,
                    stdout: res.stdout,
                    stderr: res.stderr,
                }
            }
            Err(e) => MergeOutput {
                outcome: MergeOutcome::Error(e.to_string()),
                stdout: String::new(),
                stderr: String::new(),
            },
        }
    }
}
