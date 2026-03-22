//! Lifecycle script runner — on_enter/on_done/on_fail 실행 추상화.
//!
//! yaml state에 정의된 lifecycle action(script/prompt)을 실행한다.
//! `WORK_ID`와 `WORKTREE` 환경변수를 주입한다.

use std::collections::HashMap;
use std::fmt;

use async_trait::async_trait;

use super::config::models::LifecycleAction;

/// Lifecycle 실행 시점.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecyclePhase {
    OnEnter,
    OnDone,
    OnFail,
}

impl fmt::Display for LifecyclePhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LifecyclePhase::OnEnter => write!(f, "on_enter"),
            LifecyclePhase::OnDone => write!(f, "on_done"),
            LifecyclePhase::OnFail => write!(f, "on_fail"),
        }
    }
}

/// Lifecycle 실행에 필요한 환경 정보.
#[derive(Debug, Clone)]
pub struct LifecycleContext {
    /// 큐 아이템 식별자
    pub work_id: String,
    /// worktree 경로
    pub worktree: String,
    /// 추가 환경변수 (확장용)
    pub extra_env: HashMap<String, String>,
}

/// Lifecycle action 실행 결과.
#[derive(Debug)]
pub struct LifecycleResult {
    pub success: bool,
    pub output: String,
    pub error: String,
}

/// Lifecycle action 실행기.
///
/// script/prompt 액션을 실행하고 결과를 반환한다.
/// DIP: 코어는 이 trait에 의존하고, 인프라가 구현체를 제공한다.
#[async_trait]
pub trait LifecycleRunner: Send + Sync {
    /// 단일 lifecycle action을 실행한다.
    async fn run_action(&self, action: &LifecycleAction, ctx: &LifecycleContext)
        -> LifecycleResult;

    /// 여러 lifecycle action을 순차 실행한다.
    /// 하나라도 실패하면 나머지를 건너뛰고 실패를 반환한다.
    async fn run_actions(
        &self,
        actions: &[LifecycleAction],
        phase: LifecyclePhase,
        ctx: &LifecycleContext,
    ) -> Result<(), String> {
        for (i, action) in actions.iter().enumerate() {
            tracing::info!(
                "lifecycle {phase}[{i}]: executing for work_id={}",
                ctx.work_id
            );
            let result = self.run_action(action, ctx).await;
            if !result.success {
                let err_msg = format!(
                    "lifecycle {phase}[{i}] failed: {}",
                    if result.error.is_empty() {
                        "unknown error"
                    } else {
                        &result.error
                    }
                );
                tracing::error!("{err_msg}");
                return Err(err_msg);
            }
            tracing::info!("lifecycle {phase}[{i}]: completed successfully");
        }
        Ok(())
    }
}

/// No-op lifecycle runner (lifecycle 미설정 시 사용).
pub struct NoopLifecycleRunner;

#[async_trait]
impl LifecycleRunner for NoopLifecycleRunner {
    async fn run_action(
        &self,
        _action: &LifecycleAction,
        _ctx: &LifecycleContext,
    ) -> LifecycleResult {
        LifecycleResult {
            success: true,
            output: String::new(),
            error: String::new(),
        }
    }
}

#[cfg(test)]
pub mod testing {
    use super::*;
    use std::sync::Mutex;

    /// 테스트용 lifecycle runner.
    /// 호출된 action을 기록하고, 설정된 결과를 반환한다.
    pub struct MockLifecycleRunner {
        pub calls: Mutex<Vec<(LifecyclePhase, Vec<LifecycleAction>)>>,
        pub fail_phase: Mutex<Option<LifecyclePhase>>,
    }

    impl MockLifecycleRunner {
        pub fn new() -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                fail_phase: Mutex::new(None),
            }
        }

        pub fn failing_on(phase: LifecyclePhase) -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                fail_phase: Mutex::new(Some(phase)),
            }
        }
    }

    #[async_trait]
    impl LifecycleRunner for MockLifecycleRunner {
        async fn run_action(
            &self,
            _action: &LifecycleAction,
            _ctx: &LifecycleContext,
        ) -> LifecycleResult {
            LifecycleResult {
                success: true,
                output: String::new(),
                error: String::new(),
            }
        }

        async fn run_actions(
            &self,
            actions: &[LifecycleAction],
            phase: LifecyclePhase,
            _ctx: &LifecycleContext,
        ) -> Result<(), String> {
            self.calls.lock().unwrap().push((phase, actions.to_vec()));

            let fail_phase = self.fail_phase.lock().unwrap();
            if let Some(fp) = &*fail_phase {
                if *fp == phase {
                    return Err(format!("mock {phase} failure"));
                }
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_phase_display() {
        assert_eq!(LifecyclePhase::OnEnter.to_string(), "on_enter");
        assert_eq!(LifecyclePhase::OnDone.to_string(), "on_done");
        assert_eq!(LifecyclePhase::OnFail.to_string(), "on_fail");
    }

    #[tokio::test]
    async fn noop_runner_always_succeeds() {
        let runner = NoopLifecycleRunner;
        let ctx = LifecycleContext {
            work_id: "test:org/repo:1".into(),
            worktree: "/tmp/wt".into(),
            extra_env: HashMap::new(),
        };
        let actions = vec![LifecycleAction::Script {
            script: "echo hello".into(),
        }];
        let result = runner
            .run_actions(&actions, LifecyclePhase::OnEnter, &ctx)
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn run_actions_stops_on_first_failure() {
        use testing::MockLifecycleRunner;

        let runner = MockLifecycleRunner::failing_on(LifecyclePhase::OnDone);
        let ctx = LifecycleContext {
            work_id: "test:org/repo:1".into(),
            worktree: "/tmp/wt".into(),
            extra_env: HashMap::new(),
        };
        let actions = vec![LifecycleAction::Script {
            script: "echo hello".into(),
        }];

        // on_enter should succeed
        let result = runner
            .run_actions(&actions, LifecyclePhase::OnEnter, &ctx)
            .await;
        assert!(result.is_ok());

        // on_done should fail
        let result = runner
            .run_actions(&actions, LifecyclePhase::OnDone, &ctx)
            .await;
        assert!(result.is_err());
    }
}
