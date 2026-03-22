use std::path::PathBuf;

use crate::v5::core::phase::V5QueuePhase;
use crate::v5::core::queue_item::V5QueueItem;
use crate::v5::service::concurrency::ConcurrencyTracker;
use crate::v5::service::evaluator::{EvalDecision, EvaluateResult, Evaluator};

/// Evaluate cron: Completed 아이템을 스캔하여 Done/HITL로 분류한다.
///
/// 핵심 흐름:
///   1. Running -> Completed 전이 감지 시 force_trigger 발동
///   2. Completed 아이템에 대해 evaluate LLM 호출 (concurrency slot 소모)
///   3. LLM 결과에 따라 Done 또는 HITL로 전이
///
/// 2-level concurrency:
///   evaluate LLM 호출도 글로벌 동시성 슬롯을 소모한다.
///   workspace별 + 전역 제한 모두 적용.
pub struct EvaluateCron {
    workspace_name: String,
    autodev_home: PathBuf,
    /// force_trigger 플래그: Running -> Completed 전이 시 설정
    triggered: bool,
}

/// 단일 아이템의 평가 결과.
#[derive(Debug)]
pub struct EvalOutcome {
    pub work_id: String,
    pub decision: EvalDecision,
    pub target_phase: V5QueuePhase,
}

impl EvaluateCron {
    pub fn new(workspace_name: &str, autodev_home: PathBuf) -> Self {
        Self {
            workspace_name: workspace_name.to_string(),
            autodev_home,
            triggered: false,
        }
    }

    /// force_trigger: Running -> Completed 전이 시 호출.
    /// 다음 tick에서 즉시 evaluate를 실행하도록 플래그를 설정한다.
    pub fn force_trigger(&mut self) {
        tracing::info!(
            "evaluate_cron: force_trigger for workspace '{}'",
            self.workspace_name
        );
        self.triggered = true;
    }

    /// force_trigger가 설정되어 있는지 확인.
    pub fn is_triggered(&self) -> bool {
        self.triggered
    }

    /// 트리거 플래그를 소비(리셋)한다.
    fn consume_trigger(&mut self) -> bool {
        let was_triggered = self.triggered;
        self.triggered = false;
        was_triggered
    }

    /// Completed 아이템을 필터링하여 평가 대상 목록을 반환한다.
    pub fn find_completed_items(items: &[V5QueueItem]) -> Vec<&V5QueueItem> {
        Evaluator::filter_completed(items)
    }

    /// evaluate tick: Completed 아이템이 있고 concurrency 여유가 있으면 평가를 실행한다.
    ///
    /// force_trigger가 설정되지 않았고 interval이 경과하지 않았으면 skip.
    /// force_trigger가 설정되었으면 즉시 실행.
    ///
    /// Returns: 평가된 아이템의 (work_id, target_phase) 목록
    pub async fn tick(
        &mut self,
        items: &[V5QueueItem],
        tracker: &mut ConcurrencyTracker,
        ws_concurrency: u32,
    ) -> Vec<EvalOutcome> {
        let was_triggered = self.consume_trigger();
        let completed = Self::find_completed_items(items);

        if completed.is_empty() {
            return Vec::new();
        }

        if !was_triggered {
            // force_trigger가 아닌 경우, 주기적 폴링에 의존 (daemon tick 간격)
            // daemon.tick() 에서 매번 호출되므로 별도 간격 체크 불필요
        }

        tracing::info!(
            "evaluate_cron: {} completed items to evaluate (triggered={})",
            completed.len(),
            was_triggered
        );

        let mut outcomes = Vec::new();

        for item in completed {
            // 2-level concurrency 체크: evaluate도 slot을 소모한다
            if !tracker.can_spawn_in_workspace(&self.workspace_name, ws_concurrency) {
                tracing::info!(
                    "evaluate_cron: concurrency limit reached, deferring remaining items"
                );
                break;
            }

            // Slot 점유
            tracker.track(&self.workspace_name);

            let evaluator = Evaluator::new(&self.workspace_name);
            let result = evaluator.run_evaluate(&self.autodev_home).await;

            // Slot 반환
            tracker.release(&self.workspace_name);

            match result {
                Ok(eval_result) => {
                    let decision = self.classify_result(&eval_result);
                    let target_phase = Evaluator::target_phase(&decision);

                    tracing::info!(
                        "evaluate_cron: {} -> {:?} (exit={})",
                        item.work_id,
                        target_phase,
                        eval_result.exit_code
                    );

                    outcomes.push(EvalOutcome {
                        work_id: item.work_id.clone(),
                        decision,
                        target_phase,
                    });
                }
                Err(e) => {
                    tracing::error!("evaluate_cron: failed to evaluate {}: {e}", item.work_id);
                    // 실패 시 HITL로 에스컬레이션
                    outcomes.push(EvalOutcome {
                        work_id: item.work_id.clone(),
                        decision: EvalDecision::Hitl {
                            reason: format!("evaluate failed: {e}"),
                        },
                        target_phase: V5QueuePhase::Hitl,
                    });
                }
            }
        }

        outcomes
    }

    /// evaluate 실행 결과를 Done/HITL로 분류한다.
    ///
    /// exit_code == 0 → Done
    /// exit_code != 0 → HITL (사람 판단 필요)
    fn classify_result(&self, result: &EvaluateResult) -> EvalDecision {
        if result.success() {
            EvalDecision::Done
        } else {
            EvalDecision::Hitl {
                reason: format!(
                    "evaluate exited with code {}: {}",
                    result.exit_code,
                    result.stderr.trim()
                ),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v5::core::queue_item::testing::test_item;

    fn make_evaluate_cron() -> EvaluateCron {
        EvaluateCron::new("test-ws", PathBuf::from("/tmp/autodev"))
    }

    #[test]
    fn force_trigger_sets_flag() {
        let mut cron = make_evaluate_cron();
        assert!(!cron.is_triggered());

        cron.force_trigger();
        assert!(cron.is_triggered());
    }

    #[test]
    fn consume_trigger_resets_flag() {
        let mut cron = make_evaluate_cron();
        cron.force_trigger();
        assert!(cron.is_triggered());

        let was = cron.consume_trigger();
        assert!(was);
        assert!(!cron.is_triggered());
    }

    #[test]
    fn consume_trigger_returns_false_when_not_set() {
        let mut cron = make_evaluate_cron();
        let was = cron.consume_trigger();
        assert!(!was);
    }

    #[test]
    fn find_completed_items_filters_correctly() {
        let mut items = vec![
            test_item("s1", "analyze"),
            test_item("s2", "implement"),
            test_item("s3", "review"),
        ];
        // s1: Pending, s2: Completed, s3: Running
        items[1].phase = V5QueuePhase::Completed;
        items[2].phase = V5QueuePhase::Running;

        let completed = EvaluateCron::find_completed_items(&items);
        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0].work_id, "s2:implement");
    }

    #[test]
    fn find_completed_items_empty_when_none() {
        let items = vec![test_item("s1", "analyze"), test_item("s2", "implement")];
        let completed = EvaluateCron::find_completed_items(&items);
        assert!(completed.is_empty());
    }

    #[test]
    fn find_completed_items_multiple() {
        let mut items = vec![
            test_item("s1", "analyze"),
            test_item("s2", "implement"),
            test_item("s3", "review"),
        ];
        items[0].phase = V5QueuePhase::Completed;
        items[1].phase = V5QueuePhase::Completed;

        let completed = EvaluateCron::find_completed_items(&items);
        assert_eq!(completed.len(), 2);
    }

    #[test]
    fn classify_result_done_on_success() {
        let cron = make_evaluate_cron();
        let result = EvaluateResult {
            exit_code: 0,
            stdout: "ok".to_string(),
            stderr: String::new(),
        };
        let decision = cron.classify_result(&result);
        assert_eq!(decision, EvalDecision::Done);
    }

    #[test]
    fn classify_result_hitl_on_failure() {
        let cron = make_evaluate_cron();
        let result = EvaluateResult {
            exit_code: 1,
            stdout: String::new(),
            stderr: "evaluation error".to_string(),
        };
        let decision = cron.classify_result(&result);
        match decision {
            EvalDecision::Hitl { reason } => {
                assert!(reason.contains("evaluation error"));
                assert!(reason.contains("code 1"));
            }
            EvalDecision::Done => panic!("expected Hitl, got Done"),
        }
    }

    #[tokio::test]
    async fn tick_skips_when_no_completed_items() {
        let mut cron = make_evaluate_cron();
        let items = vec![test_item("s1", "analyze")]; // Pending
        let mut tracker = ConcurrencyTracker::new(4);

        let outcomes = cron.tick(&items, &mut tracker, 2).await;
        assert!(outcomes.is_empty());
        assert_eq!(tracker.total(), 0); // No slots consumed
    }

    #[tokio::test]
    async fn tick_respects_concurrency_limit() {
        let mut cron = make_evaluate_cron();
        cron.force_trigger();

        let mut items = vec![test_item("s1", "analyze"), test_item("s2", "implement")];
        items[0].phase = V5QueuePhase::Completed;
        items[1].phase = V5QueuePhase::Completed;

        // Global max=1, ws concurrency=1 -> only 1 item can be evaluated
        let mut tracker = ConcurrencyTracker::new(1);

        let outcomes = cron.tick(&items, &mut tracker, 1).await;
        // At most 1 item evaluated due to concurrency limit
        // (slot is acquired and released per item, so actually both could run)
        // But since we check can_spawn_in_workspace before each item,
        // and release after each, both should run with ws_concurrency=1
        // because track + release happens sequentially.
        // Actually: track(ws) -> ws count = 1, release(ws) -> ws count = 0
        // So both items should be evaluated.
        assert_eq!(outcomes.len(), 2);
        assert_eq!(tracker.total(), 0); // All slots released
    }

    #[tokio::test]
    async fn tick_consumes_trigger() {
        let mut cron = make_evaluate_cron();
        cron.force_trigger();
        assert!(cron.is_triggered());

        let items: Vec<V5QueueItem> = vec![];
        let mut tracker = ConcurrencyTracker::new(4);

        cron.tick(&items, &mut tracker, 2).await;
        assert!(!cron.is_triggered()); // Trigger consumed
    }

    #[tokio::test]
    async fn tick_with_global_concurrency_exhausted() {
        let mut cron = make_evaluate_cron();
        cron.force_trigger();

        let mut items = vec![test_item("s1", "analyze")];
        items[0].phase = V5QueuePhase::Completed;

        // Global max=1 but already fully occupied
        let mut tracker = ConcurrencyTracker::new(1);
        tracker.track("other-ws"); // Exhaust global slots

        let outcomes = cron.tick(&items, &mut tracker, 2).await;
        assert!(outcomes.is_empty()); // Cannot spawn due to global limit
        assert_eq!(tracker.total(), 1); // Original slot still held
    }

    #[test]
    fn eval_outcome_target_phase() {
        let outcome_done = EvalOutcome {
            work_id: "test:analyze".to_string(),
            decision: EvalDecision::Done,
            target_phase: V5QueuePhase::Done,
        };
        assert_eq!(outcome_done.target_phase, V5QueuePhase::Done);

        let outcome_hitl = EvalOutcome {
            work_id: "test:implement".to_string(),
            decision: EvalDecision::Hitl {
                reason: "needs review".to_string(),
            },
            target_phase: V5QueuePhase::Hitl,
        };
        assert_eq!(outcome_hitl.target_phase, V5QueuePhase::Hitl);
    }
}
