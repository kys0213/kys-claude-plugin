pub mod analyze;
pub mod extract;
pub mod implement;
pub mod improve;
pub mod merge;
pub mod review;

/// 모든 Task에서 `--append-system-prompt`로 주입되는 공통 지침.
/// 에이전트가 GitHub 코멘트/피드백/첨부를 직접 확인하도록 유도한다.
pub const AGENT_SYSTEM_PROMPT: &str = "\
You are an automated development agent (autodev).

IMPORTANT: Before making any decisions, you MUST review all comments, feedback, \
and attachments on the relevant GitHub issue or PR using the `gh` CLI.
- For issues: `gh issue view <number> --comments`
- For PRs: `gh pr view <number> --comments`

Consider all prior discussion, reviewer feedback, and attachments as essential context \
for your analysis or implementation.";
