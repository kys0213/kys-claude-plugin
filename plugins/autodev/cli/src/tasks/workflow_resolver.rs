//! WorkflowResolver — config 값을 system prompt 텍스트로 변환.
//!
//! `"builtin"` prefix → autodev 내장 에이전트 위임 지시문
//! 그 외 → 커스텀 슬래시 커맨드 (패스스루)

/// 워크플로우 대상 구분.
pub enum TaskType {
    Issue,
    Pr,
}

const BUILTIN_PREFIX: &str = "builtin";

/// config에 저장된 워크플로우 값을 system prompt 텍스트로 변환한다.
///
/// - `"builtin"` 또는 `"builtin:*"` → 내장 에이전트 위임 지시문
/// - 그 외 → 커스텀 슬래시 커맨드 그대로 반환
pub fn resolve_workflow_prompt(config_value: &str, task_type: TaskType) -> String {
    if config_value.starts_with(BUILTIN_PREFIX) {
        match task_type {
            TaskType::Issue => "\
You MUST delegate this task to the `autodev:issue-analyzer` agent \
using the Agent tool with subagent_type=\"autodev:issue-analyzer\". \
Pass all issue context (number, repo, comments) to the agent. \
Do not attempt to perform the analysis yourself."
                .to_string(),
            TaskType::Pr => "\
You MUST delegate this task to the `autodev:pr-reviewer` agent \
using the Agent tool with subagent_type=\"autodev:pr-reviewer\". \
Pass all PR context (number, repo, diff, comments) to the agent. \
Do not attempt to perform the review yourself."
                .to_string(),
        }
    } else {
        // 커스텀 슬래시 커맨드 — 그대로 반환
        config_value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_builtin_issue() {
        let result = resolve_workflow_prompt("builtin", TaskType::Issue);
        assert!(result.contains("autodev:issue-analyzer"));
        assert!(result.contains("Agent tool"));
    }

    #[test]
    fn resolve_builtin_pr() {
        let result = resolve_workflow_prompt("builtin", TaskType::Pr);
        assert!(result.contains("autodev:pr-reviewer"));
        assert!(result.contains("Agent tool"));
    }

    #[test]
    fn resolve_custom_command() {
        let custom = "/develop-workflow:multi-review";
        let result = resolve_workflow_prompt(custom, TaskType::Pr);
        assert_eq!(result, custom);
    }

    #[test]
    fn resolve_builtin_prefix() {
        // "builtin:v2" 등 prefix 확장성
        let result = resolve_workflow_prompt("builtin:v2", TaskType::Issue);
        assert!(result.contains("autodev:issue-analyzer"));
    }
}
