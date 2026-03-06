//! WorkflowResolver — WorkflowStage를 system prompt 텍스트로 변환.
//!
//! `agent` 지정 → builtin 에이전트 위임 지시문
//! `command` 지정 → 커스텀 슬래시 커맨드 (패스스루)
//! 둘 다 미지정 → task_type별 기본 agent 사용

use crate::config::models::WorkflowStage;

/// 워크플로우 대상 구분.
pub enum TaskType {
    Analyze,
    Implement,
    Review,
}

/// task_type별 기본 builtin agent 이름.
fn default_agent(task_type: &TaskType) -> &'static str {
    match task_type {
        TaskType::Analyze => "autodev:issue-analyzer",
        TaskType::Implement => "autodev:issue-analyzer",
        TaskType::Review => "autodev:pr-reviewer",
    }
}

/// 이슈 분석/구현 위임 프롬프트 템플릿. `{agent_name}` 플레이스홀더 사용.
const ISSUE_PROMPT: &str = "You MUST delegate this task to the `{agent_name}` agent \
    using the Agent tool with subagent_type=\"{agent_name}\". \
    Pass all issue context (number, repo, comments) to the agent. \
    Do not attempt to perform the analysis yourself.";

/// PR 리뷰 위임 프롬프트 템플릿. `{agent_name}` 플레이스홀더 사용.
const REVIEW_PROMPT: &str = "You MUST delegate this task to the `{agent_name}` agent \
    using the Agent tool with subagent_type=\"{agent_name}\". \
    Pass all PR context (number, repo, diff, comments) to the agent. \
    Do not attempt to perform the review yourself.";

/// WorkflowStage를 system prompt 텍스트로 변환한다.
///
/// 우선순위:
/// 1. `command`가 Some → 커스텀 슬래시 커맨드 그대로 반환
/// 2. `agent`가 Some → 해당 agent 위임 지시문 생성
/// 3. 둘 다 None → task_type별 기본 agent 위임 지시문 생성
pub fn resolve_workflow_prompt(stage: &WorkflowStage, task_type: TaskType) -> String {
    // 커스텀 슬래시 커맨드 우선
    if let Some(ref cmd) = stage.command {
        return cmd.clone();
    }

    let agent_name = stage
        .agent
        .as_deref()
        .unwrap_or_else(|| default_agent(&task_type));

    let template = match task_type {
        TaskType::Analyze | TaskType::Implement => ISSUE_PROMPT,
        TaskType::Review => REVIEW_PROMPT,
    };

    template.replace("{agent_name}", agent_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_builtin_analyze() {
        let stage = WorkflowStage {
            agent: Some("autodev:issue-analyzer".into()),
            command: None,
        };
        let result = resolve_workflow_prompt(&stage, TaskType::Analyze);
        assert!(result.contains("autodev:issue-analyzer"));
        assert!(result.contains("Agent tool"));
    }

    #[test]
    fn resolve_builtin_implement() {
        let stage = WorkflowStage {
            agent: Some("autodev:issue-analyzer".into()),
            command: None,
        };
        let result = resolve_workflow_prompt(&stage, TaskType::Implement);
        assert!(result.contains("autodev:issue-analyzer"));
        assert!(result.contains("issue context"));
    }

    #[test]
    fn resolve_builtin_review() {
        let stage = WorkflowStage {
            agent: Some("autodev:pr-reviewer".into()),
            command: None,
        };
        let result = resolve_workflow_prompt(&stage, TaskType::Review);
        assert!(result.contains("autodev:pr-reviewer"));
        assert!(result.contains("PR context"));
    }

    #[test]
    fn resolve_custom_command() {
        let stage = WorkflowStage {
            agent: None,
            command: Some("/review:multi-review".into()),
        };
        let result = resolve_workflow_prompt(&stage, TaskType::Review);
        assert_eq!(result, "/review:multi-review");
    }

    #[test]
    fn resolve_command_takes_precedence_over_agent() {
        let stage = WorkflowStage {
            agent: Some("autodev:pr-reviewer".into()),
            command: Some("/custom-review".into()),
        };
        let result = resolve_workflow_prompt(&stage, TaskType::Review);
        assert_eq!(result, "/custom-review");
    }

    #[test]
    fn resolve_none_falls_back_to_default_agent() {
        let stage = WorkflowStage {
            agent: None,
            command: None,
        };
        let result = resolve_workflow_prompt(&stage, TaskType::Review);
        assert!(result.contains("autodev:pr-reviewer"));

        let result = resolve_workflow_prompt(&stage, TaskType::Analyze);
        assert!(result.contains("autodev:issue-analyzer"));

        let result = resolve_workflow_prompt(&stage, TaskType::Implement);
        assert!(result.contains("autodev:issue-analyzer"));
    }
}
