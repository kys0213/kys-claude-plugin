# DESIGN v3: Daemon Architecture Refactoring

> **Date**: 2026-02-28
> **Base**: DESIGN-v2.md — Label-Positive 모델, HITL 게이트, Issue-PR 연동 (플로우 유지)
> **변경 범위**: Daemon 내부 구조 리팩토링 (외부 동작 변경 없음)

---

## 1. 변경 동기

### 현재 구조의 문제

현재 `daemon/mod.rs`(746줄)에 다음 책임이 혼재되어 있다:

- event loop 관리
- heartbeat 상태 기록
- repo 동기화 (DB → in-memory)
- recovery (고아 아이템 정리)
- scanning (GitHub 이슈/PR 감지)
- task spawning 및 동시성 제어
- task 결과 처리 (queue 상태 전이)
- daily report 스케줄링

`GitRepository`(1639줄)도 도메인 모델과 scanning 로직이 섞여 있다.

### v3 목표

1. **Daemon 분리**: Daemon → TaskManager + TaskRunner로 책임 분리
2. **TaskSource 추상화**: scanning 로직을 trait으로 분리하여 확장 가능하게
3. **Task trait**: pipeline 함수들을 통일된 생명주기로 추상화
4. **Agent trait**: Claude 실행을 추상화하여 다른 LLM으로 교체 가능하게

---

## 2. 전체 구조

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                                  Daemon                                     │
│─────────────────────────────────────────────────────────────────────────────│
│  manager: TaskManager                                                       │
│  runner:  TaskRunner                                                        │
│─────────────────────────────────────────────────────────────────────────────│
│  async run()       // loop { manager.poll_all() → runner.run(task) }        │
│  write_heartbeat() // 5초 주기 daemon.status.json                           │
└──────────┬──────────────────────────────────┬───────────────────────────────┘
           │ owns                             │ owns
           ▼                                  ▼
┌─────────────────────────┐        ┌─────────────────────────────┐
│      TaskManager        │        │        TaskRunner            │
│─────────────────────────│        │─────────────────────────────│
│  sources: Vec<          │        │  agent: Arc<dyn Agent>      │
│    Box<dyn TaskSource>> │        │─────────────────────────────│
│─────────────────────────│        │  async run(Box<dyn Task>)   │
│  register(TaskSource)   │        │    // before → agent → after│
│  async poll_all()       │        └──────────────┬──────────────┘
│    -> Vec<Box<dyn Task>>│                       │ uses
│  apply(TaskResult)      │                       ▼
│  schedule_daily_report()│             ┌─────────────────────┐
└────────────┬────────────┘             │  «trait» Agent      │
             │ owns                     │─────────────────────│
             ▼                          │  async invoke(      │
  ┌─────────────────────┐              │    AgentRequest     │
  │  «trait» TaskSource  │              │  ) -> AgentResponse │
  │─────────────────────│              └─────────────────────┘
  │  async poll()       │
  │   -> Vec<Box<       │
  │      dyn Task>>     │
  └──────────┬──────────┘
             △ impl
             │
     ┌───────┴────────────────┐
     │                        │
     ▼                        ▼
┌──────────────────┐  ┌──────────────────┐
│GitHubTaskSource  │  │(future sources)  │
│──────────────────│  │  SlackTaskSource │
│ ctx: TaskContext  │  │  JiraTaskSource  │
│──────────────────│  └──────────────────┘
│ async poll()     │
│  // 1. recovery  │
│  // 2. scan      │
│  → AnalyzeTask   │
│  → ImplementTask │
│  → ReviewTask    │
│  → ImproveTask   │
│  → ReReviewTask  │
│  → MergeTask     │
└────────┬─────────┘
         │ creates
         ▼
  ┌──────────────────────┐
  │    «trait» Task       │
  │──────────────────────│
  │  fn work_id() -> &str│
  │  fn repo_name()      │
  │   -> &str            │
  │                      │
  │  async before_invoke │
  │   () -> Result<      │
  │     AgentRequest,    │
  │     SkipReason>      │
  │                      │
  │  async after_invoke  │
  │   (AgentResponse)    │
  │   -> TaskResult      │
  └──────────┬───────────┘
             △ impl
             │
  ┌──────────┼──────────────────────────────────┐
  │          │          │          │             │
  ▼          ▼          ▼          ▼             ▼
┌──────┐ ┌────────┐ ┌────────┐ ┌─────────┐ ┌────────┐
│Analy │ │Implmnt │ │Review  │ │Improve  │ │Merge   │
│zeTask│ │Task    │ │Task    │ │Task     │ │Task    │
└──────┘ └────────┘ └────────┘ └─────────┘ └────────┘
```

---

## 3. 책임 분배

### 현재 → v3 매핑

| 책임 | 현재 위치 | v3 위치 | 이유 |
|------|----------|---------|------|
| event loop (`select!`) | `daemon/mod.rs` | **Daemon** | Daemon 고유 책임 |
| heartbeat (5초) | `daemon/mod.rs` | **Daemon** | Daemon 생존 신호 |
| graceful shutdown | `daemon/mod.rs` | **Daemon** | Daemon 생명주기 |
| InFlightTracker | `daemon/mod.rs` | **Daemon** | "몇 개까지 동시 run할지"는 오케스트레이터 결정 |
| repo 동기화 (DB → memory) | `daemon/mod.rs` | **GitHubTaskSource** | source가 자신의 데이터 관리 |
| recovery (고아 정리) | `GitRepository` | **GitHubTaskSource.poll()** | source가 자신의 건강 관리 |
| scanning (이슈/PR 감지) | `GitRepository` | **GitHubTaskSource.poll()** | source가 작업 생성 |
| task spawning | `daemon/mod.rs` | **TaskRunner** | Runner가 task 실행 |
| task 결과 처리 (queue ops) | `daemon/mod.rs` | **TaskManager.apply()** | Manager가 결과 반영 |
| daily report | `daemon/mod.rs` | **TaskManager** | 주기적 작업 스케줄링 |

### Daemon (얇은 오케스트레이터)

```rust
pub struct Daemon {
    manager: TaskManager,
    runner: TaskRunner,
    inflight: InFlightTracker,
}
```

Daemon은 **제어 흐름만** 담당한다:
- `select!` 루프 돌리기
- heartbeat 기록
- inflight 체크 후 runner에 task 위임
- runner 결과를 manager에 전달
- graceful shutdown

### TaskManager (작업 수집 + 결과 반영)

```rust
pub struct TaskManager {
    sources: Vec<Box<dyn TaskSource>>,
}
```

- `poll_all()`: 모든 source에서 실행 가능한 Task를 수집
- `apply(TaskResult)`: Task 실행 결과를 source의 큐에 반영
- `schedule_daily_report()`: 일간 리포트 생성 시점 판단 + Task 생성

### TaskRunner (실행 엔진)

```rust
pub struct TaskRunner {
    agent: Arc<dyn Agent>,
}
```

- `run(task)`: `before_invoke → agent.invoke → after_invoke` 생명주기 실행
- Agent 구현체가 무엇이든 동일하게 동작

---

## 4. Trait 정의

### TaskSource

```rust
#[async_trait]
pub trait TaskSource: Send + Sync {
    /// 실행 가능한 Task를 수집하여 반환.
    /// 내부적으로 recovery, scanning을 포함할 수 있다.
    async fn poll(&mut self) -> Vec<Box<dyn Task>>;

    /// Task 실행 결과를 source 내부 상태에 반영.
    fn apply(&mut self, result: &TaskResult);
}
```

### Task

```rust
#[async_trait]
pub trait Task: Send + Sync {
    /// 고유 식별자 (예: "issue:org/repo:123")
    fn work_id(&self) -> &str;

    /// Task가 속한 repo (동시성 제어용)
    fn repo_name(&self) -> &str;

    /// Agent 호출 전 사전 준비.
    /// - worktree 생성, preflight check (이슈가 아직 open인지 등)
    /// - 성공 시 AgentRequest 반환, 실패 시 SkipReason 반환
    async fn before_invoke(&mut self) -> Result<AgentRequest, SkipReason>;

    /// Agent 호출 후 후처리.
    /// - 결과 파싱, 라벨 전이, 큐 연산 생성, worktree 정리
    async fn after_invoke(&mut self, response: AgentResponse) -> TaskResult;
}
```

### Agent

```rust
#[async_trait]
pub trait Agent: Send + Sync {
    /// 프롬프트를 실행하고 결과를 반환.
    async fn invoke(&self, request: AgentRequest) -> AgentResponse;
}
```

현재 `Claude` trait과의 관계:
- `Agent`는 상위 추상화 (Claude 외 다른 LLM도 가능)
- `ClaudeAgent`가 `Agent`를 구현하며 내부적으로 `Claude` trait 사용

---

## 5. Data Flow (DTO)

### 요청/응답

```
  ┌──────────────┐    ┌───────────────┐    ┌────────────┐
  │ AgentRequest │    │ AgentResponse │    │ TaskResult │
  │──────────────│    │───────────────│    │────────────│
  │ working_dir  │    │ exit_code     │    │ work_id    │
  │ prompt       │    │ stdout        │    │ repo_name  │
  │ system_prompt│    │ stderr        │    │ logs       │
  │ session_opts │    │ duration      │    │ queue_ops  │
  └──────────────┘    └───────────────┘    │ status     │
                                           └────────────┘
```

### 생명주기

```
  Task.before_invoke()         Agent.invoke()        Task.after_invoke()
       │                            │                      │
       └──► AgentRequest ──────────►└──► AgentResponse ───►└──► TaskResult
```

### AgentRequest

```rust
pub struct AgentRequest {
    /// Claude 세션을 실행할 워크트리 경로
    pub working_dir: PathBuf,
    /// 메인 프롬프트
    pub prompt: String,
    /// 시스템 프롬프트 (선택)
    pub system_prompt: Option<String>,
    /// Claude 세션 옵션 (output_format, json_schema 등)
    pub session_opts: SessionOptions,
}
```

### AgentResponse

```rust
pub struct AgentResponse {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub duration: Duration,
}
```

### TaskResult

```rust
pub struct TaskResult {
    pub work_id: String,
    pub repo_name: String,
    pub queue_ops: Vec<QueueOp>,
    pub logs: Vec<NewConsumerLog>,
    pub status: TaskStatus,
}

pub enum TaskStatus {
    Completed,
    Skipped(SkipReason),
    Failed(String),
}
```

### SkipReason

```rust
pub enum SkipReason {
    /// preflight에서 이슈가 이미 닫혀있음 등
    PreflightFailed(String),
    /// 이미 처리됨 (dedup)
    AlreadyProcessed,
}
```

### 현재 코드와의 매핑

| 현재 | v3 |
|------|-----|
| `SessionOptions` | `AgentRequest.session_opts` (그대로 사용) |
| `SessionResult` | `AgentResponse` (duration 추가) |
| `TaskOutput` | `TaskResult` (status 추가) |
| `QueueOp` | `QueueOp` (그대로 사용) |
| `NewConsumerLog` | `TaskResult.logs` (그대로 사용) |

---

## 6. Task 구현체

### 공통 구조

모든 Task는 `TaskContext`와 큐 아이템을 생성자 주입받는다.

```rust
pub struct TaskContext {
    pub workspace: Arc<dyn WorkspaceOps>,
    pub gh: Arc<dyn Gh>,
    pub config: Arc<dyn ConfigLoader>,
}
```

### AnalyzeTask

```rust
pub struct AnalyzeTask {
    ctx: TaskContext,
    item: IssueItem,
}
```

| 메서드 | 동작 |
|--------|------|
| `before_invoke` | preflight: 이슈가 아직 open인지 확인 → worktree 생성 → 분석 프롬프트 + JSON schema로 AgentRequest 구성 |
| `after_invoke` | verdict 파싱 → implement: 분석 코멘트 게시 + `analyzed` 라벨 + QueueOp::Remove → clarify/wontfix: `skip` 라벨 + QueueOp::Remove → worktree 정리 |

### ImplementTask

```rust
pub struct ImplementTask {
    ctx: TaskContext,
    item: IssueItem,
}
```

| 메서드 | 동작 |
|--------|------|
| `before_invoke` | worktree 생성 + feature branch → 구현 프롬프트 구성 (analysis_report 포함) |
| `after_invoke` | commit + push → PR 생성 → QueueOp::PushPr + QueueOp::Remove → `implementing` 라벨 → worktree 정리 |

### ReviewTask

```rust
pub struct ReviewTask {
    ctx: TaskContext,
    item: PrItem,
}
```

| 메서드 | 동작 |
|--------|------|
| `before_invoke` | preflight: PR이 아직 리뷰 가능한지 확인 → worktree 생성 + PR branch checkout → 리뷰 프롬프트 구성 |
| `after_invoke` | verdict 파싱 → approve: knowledge extraction + `done` 라벨 + source_issue done 전이 → request_changes: 리뷰 코멘트 게시 + QueueOp::PushPr(ReviewDone) → worktree 정리 |

### ImproveTask

```rust
pub struct ImproveTask {
    ctx: TaskContext,
    item: PrItem,
    sw: Arc<dyn SuggestWorkflow>,  // 선택적 의존
}
```

| 메서드 | 동작 |
|--------|------|
| `before_invoke` | worktree 생성 + PR branch checkout → 피드백 반영 프롬프트 구성 (review_comment 포함) |
| `after_invoke` | commit + push → review_iteration 증가 → QueueOp::PushPr(Improved) → worktree 정리 |

### MergeTask

```rust
pub struct MergeTask {
    ctx: TaskContext,
    item: MergeItem,
}
```

| 메서드 | 동작 |
|--------|------|
| `before_invoke` | preflight: PR이 아직 merge 가능한지 확인 → merge 요청 구성 |
| `after_invoke` | merge 결과 처리 → 성공: QueueOp::Remove → 충돌: 재시도 |

---

## 7. GitHubTaskSource 상세

### 구조

```rust
pub struct GitHubTaskSource {
    ctx: TaskContext,
    repos: HashMap<String, GitRepository>,
    db: Arc<Database>,
    scan_interval: Duration,
}
```

### poll() 내부 흐름

```
async fn poll(&mut self) -> Vec<Box<dyn Task>> {
    // 1. Repo 동기화 (DB → memory)
    self.sync_repos().await;

    // 2. Recovery (GitHub 상태 동기화)
    //    - 고아 wip 아이템 정리
    //    - implementing 상태 이슈의 PR 상태 확인
    for repo in self.repos.values_mut() {
        repo.recover_orphan_wip().await;
        repo.recover_orphan_implementing().await;
    }

    // 3. Scanning (새 작업 감지)
    //    - issues::scan()          → issue_queue(Pending)
    //    - issues::scan_approved() → issue_queue(Ready)
    //    - pulls::scan()           → pr_queue(Pending)
    //    - pulls::scan_merges()    → merge_queue(Pending)
    for repo in self.repos.values_mut() {
        repo.scan_issues().await;
        repo.scan_approved_issues().await;
        repo.scan_pulls().await;
        repo.scan_merges().await;
    }

    // 4. 큐에서 실행 가능한 Task 생성
    let mut tasks: Vec<Box<dyn Task>> = Vec::new();
    for repo in self.repos.values_mut() {
        // issue Pending → AnalyzeTask
        if let Some(item) = repo.issue_queue.pop("Pending") {
            tasks.push(Box::new(AnalyzeTask::new(self.ctx.clone(), item)));
        }
        // issue Ready → ImplementTask
        if let Some(item) = repo.issue_queue.pop("Ready") {
            tasks.push(Box::new(ImplementTask::new(self.ctx.clone(), item)));
        }
        // pr Pending → ReviewTask
        if let Some(item) = repo.pr_queue.pop("Pending") {
            tasks.push(Box::new(ReviewTask::new(self.ctx.clone(), item)));
        }
        // pr ReviewDone → ImproveTask
        if let Some(item) = repo.pr_queue.pop("ReviewDone") {
            tasks.push(Box::new(ImproveTask::new(self.ctx.clone(), item)));
        }
        // merge Pending → MergeTask
        if let Some(item) = repo.merge_queue.pop("Pending") {
            tasks.push(Box::new(MergeTask::new(self.ctx.clone(), item)));
        }
    }
    tasks
}
```

### apply() 내부 흐름

```
fn apply(&mut self, result: &TaskResult) {
    let repo = self.repos.get_mut(&result.repo_name);
    for op in &result.queue_ops {
        match op {
            QueueOp::Remove => repo.remove(&result.work_id),
            QueueOp::PushIssue { phase, item } => repo.issue_queue.push(phase, item),
            QueueOp::PushPr { phase, item } => repo.pr_queue.push(phase, item),
            QueueOp::PushMerge { phase, item } => repo.merge_queue.push(phase, item),
        }
    }
    // 로그 기록
    for log in &result.logs {
        self.db.log_insert(log);
    }
}
```

---

## 8. TaskRunner 상세

### 실행 흐름

```rust
impl TaskRunner {
    pub async fn run(&self, mut task: Box<dyn Task>) -> TaskResult {
        // 1. before_invoke
        let request = match task.before_invoke().await {
            Ok(req) => req,
            Err(skip) => return TaskResult::skipped(task, skip),
        };

        // 2. agent.invoke
        let response = self.agent.invoke(request).await;

        // 3. after_invoke
        task.after_invoke(response).await
    }
}
```

### ClaudeAgent 구현

```rust
pub struct ClaudeAgent {
    claude: Arc<dyn Claude>,
}

#[async_trait]
impl Agent for ClaudeAgent {
    async fn invoke(&self, request: AgentRequest) -> AgentResponse {
        let result = self.claude.run_session(
            &request.working_dir,
            &request.prompt,
            &request.session_opts,
        ).await;

        match result {
            Ok(r) => AgentResponse {
                exit_code: r.exit_code,
                stdout: r.stdout,
                stderr: r.stderr,
                duration: r.duration,
            },
            Err(e) => AgentResponse::error(e),
        }
    }
}
```

---

## 9. Daemon Event Loop

```rust
impl Daemon {
    pub async fn run(&mut self) -> Result<()> {
        let mut join_set = JoinSet::new();
        let mut tick = interval(self.config.tick_interval);
        let mut heartbeat = interval(Duration::from_secs(5));

        loop {
            select! {
                // Task 완료 → 결과 반영
                Some(result) = join_set.join_next() => {
                    let result = result?;
                    self.inflight.release(&result.repo_name);
                    self.manager.apply(result);
                    // 즉시 새 task 스폰 시도
                    self.try_spawn(&mut join_set).await;
                }

                // Tick → 새 작업 폴링 + 스폰
                _ = tick.tick() => {
                    self.manager.tick().await;  // poll_all + daily_report
                    self.try_spawn(&mut join_set).await;
                }

                // Heartbeat
                _ = heartbeat.tick() => {
                    self.write_heartbeat();
                }

                // Shutdown
                _ = signal::ctrl_c() => break,
            }
        }

        // Graceful shutdown: 진행 중인 task 완료 대기
        while let Some(result) = join_set.join_next().await {
            self.manager.apply(result?);
        }
        Ok(())
    }

    async fn try_spawn(&mut self, join_set: &mut JoinSet<TaskResult>) {
        for task in self.manager.drain_ready() {
            if !self.inflight.can_spawn() { break; }
            self.inflight.track(task.repo_name());
            let runner = self.runner.clone();
            join_set.spawn(async move {
                runner.run(task).await
            });
        }
    }
}
```

---

## 10. 인프라 trait 정리

### 현재 유지

| Trait | 위치 | 변경 |
|-------|------|------|
| `Gh` | `infrastructure/gh` | 변경 없음 |
| `Git` | `infrastructure/git` | 변경 없음 |
| `Claude` | `infrastructure/claude` | 변경 없음 (Agent가 래핑) |
| `SuggestWorkflow` | `infrastructure/suggest_workflow` | 변경 없음 |
| `Env` | `config` | 변경 없음 |

### 신규

| Trait | 위치 | 역할 |
|-------|------|------|
| `TaskSource` | `daemon/task_source` | 작업 소스 추상화 |
| `Task` | `daemon/task` | 작업 단위 추상화 |
| `Agent` | `daemon/agent` | LLM 실행 추상화 |
| `WorkspaceOps` | `infrastructure/workspace` | Git worktree 연산 추상화 |
| `ConfigLoader` | `config` | 설정 로드 추상화 |

### WorkspaceOps

현재 `Workspace` struct를 trait으로 추출:

```rust
#[async_trait]
pub trait WorkspaceOps: Send + Sync {
    async fn ensure_cloned(&self, url: &str, name: &str) -> Result<PathBuf>;
    async fn create_worktree(
        &self,
        repo_name: &str,
        task_id: &str,
        branch: Option<&str>,
    ) -> Result<PathBuf>;
    async fn remove_worktree(&self, repo_name: &str, task_id: &str) -> Result<()>;
}
```

### ConfigLoader

```rust
#[async_trait]
pub trait ConfigLoader: Send + Sync {
    fn load(&self, repo_name: &str) -> Result<WorkspaceConfig>;
    fn global(&self) -> &GlobalConfig;
}
```

---

## 11. TaskContext

Task와 TaskSource가 공유하는 의존성 묶음:

```rust
#[derive(Clone)]
pub struct TaskContext {
    pub workspace: Arc<dyn WorkspaceOps>,
    pub gh: Arc<dyn Gh>,
    pub config: Arc<dyn ConfigLoader>,
}
```

- 각 Task는 생성 시 TaskContext를 주입받는다
- TaskContext의 모든 필드는 `Arc<dyn Trait>`이므로 clone 비용이 낮다
- GitHubTaskSource도 동일한 TaskContext를 사용한다

---

## 12. Queue Items (변경 없음)

v2에서 정의한 큐 아이템은 그대로 사용한다:

```
┌───────────┐  ┌───────────┐  ┌───────────┐
│ IssueItem │  │  PrItem   │  │ MergeItem │
│───────────│  │───────────│  │───────────│
│ work_id   │  │ work_id   │  │ work_id   │
│ repo_id   │  │ repo_id   │  │ repo_id   │
│ repo_name │  │ repo_name │  │ repo_name │
│ number    │  │ number    │  │ pr_number │
│ body      │  │ head_br   │  │ head_br   │
│ labels    │  │ base_br   │  │ base_br   │
│ analysis_ │  │ review_   │  └───────────┘
│  report   │  │  comment  │
└───────────┘  │ review_   │
               │  iteration│
               │ source_   │
               │  issue_num│
               └───────────┘
```

---

## 13. 모듈 구조 (변경 후)

```
cli/src/
├── main.rs
├── lib.rs
├── daemon/
│   ├── mod.rs              // Daemon struct + event loop
│   ├── task_manager.rs     // TaskManager
│   ├── task_runner.rs      // TaskRunner
│   ├── task_source.rs      // TaskSource trait
│   ├── task.rs             // Task trait + SkipReason + TaskResult
│   └── agent.rs            // Agent trait + ClaudeAgent
├── tasks/                  // Task 구현체 (현재 pipeline/ 대체)
│   ├── mod.rs
│   ├── analyze.rs          // AnalyzeTask
│   ├── implement.rs        // ImplementTask
│   ├── review.rs           // ReviewTask
│   ├── improve.rs          // ImproveTask
│   └── merge.rs            // MergeTask
├── sources/                // TaskSource 구현체
│   ├── mod.rs
│   └── github.rs           // GitHubTaskSource
├── domain/
│   ├── models.rs           // 변경 없음
│   ├── git_repository.rs   // scanning 로직 제거 → 큐 + 도메인만
│   └── repository.rs       // DB trait 변경 없음
├── infrastructure/
│   ├── gh/                 // 변경 없음
│   ├── git/                // 변경 없음
│   ├── claude/             // 변경 없음
│   ├── workspace/          // WorkspaceOps trait + RealWorkspace
│   └── suggest_workflow/   // 변경 없음
├── queue/                  // 변경 없음
│   └── state_queue.rs
├── components/             // 변경 없음 (Task에서 사용)
│   ├── analyzer.rs
│   ├── reviewer.rs
│   ├── notifier.rs
│   └── verdict.rs
├── config/                 // ConfigLoader trait 추가
├── knowledge/              // 변경 없음
├── scanner/                // → sources/github.rs로 이동
└── tui/                    // 변경 없음
```

### 주요 변경

| 현재 | v3 | 설명 |
|------|-----|------|
| `daemon/mod.rs` (746줄) | `daemon/mod.rs` + `task_manager.rs` + `task_runner.rs` | 3분할 |
| `pipeline/issue.rs` | `tasks/analyze.rs` + `tasks/implement.rs` | Task trait 구현 |
| `pipeline/pr.rs` | `tasks/review.rs` + `tasks/improve.rs` | Task trait 구현 |
| `pipeline/merge.rs` | `tasks/merge.rs` | Task trait 구현 |
| `scanner/` | `sources/github.rs` | GitHubTaskSource 내부 |
| `domain/git_repository.rs` (1639줄) | 큐 + 도메인만 유지 (scanning 제거) | 책임 축소 |

---

## 14. 마이그레이션 전략

### Phase 1: Trait 정의 + DTO

1. `daemon/task_source.rs` — TaskSource trait
2. `daemon/task.rs` — Task trait + SkipReason + TaskResult
3. `daemon/agent.rs` — Agent trait + AgentRequest + AgentResponse
4. `infrastructure/workspace/` — WorkspaceOps trait 추출
5. `config/` — ConfigLoader trait 추출
6. 기존 코드 변경 없음 (additive only)

### Phase 2: Task 구현체

7. `tasks/analyze.rs` — pipeline/issue.rs의 analyze_one에서 추출
8. `tasks/implement.rs` — pipeline/issue.rs의 implement_one에서 추출
9. `tasks/review.rs` — pipeline/pr.rs의 review_one에서 추출
10. `tasks/improve.rs` — pipeline/pr.rs의 improve_one에서 추출
11. `tasks/merge.rs` — pipeline/merge.rs의 merge_one에서 추출
12. 각 Task의 before_invoke/after_invoke 테스트

### Phase 3: Source + Runner + Manager

13. `daemon/agent.rs` — ClaudeAgent 구현
14. `daemon/task_runner.rs` — TaskRunner (before → agent → after)
15. `sources/github.rs` — GitHubTaskSource (recovery + scan + Task 생성)
16. `daemon/task_manager.rs` — TaskManager (poll_all + apply + daily_report)
17. 각 컴포넌트 단위 테스트

### Phase 4: Daemon 전환

18. `daemon/mod.rs` — 새 Daemon struct로 event loop 재작성
19. `main.rs` — 새 Daemon 조립 (DI)
20. 기존 `pipeline/`, `scanner/` 모듈 제거
21. 통합 테스트

---

## 15. 테스트 전략

### Task 단위 테스트

```rust
// Mock Agent + Mock WorkspaceOps + Mock Gh
#[tokio::test]
async fn analyze_task_skips_closed_issue() {
    let gh = MockGh::new().with_issue_closed(42);
    let ctx = TaskContext { gh: Arc::new(gh), ... };
    let mut task = AnalyzeTask::new(ctx, issue_item(42));

    let result = task.before_invoke().await;
    assert!(matches!(result, Err(SkipReason::PreflightFailed(_))));
}

#[tokio::test]
async fn analyze_task_returns_implement_request() {
    let gh = MockGh::new().with_issue_open(42);
    let ws = MockWorkspace::new();
    let ctx = TaskContext { gh: Arc::new(gh), workspace: Arc::new(ws), ... };
    let mut task = AnalyzeTask::new(ctx, issue_item(42));

    let request = task.before_invoke().await.unwrap();
    assert!(request.prompt.contains("Analyze"));
    assert!(request.session_opts.json_schema.is_some());
}
```

### TaskRunner 테스트

```rust
#[tokio::test]
async fn runner_skips_task_on_preflight_failure() {
    let agent = MockAgent::new();
    let runner = TaskRunner::new(Arc::new(agent));
    let task = FailingPreflightTask::new();

    let result = runner.run(Box::new(task)).await;
    assert!(matches!(result.status, TaskStatus::Skipped(_)));
    assert_eq!(agent.invoke_count(), 0);  // agent 호출 안 됨
}
```

### GitHubTaskSource 테스트

```rust
#[tokio::test]
async fn poll_creates_analyze_task_for_pending_issue() {
    let gh = MockGh::new().with_labeled_issue(42, "autodev:analyze");
    let mut source = GitHubTaskSource::new(ctx, db);

    let tasks = source.poll().await;
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].work_id(), "issue:org/repo:42");
}
```

---

## 16. 관계 범례

```
  ──▶   소유 (owns / has-a)
  ─ ─▶  생성 (creates)
  ──▷   구현 (implements trait)
  ◇───▶ 선택적 의존 (optional dependency)
```
