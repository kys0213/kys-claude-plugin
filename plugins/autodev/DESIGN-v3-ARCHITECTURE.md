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
│ workspace, gh,   │  │  JiraTaskSource  │
│ config, db, ...  │  └──────────────────┘
│ async poll()     │
│  // 1. recovery  │
│  // 2. scan      │
│  → AnalyzeTask   │
│  → ImplementTask │
│  → ReviewTask    │
│  → ImproveTask   │
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
  ┌──────────┼──────────────────────┐
  │          │          │          │
  ▼          ▼          ▼          ▼
┌──────┐ ┌────────┐ ┌────────┐ ┌─────────┐
│Analy │ │Implmnt │ │Review  │ │Improve  │
│zeTask│ │Task    │ │Task    │ │Task     │
└──────┘ └────────┘ └────────┘ └─────────┘
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
    manager: Box<dyn TaskManager>,
    runner: Arc<dyn TaskRunner>,
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

trait으로 정의하여 Daemon이 Mock을 주입받을 수 있게 한다.

```rust
pub struct DefaultTaskManager {
    sources: Vec<Box<dyn TaskSource>>,
}
```

- `tick()`: 모든 source에서 `poll()`하여 실행 가능한 Task를 내부에 수집
- `drain_ready()`: 수집된 Task들을 꺼내서 반환
- `apply(TaskResult)`: Task 실행 결과를 source의 큐에 반영
- `schedule_daily_report()`: 일간 리포트 생성 시점 판단 + Task 생성

### TaskRunner (실행 엔진)

trait으로 정의하여 Daemon이 Mock을 주입받을 수 있게 한다.

```rust
pub struct DefaultTaskRunner {
    agent: Arc<dyn Agent>,
}
```

- `run(task)`: `before_invoke → agent.invoke → after_invoke` 생명주기 실행
- Agent 구현체가 무엇이든 동일하게 동작

---

## 4. Trait 정의

### TaskManager

```rust
#[async_trait]
pub trait TaskManager: Send + Sync {
    /// source들에서 실행 가능한 Task를 폴링하여 내부에 수집.
    async fn tick(&mut self);

    /// 수집된 Task들을 꺼내서 반환.
    fn drain_ready(&mut self) -> Vec<Box<dyn Task>>;

    /// Task 실행 결과를 source의 큐에 반영.
    fn apply(&mut self, result: TaskResult);
}
```

### TaskRunner

```rust
#[async_trait]
pub trait TaskRunner: Send + Sync {
    /// Task의 생명주기를 실행하고 결과를 반환.
    /// before_invoke → agent.invoke → after_invoke
    async fn run(&self, task: Box<dyn Task>) -> TaskResult;
}
```

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

각 Task는 자신에게 필요한 의존성만 개별 `Arc<dyn Trait>`로 생성자 주입받는다.

> ~~TaskContext~~ (폐기): TaskSource가 OCP로 확장 가능하므로, 각 소스가 생성하는
> Task는 서로 다른 의존성과 관심사를 가진다. 하나의 TaskContext로 묶으면
> 소스 추가 시 god object가 되어 OCP/ISP 위반. 각 Source가 자기 Task에
> 필요한 의존성을 직접 주입하는 패턴이 적절하다.

### AnalyzeTask

```rust
pub struct AnalyzeTask {
    workspace: Arc<dyn WorkspaceOps>,
    gh: Arc<dyn Gh>,
    config: Arc<dyn ConfigLoader>,
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
    workspace: Arc<dyn WorkspaceOps>,
    gh: Arc<dyn Gh>,
    config: Arc<dyn ConfigLoader>,
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
    workspace: Arc<dyn WorkspaceOps>,
    gh: Arc<dyn Gh>,
    config: Arc<dyn ConfigLoader>,
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
    workspace: Arc<dyn WorkspaceOps>,
    gh: Arc<dyn Gh>,
    config: Arc<dyn ConfigLoader>,
    item: PrItem,
    sw: Arc<dyn SuggestWorkflow>,  // 선택적 의존
}
```

| 메서드 | 동작 |
|--------|------|
| `before_invoke` | worktree 생성 + PR branch checkout → 피드백 반영 프롬프트 구성 (review_comment 포함) |
| `after_invoke` | commit + push → review_iteration 증가 → QueueOp::PushPr(Improved) → worktree 정리 |

---

## 7. GitHubTaskSource 상세

### 구조

```rust
pub struct GitHubTaskSource {
    workspace: Arc<dyn WorkspaceOps>,
    gh: Arc<dyn Gh>,
    config: Arc<dyn ConfigLoader>,
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
    for repo in self.repos.values_mut() {
        repo.scan_issues().await;
        repo.scan_approved_issues().await;
        repo.scan_pulls().await;
    }

    // 4. 큐에서 실행 가능한 Task 생성
    //    각 Task에 필요한 의존성을 개별 Arc::clone으로 주입
    let mut tasks: Vec<Box<dyn Task>> = Vec::new();
    for repo in self.repos.values_mut() {
        // issue Pending → AnalyzeTask
        if let Some(item) = repo.issue_queue.pop("Pending") {
            tasks.push(Box::new(AnalyzeTask::new(
                self.workspace.clone(), self.gh.clone(), self.config.clone(), item,
            )));
        }
        // issue Ready → ImplementTask
        if let Some(item) = repo.issue_queue.pop("Ready") {
            tasks.push(Box::new(ImplementTask::new(
                self.workspace.clone(), self.gh.clone(), self.config.clone(), item,
            )));
        }
        // pr Pending → ReviewTask
        if let Some(item) = repo.pr_queue.pop("Pending") {
            tasks.push(Box::new(ReviewTask::new(
                self.workspace.clone(), self.gh.clone(), self.config.clone(), item,
            )));
        }
        // pr ReviewDone → ImproveTask
        if let Some(item) = repo.pr_queue.pop("ReviewDone") {
            tasks.push(Box::new(ImproveTask::new(
                self.workspace.clone(), self.gh.clone(), self.config.clone(), item,
            )));
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
impl DefaultTaskRunner {
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

## 11. ~~TaskContext~~ (폐기)

> **폐기 사유**: TaskSource가 OCP로 확장 가능하므로, 각 소스가 생성하는 Task는
> 서로 다른 의존성과 관심사를 가진다 (GitHub → gh/git/workspace, Slack → slack client,
> Jira → jira client). 하나의 TaskContext로 묶으면:
>
> 1. 소스 추가 시 TaskContext가 비대해져 **god object** 화
> 2. 사용하지 않는 필드가 `Option`으로 채워져 **ISP 위반**
> 3. TaskSource OCP 달성의 의미가 퇴색
>
> **대안**: 각 TaskSource가 자기 Task에 필요한 의존성을 개별 `Arc<dyn Trait>`로
> 직접 주입. Task 생성자가 자신의 관심사만 받으므로 ISP 준수.
>
> `daemon/task_context.rs`는 dead code로 삭제 대상.

---

## 12. Queue Items (변경 없음)

v2에서 정의한 큐 아이템은 그대로 사용한다:

```
┌───────────┐  ┌───────────┐
│ IssueItem │  │  PrItem   │
│───────────│  │───────────│
│ work_id   │  │ work_id   │
│ repo_id   │  │ repo_id   │
│ repo_name │  │ repo_name │
│ number    │  │ number    │
│ body      │  │ head_br   │
│ labels    │  │ base_br   │
│ analysis_ │  │ review_   │
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
│   └── improve.rs          // ImproveTask
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
│   ├── verdict.rs
│   └── workspace.rs
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
| `pipeline/merge.rs` | (삭제) | Merge 파이프라인 제거 |
| `scanner/` | `sources/github.rs` | GitHubTaskSource 내부 |
| `domain/git_repository.rs` (1639줄) | 큐 + 도메인만 유지 (scanning 제거) | 책임 축소 |

---

## 14. 마이그레이션 전략 (TDD)

각 Phase에서 **인터페이스 정의 → 테스트 작성 (fail) → 구현 (pass)** 순서를 따른다.
SOLID 원칙으로 컴포넌트 간 의존이 trait 경계에서 끊기므로,
**각 컴포넌트의 단위 테스트만으로 정확성을 검증**할 수 있다.
컴포넌트 간 상호작용은 trait 계약이 보장하므로 통합 테스트는 작성하지 않는다.

```
❌ 구현 → 나중에 테스트
❌ 컴포넌트 A + B를 조합한 통합 테스트
✅ trait 정의 → Mock 구현 → 테스트 작성 (fail) → 구현 (pass)
✅ 각 컴포넌트를 Mock 의존성으로 격리하여 단위 테스트
```

### Phase 1: Trait 정의 + DTO

1. `daemon/task.rs` — Task trait + SkipReason + TaskResult + AgentRequest + AgentResponse
2. `daemon/task_source.rs` — TaskSource trait
3. `daemon/agent.rs` — Agent trait
4. `infrastructure/workspace/` — WorkspaceOps trait 추출
5. `config/` — ConfigLoader trait 추출
6. 기존 코드 변경 없음 (additive only)
7. 테스트 없음 (trait 정의만, 로직 없음)

### Phase 2: Task 구현체

각 Task마다 **테스트 먼저 → 구현** 순서로 진행한다.
Mock 의존성(MockGh, MockWorkspace, MockConfig)은 Phase 2 시작 시 한 번 작성한다.

```
Mock 작성 → AnalyzeTask 테스트(fail) → AnalyzeTask 구현(pass)
         → ImplementTask 테스트(fail) → ImplementTask 구현(pass)
         → ReviewTask 테스트(fail) → ReviewTask 구현(pass)
         → ImproveTask 테스트(fail) → ImproveTask 구현(pass)
```

8. Mock 의존성 모듈 작성 (`tests/mocks/`)
9. AnalyzeTask — 테스트 → 구현
10. ImplementTask — 테스트 → 구현
11. ReviewTask — 테스트 → 구현
12. ImproveTask — 테스트 → 구현

### Phase 3: Source + Runner + Manager

각 컴포넌트 역시 **테스트 먼저 → 구현**.
Task는 Phase 2에서 완성된 실제 구현이 아닌 MockTask로 테스트한다.

```
MockTask 작성 → TaskRunner 테스트(fail) → TaskRunner 구현(pass)
MockAgent 작성 → ClaudeAgent 테스트(fail) → ClaudeAgent 구현(pass)
MockTaskSource 작성 → TaskManager 테스트(fail) → TaskManager 구현(pass)
MockDb + MockGh → GitHubTaskSource 테스트(fail) → GitHubTaskSource 구현(pass)
```

14. TaskRunner — 테스트 → 구현
15. ClaudeAgent — 테스트 → 구현
16. TaskManager — 테스트 → 구현
17. GitHubTaskSource — 테스트 → 구현

### Phase 4: Daemon 전환

Daemon도 TaskManager/TaskRunner를 인터페이스로 의존하므로
MockTaskManager + MockTaskRunner를 주입하여 오케스트레이션 동작을 단위 테스트한다.

```
MockTaskManager + MockTaskRunner → Daemon 테스트(fail) → Daemon 구현(pass)
```

18. Daemon — 테스트 → 구현
19. `main.rs` — 새 Daemon 조립 (DI)
20. 기존 `pipeline/`, `scanner/` 모듈 제거

---

## 15. 테스트 전략

### 원칙

SOLID 준수로 각 컴포넌트가 trait 경계에서 완전히 분리되므로:

1. **단위 테스트만 작성한다** — Mock 의존성을 주입하여 각 컴포넌트를 격리 검증
2. **통합 테스트는 작성하지 않는다** — 컴포넌트 간 계약은 trait이 보장
3. **블랙박스 테스트** — 내부 구현이 아닌 입출력(AgentRequest/TaskResult)을 검증
4. **TDD 순서** — 인터페이스 정의 → 테스트 작성(fail) → 구현(pass)

```
┌──────────────────────────────────────────────────────────────────────┐
│  각 컴포넌트의 테스트 경계                                              │
│                                                                      │
│  ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌──────────┐               │
│  │  Task   │  │ Runner  │  │ Source   │  │  Daemon  │               │
│  │─────────│  │─────────│  │─────────│  │──────────│               │
│  │ Mock:   │  │ Mock:   │  │ Mock:   │  │ Mock:    │               │
│  │  Gh     │  │  Agent  │  │  Gh     │  │  Manager │               │
│  │  WsOps  │  │  Task   │  │  Db     │  │  Runner  │               │
│  │  Config │  │         │  │         │  │          │               │
│  │─────────│  │─────────│  │─────────│  │──────────│               │
│  │ 검증:   │  │ 검증:   │  │ 검증:   │  │ 검증:    │               │
│  │  before │  │  skip시 │  │  poll이 │  │  tick시  │               │
│  │   →Req  │  │  agent  │  │  올바른 │  │  drain   │               │
│  │  after  │  │  미호출 │  │  Task를 │  │  +spawn  │               │
│  │   →Res  │  │  정상시 │  │  생성   │  │  완료시  │               │
│  │         │  │ lifecycle│  │  apply가│  │  apply   │               │
│  │         │  │  호출순서│  │  큐반영 │  │  inflight│               │
│  └─────────┘  └─────────┘  └─────────┘  │  제어    │               │
│                                          └──────────┘               │
│                                                                      │
│  컴포넌트 간 연결은 trait 계약이 보장 → 통합 테스트 불필요               │
└──────────────────────────────────────────────────────────────────────┘
```

### Task 단위 테스트 (before_invoke / after_invoke 각각 격리)

```rust
// ── before_invoke 테스트 ──

#[tokio::test]
async fn analyze_before_skips_closed_issue() {
    // Given: 이미 닫힌 이슈
    let gh = MockGh::new().with_issue_closed(42);
    let ws = Arc::new(MockWorkspace::new());
    let config = Arc::new(MockConfig::new());
    let mut task = AnalyzeTask::new(ws.clone(), gh.clone(), config.clone(), issue_item(42));

    // When
    let result = task.before_invoke().await;

    // Then: preflight 실패로 skip
    assert!(matches!(result, Err(SkipReason::PreflightFailed(_))));
}

#[tokio::test]
async fn analyze_before_creates_worktree_and_returns_request() {
    // Given: open 이슈
    let gh = Arc::new(MockGh::new().with_issue_open(42));
    let ws = Arc::new(MockWorkspace::new());
    let config = Arc::new(MockConfig::new());
    let mut task = AnalyzeTask::new(ws.clone(), gh.clone(), config.clone(), issue_item(42));

    // When
    let request = task.before_invoke().await.unwrap();

    // Then: worktree 생성됨 + 분석 프롬프트 + JSON schema
    assert_eq!(ws.created_worktrees(), 1);
    assert!(request.prompt.contains("Analyze"));
    assert!(request.session_opts.json_schema.is_some());
}

// ── after_invoke 테스트 ──

#[tokio::test]
async fn analyze_after_implement_verdict_posts_comment_and_exits_queue() {
    // Given: 분석 결과가 implement verdict
    let gh = Arc::new(MockGh::new());
    let ws = Arc::new(MockWorkspace::new());
    let config = Arc::new(MockConfig::new());
    let mut task = AnalyzeTask::new(ws.clone(), gh.clone(), config.clone(), issue_item(42));
    let response = AgentResponse::ok(r#"{"verdict":"implement","report":"..."}"#);

    // When
    let result = task.after_invoke(response).await;

    // Then: 분석 코멘트 게시 + analyzed 라벨 + 큐에서 제거
    assert_eq!(gh.comments_posted(), 1);
    assert!(gh.label_added("autodev:analyzed"));
    assert!(gh.label_removed("autodev:wip"));
    assert!(matches!(result.queue_ops[0], QueueOp::Remove));
}

#[tokio::test]
async fn analyze_after_clarify_verdict_marks_skip() {
    // Given: 분석 결과가 clarify verdict
    let gh = Arc::new(MockGh::new());
    let ws = Arc::new(MockWorkspace::new());
    let config = Arc::new(MockConfig::new());
    let mut task = AnalyzeTask::new(ws.clone(), gh.clone(), config.clone(), issue_item(42));
    let response = AgentResponse::ok(r#"{"verdict":"clarify","report":"..."}"#);

    // When
    let result = task.after_invoke(response).await;

    // Then: skip 라벨 + 큐에서 제거
    assert!(matches!(result.queue_ops[0], QueueOp::Remove));
}
```

### implement, review 등도 동일 패턴

```rust
// ImplementTask: before_invoke는 worktree + feature branch 생성 검증
// ImplementTask: after_invoke는 PR 생성 + PushPr 큐 연산 검증

#[tokio::test]
async fn implement_after_creates_pr_and_pushes_to_pr_queue() {
    let gh = Arc::new(MockGh::new().with_pr_created(99));
    let ws = Arc::new(MockWorkspace::new());
    let config = Arc::new(MockConfig::new());
    let mut task = ImplementTask::new(ws.clone(), gh.clone(), config.clone(), issue_item_ready(42));
    let response = AgentResponse::ok("https://github.com/org/repo/pull/99");

    let result = task.after_invoke(response).await;

    assert!(gh.label_added("autodev:wip"));   // PR에 wip 라벨
    assert!(matches!(result.queue_ops[0], QueueOp::Remove));  // issue 큐 제거
    assert!(matches!(result.queue_ops[1], QueueOp::PushPr { .. }));  // PR 큐 push
}

// ReviewTask: after_invoke approve → done 라벨 + source_issue done 검증
#[tokio::test]
async fn review_after_approve_transitions_issue_to_done() {
    let gh = Arc::new(MockGh::new());
    let ws = Arc::new(MockWorkspace::new());
    let config = Arc::new(MockConfig::new());
    let item = pr_item(99).with_source_issue(42);
    let mut task = ReviewTask::new(ws.clone(), gh.clone(), config.clone(), item);
    let response = AgentResponse::ok(r#"{"verdict":"approve","review":"LGTM"}"#);

    let result = task.after_invoke(response).await;

    // PR done
    assert!(gh.label_added_for(99, "autodev:done"));
    // source issue도 done
    assert!(gh.label_added_for(42, "autodev:done"));
    assert!(gh.label_removed_for(42, "autodev:implementing"));
}
```

### TaskRunner 단위 테스트

```rust
#[tokio::test]
async fn runner_skips_without_calling_agent() {
    // Given: before_invoke가 실패하는 Task
    let agent = MockAgent::new();
    let runner = TaskRunner::new(Arc::new(agent.clone()));
    let task = MockTask::failing_preflight("issue:org/repo:42");

    // When
    let result = runner.run(Box::new(task)).await;

    // Then: agent 호출 안 됨 + Skipped 상태
    assert_eq!(agent.invoke_count(), 0);
    assert!(matches!(result.status, TaskStatus::Skipped(_)));
}

#[tokio::test]
async fn runner_calls_agent_and_after_invoke() {
    // Given: 정상 Task + Agent
    let agent = MockAgent::new().returning(AgentResponse::ok("output"));
    let runner = TaskRunner::new(Arc::new(agent.clone()));
    let task = MockTask::succeeding("issue:org/repo:42");

    // When
    let result = runner.run(Box::new(task)).await;

    // Then: agent 1회 호출 + Completed 상태
    assert_eq!(agent.invoke_count(), 1);
    assert!(matches!(result.status, TaskStatus::Completed));
}
```

### GitHubTaskSource 단위 테스트

```rust
#[tokio::test]
async fn poll_creates_analyze_task_for_labeled_issue() {
    // Given: autodev:analyze 라벨이 있는 이슈
    let gh = MockGh::new().with_labeled_issues(vec![
        (42, vec!["autodev:analyze"]),
    ]);
    let db = MockDb::new();
    let mut source = GitHubTaskSource::new(Arc::new(MockWorkspace::new()), gh, Arc::new(MockConfig::new()), db);

    // When
    let tasks = source.poll().await;

    // Then: AnalyzeTask 1개 생성
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].work_id(), "issue:org/repo:42");
}

#[tokio::test]
async fn poll_skips_already_queued_item() {
    // Given: 이미 큐에 있는 이슈
    let gh = MockGh::new().with_labeled_issues(vec![
        (42, vec!["autodev:analyze"]),
    ]);
    let mut source = GitHubTaskSource::new(Arc::new(MockWorkspace::new()), gh, Arc::new(MockConfig::new()), MockDb::new());

    // When: 첫 poll → 두 번째 poll
    let first = source.poll().await;
    let second = source.poll().await;

    // Then: 두 번째는 비어있음 (dedup)
    assert_eq!(first.len(), 1);
    assert_eq!(second.len(), 0);
}

#[tokio::test]
async fn apply_removes_item_from_queue() {
    // Given: 큐에 아이템이 있는 상태
    let mut source = source_with_queued_issue(42);
    let result = TaskResult {
        work_id: "issue:org/repo:42".into(),
        repo_name: "org/repo".into(),
        queue_ops: vec![QueueOp::Remove],
        ..Default::default()
    };

    // When
    source.apply(&result);

    // Then: 큐에서 제거됨
    assert!(!source.contains("issue:org/repo:42"));
}
```

### TaskManager 단위 테스트

```rust
#[tokio::test]
async fn poll_all_collects_from_all_sources() {
    // Given: 두 source에서 각각 task를 반환
    let source1 = MockTaskSource::returning(vec![mock_task("a")]);
    let source2 = MockTaskSource::returning(vec![mock_task("b")]);
    let mut manager = TaskManager::new(vec![
        Box::new(source1),
        Box::new(source2),
    ]);

    // When
    manager.tick().await;
    let tasks = manager.drain_ready();

    // Then: 2개 task 수집
    assert_eq!(tasks.len(), 2);
}

#[tokio::test]
async fn apply_delegates_to_correct_source() {
    // Given
    let source = MockTaskSource::new();
    let mut manager = TaskManager::new(vec![Box::new(source.clone())]);
    let result = TaskResult { work_id: "issue:org/repo:42".into(), .. };

    // When
    manager.apply(result);

    // Then: source.apply() 1회 호출
    assert_eq!(source.apply_count(), 1);
}
```

### Daemon 단위 테스트

Daemon은 MockTaskManager + MockTaskRunner를 주입받아 오케스트레이션 로직을 검증한다.

```rust
#[tokio::test]
async fn daemon_spawns_tasks_from_manager_to_runner() {
    // Given: manager가 task 2개를 반환
    let manager = MockTaskManager::new()
        .on_drain(vec![mock_task("a"), mock_task("b")]);
    let runner = MockTaskRunner::new()
        .returning(TaskResult::completed("a"))
        .returning(TaskResult::completed("b"));
    let mut daemon = Daemon::new(
        Box::new(manager.clone()),
        Arc::new(runner.clone()),
        InFlightTracker::new(10),
    );

    // When: 1 tick 실행
    daemon.run_one_tick().await;

    // Then: runner에 2개 task 전달됨
    assert_eq!(runner.run_count(), 2);
}

#[tokio::test]
async fn daemon_applies_completed_result_to_manager() {
    // Given: runner가 결과를 반환
    let manager = MockTaskManager::new()
        .on_drain(vec![mock_task("a")]);
    let runner = MockTaskRunner::new()
        .returning(TaskResult::completed("a"));
    let mut daemon = Daemon::new(
        Box::new(manager.clone()),
        Arc::new(runner),
        InFlightTracker::new(10),
    );

    // When: tick → task 완료
    daemon.run_one_tick().await;

    // Then: manager.apply() 호출됨
    assert_eq!(manager.apply_count(), 1);
}

#[tokio::test]
async fn daemon_respects_inflight_limit() {
    // Given: inflight 최대 1개, task 3개
    let manager = MockTaskManager::new()
        .on_drain(vec![mock_task("a"), mock_task("b"), mock_task("c")]);
    let runner = MockTaskRunner::new()
        .returning_delayed(TaskResult::completed("a"), Duration::from_millis(50));
    let mut daemon = Daemon::new(
        Box::new(manager),
        Arc::new(runner.clone()),
        InFlightTracker::new(1),  // 최대 1개
    );

    // When: 1 tick
    daemon.run_one_tick().await;

    // Then: 동시에 1개만 spawn됨 (나머지는 다음 tick에서)
    assert_eq!(runner.concurrent_max(), 1);
}

#[tokio::test]
async fn daemon_spawns_immediately_after_task_completion() {
    // Given: inflight 최대 1개, task 2개
    let manager = MockTaskManager::new()
        .on_drain(vec![mock_task("a"), mock_task("b")]);
    let runner = MockTaskRunner::new()
        .returning(TaskResult::completed("a"))
        .returning(TaskResult::completed("b"));
    let mut daemon = Daemon::new(
        Box::new(manager.clone()),
        Arc::new(runner.clone()),
        InFlightTracker::new(1),
    );

    // When: 루프 실행 (task "a" 완료 → 즉시 "b" spawn)
    daemon.run_until_idle().await;

    // Then: 2개 모두 실행됨 + apply 2회
    assert_eq!(runner.run_count(), 2);
    assert_eq!(manager.apply_count(), 2);
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
