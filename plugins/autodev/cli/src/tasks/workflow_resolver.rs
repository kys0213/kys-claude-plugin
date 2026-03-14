//! WorkflowResolver — WorkflowStage를 system prompt 텍스트로 변환.
//!
//! task_type별 출력 스펙을 정의하고, 커스텀 슬래시 커맨드가 있으면 그대로 반환한다.
//! user prompt가 동적으로 분석/리뷰/구현 지시를 전달하고,
//! system prompt는 최종 출력 형식(JSON 스펙)과 절차를 정의한다.

use crate::core::config::models::WorkflowStage;

/// 워크플로우 대상 구분.
pub enum TaskType {
    Analyze,
    Implement,
    Review,
}

/// 분석 결과 출력 스펙.
///
/// `--output-format json` + `--json-schema`와 함께 사용된다.
/// user prompt의 분석 요청을 수행한 뒤, 결과를 JSON schema에 맞춰 응답해야 한다.
const ANALYZE_PROMPT: &str = "You are an issue analyzer. \
    Perform the requested analysis and respond with a JSON object matching the required schema. \
    Your response must contain: verdict, confidence, summary, questions, reason, report, and related_issues.";

/// 구현 절차 스펙.
///
/// 구현 완료 후 품질 리뷰를 거쳐 커밋/PR을 생성하는 절차를 정의한다.
const IMPLEMENT_PROMPT: &str = "You are an issue implementer. \
    Perform the requested implementation based on the issue context.\n\n\
    After completing the implementation, you MUST review the changes \
    for code quality before creating the PR:\n\
    1. Run `git diff` to see all changes\n\
    2. Review for code reuse (search for existing utilities that could replace new code)\n\
    3. Review for code quality (redundant state, copy-paste, stringly-typed code)\n\
    4. Review for efficiency (unnecessary work, missed concurrency, unbounded data)\n\
    5. Fix any issues found directly — do not just report them\n\
    6. Then proceed with commit and PR creation";

/// PR 리뷰 결과 출력 스펙.
///
/// `--output-format json` + `--json-schema`와 함께 사용된다.
/// user prompt의 리뷰 요청을 수행한 뒤, 결과를 JSON schema에 맞춰 응답해야 한다.
const REVIEW_PROMPT: &str = "You are a PR reviewer. \
    Perform the requested code review and respond with a JSON object matching the required schema. \
    Your response must contain: verdict, summary, and report.";

/// WorkflowStage를 system prompt 텍스트로 변환한다.
///
/// 우선순위:
/// 1. `command`가 Some → 커스텀 슬래시 커맨드 그대로 반환
/// 2. 그 외 → task_type별 출력 스펙 반환
pub fn resolve_workflow_prompt(stage: &WorkflowStage, task_type: TaskType) -> String {
    // 커스텀 슬래시 커맨드 우선
    if let Some(ref cmd) = stage.command {
        return cmd.clone();
    }

    match task_type {
        TaskType::Analyze => ANALYZE_PROMPT.to_string(),
        TaskType::Implement => IMPLEMENT_PROMPT.to_string(),
        TaskType::Review => REVIEW_PROMPT.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_analyze_returns_output_spec() {
        let stage = WorkflowStage::default();
        let result = resolve_workflow_prompt(&stage, TaskType::Analyze);
        assert!(result.contains("issue analyzer"));
        assert!(result.contains("JSON object"));
        assert!(result.contains("verdict"));
    }

    #[test]
    fn resolve_implement_returns_procedure() {
        let stage = WorkflowStage::default();
        let result = resolve_workflow_prompt(&stage, TaskType::Implement);
        assert!(result.contains("issue implementer"));
        assert!(result.contains("review the changes"));
        assert!(result.contains("code quality"));
    }

    #[test]
    fn resolve_review_returns_output_spec() {
        let stage = WorkflowStage::default();
        let result = resolve_workflow_prompt(&stage, TaskType::Review);
        assert!(result.contains("PR reviewer"));
        assert!(result.contains("JSON object"));
        assert!(result.contains("verdict"));
    }

    #[test]
    fn resolve_custom_command() {
        let stage = WorkflowStage {
            command: Some("/review:multi-review".into()),
        };
        let result = resolve_workflow_prompt(&stage, TaskType::Review);
        assert_eq!(result, "/review:multi-review");
    }

    #[test]
    fn resolve_with_command() {
        let stage = WorkflowStage {
            command: Some("/custom-review".into()),
        };
        let result = resolve_workflow_prompt(&stage, TaskType::Review);
        assert_eq!(result, "/custom-review");
    }
}
