# autodev SOLID 리팩토링 설계

## 요구사항 정리

### 핵심 문제

현재 autodev의 파이프라인 구조에 다음 SOLID 위반이 존재:

1. **SRP 위반**: `daemon/mod.rs`의 `spawn_ready_tasks()`가 6종 task 유형의 dispatch를 하드코딩
2. **OCP 위반**: 새로운 task 유형 추가 시 `spawn_ready_tasks()`, `apply_queue_ops()` 모두 수정 필요
3. **DIP 위반**: `Claude` trait이 특정 벤더에 이름이 종속 (GeminiAgent, CodexAgent 추가 시 부자연스러움)
4. **ISP 위반(경미)**: pipeline 함수들이 불필요한 의존성을 받음 (예: `review_one`이 `SuggestWorkflow`를 받지만 일부 경로에서만 사용)

### 목표

```
AS-IS: trait Claude → pipeline::analyze_one() → daemon::spawn_ready_tasks() (하드코딩 dispatch)
TO-BE: trait Agent  → AnalyzeTask impl Task  → TaskRunner::run(task) (다형성 dispatch)
```

---

## 사이드이펙트 조사

### 영향받는 파일 (34개)

| Phase | 변경 대상 | 영향도 |
|-------|----------|--------|
| Phase 1 | `infrastructure/claude/{mod,real,mock}.rs` | 이름 변경 (Agent, ClaudeAgent, MockAgent) |
| Phase 1 | `infrastructure/claude/output.rs` → `infrastructure/agent/output.rs` | 디렉토리 이동 |
| Phase 1 | `infrastructure/mod.rs` | claude → agent 모듈명 변경 |
| Phase 1 | `components/{analyzer,reviewer,merger}.rs` | `Claude` → `Agent` 참조 변경 |
| Phase 1 | `pipeline/{mod,issue,pr,merge}.rs` | `Claude` → `Agent` 참조 변경 |
| Phase 1 | `daemon/mod.rs` | `Claude` → `Agent` 참조 변경 |
| Phase 1 | `main.rs` | import 경로 + 변수명 변경 |
| Phase 1 | `knowledge/daily.rs` | `Claude` → `Agent` 참조 변경 |
| Phase 2 | `pipeline/{issue,pr,merge}.rs` (spawnable 함수) | Task struct 내부로 로직 이동 |
| Phase 3 | `daemon/mod.rs` | spawn_ready_tasks → TaskRunner 사용 |

### 기존 테스트 영향

- `infrastructure/claude/output.rs` 테스트: 경로만 변경, 로직 불변
- `infrastructure/claude/mock.rs` 테스트: 이름만 변경
- `pipeline/mod.rs` 테스트: `handle_task_output` → 변경 없음
- `daemon/mod.rs` 테스트: `InFlightTracker`, `apply_queue_ops` → Phase 3에서 시그니처 변경
- `domain/git_repository_factory.rs` 테스트: Phase 1에서 `MockClaude` → `MockAgent` 변경

### 위험 완화

- Phase 1은 순수 리네임 → 컴파일 에러로 누락 100% 검출
- Phase 2는 기존 함수를 `Task` struct 메서드로 이동 (동작 동일, 시그니처만 변경)
- Phase 3은 `spawn_ready_tasks` 리팩토링 → 기존 테스트가 새 구조를 검증

---

## 설계

### Phase 1: Claude → Agent 리네임

```
infrastructure/claude/       → infrastructure/agent/
trait Claude                 → trait Agent
struct RealClaude            → struct ClaudeAgent
struct MockClaude            → struct MockAgent
struct SessionResult         → (유지 — Agent 응답 DTO)
struct SessionOptions        → (유지 — Agent 요청 DTO)
infrastructure/claude/output → infrastructure/agent/output (그대로 유지)
```

**변경 원칙**: output.rs의 `ClaudeJsonOutput`, `parse_output()` 등은 Claude CLI의 JSON envelope 파싱이므로 이름 유지. Agent trait과 구현체만 리네임.

### Phase 2: Task trait 도입

```rust
// pipeline/task.rs (새 파일)

use async_trait::async_trait;
use super::TaskOutput;

/// 개별 작업의 실행 계약.
///
/// 각 구현체는 before_invoke(전처리) → Agent 호출 → after_invoke(후처리) 패턴을 따른다.
/// TaskRunner가 이 trait을 사용하여 실행한다.
#[async_trait]
pub trait Task: Send {
    /// Task 실행 (전처리 → Agent 호출 → 후처리를 내부에서 수행)
    async fn run(&mut self) -> TaskOutput;
}
```

**구현체 6종**:

| Struct | 현재 함수 | 역할 |
|--------|----------|------|
| `AnalyzeTask` | `issue::analyze_one()` | Issue 분석 |
| `ImplementTask` | `issue::implement_one()` | Issue 구현 |
| `ReviewTask` | `pr::review_one()` | PR 리뷰 |
| `ImproveTask` | `pr::improve_one()` | PR 개선 |
| `ReReviewTask` | `pr::re_review_one()` | PR 재리뷰 |
| `MergeTask` | `merge::merge_one()` | PR 머지 |

각 Task struct는 필요한 의존성만 필드로 보유:

```rust
// 예: AnalyzeTask
pub struct AnalyzeTask {
    pub item: IssueItem,
    env: Arc<dyn Env>,
    gh: Arc<dyn Gh>,
    git: Arc<dyn Git>,
    agent: Arc<dyn Agent>,
}

#[async_trait]
impl Task for AnalyzeTask {
    async fn run(&mut self) -> TaskOutput {
        // 기존 analyze_one() 로직을 그대로 이동
    }
}
```

**중요**: 기존 `analyze_one()` 등의 free function은 제거하고, Task struct의 `run()` 메서드로 이동. 로직 자체는 변경하지 않음.

### Phase 3: TaskRunner 도입 + daemon 연결

```rust
// pipeline/task_runner.rs (새 파일)

use tokio::task::JoinSet;
use super::task::Task;
use super::TaskOutput;

/// Task를 tokio::spawn으로 실행하는 runner.
///
/// Task trait 객체를 받아서 실행하므로 daemon이 task 유형을 알 필요 없음.
pub struct TaskRunner;

impl TaskRunner {
    /// Task를 JoinSet에 spawn한다.
    pub fn spawn(join_set: &mut JoinSet<TaskOutput>, task: impl Task + 'static) {
        join_set.spawn(async move {
            let mut task = task;
            task.run().await
        });
    }
}
```

**daemon/mod.rs 변경**:

```rust
// AS-IS: 6종 task를 하드코딩 dispatch
fn spawn_ready_tasks(..., claude: &Arc<dyn Claude>, ...) {
    // Issue: Pending → Analyzing
    while tracker.can_spawn() {
        let item = repo.issue_queue.pop(issue_phase::PENDING)?;
        let (e, g, gi, c) = (Arc::clone(env), ...);
        join_set.spawn(async move {
            pipeline::issue::analyze_one(item, &*e, &*g, &*gi, &*c).await
        });
    }
    // ... PR, Merge 각각 반복
}

// TO-BE: Task trait으로 다형성 dispatch
fn spawn_ready_tasks(..., agent: &Arc<dyn Agent>, ...) {
    for repo in repos.values_mut() {
        // Issue: Pending → Analyzing
        while tracker.can_spawn() {
            let item = match repo.issue_queue.pop(issue_phase::PENDING) {
                Some(item) => item,
                None => break,
            };
            tracker.track(&item.repo_name);
            repo.issue_queue.push(issue_phase::ANALYZING, item.clone());

            let task = AnalyzeTask::new(
                item, Arc::clone(env), Arc::clone(gh),
                Arc::clone(git), Arc::clone(agent),
            );
            TaskRunner::spawn(&mut join_set, task);
        }
        // ... 나머지도 동일 패턴
    }
}
```

**핵심 개선**: `TaskRunner::spawn(join_set, task)`는 task 유형을 몰라도 실행 가능. 미래에 새로운 task 유형 추가 시 `Task` 구현체만 만들면 됨.

---

## 구현 순서

### Phase 1: Agent 리네임 (안전, 컴파일 타임 검증)
1. `infrastructure/claude/` → `infrastructure/agent/` 디렉토리 리네임
2. `trait Claude` → `trait Agent`, `RealClaude` → `ClaudeAgent`, `MockClaude` → `MockAgent`
3. `infrastructure/mod.rs`: `pub mod claude` → `pub mod agent`
4. 모든 참조 업데이트 (components, pipeline, daemon, main, knowledge)
5. `cargo fmt && cargo clippy -- -D warnings && cargo test` 통과 확인

### Phase 2: Task trait 도입 (구조 변경, 로직 불변)
1. `pipeline/task.rs` 생성 — Task trait 정의
2. `pipeline/tasks/` 디렉토리 생성 — 6종 Task 구현체
3. 기존 pipeline free function(`analyze_one` 등) → Task::run()으로 이동
4. 기존 legacy 함수(`process_pending` 등) 유지 (이미 `#[allow(dead_code)]`)
5. `cargo fmt && cargo clippy -- -D warnings && cargo test`

### Phase 3: TaskRunner + daemon 연결
1. `pipeline/task_runner.rs` 생성
2. `daemon/mod.rs`의 `spawn_ready_tasks()` 리팩토링 → TaskRunner 사용
3. `claude` 파라미터명 → `agent`로 변경 (daemon, main)
4. 기존 daemon 테스트 업데이트
5. `cargo fmt && cargo clippy -- -D warnings && cargo test`

---

## 변경하지 않는 것

- `infrastructure/agent/output.rs`: Claude CLI JSON envelope 파싱 → 이름/로직 유지
- `components/`: Analyzer, Reviewer, Merger, Notifier, Workspace → 이름 유지 (Agent 참조만 변경)
- `queue/`: StateQueue, TaskQueues, work items → 변경 없음
- `domain/`: GitRepository, labels, models → 변경 없음
- `scanner/`, `tui/`, `client/`: 변경 없음
- `config/`: 변경 없음
- Legacy `process_pending`/`process_ready` 함수: 이미 dead_code, 삭제하지 않음
