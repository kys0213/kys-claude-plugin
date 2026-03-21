# DESIGN v5: DataSource + AgentRuntime 아키텍처

> **Date**: 2026-03-21
> **Status**: Draft
> **기준**: v4 DESIGN.md + 운영 피드백 + 열린 이슈 반영

---

## 핵심 원칙

### 1. CPU 모델

```
상태 전이가 유일한 실행 트리거.
전이 발생 → 코어 로직 실행 → DataSource hook 실행 → DB commit
```

### 2. OCP 확장점 3개

```
새 외부 시스템  = DataSource impl 추가     → 코어 변경 0
새 LLM        = AgentRuntime impl 추가    → 코어 변경 0
새 파이프라인   = Task impl 추가           → 코어 변경 0
```

### 3. 관심사 분리

```
Daemon  = 토큰 안 쓰는 인프라 (수집, 상태 관리, 실행, cron)
Claw    = 토큰 쓰는 판단 전부 (큐 평가, 스펙 분해, gap 탐지)
코어    = DB 전이, 의존성, 스펙 링크, decision 기록 (내부 로직)
DataSource = 외부 시스템 hook (라벨, 알림, escalation)
AgentRuntime = LLM 실행 추상화 (Claude, Gemini, Codex, ...)
```

### 4. Workspace + DataSource 소유권

```
"repo"는 GitHub 전용 개념이다. DataSource가 늘어나면 깨진다.
→ "workspace"가 최상위 그룹, DataSource가 자신의 ID 체계로 리소스를 소유한다.

workspace "auth-project"
  ├── github datasource → issues, pulls (org/repo 기준)
  ├── jira datasource   → tickets (AUTH-xxx 기준)
  └── slack datasource  → threads (channel 기준)

QueueItem은 반드시 하나의 DataSource에 귀속된다.
work_id = "{datasource}:{type}:{external_id}"
  github:issue:42
  github:pr:43
  jira:ticket:AUTH-123
  slack:thread:C01-1234
```

---

## 전체 아키텍처

```
┌──────────────────────────────────────────────────────────────────┐
│  사용자                                                          │
│                                                                  │
│  레포 Claude 세션          Claw 세션              터미널          │
│  └─ /spec                 └─ /claw               autodev        │
│       │                       │                  dashboard      │
└───────┼───────────────────────┼──────────────────────┼───────────┘
        │                       │                      │
        ▼                       ▼                      ▼
┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐
│  Plugin Commands │  │  Claw Agent      │  │  TUI Dashboard   │
│  (3개: thin      │  │  (AgentRuntime   │  │  (읽기 전용)      │
│   wrapper)       │  │   기반 세션)      │  │                  │
│  /auto           │  │  자연어 → CLI     │  │  BoardRenderer   │
│  /spec           │  │                  │  │                  │
│  /claw           │  │                  │  │                  │
└────────┬─────────┘  └────────┬─────────┘  └────────┬─────────┘
         │                     │                      │
         │      ┌──────────────┘                      │
         ▼      ▼                                     ▼
┌──────────────────────────────────────────────────────────────────┐
│  autodev CLI (SSOT)                                              │
│                                                                  │
│  autodev workspace  add/list/show/update/remove                  │
│  autodev spec    add/list/show/update/pause/resume/complete/     │
│                  remove/link/unlink/status/verify/decisions      │
│  autodev queue   list/show/advance/skip/dependency               │
│  autodev hitl    list/show/respond/timeout                       │
│  autodev cron    list/add/update/pause/resume/remove/trigger     │
│  autodev claw    init/rules/edit                                 │
│  autodev agent / dashboard / start / stop / status               │
└────────┬─────────────────────────────────────────────────────────┘
         │
         ▼
┌──────────────────────────────────────────────────────────────────┐
│  코어 (내부 로직, DataSource/AgentRuntime 무관)                    │
│                                                                  │
│  ┌─ DependencyAnalyzer    파일 단위 의존성 분석                   │
│  ├─ SpecLinker            spec_issues 자동 링크                  │
│  ├─ DependencyGuard       advance 시 선행 아이템 검증             │
│  ├─ SpecCompletionCheck   linked issues 완료 감지                │
│  ├─ DecisionRecorder      claw_decisions 기록                    │
│  ├─ ForceClawEvaluate     claw-evaluate cron 즉시 트리거          │
│  ├─ TestRunner            test_commands 실행 (결정적)             │
│  └─ TokenUsageRecorder    토큰 사용량 기록                        │
└────────┬─────────────────────────────────────────────────────────┘
         │
         ▼
┌──────────────────────────────────────────────────────────────────┐
│  Daemon (토큰 0 — LLM 호출 없음)                                  │
│                                                                  │
│  ┌────────────────────────────────────────────────────────┐      │
│  │  Heartbeat (tick_interval_secs, 기본 10초)             │      │
│  │                                                        │      │
│  │  1. DataSource.collect()  → Pending 저장               │      │
│  │     → 코어.on_enter_pending()                          │      │
│  │     → DataSource.on_phase_enter(Pending)               │      │
│  │                                                        │      │
│  │  2. Ready 아이템 drain                                 │      │
│  │     → DataSource.on_phase_enter(Running)               │      │
│  │     → DataSource.before_task()                         │      │
│  │     → AgentRuntime.invoke()  (Task 실행)               │      │
│  │     → DataSource.after_task()                          │      │
│  │                                                        │      │
│  │  3. 완료/실패 처리                                      │      │
│  │     → DataSource.on_done() 또는 on_failed()            │      │
│  │     → 코어.on_done() 또는 apply_escalation()           │      │
│  └────────────────────────────────────────────────────────┘      │
│                                                                  │
│  ┌────────────────────────────────────────────────────────┐      │
│  │  Cron Engine               등록된 job 주기 실행        │      │
│  │  ├─ claw-evaluate (60초)   Claw headless 호출         │      │
│  │  ├─ gap-detection (1시간)  스펙-코드 대조              │      │
│  │  ├─ knowledge-extract      merged PR 지식 추출        │      │
│  │  ├─ hitl-timeout (5분)     미응답 HITL 확인           │      │
│  │  ├─ daily-report           일간 리포트                │      │
│  │  └─ log-cleanup            오래된 로그 정리           │      │
│  └────────────────────────────────────────────────────────┘      │
└──────────────────────────────────────────────────────────────────┘
```

---

## DataSource trait

외부 시스템(GitHub, Slack, Jira, ...)의 lifecycle을 캡슐화하는 OCP 확장점.
각 DataSource는 **자신의 ID 체계와 리소스 컬렉션을 소유**한다.

### trait 정의

```rust
#[async_trait]
pub trait DataSource: Send + Sync {
    fn name(&self) -> &str;

    // ── 리소스 소유권 ──

    /// 이 DataSource가 관리하는 컬렉션 유형
    /// 예: GitHub → ["issue", "pr"], Jira → ["ticket"], Slack → ["thread"]
    fn collection_types(&self) -> Vec<&str>;

    /// work_id로 외부 시스템의 리소스 상세 정보 조회
    /// 코어가 DataSource 내부 구조를 몰라도 필요한 정보를 가져올 수 있음
    async fn resolve(&self, work_id: &str) -> Result<ResourceDetail>;

    /// 외부 시스템에서 사람이 볼 수 있는 URL
    fn external_url(&self, item: &QueueItem) -> Option<String>;

    // ── 수집 ──

    /// 외부 소스를 스캔하여 새 아이템 반환
    /// 반환된 QueueItem.work_id는 반드시 "{self.name()}:{type}:{external_id}" 형식
    /// collect()는 멱등: 이미 Ready/Running/Done인 아이템은 반환하지 않음
    async fn collect(&mut self, workspace: &WorkspaceConfig) -> Result<Vec<QueueItem>>;

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

### QueueItem과 DataSource 귀속

```rust
pub struct QueueItem {
    /// "{datasource}:{type}:{external_id}" 형식
    /// 예: "github:issue:42", "jira:ticket:AUTH-123", "slack:thread:C01-1234"
    pub work_id: String,
    pub workspace_id: String,
    pub phase: QueuePhase,
    pub title: String,
    pub task_kind: TaskKind,
    pub metadata_json: String,
    // ...
}

impl QueueItem {
    /// work_id에서 DataSource 이름 추출
    pub fn datasource_name(&self) -> &str {
        self.work_id.split(':').next().unwrap()
    }

    /// work_id에서 컬렉션 타입 추출
    pub fn collection_type(&self) -> &str {
        self.work_id.splitn(3, ':').nth(1).unwrap()
    }

    /// work_id에서 외부 시스템 고유 ID 추출
    pub fn external_id(&self) -> &str {
        self.work_id.splitn(3, ':').nth(2).unwrap()
    }
}
```

### ResourceDetail (DataSource가 반환)

```rust
pub struct ResourceDetail {
    pub title: String,
    pub body: String,
    pub author: String,
    pub labels: Vec<String>,
    pub url: Option<String>,
    pub extra: serde_json::Value,  // DataSource별 추가 정보
}
```

코어는 `resolve()`를 통해 DataSource의 내부 구조를 몰라도 필요한 정보를 가져온다.
예: DependencyAnalyzer가 이슈 본문에서 파일 경로를 추출할 때 `source.resolve(work_id).body`를 사용.

### HookContext

```rust
pub struct HookContext<'a> {
    pub workspace: &'a WorkspaceConfig,
    pub workspace_id: &'a str,
}
```

DataSource hook에 최소한의 정보만 전달. DataSource가 코어 내부 구조에 의존하지 않도록.

### EscalationAction (DataSource가 결정, 코어가 실행)

```rust
pub enum EscalationAction {
    Retry,
    CommentAndRetry { comment: String },
    Hitl { event: NewHitlEvent },
    Skip { reason: String },
    Replan { event: NewHitlEvent },
}
```

### GitHubDataSource 구현 예시

```rust
impl DataSource for GitHubDataSource {
    fn name(&self) -> &str { "github" }

    fn collection_types(&self) -> Vec<&str> { vec!["issue", "pr"] }

    async fn resolve(&self, work_id: &str) -> Result<ResourceDetail> {
        // work_id = "github:issue:42"
        let (_, type_, number) = parse_work_id(work_id);
        let item = self.gh.get_issue_or_pr(number).await?;
        Ok(ResourceDetail {
            title: item.title,
            body: item.body,
            author: item.author,
            labels: item.labels,
            url: Some(format!("https://github.com/{}/{}/{}", self.owner, self.repo, number)),
            extra: json!({ "state": item.state, "assignees": item.assignees }),
        })
    }

    fn external_url(&self, item: &QueueItem) -> Option<String> {
        let number = item.external_id();
        Some(format!("https://github.com/{}/{}/issues/{}", self.owner, self.repo, number))
    }

    async fn collect(&mut self, workspace: &WorkspaceConfig) -> Result<Vec<QueueItem>> {
        // autodev:analyze 라벨이 붙은 이슈 감지
        // work_id = "github:issue:{number}" 형식으로 QueueItem 생성
    }

    async fn on_phase_enter(&self, phase: QueuePhase, item: &QueueItem, _ctx: &HookContext) -> Result<()> {
        match phase {
            Pending  => self.gh.add_label(item.external_id(), "autodev:queued").await,
            Ready    => self.gh.replace_label(item.external_id(), "autodev:queued", "autodev:ready").await,
            Running  => self.gh.replace_label(item.external_id(), "autodev:ready", "autodev:wip").await,
            Done     => self.gh.replace_label(item.external_id(), "autodev:wip", "autodev:done").await,
            Skipped  => self.gh.add_label(item.external_id(), "autodev:skip").await,
        }
    }

    async fn before_task(&self, kind: TaskKind, item: &QueueItem, ctx: &HookContext) -> Result<()> {
        if kind == TaskKind::Implement {
            let branch = ctx.workspace.convention.branch_name(item);
            self.gh.comment(item.external_id(), &format!("🔨 구현 시작: `{branch}`")).await?;
        }
        Ok(())
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

    async fn on_done(&self, item: &QueueItem, _ctx: &HookContext) -> Result<()> {
        self.gh.comment(item.external_id(), "✅ 완료되었습니다.").await
    }

    async fn on_skip(&self, item: &QueueItem, reason: &str, _ctx: &HookContext) -> Result<()> {
        self.gh.comment(item.external_id(), &format!("⏭️ 건너뜁니다: {reason}")).await
    }

    async fn after_hitl_created(&self, event: &HitlEvent, _ctx: &HookContext) -> Result<()> {
        self.gh.comment_hitl(event).await
    }
}
```

### Workspace 설정에서 DataSource 바인딩

```yaml
# .autodev.yaml (workspace 설정)
name: "auth-project"

sources:
  github:
    url: https://github.com/org/repo
    scan_interval_secs: 300
    issue_concurrency: 1
    pr_concurrency: 1
```

다중 소스:

```yaml
name: "auth-project"

sources:
  github:
    url: https://github.com/org/repo
    scan_interval_secs: 300
  jira:
    host: jira.company.com
    project: AUTH
    scan_interval_secs: 300
  slack:
    channel: "#dev-auth"
    scan_interval_secs: 60
```

하나의 workspace에 여러 DataSource가 바인딩되면, 각 DataSource가 독립적으로 collect하고 hook을 실행.
코어는 `item.datasource_name()`으로 어떤 DataSource의 아이템인지 판별하여 적절한 DataSource에 hook을 위임.

### Daemon의 DataSource 라우팅

```rust
fn source_for(&self, item: &QueueItem) -> &dyn DataSource {
    let name = item.datasource_name();  // "github", "jira", "slack"
    self.sources.get(name).unwrap()
}
```

### 트랜잭션 정책

```
1. DB 전이 (코어, atomic)          ← 필수, 실패 시 롤백
2. DataSource hook (best-effort)    ← 실패해도 전이는 유효
   → 실패 시 보상 큐에 추가 → 다음 tick에서 재시도 (최대 3회)
   → 3회 실패 시 로그 경고만 (다음 collect에서 보정 가능)
```

### 보상 큐 (영속, Daemon 재시작에도 유지)

```sql
compensation_queue (
    id            TEXT PRIMARY KEY,
    work_id       TEXT NOT NULL,
    datasource    TEXT NOT NULL,
    hook_type     TEXT NOT NULL,     -- "on_phase_enter", "on_failed" 등
    hook_args     TEXT,              -- JSON (phase, reason 등)
    attempt_count INT DEFAULT 0,
    next_retry_at TEXT NOT NULL,
    created_at    TEXT NOT NULL
)
```

Daemon 시작 시 이 테이블을 스캔하여 미완료 hook을 복구.

---

## AgentRuntime trait

LLM 실행 시스템(Claude, Gemini, Codex, ...)을 추상화하는 OCP 확장점.

### trait 정의

autodev가 **실제로 필요한 기능** 에서 도출한 인터페이스.

```rust
#[async_trait]
pub trait AgentRuntime: Send + Sync {
    fn name(&self) -> &str;
    async fn invoke(&self, request: RuntimeRequest) -> RuntimeResponse;
    fn capabilities(&self) -> RuntimeCapabilities;
}

/// autodev → 런타임 요청 (core 소유)
pub struct RuntimeRequest {
    pub working_dir: PathBuf,
    pub prompt: String,
    pub system_prompt: Option<String>,
    pub structured_output: Option<StructuredOutput>,
    pub session_id: Option<String>,
}

pub struct StructuredOutput {
    pub schema: String,   // JSON Schema
}

/// 런타임 → autodev 응답 (core 소유)
pub struct RuntimeResponse {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub duration: Duration,
    pub token_usage: Option<TokenUsage>,
    pub session_id: Option<String>,
}

/// 런타임 기능 선언
pub struct RuntimeCapabilities {
    pub can_edit_files: bool,
    pub supports_structured_output: bool,
    pub supports_system_prompt: bool,
    pub supports_session_resume: bool,
    pub max_context_tokens: usize,
}
```

### 의존성 방향

```
core/runtime.rs (trait + DTO)     ← autodev가 정의하는 인터페이스
     ↑ impl
infra/runtimes/
  ├── claude.rs                   ← Claude CLI 플래그로 매핑
  ├── gemini.rs                   ← Gemini CLI 플래그로 매핑
  ├── codex.rs                    ← Codex CLI 플래그로 매핑
  └── custom.rs                  ← 임의 CLI

core → infra 방향 의존 없음.
```

### core 옵션 → CLI 매핑 (각 런타임의 책임)

| core 옵션 | Claude | Gemini | Codex |
|-----------|--------|--------|-------|
| `system_prompt` | `--append-system-prompt` | prompt prepend | prompt prepend |
| `structured_output` | `--output-format json` + `--json-schema` | `--output-format json` | `--output-schema <file>` + `--json` |
| `working_dir` | `current_dir()` | `current_dir()` | `--cd <dir>` |
| `session_id` | `--resume <uuid>` | `--resume <id>` | `codex exec resume <id>` |

기능 미지원 시 각 런타임이 폴백 처리 (예: system_prompt → prompt prepend).

### 설정

```yaml
# .autodev.yaml
runtime:
  default: claude
  claude:
    model: sonnet
  overrides:
    analyze: claude       # 분석은 Claude
    implement: claude     # 구현은 Claude
    review: gemini        # 리뷰는 Gemini (대규모 컨텍스트)
    claw_evaluate: claude # Claw 판단은 Claude
```

### RuntimeRegistry

```rust
pub struct RuntimeRegistry {
    runtimes: HashMap<String, Arc<dyn AgentRuntime>>,
    default_name: String,
    overrides: HashMap<TaskKind, String>,
}

impl RuntimeRegistry {
    pub fn resolve(&self, task_kind: TaskKind) -> Arc<dyn AgentRuntime> {
        let name = self.overrides.get(&task_kind).unwrap_or(&self.default_name);
        self.runtimes[name].clone()
    }
}
```

---

## 큐 상태 머신

### Phase 전이

```
         ┌──────────────────────────────────────────────┐
         │                                              │
         ▼                                              │
      Pending ──(advance)──> Ready ──(drain)──> Running │
         │                                       │      │
         │                                  ┌────┴────┐ │
         │                                  │         │ │
         │                               Success   Failure
         │                                  │         │
         └──────(skip)───> Skipped          ▼         │
                                          Done     (escalation)
                                                      │
                                              ┌───────┴──────┐
                                              │              │
                                          Retry→Pending   HITL/Skip
```

### Daemon 실행 사이클

```rust
// Heartbeat tick
async fn tick(&mut self) {
    // 1. Collect
    for source in &mut self.sources {
        let items = source.collect(&workspace).await;
        for item in items {
            self.db.queue_upsert(&item);
            self.core.on_enter_pending(&item);       // 의존성, 스펙링크, decision
            source.on_phase_enter(Pending, &item, &ctx).await;  // 라벨
        }
    }

    // 2. Drain: Ready → Running
    for item in self.queue.drain_ready() {
        let source = self.source_for(&item);
        source.on_phase_enter(Running, &item, &ctx).await;
        source.before_task(item.task_kind, &item, &ctx).await;

        let runtime = self.registry.resolve(item.task_kind);
        self.spawn_task(item, runtime);
    }
}

// Task 완료
async fn on_task_complete(&mut self, result: TaskResult) {
    let source = self.source_for(&result);

    match result.status {
        Success => {
            source.after_task(kind, &item, &result.outcome, &ctx).await;
            source.on_done(&item, &ctx).await;
            self.core.on_done(&item);  // 스펙완료체크, decision, force_evaluate
        }
        Failed => {
            source.after_task(kind, &item, &result.outcome, &ctx).await;
            let action = source.on_failed(&item, failure_count, &ctx).await;
            self.core.apply_escalation(&item, action);  // DB 전이, HITL 생성
        }
    }
}
```

### 코어 on_enter_pending

```
1. DependencyAnalyzer
   → 이슈 본문에서 파일 경로 추출
   → 큐의 다른 아이템과 파일 단위 겹침 확인
   → 겹침 발견 시 dependency 메타데이터에 선행 work_id 기록
   → Claw가 다음 evaluate에서 의미론적 보정

2. SpecLinker
   → 라벨 [XX-spec-name] 패턴, 이슈 본문 스펙 참조 추출
   → 매칭 시 spec_issues 테이블 링크
   → 실패 시 로그만 (Claw가 보정)

3. DecisionRecorder
   → source: collector 기록
```

### 코어 on_done

```
1. DecisionRecorder: 완료 기록
2. SpecCompletionCheck: linked spec의 모든 issues Done?
   → 모두 Done → on_spec_completing 이벤트
3. ForceClawEvaluate: claw-evaluate 즉시 트리거
4. TokenUsageRecorder: 토큰 기록
```

### DependencyGuard (advance 시)

```
queue advance 요청 시:
  1. dependency 메타데이터 확인
  2. 선행 아이템이 Done 아니면 → advance 차단
  3. Done이면 → 통과
```

---

## Spec 상태 머신

### 상태 전이

```
Draft ──→ Active ←──→ Paused
              │
              ▼
          Completing
              │
              ▼
          Completed (terminal)

Any ──→ Archived (soft delete)
Archived ──resume──→ Active (복구)
```

### 상태별 전이/동작

| 상태 | 가능한 전이 | CLI | 코어 이벤트 |
|------|------------|-----|-----------|
| Draft | → Active, → Archived | Claw 분석 완료 + HITL 승인 | on_spec_active |
| Active | → Paused, → Completing, → Archived | `spec pause`, (자동), `spec remove` | - |
| Paused | → Active, → Archived | `spec resume`, `spec remove` | on_spec_active |
| Completing | → Active, → Completed | (테스트 실패), (HITL 승인) | on_spec_completing |
| Completed | → Archived | `spec remove` | on_spec_completed |
| Archived | → Active | `spec resume` | on_spec_active |

### on_spec_completing 파이프라인

```
모든 linked issues Done 감지 (on_done에서)
  → TestRunner: spec.test_commands 순차 실행
    → 실패: 실패 항목을 이슈로 생성 → on_enter_pending
    → 성공: 다음 단계
  → ForceClawEvaluate: gap detection (Claw에게 위임)
    → gap 발견: 이슈 생성 → 루프 계속
    → gap 없음: 다음 단계
  → HitlCreator: 최종 확인 HITL (Low severity)
    → approve → on_spec_completed
    → request-changes → Active로 복귀
```

---

## HITL 시스템

### HITL 생성 경로

| 생성자 | 트리거 |
|--------|--------|
| DataSource.on_failed() | EscalationAction::Hitl/Replan 반환 시 |
| 코어 on_spec_completing | 최종 확인 요청 시 |
| 코어 DependencyGuard | 스펙 충돌 감지 시 |

### 이벤트 유형별 선택지 (고정)

```rust
pub struct HitlOption {
    pub key: String,      // "retry", "skip" 등
    pub label: String,    // 사용자 표시용
    pub action: String,   // 라우팅 대상: "advance", "skip", "replan"
}
```

| 유형 | 선택지 |
|------|--------|
| Escalation Level 3 | retry→advance, skip→skip, reassign→replan |
| Escalation Level 5 | replan→replan, force-retry→advance, abandon→skip |
| Spec Completion | approve→spec_completed, request-changes→spec_active |
| Conflict | prioritize-A→advance, prioritize-B→advance, pause-both→pause |

### on_hitl_responded

```
HITL 응답 → DB 저장 → HitlResponseRouter:
  "advance"        → queue advance
  "skip"           → queue skip
  "replan"         → Claw에게 스펙 수정 제안 위임
  "spec_completed" → on_spec_completed
  "spec_active"    → spec Active 복귀
→ ForceClawEvaluate
```

---

## Graceful Shutdown

```
SIGINT → on_shutdown:
  1. Running 아이템 완료 대기 (timeout: 30초, 설정 가능)
     → timeout 초과: Pending으로 롤백
  2. DataSource.on_phase_exit(Running) 호출 (best-effort)
  3. Cron engine 정지
```

### Worktree 보존

```
DataSource.after_task(Implement, Failed):
  → worktree 보존 (삭제하지 않음)
  → HITL 알림에 경로 포함
  → 보존 기간: 7일 기본 (설정 가능)
  → autodev worktree list/clean으로 관리
```

---

## Cron

### Force Trigger (코어)

```
코어.on_done()            → force_claw_evaluate()
코어.on_failed()          → force_claw_evaluate()
코어.on_spec_registered() → force_claw_evaluate()   // Draft 진입 → 즉시 분석
코어.on_spec_active()     → force_claw_evaluate()
코어.on_hitl_responded()  → force_claw_evaluate()
```

### Force Trigger Debounce

```
무한 루프 방지:
  on_done → force → Claw advance → on_done → force → ...

대응:
  1. 5초 윈도우 내 중복 force 신호는 병합 (1회만 실행)
  2. 동일 tick 내 최대 1회만 Claw 호출
  3. 실행 중인 Claw 세션이 있으면 다음 tick으로 연기
```

### 기본 Cron Jobs

| Job | 유형 | 주기 | 동작 |
|-----|------|------|------|
| claw-evaluate | per-workspace, LLM | 60초 | Claw headless (큐 평가) |
| gap-detection | per-workspace, LLM | 1시간 | 스펙-코드 대조 |
| knowledge-extract | per-workspace, LLM | 1시간 | merged PR 지식 추출 |
| hitl-timeout | global, 결정적 | 5분 | 미응답 HITL 만료 |
| daily-report | global, 결정적 | 매일 06시 | 일간 리포트 |
| log-cleanup | global, 결정적 | 매일 00시 | 오래된 로그 정리 |

---

## Decisions 테이블

```sql
claw_decisions (
    id              TEXT PRIMARY KEY,
    workspace_id    TEXT NOT NULL,
    spec_id         TEXT,
    work_id         TEXT,
    action          TEXT NOT NULL,     -- advance, skip, decompose, prioritize, replan, approve
    source          TEXT NOT NULL,     -- claw, collector, hitl_response, system
    reasoning       TEXT,
    confidence      REAL,
    context_json    TEXT,
    created_at      TEXT NOT NULL
)
```

---

## Plugin 구조

### commands/ (15개 → 3개)

```
plugins/autodev/
├── .claude-plugin/plugin.json     # 3 commands, 3 agents
├── commands/
│   ├── auto.md                    # /auto (start/stop/setup/config/dashboard/update)
│   ├── spec.md                    # /spec (add/update/list/status/remove/pause/resume)
│   └── claw.md                    # /claw (자연어 대화 세션)
├── agents/
│   ├── issue-analyzer.md
│   ├── pr-reviewer.md
│   └── conflict-resolver.md
├── skills/
│   ├── cli-reference/
│   └── label-setup/
└── cli/src/
```

### /claw 세션 진입 경험

```
/claw 실행 →

Step 1: 상태 수집 (autodev status/hitl/decisions/spec --json)

Step 2: 요약 표시

  📊 autodev 상태 요약

  ● daemon running (uptime 2h 15m)

  Workspaces:
    org/repo-a — queue: 1R 2P | specs: auth-v2 ████████░░ 60%
    org/repo-b — queue: 5D   | specs: payment ██░░░░░░░░ 25%

  ⚠ HITL 대기: 1건
    → #44 Session adapter — 3회 실패, 사람 확인 필요

  🧠 최근 판단:
    14:30 advance #42 | 14:25 decompose auth-v2 | 14:20 skip #39

  무엇을 도와드릴까요?

Step 3: 자연어 대화 (Bash tool로 autodev CLI 호출)
```

---

## 시각화

### CLI 출력 (--format 옵션)

| 값 | 용도 |
|---|------|
| `text` | 기본 텍스트 (기존 호환) |
| `json` | 구조화된 JSON (Claw 파싱용) |
| `rich` | 색상 + 박스 + 진행률 바 |

#### `autodev status --format rich`

```
● autodev daemon (uptime 2h 15m)

Repos:
  org/repo-a    ● active   queue: 3P 1R 2D   specs: 2/3
  org/repo-b    ● active   queue: 0P 0R 5D   specs: 1/1 ✓

Runtime: claude/sonnet (45.2K tokens/1h)
HITL: 1 pending ⚠
Next claw-evaluate: 25s
```

#### `autodev board --format rich`

```
auth-v2  Auth Module v2                    ████████░░░░ 60% (3/5)
  ✅ #42 JWT middleware
  ✅ #43 Token API
  🔄 #44 Session adapter (running, claude/sonnet)
  ⏳ #45 Error handling (dep: #44)
  ⏳ #46 Missing tests
```

### TUI Dashboard 강화

#### 추가 패널

```
┌─ Runtime ──────────────────┐  ┌─ DataSource ────────────────┐
│ claude/sonnet  12 runs  OK │  │ github  ● connected         │
│ claude/opus     2 runs  OK │  │   last scan: 30s ago        │
│ Tokens: 45.2K in / 12.1K  │  │   compensation queue: 0     │
│ Avg duration: 4m 32s      │  │                             │
└────────────────────────────┘  └─────────────────────────────┘
```

#### 전이 타임라인 (ItemDetail 오버레이)

```
┌─ #42 JWT middleware ─────────────────────────┐
│ Timeline:                                     │
│  14:00 ○ Pending  ← github.collect()         │
│         └ SpecLinker: linked to auth-v2       │
│  14:05 ○ Ready    ← Claw advance             │
│  14:06 ○ Running  ← AnalyzeTask              │
│         └ claude/sonnet (1.2K tokens, 6m)    │
│  14:12 ● Done                                 │
│         └ SpecCompletionCheck: 3/5 done       │
└───────────────────────────────────────────────┘
```

### 추가 테이블 (시각화용)

```sql
transition_events (
    id          TEXT PRIMARY KEY,
    work_id     TEXT NOT NULL,
    event_type  TEXT NOT NULL,
    detail      TEXT,
    created_at  TEXT NOT NULL
)
```

---

## 해소하는 이슈

| # | 이슈 | 해소 위치 |
|---|------|----------|
| 430 | claw-evaluate decisions 미기록 | DecisionRecorder (코어, 모든 전이 시 자동) |
| 416 | agent 세션 시작 시 상태 요약 | /claw 진입 Step 2 |
| 414 | 스펙 분석(Draft) → 승인 후 이슈 생성 | on_spec_registered → ForceClawEvaluate → HITL 승인 → on_spec_active |
| 413 | 이슈 의존성 + spec_issues 링크 | on_enter_pending (DependencyAnalyzer + SpecLinker) |
| 405 | worktree 브랜치 네이밍 | DataSource.before_task() + convention |
| 395 | 이슈 템플릿 시스템 | Claw decompose + convention |
| 386 | done 시 데이터소스별 완료 처리 | DataSource.on_done() (OCP) |
| 382 | graceful shutdown | on_shutdown 이벤트 |

## REMAINING-WORK 해소

| 항목 | 해소 위치 |
|------|----------|
| C2 Notifier 연결 | DataSource.on_done/on_failed에 흡수 |
| C3 Force trigger | 코어에서 자동 (on_done, on_failed, on_spec_active, on_hitl_responded) |
| C4 Escalation 5단계 | DataSource.on_failed() → EscalationAction |
| H1 Spec completion | on_spec_completing 파이프라인 |

---

## 구현 순서

```
Phase 1: 코어 리팩토링
  → DataSource trait 정의 (core/datasource.rs)
  → AgentRuntime trait 정의 (core/runtime.rs)
  → GitHubDataSource 구현 (기존 Collector/Notifier 통합)
  → ClaudeRuntime 구현 (기존 Agent/Claude 통합)

Phase 2: 상태 머신 강화
  → 코어 on_enter_pending, on_done, on_failed 구현
  → DependencyAnalyzer, SpecLinker, DependencyGuard
  → Spec lifecycle (Completing, Archived 추가)

Phase 3: Plugin 통합
  → commands/ 15개 → 3개 통합
  → /claw 세션 진입 경험
  → CLI --format rich 옵션

Phase 4: 시각화
  → TUI Runtime/DataSource 패널
  → 전이 타임라인
  → transition_events 테이블

Phase 5: 확장
  → GeminiRuntime, CodexRuntime 구현
  → RuntimeRegistry task별 오버라이드
  → Multi-LLM 리뷰 (MultiRuntime)
```
