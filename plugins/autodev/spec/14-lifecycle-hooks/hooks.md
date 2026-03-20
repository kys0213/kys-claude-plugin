# Hook 구현체: DataSource별 상세

> 각 LifecycleHook 구현체의 전이별 동작 정의.

---

## GitHubLifecycleHook

GitHub 이슈/PR의 라벨과 코멘트를 상태 전이에 맞춰 관리한다.

### before_transition

| 전이 | 동작 |
|------|------|
| `Ready → Running` | PR 충돌 검사. 충돌 있으면 `Deny("merge conflict")` |

### after_transition

| 전이 | 동작 |
|------|------|
| `Pending → Ready` | 라벨: `autodev:ready` 추가 |
| `Ready → Running` | 라벨: `autodev:wip` 추가, 이전 phase 라벨 제거. 코멘트: 작업 시작 안내 |
| `Running → Done` | 라벨: `autodev:done` 추가, `autodev:wip` 제거. 코멘트: 완료 안내 |
| `Running → Failed` | 라벨: `autodev:failed` 추가, `autodev:wip` 제거 |
| `Running → Skipped` | 라벨: `autodev:skip` 추가, `autodev:wip` 제거 |

```rust
pub struct GitHubLifecycleHook {
    gh: Arc<dyn Gh>,
}

#[async_trait]
impl LifecycleHook for GitHubLifecycleHook {
    fn name(&self) -> &str { "github" }

    async fn before_transition(&self, t: &Transition) -> HookDecision {
        match (t.from, t.to) {
            (QueuePhase::Ready, QueuePhase::Running) => {
                if self.has_merge_conflict(t).await {
                    HookDecision::Deny("PR has merge conflicts".into())
                } else {
                    HookDecision::Allow
                }
            }
            _ => HookDecision::Allow,
        }
    }

    async fn after_transition(&self, t: &Transition) {
        match (t.from, t.to) {
            (QueuePhase::Pending, QueuePhase::Ready) => {
                self.set_label(t, "autodev:ready").await;
            }
            (QueuePhase::Ready, QueuePhase::Running) => {
                self.set_label(t, "autodev:wip").await;
                self.add_comment(t, "작업을 시작합니다.").await;
            }
            (QueuePhase::Running, QueuePhase::Done) => {
                self.set_label(t, "autodev:done").await;
                self.add_comment(t, "작업이 완료되었습니다.").await;
            }
            (QueuePhase::Running, QueuePhase::Failed) => {
                self.set_label(t, "autodev:failed").await;
            }
            _ => {}
        }
    }
}
```

---

## NotificationLifecycleHook

기존 `Notifier` trait과 `NotificationDispatcher`를 hook으로 래핑한다.
상태 전이 시 자동으로 알림을 발송한다.

### after_transition

| 전이 | 동작 |
|------|------|
| `Running → Failed` | `NotificationEvent::from_task_failed()` → dispatcher |
| `Running → Done` | (설정에 따라) 완료 알림 발송 |

```rust
pub struct NotificationLifecycleHook {
    dispatcher: NotificationDispatcher,
}

#[async_trait]
impl LifecycleHook for NotificationLifecycleHook {
    fn name(&self) -> &str { "notification" }

    async fn after_transition(&self, t: &Transition) {
        match (t.from, t.to) {
            (QueuePhase::Running, QueuePhase::Failed) => {
                if let Some(TransitionResult::Failed(ref msg)) = t.result {
                    let event = NotificationEvent::from_task_failed(
                        &t.work_id, &t.repo_name, msg,
                    );
                    self.dispatcher.dispatch(&event).await;
                }
            }
            _ => {}
        }
    }
}
```

---

## EscalationLifecycleHook

Task 실패 시 `failure_count` 증가, 에스컬레이션 레벨 판단, HITL 생성을 수행한다.
기존 `escalation::escalate()` 로직을 hook으로 이동.

### after_transition

| 전이 | 동작 |
|------|------|
| `Running → Failed` | `escalate()` 호출 → Retry / Remove / HITL 판단 |

```rust
pub struct EscalationLifecycleHook {
    db: Database,
}

#[async_trait]
impl LifecycleHook for EscalationLifecycleHook {
    fn name(&self) -> &str { "escalation" }

    async fn after_transition(&self, t: &Transition) {
        if let (QueuePhase::Running, QueuePhase::Failed) = (t.from, t.to) {
            if let Some(TransitionResult::Failed(ref msg)) = t.result {
                match resolve_repo_id(&self.db, &t.repo_name) {
                    Ok(repo_id) => {
                        escalation::escalate(&self.db, &t.work_id, &repo_id, msg);
                    }
                    Err(e) => {
                        tracing::warn!("escalation skipped for {}: {e}", t.work_id);
                    }
                }
            }
        }
    }
}
```

---

## LoggingLifecycleHook

Task 실행 결과의 DB 로깅과 토큰 사용량 기록을 수행한다.
기존 Daemon main loop의 `log_insert()` + `usage_insert()` 로직을 hook으로 이동.

### after_transition

| 전이 | 동작 |
|------|------|
| `Running → Done` | consumer log 기록, 토큰 사용량 기록 |
| `Running → Failed` | consumer log 기록, 토큰 사용량 기록 |

```rust
pub struct LoggingLifecycleHook {
    db: Database,
}

#[async_trait]
impl LifecycleHook for LoggingLifecycleHook {
    fn name(&self) -> &str { "logging" }

    async fn after_transition(&self, t: &Transition) {
        match (t.from, t.to) {
            (QueuePhase::Running, QueuePhase::Done)
            | (QueuePhase::Running, QueuePhase::Failed) => {
                // TransitionResult에서 log entries를 추출하여 기록
                self.record_logs(t).await;
            }
            _ => {}
        }
    }
}
```

---

## 향후 확장 예시

### SlackLifecycleHook (예정)

```rust
impl LifecycleHook for SlackLifecycleHook {
    fn name(&self) -> &str { "slack" }

    async fn after_transition(&self, t: &Transition) {
        match (t.from, t.to) {
            (QueuePhase::Ready, QueuePhase::Running) => {
                self.post_message(t, "🚀 작업 시작").await;
            }
            (QueuePhase::Running, QueuePhase::Done) => {
                self.post_message(t, "✅ 작업 완료").await;
            }
            (QueuePhase::Running, QueuePhase::Failed) => {
                self.post_message(t, "❌ 작업 실패").await;
            }
            _ => {}
        }
    }
}
```

### JiraLifecycleHook (예정)

```rust
impl LifecycleHook for JiraLifecycleHook {
    fn name(&self) -> &str { "jira" }

    async fn after_transition(&self, t: &Transition) {
        match (t.from, t.to) {
            (QueuePhase::Ready, QueuePhase::Running) => {
                self.transition_issue(t, "In Progress").await;
            }
            (QueuePhase::Running, QueuePhase::Done) => {
                self.transition_issue(t, "Done").await;
            }
            _ => {}
        }
    }
}
```
