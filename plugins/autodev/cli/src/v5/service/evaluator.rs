use std::path::Path;

use anyhow::Result;

use crate::v5::core::phase::V5QueuePhase;
use crate::v5::core::queue_item::V5QueueItem;

/// Completed 아이템의 평가 결과.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvalDecision {
    /// 완료 판정 → Done 전이 (on_done script 실행)
    Done,
    /// 사람 판단 필요 → HITL 전이
    Hitl { reason: String },
}

/// Completed 아이템을 스캔하여 Done 또는 HITL로 분류한다.
///
/// 실행 방식:
///   1. Cron evaluate job이 주기적으로 실행 (또는 force_trigger)
///   2. `autodev agent -p`로 LLM 호출
///   3. LLM이 `autodev queue done/hitl` CLI를 실행하여 전이
///
/// Evaluator는 이 흐름을 조율하는 구조체다.
pub struct Evaluator {
    workspace_name: String,
}

impl Evaluator {
    pub fn new(workspace_name: &str) -> Self {
        Self {
            workspace_name: workspace_name.to_string(),
        }
    }

    /// Completed 상태인 아이템을 필터링한다.
    pub fn filter_completed(items: &[V5QueueItem]) -> Vec<&V5QueueItem> {
        items
            .iter()
            .filter(|item| item.phase == V5QueuePhase::Completed)
            .collect()
    }

    /// 평가 결과를 아이템에 적용하여 목표 phase를 결정한다.
    pub fn target_phase(decision: &EvalDecision) -> V5QueuePhase {
        match decision {
            EvalDecision::Done => V5QueuePhase::Done,
            EvalDecision::Hitl { .. } => V5QueuePhase::Hitl,
        }
    }

    /// evaluate cron script를 생성한다.
    ///
    /// 이 script는 cron engine에 의해 주기적으로 실행되며,
    /// `autodev agent -p`를 호출하여 Completed 아이템을 평가한다.
    pub fn build_evaluate_script(&self) -> String {
        format!(
            r#"#!/bin/bash
# Guard: Only run when Completed items exist
COMPLETED=$(autodev queue list --state completed --json 2>/dev/null | jq 'length' 2>/dev/null)
if [ "$COMPLETED" = "0" ] || [ -z "$COMPLETED" ]; then exit 0; fi

# Execute: evaluate Completed items
autodev agent --workspace "{ws}" -p \
  "Completed 아이템의 완료 여부를 판단하고, autodev queue done 또는 autodev queue hitl 을 실행해줘"
"#,
            ws = self.workspace_name
        )
    }

    /// evaluate를 bash script로 실행한다.
    ///
    /// Cron이 아닌 즉시 실행(force_trigger) 시 사용.
    pub async fn run_evaluate(&self, autodev_home: &Path) -> Result<EvaluateResult> {
        let script = self.build_evaluate_script();
        let output = tokio::process::Command::new("bash")
            .arg("-c")
            .arg(&script)
            .env("WORKSPACE", &self.workspace_name)
            .env("AUTODEV_HOME", autodev_home.to_string_lossy().as_ref())
            .output()
            .await?;

        Ok(EvaluateResult {
            exit_code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    }
}

/// evaluate 실행 결과.
#[derive(Debug)]
pub struct EvaluateResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl EvaluateResult {
    pub fn success(&self) -> bool {
        self.exit_code == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v5::core::queue_item::testing::test_item;

    #[test]
    fn filter_completed_items() {
        let mut items = vec![
            test_item("s1", "analyze"),
            test_item("s2", "implement"),
            test_item("s3", "review"),
        ];
        items[1].phase = V5QueuePhase::Completed;

        let completed = Evaluator::filter_completed(&items);
        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0].work_id, "s2:implement");
    }

    #[test]
    fn target_phase_done() {
        assert_eq!(
            Evaluator::target_phase(&EvalDecision::Done),
            V5QueuePhase::Done
        );
    }

    #[test]
    fn target_phase_hitl() {
        let decision = EvalDecision::Hitl {
            reason: "needs review".to_string(),
        };
        assert_eq!(Evaluator::target_phase(&decision), V5QueuePhase::Hitl);
    }

    #[test]
    fn no_completed_items() {
        let items = vec![test_item("s1", "analyze"), test_item("s2", "implement")];
        let completed = Evaluator::filter_completed(&items);
        assert!(completed.is_empty());
    }

    #[test]
    fn build_evaluate_script_contains_workspace() {
        let evaluator = Evaluator::new("auth-project");
        let script = evaluator.build_evaluate_script();
        assert!(script.contains("auth-project"));
        assert!(script.contains("autodev agent"));
        assert!(script.contains("autodev queue done"));
        assert!(script.contains("autodev queue hitl"));
    }

    #[test]
    fn evaluate_result_success() {
        let result = EvaluateResult {
            exit_code: 0,
            stdout: "ok".to_string(),
            stderr: String::new(),
        };
        assert!(result.success());

        let fail = EvaluateResult {
            exit_code: 1,
            stdout: String::new(),
            stderr: "error".to_string(),
        };
        assert!(!fail.success());
    }
}
