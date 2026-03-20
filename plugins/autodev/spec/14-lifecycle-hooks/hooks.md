# Hook 구현체

> 각 LifecycleHook 구현체의 전이별 동작.

---

## GitHubLifecycleHook

| 전이 | before | after |
|------|--------|-------|
| Pending → Ready | - | 라벨: `autodev:ready` |
| Ready → Running | PR 충돌 → Deny | 라벨: `autodev:wip`, 코멘트 |
| Running → Done | - | 라벨: `autodev:done`, 코멘트 |
| Running → Failed | - | 라벨: `autodev:failed` |
| Running → Skipped | - | 라벨: `autodev:skip` |

```rust
impl LifecycleHook for GitHubLifecycleHook {
    fn name(&self) -> &str { "github" }

    async fn before_transition(&self, t: &Transition) -> HookDecision {
        match (t.from, t.to) {
            (QueuePhase::Ready, QueuePhase::Running) if self.has_merge_conflict(t).await => {
                HookDecision::Deny("PR has merge conflicts".into())
            }
            _ => HookDecision::Allow,
        }
    }

    async fn after_transition(&self, t: &Transition) {
        match (t.from, t.to) {
            (_, QueuePhase::Ready)   => self.set_label(t, "autodev:ready").await,
            (_, QueuePhase::Running) => {
                self.set_label(t, "autodev:wip").await;
                self.add_comment(t, "작업을 시작합니다.").await;
            }
            (_, QueuePhase::Done)    => {
                self.set_label(t, "autodev:done").await;
                self.add_comment(t, "작업이 완료되었습니다.").await;
            }
            (_, QueuePhase::Failed)  => self.set_label(t, "autodev:failed").await,
            (_, QueuePhase::Skipped) => self.set_label(t, "autodev:skip").await,
            _ => {}
        }
    }
}
```

---

## NotificationLifecycleHook

기존 `NotificationDispatcher`를 래핑.

| 전이 | after |
|------|-------|
| Running → Failed | `NotificationEvent::from_task_failed()` 발송 |
| Running → Done | (설정에 따라) 완료 알림 발송 |

---

## EscalationLifecycleHook

기존 `escalation::escalate()` 로직을 hook으로 이동.

| 전이 | after |
|------|-------|
| Running → Failed | failure_count 증가 → Retry / Remove / HITL 판단 |

---

## LoggingLifecycleHook

기존 Daemon의 `log_insert()` + `usage_insert()` 로직을 hook으로 이동.

| 전이 | after |
|------|-------|
| Running → Done/Failed | consumer log 기록, 토큰 사용량 기록 |

---

## 확장 예시

```rust
// Jira — 상태 전이만
impl LifecycleHook for JiraLifecycleHook {
    fn name(&self) -> &str { "jira" }
    async fn after_transition(&self, t: &Transition) {
        match (t.from, t.to) {
            (_, QueuePhase::Running) => self.move_to(t, "In Progress").await,
            (_, QueuePhase::Done)    => self.move_to(t, "Done").await,
            _ => {}
        }
    }
}

// Slack — 메시지만
impl LifecycleHook for SlackLifecycleHook {
    fn name(&self) -> &str { "slack" }
    async fn after_transition(&self, t: &Transition) {
        match (t.from, t.to) {
            (_, QueuePhase::Running) => self.post(t, "작업 시작").await,
            (_, QueuePhase::Done)    => self.post(t, "작업 완료").await,
            (_, QueuePhase::Failed)  => self.post(t, "작업 실패").await,
            _ => {}
        }
    }
}
```
