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

/// `builtin:` 접두어 워크플로우를 내장 프롬프트로 해석한다.
/// 슬래시 커맨드(예: `/develop-workflow:develop-auto`)는 그대로 반환한다.
pub fn resolve_workflow(workflow: &str) -> &str {
    match workflow {
        "builtin:analyze-and-implement" => BUILTIN_ANALYZE_AND_IMPLEMENT,
        "builtin:analyze-only" => BUILTIN_ANALYZE_ONLY,
        "builtin:review" => BUILTIN_REVIEW,
        _ => workflow,
    }
}

const BUILTIN_ANALYZE_AND_IMPLEMENT: &str = "\
You are implementing a GitHub issue autonomously.

Workflow:
1. Read the issue details using `gh issue view <number> --comments`
2. Analyze the codebase to understand affected files and dependencies
3. Design a minimal, focused solution
4. Create a feature branch: `git checkout -b autodev/issue-<number>`
5. Implement the changes following existing code conventions
6. Run tests and linters; fix any failures
7. Commit with a conventional commit message: `fix|feat(<scope>): <description>`
8. Push the branch and create a PR: `gh pr create --title '<title>' --body '<body>'`

Rules:
- Keep changes minimal and focused on the issue
- Follow existing code patterns and conventions
- Ensure all tests pass before creating the PR
- Include the issue number in the PR body (Closes #<number>)";

const BUILTIN_ANALYZE_ONLY: &str = "\
You are analyzing a GitHub issue to produce a structured report.

Workflow:
1. Read the issue details using `gh issue view <number> --comments`
2. Analyze the codebase to identify affected files and dependencies
3. Produce a structured analysis report

Do NOT implement any code changes. Only analyze and report.";

const BUILTIN_REVIEW: &str = "\
You are reviewing a GitHub pull request.

Workflow:
1. Read the PR details using `gh pr view <number> --comments`
2. Review the diff: `gh pr diff <number>`
3. Analyze code quality, correctness, security, and performance
4. Provide a structured review with verdict (approve or request_changes)

Respond with JSON:
{
  \"verdict\": \"approve\" | \"request_changes\",
  \"summary\": \"concise review summary\"
}";
