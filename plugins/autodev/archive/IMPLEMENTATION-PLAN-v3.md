# IMPLEMENTATION PLAN: v3 Daemon Architecture Refactoring

> **Date**: 2026-02-28
> **Base Document**: [DESIGN-v3-ARCHITECTURE.md](DESIGN-v3-ARCHITECTURE.md)
> **Branch**: `claude/autodev-solid-design-v2-EYEe2`

---

## 현재 → v3 변경 맵

| 현재 파일 | LOC | v3 대응 | 변경 |
|-----------|-----|---------|------|
| `daemon/mod.rs` | 746 | `daemon/mod.rs` (Daemon) + `task_manager.rs` + `task_runner.rs` | 3분할 |
| `pipeline/issue.rs` | 998 | `tasks/analyze.rs` + `tasks/implement.rs` | Task trait 구현 |
| `pipeline/pr.rs` | 1000+ | `tasks/review.rs` + `tasks/improve.rs` | Task trait 구현 |
| `pipeline/merge.rs` | 340 | `tasks/merge.rs` | Task trait 구현 |
| `pipeline/mod.rs` | 210 | 제거 (TaskOutput → TaskResult) | DTO 이동 |
| `domain/git_repository.rs` | 1639 | 큐+도메인만 유지, scan → `sources/github.rs` | 책임 축소 |
| `components/workspace.rs` | 85 | `infrastructure/workspace/` (WorkspaceOps trait) | trait 추출 |
| `config/mod.rs` | 131 | ConfigLoader trait 추가 | trait 추가 |
| `scanner/` | - | `sources/github.rs` 내부로 흡수 | 이동 |

---

## Phase 1: Trait 정의 + DTO (additive only)

기존 코드 변경 없이, v3의 모든 trait과 DTO를 추가한다.

### 1-1. `daemon/task.rs` — Task trait + DTO

```rust
use std::path::PathBuf;
use std::time::Duration;
use async_trait::async_trait;

use crate::domain::models::NewConsumerLog;
use crate::infrastructure::claude::SessionOptions;
use crate::pipeline::QueueOp;  // 기존 QueueOp 재사용

// ─── Agent 요청/응답 DTO ───

pub struct AgentRequest {
    pub working_dir: PathBuf,
    pub prompt: String,
    pub system_prompt: Option<String>,
    pub session_opts: SessionOptions,
}

pub struct AgentResponse {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub duration: Duration,
}

impl AgentResponse {
    pub fn error(msg: impl ToString) -> Self { ... }
}

// ─── Task 결과 DTO ───

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

pub enum SkipReason {
    PreflightFailed(String),
    AlreadyProcessed,
}

// ─── Task trait ───

#[async_trait]
pub trait Task: Send + Sync {
    fn work_id(&self) -> &str;
    fn repo_name(&self) -> &str;
    async fn before_invoke(&mut self) -> Result<AgentRequest, SkipReason>;
    async fn after_invoke(&mut self, response: AgentResponse) -> TaskResult;
}
```

### 1-2. `daemon/task_source.rs` — TaskSource trait

```rust
#[async_trait]
pub trait TaskSource: Send + Sync {
    async fn poll(&mut self) -> Vec<Box<dyn Task>>;
    fn apply(&mut self, result: &TaskResult);
}
```

### 1-3. `daemon/agent.rs` — Agent trait

```rust
#[async_trait]
pub trait Agent: Send + Sync {
    async fn invoke(&self, request: AgentRequest) -> AgentResponse;
}
```

### 1-4. `daemon/task_manager.rs` — TaskManager trait (skeleton)

```rust
#[async_trait]
pub trait TaskManager: Send + Sync {
    async fn tick(&mut self);
    fn drain_ready(&mut self) -> Vec<Box<dyn Task>>;
    fn apply(&mut self, result: TaskResult);
}
```

### 1-5. `daemon/task_runner.rs` — TaskRunner trait (skeleton)

```rust
#[async_trait]
pub trait TaskRunner: Send + Sync {
    async fn run(&self, task: Box<dyn Task>) -> TaskResult;
}
```

### 1-6. `infrastructure/workspace/mod.rs` — WorkspaceOps trait 추출

```rust
#[async_trait]
pub trait WorkspaceOps: Send + Sync {
    async fn ensure_cloned(&self, url: &str, name: &str) -> Result<PathBuf>;
    async fn create_worktree(&self, repo_name: &str, task_id: &str, branch: Option<&str>) -> Result<PathBuf>;
    async fn remove_worktree(&self, repo_name: &str, task_id: &str) -> Result<()>;
}
```

기존 `Workspace` struct에 `impl WorkspaceOps for Workspace<'_>` 추가.

### 1-7. `config/mod.rs` — ConfigLoader trait 추출

```rust
pub trait ConfigLoader: Send + Sync {
    fn load(&self, workspace_path: Option<&Path>) -> MergedConfig;
}
```

### ~~1-8. `daemon/task_context.rs` — TaskContext~~ (폐기 → ✅ 삭제 완료)

> TaskContext는 성급한 추상화로 판단하여 폐기.
> TaskSource가 OCP로 확장 가능하므로, 각 소스가 생성하는 Task는 서로 다른
> 의존성을 가진다. 각 Source가 자기 Task에 필요한 의존성을 개별 `Arc<dyn Trait>`로
> 직접 주입하는 패턴이 적절.
> ~~기존 `daemon/task_context.rs`는 dead code로 삭제 대상.~~ → 삭제 완료.

### 수정 파일
- `daemon/mod.rs`: 새 모듈 선언 추가 (`pub mod task; pub mod agent;` 등)
- `infrastructure/mod.rs`: `pub mod workspace;` 추가

### 완료 기준
- `cargo build` 성공 (기존 코드 변경 없음)
- trait 정의만이므로 테스트 없음

---

## Phase 2: Task 구현체 (TDD)

### 실행 순서

```
Mock 작성 → AnalyzeTask 테스트(fail) → 구현(pass)
          → ImplementTask 테스트(fail) → 구현(pass)
          → ReviewTask 테스트(fail) → 구현(pass)
          → ImproveTask 테스트(fail) → 구현(pass)
          → MergeTask 테스트(fail) → 구현(pass)
```

### 2-0. Mock 의존성 작성

Phase 1의 trait에 대한 테스트용 mock:

```rust
// tests/mocks/mod.rs 또는 각 Task 파일의 #[cfg(test)] 내부

struct MockWorkspace { ... }   // ensure_cloned, create_worktree 호출 기록
struct MockConfigLoader { ... } // 고정 config 반환
// MockGh, MockClaude는 기존 것 재사용
```

### 2-1. `tasks/analyze.rs` — AnalyzeTask

**현재 코드**: `pipeline/issue.rs` → `analyze_one()` (230줄)

| 메서드 | 현재 코드 위치 | 로직 |
|--------|--------------|------|
| `before_invoke` | issue.rs:571-645 | preflight(issue open?) → worktree 생성 → prompt 구성 |
| `after_invoke` | issue.rs:647-773 | verdict 파싱 → 라벨/코멘트 → QueueOp |

```rust
pub struct AnalyzeTask {
    workspace: Arc<dyn WorkspaceOps>,
    gh: Arc<dyn Gh>,
    config: Arc<dyn ConfigLoader>,
    item: IssueItem,
    // before_invoke에서 생성되는 상태
    wt_path: Option<PathBuf>,
    task_id: String,
}
```

**before_invoke**:
1. `ctx.gh.api_get_field()` → issue open 확인
2. `ctx.workspace.ensure_cloned()` + `create_worktree()`
3. 분석 프롬프트 + JSON schema로 `AgentRequest` 구성

**after_invoke**:
1. `output::parse_analysis()` → verdict 파싱
2. verdict 분기:
   - implement → comment + `analyzed` 라벨 + `QueueOp::Remove`
   - clarify/wontfix → comment + `skip` 라벨 + `QueueOp::Remove`
   - fallback → comment + `analyzed` 라벨 + `QueueOp::Remove`
3. worktree 정리

**테스트** (before_invoke / after_invoke 각각 격리):

```
analyze_before_skips_closed_issue
analyze_before_creates_worktree_and_returns_request
analyze_after_implement_verdict_posts_comment_and_removes
analyze_after_clarify_verdict_marks_skip
analyze_after_wontfix_verdict_marks_skip
analyze_after_low_confidence_marks_skip
analyze_after_parse_failure_fallback_analyzed
analyze_after_nonzero_exit_removes
```

### 2-2. `tasks/implement.rs` — ImplementTask

**현재 코드**: `pipeline/issue.rs` → `implement_one()` (200줄)

```rust
pub struct ImplementTask {
    workspace: Arc<dyn WorkspaceOps>,
    gh: Arc<dyn Gh>,
    config: Arc<dyn ConfigLoader>,
    item: IssueItem,
    wt_path: Option<PathBuf>,
    task_id: String,
}
```

**before_invoke**:
1. `ctx.workspace.ensure_cloned()` + `create_worktree()`
2. config에서 `workflow.issue` 로드
3. 구현 프롬프트 + system_prompt 구성

**after_invoke**:
1. exit_code 확인
2. `output::extract_pr_number()` + `find_existing_pr()` fallback
3. PR 번호 있으면:
   - PR에 `wip` 라벨 + issue에 `pr-link` 코멘트
   - `QueueOp::Remove` + `QueueOp::PushPr`
4. PR 번호 없으면:
   - `implementing` 라벨 제거 + `QueueOp::Remove`
5. worktree 정리

**테스트**:

```
implement_before_creates_worktree_and_returns_request
implement_after_creates_pr_and_pushes_to_pr_queue
implement_after_pr_extract_fail_removes_implementing
implement_after_nonzero_exit_removes
implement_after_uses_find_existing_pr_fallback
```

### 2-3. `tasks/review.rs` — ReviewTask

**현재 코드**: `pipeline/pr.rs` → `review_one()` + `re_review_one()` 통합

```rust
pub struct ReviewTask {
    workspace: Arc<dyn WorkspaceOps>,
    gh: Arc<dyn Gh>,
    config: Arc<dyn ConfigLoader>,
    item: PrItem,
    wt_path: Option<PathBuf>,
    task_id: String,
}
```

**before_invoke**:
1. preflight: PR mergeable 확인
2. worktree + PR branch checkout
3. 리뷰 프롬프트 구성 (review_iteration에 따라 분기)

**after_invoke**:
1. `output::parse_review()` → verdict 파싱
2. approve:
   - `pr_review("APPROVE")` + `done` 라벨
   - source_issue가 있으면 `implementing` → `done` 전이
   - knowledge extraction 호출
   - `QueueOp::Remove`
3. request_changes:
   - `pr_review("REQUEST_CHANGES")` + 리뷰 코멘트 저장
   - max_review_iterations 초과 시 `QueueOp::Remove`
   - 아니면 `QueueOp::PushPr(ReviewDone)`
4. worktree 정리

**테스트**:

```
review_before_skips_closed_pr
review_before_creates_worktree_with_pr_branch
review_after_approve_transitions_pr_and_source_issue_to_done
review_after_request_changes_pushes_to_review_done
review_after_max_iterations_removes
review_after_nonzero_exit_removes
```

### 2-4. `tasks/improve.rs` — ImproveTask

**현재 코드**: `pipeline/pr.rs` → `improve_one()` (170줄)

```rust
pub struct ImproveTask {
    workspace: Arc<dyn WorkspaceOps>,
    gh: Arc<dyn Gh>,
    config: Arc<dyn ConfigLoader>,
    item: PrItem,
    wt_path: Option<PathBuf>,
    task_id: String,
    sw: Arc<dyn SuggestWorkflow>,
}
```

**before_invoke**:
1. worktree + PR branch checkout
2. 피드백 반영 프롬프트 구성 (review_comment 포함)

**after_invoke**:
1. exit_code 확인
2. `review_iteration + 1` + `QueueOp::PushPr(Improved)`
3. worktree 정리

**테스트**:

```
improve_before_creates_worktree_with_pr_branch
improve_after_success_pushes_to_improved
improve_after_nonzero_exit_removes
```

### ~~2-5. `tasks/merge.rs` — MergeTask~~ (scope 외 → 제거)

> DESIGN-v2.md §12 "Scope 외"에서 Merge 파이프라인을 명시적으로 제외.
> `autodev:done` 이후의 머지는 사람의 판단 또는 별도 자동화가 처리한다.
> MergeTask는 구현하지 않으며, 이 항목은 IMPL-PLAN에서 제거한다.

### 수정 파일
- `lib.rs`: `pub mod tasks;` 추가

### 완료 기준
- 4개 Task 구현체 (Analyze, Implement, Review, Improve) × 각 before/after 테스트 → 모든 테스트 통과
- `cargo test` 통과
- 기존 `pipeline/` 코드는 아직 변경하지 않음
- ~~MergeTask~~: DESIGN-v2 scope 외로 제거

---

## Phase 3: Source + Runner + Manager + ClaudeAgent (TDD)

### 실행 순서

```
MockTask → TaskRunner 테스트(fail) → 구현(pass)
MockAgent → ClaudeAgent 테스트(fail) → 구현(pass)
MockTaskSource → DefaultTaskManager 테스트(fail) → 구현(pass)
MockDb+MockGh → GitHubTaskSource 테스트(fail) → 구현(pass)
```

### 3-1. `daemon/task_runner.rs` — DefaultTaskRunner

```rust
pub struct DefaultTaskRunner {
    agent: Arc<dyn Agent>,
}

impl DefaultTaskRunner {
    pub async fn run(&self, mut task: Box<dyn Task>) -> TaskResult {
        let request = match task.before_invoke().await {
            Ok(req) => req,
            Err(skip) => return TaskResult::skipped(task, skip),
        };
        let response = self.agent.invoke(request).await;
        task.after_invoke(response).await
    }
}
```

**테스트**:

```
runner_skips_without_calling_agent
runner_calls_agent_and_after_invoke
runner_returns_failed_on_agent_error
```

### 3-2. `daemon/agent.rs` — ClaudeAgent

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
            Ok(r) => AgentResponse { ... },
            Err(e) => AgentResponse::error(e),
        }
    }
}
```

**테스트**:

```
claude_agent_maps_session_result_to_response
claude_agent_maps_error_to_error_response
```

### 3-3. `daemon/task_manager.rs` — DefaultTaskManager

```rust
pub struct DefaultTaskManager {
    sources: Vec<Box<dyn TaskSource>>,
    ready_tasks: Vec<Box<dyn Task>>,
}

#[async_trait]
impl TaskManager for DefaultTaskManager {
    async fn tick(&mut self) {
        for source in &mut self.sources {
            let tasks = source.poll().await;
            self.ready_tasks.extend(tasks);
        }
    }

    fn drain_ready(&mut self) -> Vec<Box<dyn Task>> {
        std::mem::take(&mut self.ready_tasks)
    }

    fn apply(&mut self, result: TaskResult) {
        for source in &mut self.sources {
            source.apply(&result);
        }
    }
}
```

**테스트**:

```
tick_collects_from_all_sources
drain_ready_returns_and_clears
apply_delegates_to_sources
```

### 3-4. `sources/github.rs` — GitHubTaskSource

**현재 코드 출처**:
- `domain/git_repository.rs`의 scan 메서드들 → `poll()` 내부로
- `daemon/mod.rs`의 repo sync 로직 → `poll()` 내부로
- `daemon/mod.rs`의 `apply_queue_ops()` → `apply()` 로

```rust
pub struct GitHubTaskSource {
    workspace: Arc<dyn WorkspaceOps>,
    gh: Arc<dyn Gh>,
    config: Arc<dyn ConfigLoader>,
    repos: HashMap<String, GitRepository>,
    db: Arc<Database>,
}

#[async_trait]
impl TaskSource for GitHubTaskSource {
    async fn poll(&mut self) -> Vec<Box<dyn Task>> {
        self.sync_repos().await;
        self.run_recovery().await;
        self.run_scans().await;
        self.drain_queue_items()
    }

    fn apply(&mut self, result: &TaskResult) {
        // apply_queue_ops 로직 이동
    }
}
```

**테스트**:

```
poll_creates_analyze_task_for_labeled_issue
poll_skips_already_queued_item
poll_creates_implement_task_for_approved_issue
poll_creates_review_task_for_labeled_pr
apply_removes_item_from_queue
apply_pushes_pr_item_to_queue
```

### 수정 파일
- `lib.rs`: `pub mod sources;` 추가

### 완료 기준
- 4개 컴포넌트 × 각 2-4개 테스트 → 모든 테스트 통과
- `cargo test` 통과

---

## Phase 4: Daemon 전환 + Legacy 제거

### 4-1. `daemon/mod.rs` — 새 Daemon struct

```rust
pub struct Daemon {
    manager: Box<dyn TaskManager>,
    runner: Arc<dyn TaskRunner>,
    inflight: InFlightTracker,  // 기존 코드 재사용
    db: Database,
}

impl Daemon {
    pub async fn run(&mut self) -> Result<()> {
        let mut join_set = JoinSet::new();
        let mut tick = interval(self.config.tick_interval);
        let mut heartbeat = interval(Duration::from_secs(5));

        loop {
            select! {
                Some(result) = join_set.join_next() => {
                    let result = result?;
                    self.inflight.release(&result.repo_name);
                    // DB logging
                    for log in &result.logs {
                        let _ = self.db.log_insert(log);
                    }
                    self.manager.apply(result);
                    self.try_spawn(&mut join_set).await;
                }
                _ = tick.tick() => {
                    self.manager.tick().await;
                    self.try_spawn(&mut join_set).await;
                }
                _ = heartbeat.tick() => { self.write_heartbeat(); }
                _ = signal::ctrl_c() => break,
            }
        }

        // Graceful shutdown
        while let Some(result) = join_set.join_next().await {
            self.manager.apply(result?);
        }
        Ok(())
    }

    fn try_spawn(&mut self, join_set: &mut JoinSet<TaskResult>) {
        for task in self.manager.drain_ready() {
            if !self.inflight.can_spawn() { break; }
            self.inflight.track(task.repo_name());
            let runner = self.runner.clone();
            join_set.spawn(async move { runner.run(task).await });
        }
    }
}
```

**테스트** (MockTaskManager + MockTaskRunner 주입):

```
daemon_spawns_tasks_from_manager
daemon_applies_completed_result
daemon_respects_inflight_limit
daemon_spawns_immediately_after_completion
```

### 4-2. `main.rs` — 새 Daemon 조립 (DI)

```rust
// 기존: daemon::start(home, env, gh, git, claude, sw)
// 변경: 각 컴포넌트에 필요한 의존성을 개별 Arc로 주입 (TaskContext 없음)
let workspace = Arc::new(RealWorkspace::new(git.clone(), env.clone()));
let config_loader = Arc::new(RealConfigLoader::new(env.clone()));
let agent = Arc::new(ClaudeAgent::new(claude.clone()));
let source = Box::new(GitHubTaskSource::new(
    workspace.clone(), gh.clone(), config_loader.clone(), env.clone(), git.clone(), sw.clone(), db.clone(),
));
let manager = Box::new(DefaultTaskManager::new(vec![source]));
let runner = Arc::new(DefaultTaskRunner::new(agent));
let mut daemon = Daemon::new(manager, runner, max_concurrent_tasks, db);
daemon.run().await?;
```

### 4-3. Legacy 코드 제거

| 제거 대상 | 이유 |
|-----------|------|
| `pipeline/issue.rs` 전체 | `tasks/analyze.rs` + `tasks/implement.rs`로 대체 |
| `pipeline/pr.rs` 전체 | `tasks/review.rs` + `tasks/improve.rs`로 대체 |
| `pipeline/merge.rs` 전체 | `tasks/merge.rs`로 대체 |
| `pipeline/mod.rs`의 `process_all()` | `TaskManager.tick()`으로 대체 |
| `pipeline/mod.rs`의 `TaskOutput` | `TaskResult`로 대체 |
| `pipeline/mod.rs`의 `handle_task_output()` | `TaskSource.apply()`로 대체 |
| `daemon/mod.rs`의 `spawn_ready_tasks()` | `Daemon.try_spawn()`으로 대체 |
| `daemon/mod.rs`의 `apply_queue_ops()` | `GitHubTaskSource.apply()`로 대체 |
| `daemon/mod.rs`의 기존 `start()` | `Daemon.run()`으로 대체 |
| `domain/git_repository.rs`의 scan 메서드들 | `GitHubTaskSource.poll()`로 이동 |
| `scanner/` 모듈 | `sources/github.rs`로 흡수 |

**유지**:
- `pipeline/mod.rs`의 `AGENT_SYSTEM_PROMPT` → `tasks/` 내부로 이동
- `pipeline/mod.rs`의 `QueueOp` → `daemon/task.rs`로 이동 (또는 별도 공유 모듈)
- `domain/git_repository.rs`의 큐 관련 코드 (StateQueue, contains, etc.)
- `components/` 전체 (Task에서 내부적으로 사용)
- `infrastructure/` 전체 (기존 trait 유지)

### 4-4. `#[allow]` 제거

| 현재 | 위치 | Phase 4 이후 |
|------|------|-------------|
| `#[allow(clippy::too_many_arguments)]` | pipeline/issue.rs, pr.rs, merge.rs, daemon/mod.rs | **제거** (개별 Arc 주입 + Task struct 필드로 전환) |
| `#[allow(dead_code)]` | pipeline/mod.rs (process_all, handle_task_output) | **제거** (코드 자체 제거) |
| `#[allow(dead_code)]` | daemon/recovery.rs | 확인 후 제거 |

### 완료 기준
- `cargo build` 성공
- `cargo test` 전체 통과
- `cargo clippy -- -D warnings` 통과
- `cargo fmt --check` 통과
- `pipeline/` 디렉토리 제거 완료
- `#[allow(clippy::too_many_arguments)]` 0건

---

## Phase 간 의존성

```
Phase 1 (trait 정의) ← 독립, additive only
    │
    ▼
Phase 2 (Task 구현체) ← Phase 1 필요
    │
    ▼
Phase 3 (Runner + Manager + Source) ← Phase 1, 2 필요
    │
    ▼
Phase 4 (Daemon 전환 + Legacy 제거) ← Phase 1, 2, 3 필요
```

각 Phase 완료 시 `cargo build` + `cargo test` 통과 보장.

---

## 커밋 전략

각 Phase의 하위 단위별로 1커밋:

```
# Phase 1
refactor(autodev): define Task, Agent, TaskSource, TaskRunner traits
refactor(autodev): extract WorkspaceOps trait from Workspace struct
refactor(autodev): add ConfigLoader trait

# Phase 2
refactor(autodev): implement AnalyzeTask with TDD
refactor(autodev): implement ImplementTask with TDD
refactor(autodev): implement ReviewTask with TDD
refactor(autodev): implement ImproveTask with TDD

# Phase 3
refactor(autodev): implement DefaultTaskRunner and ClaudeAgent
refactor(autodev): implement DefaultTaskManager
refactor(autodev): implement GitHubTaskSource

# Phase 4
refactor(autodev): replace daemon event loop with Daemon struct
refactor(autodev): wire new Daemon in main.rs DI
refactor(autodev): remove legacy pipeline and scanner modules
```

---

## 리스크

| 리스크 | 확률 | 영향 | 대응 |
|--------|------|------|------|
| ~~MergeTask의 conflict resolve가 단일 Agent 호출로 불충분~~ | ~~중간~~ | ~~중간~~ | DESIGN-v2 scope 외로 제거 |
| daily report가 현재 daemon 루프에 인라인 (60줄) | 낮음 | 낮음 | ✅ 해결: `DailyReporter` trait으로 분리, Daemon이 직접 소유 (SRP) |
| git_repository.rs의 scan 로직 이동 시 회귀 | 중간 | 중간 | Phase 3에서 기존 scan 로직을 함수 단위로 추출 후 `GitHubTaskSource`에서 호출. 기존 테스트 유지 |
| `QueueOp` 이동 시 기존 코드 컴파일 실패 | 높음 | 낮음 | Phase 4까지 기존 `pipeline::QueueOp`을 유지하고, `daemon::task`에서 re-export. Phase 4 마지막에 정리 |

---

## 정량 예측

| 지표 | Before | After |
|------|--------|-------|
| `daemon/mod.rs` | 746줄 | ~120줄 (Daemon struct + event loop) |
| `pipeline/` 총 LOC | ~2500줄 | 0줄 (모듈 제거) |
| `tasks/` 총 LOC | 0줄 | ~600줄 (4개 Task + ExtractTask) |
| `sources/github.rs` | 0줄 | ~300줄 |
| `daemon/task_runner.rs` | 0줄 | ~40줄 |
| `daemon/task_manager.rs` | 0줄 | ~60줄 |
| `domain/git_repository.rs` | 1639줄 | ~400줄 (큐 + 도메인만) |
| `#[allow(too_many_arguments)]` | 5곳 | 0곳 |
| spawnable 함수 이중 구현 | 6쌍 | 0 |
| 테스트 (신규) | 0 | ~40개 |
