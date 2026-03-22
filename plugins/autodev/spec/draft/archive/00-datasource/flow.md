# Flow 0: DataSource trait

### 시나리오

외부 시스템(GitHub, Slack, Jira, ...)의 lifecycle을 하나의 trait으로 캡슐화한다.
새 외부 시스템 추가 = 새 DataSource impl, 코어 변경 0 (OCP).

### 역할 분리

```
코어    = DB 전이, 의존성 분석, 스펙 링크, decision 기록 (내부 로직)
DataSource = 수집, 라벨 동기화, 코멘트, 알림, escalation (외부 시스템)
```

코어는 "무엇을 해야 하는지", DataSource는 "외부 시스템에 어떻게 반영하는지".

### trait 정의

```rust
#[async_trait]
pub trait DataSource: Send + Sync {
    fn name(&self) -> &str;

    // ── 수집 ──
    async fn collect(&mut self, repo: &RepoConfig) -> Result<Vec<QueueItem>>;

    // ── 큐 상태 전이 hook ──
    async fn on_phase_enter(&self, phase: QueuePhase, item: &QueueItem, ctx: &HookContext) -> Result<()>;
    async fn on_phase_exit(&self, phase: QueuePhase, item: &QueueItem, ctx: &HookContext) -> Result<()>;

    // ── Task 실행 hook ──
    async fn before_task(&self, kind: TaskKind, item: &QueueItem, ctx: &HookContext) -> Result<()>;
    async fn after_task(&self, kind: TaskKind, item: &QueueItem, result: &TaskOutcome, ctx: &HookContext) -> Result<()>;

    // ── 완료/실패/skip ──
    async fn on_done(&self, item: &QueueItem, ctx: &HookContext) -> Result<()>;
    async fn on_failed(&self, item: &QueueItem, failure_count: u32, ctx: &HookContext) -> Result<EscalationAction>;
    async fn on_skip(&self, item: &QueueItem, reason: &str, ctx: &HookContext) -> Result<()>;

    // ── HITL 알림 (선택적) ──
    async fn after_hitl_created(&self, event: &HitlEvent, ctx: &HookContext) -> Result<()> { Ok(()) }
}
```

### HookContext

```rust
pub struct HookContext<'a> {
    pub db: &'a dyn Database,
    pub repo: &'a RepoConfig,
    pub decisions: &'a dyn ClawDecisionRepository,
}
```

### EscalationAction

DataSource가 실패 정책을 결정하고, 코어가 실행한다.

```rust
pub enum EscalationAction {
    Retry,
    CommentAndRetry { comment: String },
    Hitl { event: NewHitlEvent },
    Skip { reason: String },
    Replan { event: NewHitlEvent },
}
```

### GitHubDataSource 구현

```rust
impl DataSource for GitHubDataSource {
    fn name(&self) -> &str { "github" }

    async fn collect(&mut self, repo: &RepoConfig) -> Result<Vec<QueueItem>> {
        // autodev:analyze 라벨이 붙은 이슈/PR 감지 → QueueItem 반환
    }

    async fn on_phase_enter(&self, phase: QueuePhase, item: &QueueItem, _ctx: &HookContext) -> Result<()> {
        match phase {
            Pending  => self.gh.add_label(item, "autodev:queued").await,
            Ready    => self.gh.replace_label(item, "autodev:queued", "autodev:ready").await,
            Running  => self.gh.replace_label(item, "autodev:ready", "autodev:wip").await,
            Done     => self.gh.replace_label(item, "autodev:wip", "autodev:done").await,
            Skipped  => self.gh.add_label(item, "autodev:skip").await,
        }
    }

    async fn before_task(&self, kind: TaskKind, item: &QueueItem, ctx: &HookContext) -> Result<()> {
        match kind {
            TaskKind::Analyze => {
                self.gh.comment(item, "🔍 분석을 시작합니다...").await?;
            }
            TaskKind::Implement => {
                let branch = ctx.repo.convention.branch_name(item);
                self.gh.comment(item, &format!("🔨 구현 시작: `{branch}`")).await?;
            }
            _ => {}
        }
        Ok(())
    }

    async fn on_done(&self, item: &QueueItem, _ctx: &HookContext) -> Result<()> {
        self.gh.comment(item, "✅ 완료되었습니다.").await
    }

    async fn on_failed(&self, item: &QueueItem, failure_count: u32, _ctx: &HookContext) -> Result<EscalationAction> {
        match failure_count {
            1     => Ok(EscalationAction::Retry),
            2     => Ok(EscalationAction::CommentAndRetry { comment: "⚠️ 2회 실패".into() }),
            3     => Ok(EscalationAction::Hitl { event: escalation_hitl(item, Level3) }),
            4     => Ok(EscalationAction::Skip { reason: "4회 실패 자동 스킵".into() }),
            _     => Ok(EscalationAction::Replan { event: replan_hitl(item) }),
        }
    }

    async fn on_skip(&self, item: &QueueItem, reason: &str, _ctx: &HookContext) -> Result<()> {
        self.gh.comment(item, &format!("⏭️ 건너뜁니다: {reason}")).await
    }

    async fn after_hitl_created(&self, event: &HitlEvent, _ctx: &HookContext) -> Result<()> {
        // 이슈에 HITL 코멘트 (선택지 포함)
        self.gh.comment_hitl(event).await
    }
}
```

### 향후 확장 예시

```rust
// Slack
impl DataSource for SlackDataSource {
    async fn on_phase_enter(&self, phase, item, _ctx) -> Result<()> {
        match phase {
            Running => self.slack.react(item.thread_ts, "👀").await,
            Done    => self.slack.react(item.thread_ts, "✅").await,
            _       => Ok(())
        }
    }
}

// Jira
impl DataSource for JiraDataSource {
    async fn on_phase_enter(&self, phase, item, _ctx) -> Result<()> {
        match phase {
            Running => self.jira.transition(item.jira_key, "In Progress").await,
            Done    => self.jira.transition(item.jira_key, "Done").await,
            _       => Ok(())
        }
    }
}
```

### 레포 설정에서 바인딩

```yaml
# .autodev.yaml
sources:
  github:
    scan_interval_secs: 300
    issue_concurrency: 1
    pr_concurrency: 1
```

하나의 레포에 여러 DataSource 바인딩 가능:

```yaml
sources:
  github:
    scan_interval_secs: 300
  slack:
    channel: "#dev-autodev"
```

### 트랜잭션 정책

```
1. DB 전이 (코어, atomic)          ← 필수, 실패 시 롤백
2. DataSource hook (best-effort)    ← 실패해도 전이는 유효
   → 실패 시 보상 큐에 추가 → 다음 tick에서 재시도 (최대 3회)
   → 3회 실패 시 로그 경고 (다음 collect에서 보정 가능)
```

---

### 관련 플로우

- [Flow 0: AgentRuntime](../00-agent-runtime/flow.md)
- [Flow 1: 레포 등록](../01-repo-registration/flow.md)
- [Flow 2: 이슈 등록](../02-issue-registration/flow.md)
- [Flow 9: 실패 복구](../09-failure-recovery/flow.md)
