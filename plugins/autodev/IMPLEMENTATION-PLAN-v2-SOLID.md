# IMPLEMENTATION PLAN: SOLID 원칙 기반 리팩토링

> **Date**: 2026-02-28
> **Base Document**: [DESIGN-v2-SOLID.md](DESIGN-v2-SOLID.md)
> **Branch**: `claude/autodev-solid-design-v2-EYEe2`

---

## 구현 전략

### 핵심 방침

1. **점진적 마이그레이션**: 기존 코드를 한 번에 교체하지 않고, 새 구조를 먼저 만든 뒤 하나씩 전환
2. **테스트 주도**: 각 Phase에서 새 trait/구조체에 대한 테스트를 먼저 작성
3. **후방 호환**: 기존 `dyn Gh` 사용 코드가 새 sub-trait 도입 후에도 동작
4. **컴파일 가능 유지**: 각 Phase 완료 시점에 `cargo build` + `cargo test` 통과

### 의존성 그래프

```
Phase 1 (PipelineStep trait + StepResult)  ← 독립
Phase 2 (Gh sub-trait 분리)                ← 독립
     ↓                    ↓
Phase 3 (PipelineContext + ConsumerLogWriter)  ← Phase 1, 2 필요
     ↓
Phase 4 (Issue Pipeline Steps)             ← Phase 1, 3 필요
     ↓
Phase 5 (PR Pipeline Steps)               ← Phase 1, 3 필요
     ↓
Phase 6 (Merge Pipeline Step)             ← Phase 1, 3 필요
     ↓
Phase 7 (StepRunner + 통합)               ← Phase 4, 5, 6 필요
     ↓
Phase 8 (GitRepository 분해)              ← Phase 7 이후
     ↓
Phase 9 (Daemon 루프 정리)                ← Phase 7, 8 필요
     ↓
Phase 10 (Legacy 코드 제거)               ← 모든 Phase 완료
```

---

## Phase 1: PipelineStep trait + StepResult 정의

### 목표
- Pipeline 핵심 추상화 정의
- 부수 효과를 데이터로 표현하는 StepResult 타입 정의

### 신규 파일

#### `pipeline/handler.rs`

```rust
use anyhow::Result;
use async_trait::async_trait;
use std::path::PathBuf;

use crate::domain::models::NewConsumerLog;
use crate::pipeline::QueueOp;

/// 라벨 조작 (순서 보장)
pub enum LabelOp {
    Add { number: i64, label: &'static str },
    Remove { number: i64, label: &'static str },
}

/// GitHub 코멘트
pub struct Comment {
    pub number: i64,
    pub body: String,
}

/// PR 리뷰
pub struct ReviewAction {
    pub number: i64,
    pub event: String,  // "APPROVE" | "REQUEST_CHANGES"
    pub body: String,
}

/// Pipeline 단계 실행 결과 (부수 효과를 데이터로 표현)
pub struct StepResult {
    pub label_ops: Vec<LabelOp>,
    pub comments: Vec<Comment>,
    pub reviews: Vec<ReviewAction>,
    pub queue_ops: Vec<QueueOp>,
    pub log: Option<NewConsumerLog>,
}

/// Pipeline 단계 실행 컨텍스트
pub struct StepInput {
    pub wt_path: PathBuf,
    pub repo_name: String,
    pub repo_url: String,
    pub gh_host: Option<String>,
}

/// Pipeline 핵심 로직 trait (SRP: 한 phase의 핵심 로직만)
#[async_trait]
pub trait PipelineStep: Send + Sync {
    fn phase_name(&self) -> &str;
    async fn execute(&self, input: &StepInput) -> Result<StepResult>;
}
```

### 테스트

```rust
#[cfg(test)]
mod tests {
    // StepResult 생성 헬퍼 테스트
    // LabelOp 순서 보장 테스트
}
```

### 변경 파일
- `pipeline/mod.rs`: `pub mod handler;` 추가

### 완료 기준
- `cargo build` 성공
- StepResult 생성 단위 테스트 통과

---

## Phase 2: Gh sub-trait 분리

### 목표
- 사용처별 인터페이스 분리 (ISP)
- 기존 `dyn Gh` 코드와 후방 호환 유지

### 수정 파일

#### `infrastructure/gh/mod.rs`

```rust
/// 라벨 관리 (Scanner, Pipeline, Recovery)
#[async_trait]
pub trait GhLabels: Send + Sync {
    async fn label_add(&self, repo: &str, number: i64, label: &str, host: Option<&str>) -> bool;
    async fn label_remove(&self, repo: &str, number: i64, label: &str, host: Option<&str>) -> bool;
}

/// 데이터 조회 (Scanner, Notifier, Recovery)
#[async_trait]
pub trait GhQuery: Send + Sync {
    async fn api_get_field(&self, repo: &str, path: &str, jq: &str, host: Option<&str>) -> Option<String>;
    async fn api_paginate(&self, repo: &str, endpoint: &str, params: &[(&str, &str)], host: Option<&str>) -> Result<Vec<u8>>;
}

/// 코멘트/리뷰 (Pipeline)
#[async_trait]
pub trait GhInteract: Send + Sync {
    async fn issue_comment(&self, repo: &str, number: i64, body: &str, host: Option<&str>) -> bool;
    async fn pr_review(&self, repo: &str, number: i64, event: &str, body: &str, host: Option<&str>) -> bool;
}

/// PR/Issue 생성 (Knowledge)
#[async_trait]
pub trait GhCreate: Send + Sync {
    async fn create_issue(&self, repo: &str, title: &str, body: &str, host: Option<&str>) -> bool;
    async fn create_pr(&self, repo: &str, head: &str, base: &str, title: &str, body: &str, host: Option<&str>) -> Option<i64>;
}

/// 기존 호환 — 모든 sub-trait을 포함하는 합성 trait
pub trait Gh: GhLabels + GhQuery + GhInteract + GhCreate {}
impl<T: GhLabels + GhQuery + GhInteract + GhCreate> Gh for T {}
```

#### `infrastructure/gh/real.rs`

- 기존 `impl Gh for RealGh` → 4개 sub-trait에 대한 `impl` 분리
- `RealGh`가 4개 trait 모두 구현하므로 blanket impl으로 `Gh` 자동 만족

#### `infrastructure/gh/mock.rs`

- 동일하게 4개 sub-trait에 대한 `impl` 분리
- MockGh도 blanket impl으로 `Gh` 자동 만족

### 사이드이펙트 확인
- `dyn Gh` 사용하는 모든 곳: **변경 불필요** (blanket impl이 보장)
- `Arc<dyn Gh>`: **변경 불필요**

### 완료 기준
- `cargo build` 성공 (기존 코드 무변경)
- `cargo test` 전체 통과
- 새 sub-trait으로 직접 참조하는 코드 1개 이상 작성 (검증용)

---

## Phase 3: PipelineContext + ConsumerLogWriter

### 목표
- pipeline 함수의 파라미터 수 축소 (ISP)
- Database 구체 타입 의존 제거 (DIP)
- config 로딩을 외부로 분리 (DIP)

### 신규 파일

#### `pipeline/context.rs`

```rust
use std::sync::Arc;

use crate::config::Env;
use crate::domain::models::NewConsumerLog;
use crate::infrastructure::claude::Claude;
use crate::infrastructure::gh::Gh;
use crate::infrastructure::git::Git;
use crate::infrastructure::suggest_workflow::SuggestWorkflow;

/// Pipeline 설정 (config 로딩 로직에 의존하지 않음)
pub struct PipelineConfig {
    pub issue_concurrency: usize,
    pub pr_concurrency: usize,
    pub merge_concurrency: usize,
    pub confidence_threshold: f64,
    pub max_review_iterations: u32,
    pub issue_workflow: String,
    pub pr_workflow: String,
}

/// 로그 기록 전용 trait (ISP: write-only)
pub trait ConsumerLogWriter: Send + Sync {
    fn write_log(&self, log: &NewConsumerLog);
}

/// Pipeline 실행 컨텍스트
pub struct PipelineContext {
    pub gh: Arc<dyn Gh>,
    pub git: Arc<dyn Git>,
    pub claude: Arc<dyn Claude>,
    pub sw: Arc<dyn SuggestWorkflow>,
    pub env: Arc<dyn Env>,
    pub log_writer: Arc<dyn ConsumerLogWriter>,
    pub config: PipelineConfig,
}
```

#### `queue/log_writer.rs`

```rust
use crate::domain::models::NewConsumerLog;
use crate::domain::repository::ConsumerLogRepository;
use crate::pipeline::context::ConsumerLogWriter;
use crate::queue::Database;

/// Database를 ConsumerLogWriter로 adapt (Adapter pattern)
impl ConsumerLogWriter for Database {
    fn write_log(&self, log: &NewConsumerLog) {
        let _ = self.log_insert(log);
    }
}
```

### 변경 파일
- `pipeline/mod.rs`: `pub mod context;` 추가
- `queue/mod.rs`: `pub mod log_writer;` 추가

### 테스트
- `MockLogWriter` 구현 (Vec에 기록)
- PipelineContext 생성 테스트

### 완료 기준
- `cargo build` 성공
- PipelineContext 생성 + MockLogWriter 테스트 통과

---

## Phase 4: Issue Pipeline Steps

### 목표
- `pipeline/issue.rs`의 이중 구현(process_pending + analyze_one)을 **하나의 Step으로 통합**
- Verdict 분기를 별도 함수로 분리 (OCP)

### 신규 파일

#### `pipeline/steps/mod.rs`

```rust
pub mod analyze_issue;
pub mod implement_issue;
```

#### `pipeline/steps/analyze_issue.rs`

```rust
pub struct AnalyzeIssueStep {
    pub item: IssueItem,
}

#[async_trait]
impl PipelineStep for AnalyzeIssueStep {
    fn phase_name(&self) -> &str { "Analyzing" }

    async fn execute(&self, input: &StepInput, ctx: &PipelineContext) -> Result<StepResult> {
        // 1. Pre-flight: is_issue_open
        // 2. Analyzer.analyze()
        // 3. match_verdict() → StepResult (부수 효과 없음)
    }
}
```

#### `pipeline/verdict_strategy.rs`

```rust
/// Verdict → StepResult 변환 (OCP: 새 verdict는 함수 추가만)
pub fn match_verdict(item: &IssueItem, res: &AnalyzerOutput, config: &PipelineConfig) -> StepResult { ... }

fn wontfix_result(item: &IssueItem, a: &AnalysisResult) -> StepResult { ... }
fn clarify_result(item: &IssueItem, a: &AnalysisResult, config: &PipelineConfig) -> StepResult { ... }
fn analyzed_result(item: &IssueItem, a: &AnalysisResult) -> StepResult { ... }
fn fallback_analyzed_result(item: &IssueItem, stdout: &str) -> StepResult { ... }
```

#### `pipeline/steps/implement_issue.rs`

```rust
pub struct ImplementIssueStep {
    pub item: IssueItem,
}

#[async_trait]
impl PipelineStep for ImplementIssueStep {
    fn phase_name(&self) -> &str { "Implementing" }

    async fn execute(&self, input: &StepInput, ctx: &PipelineContext) -> Result<StepResult> {
        // 1. Claude run_session (implement)
        // 2. PR 번호 추출
        // 3. StepResult with QueueOp::PushPr
    }
}
```

### 테스트 (TDD)
- `AnalyzeIssueStep` + MockClaude → verdict별 StepResult 검증
- `ImplementIssueStep` + MockClaude → PR 추출 + QueueOp 검증
- `match_verdict()` 단위 테스트 (각 verdict 경로)

### 변경 파일
- `pipeline/mod.rs`: `pub mod steps; pub mod verdict_strategy;` 추가

### 완료 기준
- 새 Step 구현체에 대한 단위/통합 테스트 통과
- 기존 `pipeline/issue.rs`는 아직 변경하지 않음 (Phase 7에서 교체)

---

## Phase 5: PR Pipeline Steps

### 목표
- `pipeline/pr.rs`의 6개 함수를 3개 Step으로 통합

### 신규 파일

#### `pipeline/steps/review_pr.rs`

```rust
pub struct ReviewPrStep {
    pub item: PrItem,
}

// review_one + re_review_one 통합
// review_iteration에 따라 prompt 분기
```

#### `pipeline/steps/improve_pr.rs`

```rust
pub struct ImprovePrStep {
    pub item: PrItem,
}

// improve_one 로직 추출
```

### 테스트 (TDD)
- `ReviewPrStep` + MockClaude → approve/request_changes 분기 검증
- `ImprovePrStep` + MockClaude → 개선 후 StepResult 검증
- max_review_iterations 초과 시 동작 검증

### 완료 기준
- 새 Step 통합 테스트 통과
- 기존 `pipeline/pr.rs`는 아직 변경하지 않음

---

## Phase 6: Merge Pipeline Step

### 목표
- `pipeline/merge.rs`의 2개 함수를 1개 Step으로 통합

### 신규 파일

#### `pipeline/steps/merge_pr.rs`

```rust
pub struct MergePrStep {
    pub item: MergeItem,
}

// merge_one 로직 추출
// Conflict → resolve 시도 포함
```

### 테스트 (TDD)
- 성공 경로
- 충돌 → 해결 경로
- 충돌 → 해결 실패 경로

### 완료 기준
- 새 Step 통합 테스트 통과

---

## Phase 7: StepRunner + 기존 코드 교체

### 목표
- StepRunner 구현 (부수 효과 적용기)
- 기존 `analyze_one()`, `implement_one()`, `review_one()`, `improve_one()`, `re_review_one()`, `merge_one()` 을 Step + Runner로 교체
- legacy batch 함수 제거

### 신규 파일

#### `pipeline/runner.rs`

```rust
use std::sync::Arc;
use crate::infrastructure::gh::{GhLabels, GhInteract};

/// StepResult의 부수 효과를 실제 시스템에 반영
pub struct StepRunner {
    gh: Arc<dyn Gh>,
    log_writer: Arc<dyn ConsumerLogWriter>,
}

impl StepRunner {
    pub async fn apply(&self, result: &StepResult, repo_name: &str, gh_host: Option<&str>) {
        // 1. label_ops 순서대로 적용
        for op in &result.label_ops {
            match op {
                LabelOp::Add { number, label } => {
                    self.gh.label_add(repo_name, *number, label, gh_host).await;
                }
                LabelOp::Remove { number, label } => {
                    self.gh.label_remove(repo_name, *number, label, gh_host).await;
                }
            }
        }
        // 2. comments 게시
        for c in &result.comments {
            self.gh.issue_comment(repo_name, c.number, &c.body, gh_host).await;
        }
        // 3. reviews 제출
        for r in &result.reviews {
            self.gh.pr_review(repo_name, r.number, &r.event, &r.body, gh_host).await;
        }
        // 4. log 기록
        if let Some(log) = &result.log {
            self.log_writer.write_log(log);
        }
    }
}
```

### 수정 파일

#### `pipeline/issue.rs` — 전면 교체

```rust
// 기존 ~1000줄 → ~50줄

pub async fn analyze_one(item: IssueItem, ctx: &PipelineContext) -> TaskOutput {
    let step = AnalyzeIssueStep { item: item.clone() };
    let runner = StepRunner::new(ctx.gh.clone(), ctx.log_writer.clone());
    // workspace 생성 → step.execute() → runner.apply() → workspace 정리
    ...
}

pub async fn implement_one(item: IssueItem, ctx: &PipelineContext) -> TaskOutput {
    let step = ImplementIssueStep { item: item.clone() };
    ...
}

// process_pending(), process_ready() — 제거
```

#### `pipeline/pr.rs` — 전면 교체

```rust
// 기존 ~1000줄 → ~80줄

pub async fn review_one(item: PrItem, ctx: &PipelineContext) -> TaskOutput { ... }
pub async fn improve_one(item: PrItem, ctx: &PipelineContext) -> TaskOutput { ... }
pub async fn re_review_one(item: PrItem, ctx: &PipelineContext) -> TaskOutput { ... }

// process_pending(), process_review_done(), process_improved() — 제거
```

#### `pipeline/merge.rs` — 전면 교체

```rust
// 기존 ~340줄 → ~40줄

pub async fn merge_one(item: MergeItem, ctx: &PipelineContext) -> TaskOutput { ... }

// process_pending() — 제거
```

#### `daemon/mod.rs` — spawn 시그니처 변경

```rust
// spawn_ready_tasks: ctx: &PipelineContext 전달
// Arc clone 패턴 유지하되 PipelineContext.clone() 사용
```

### 테스트
- E2E 테스트: MockGh + MockClaude → 전체 파이프라인 플로우 검증
- StepRunner 단위 테스트: label_ops 순서, 에러 무시(best effort) 검증
- 기존 `pipeline_e2e_tests.rs` 업데이트

### 완료 기준
- `cargo build` 성공
- `cargo test` 전체 통과
- `cargo clippy -- -D warnings` 통과 (`#[allow(clippy::too_many_arguments)]` 제거 확인)
- legacy batch 함수 완전 제거

---

## Phase 8: GitRepository 분해

### 목표
- God Object인 `GitRepository` (~700줄)를 역할별 분리
- 스캔 로직을 scanner 모듈로 이동
- recovery 로직을 별도 구조체로 분리

### 수정 파일

#### `domain/git_repository.rs`

```rust
// 기존 ~700줄 → ~150줄

/// 순수 데이터 + 큐 보유
pub struct GitRepository {
    id: String,
    name: String,
    url: String,
    gh_host: Option<String>,
    pub issue_queue: StateQueue<IssueItem>,
    pub pr_queue: StateQueue<PrItem>,
    pub merge_queue: StateQueue<MergeItem>,
}

// scan_issues(), scan_pulls(), scan_merges(), scan_approved_issues() → scanner/ 로 이동
// startup_reconcile(), refresh() → reconciler 로 이동
// recover_orphan_wip(), recover_orphan_implementing() → reconciler 로 이동
```

#### `scanner/` — 확장

```rust
// 기존 scanner/issues.rs, scanner/pulls.rs 와 통합
// GitRepository의 스캔 메서드를 free function으로 변환

pub async fn scan_issues(
    repo: &mut GitRepository,
    gh: &dyn GhQuery,
    labels_gh: &dyn GhLabels,
    db: &dyn ScanCursorRepository,
    ignore_authors: &[String],
    filter_labels: &[String],
) -> Result<u64>
```

#### `daemon/reconciler.rs` — 신규

```rust
pub struct Reconciler;

impl Reconciler {
    pub async fn startup(
        repo: &mut GitRepository,
        gh: &dyn Gh,
    ) -> u64 { ... }

    pub async fn recover_orphans(
        repo: &mut GitRepository,
        gh: &dyn Gh,
    ) -> u64 { ... }
}
```

### 사이드이펙트 확인
- `daemon/mod.rs`의 `repo.scan_issues()` → `scanner::scan_issues(&mut repo, ...)` 변경
- `daemon/mod.rs`의 `repo.startup_reconcile()` → `Reconciler::startup(&mut repo, ...)` 변경
- `domain/git_repository_factory.rs` → 간소화 (큐 초기화만)

### 완료 기준
- `cargo build` 성공
- `cargo test` 전체 통과
- `git_repository.rs` 150줄 이내

---

## Phase 9: Daemon 루프 정리

### 목표
- `start()` 함수에서 daily report 분리
- 각 tick 단계를 메서드로 추출

### 수정 파일

#### `daemon/mod.rs`

```rust
// DaemonLoop 구조체로 상태 캡슐화
struct DaemonLoop {
    repos: HashMap<String, GitRepository>,
    tracker: InFlightTracker,
    join_set: JoinSet<TaskOutput>,
    ctx: PipelineContext,
    db: Database,
    daily_scheduler: DailyScheduler,
}

impl DaemonLoop {
    async fn tick(&mut self) { ... }
    async fn sync_repos(&mut self) { ... }
    async fn run_recovery(&mut self) { ... }
    async fn run_scans(&mut self) { ... }
    fn spawn_ready_tasks(&mut self) { ... }
}
```

#### `daemon/daily_scheduler.rs` — 신규

```rust
pub struct DailyScheduler {
    report_hour: u32,
    last_report_date: String,
    knowledge_enabled: bool,
}

impl DailyScheduler {
    pub async fn maybe_run(&mut self, ...) { ... }
}
```

### 완료 기준
- `start()` 함수 100줄 이내
- daily report 로직 완전 분리
- `cargo test` 전체 통과

---

## Phase 10: Legacy 코드 제거 + 최종 정리

### 목표
- 사용되지 않는 코드 제거
- `#[allow(dead_code)]`, `#[allow(clippy::too_many_arguments)]` 제거
- clippy + fmt + test 최종 검증

### 체크리스트

- [ ] `pipeline/issue.rs`의 `process_pending()`, `process_ready()` 완전 제거
- [ ] `pipeline/pr.rs`의 `process_pending()`, `process_review_done()`, `process_improved()` 완전 제거
- [ ] `pipeline/merge.rs`의 `process_pending()` 완전 제거
- [ ] `pipeline/mod.rs`의 legacy `process_all()` 제거
- [ ] 사용되지 않는 `_sw` 파라미터 정리
- [ ] 모든 `#[allow(clippy::too_many_arguments)]` 제거
- [ ] `cargo fmt --check` 통과
- [ ] `cargo clippy -- -D warnings` 통과
- [ ] `cargo test` 전체 통과
- [ ] 테스트 커버리지: 새 Step별 최소 2개 (성공 + 실패) 테스트

---

## 리스크 및 대응

| 리스크 | 확률 | 영향 | 대응 |
|--------|------|------|------|
| Gh sub-trait 분리 시 기존 코드 컴파일 실패 | 낮음 | 높음 | blanket impl으로 후방 호환 보장 |
| Step 추상화가 과도하여 복잡도 증가 | 중간 | 중간 | Step당 1개 파일 원칙, 추상화 레이어 최소화 |
| daemon 루프 리팩토링 중 동시성 버그 | 낮음 | 높음 | 기존 InFlightTracker 테스트 유지, 새 테스트 추가 |
| GitRepository 분해 시 스캔 로직 회귀 | 중간 | 중간 | 기존 `daemon_scan_tests.rs` 먼저 통과 확인 |

---

## 작업량 추정

| Phase | 파일 수 (신규/수정) | 핵심 변경 |
|-------|-------------------|----------|
| 1 | 1 신규 + 1 수정 | trait + 타입 정의 |
| 2 | 3 수정 | Gh trait 분리 |
| 3 | 2 신규 + 2 수정 | Context + LogWriter |
| 4 | 3 신규 + 1 수정 | Issue Steps + Verdict |
| 5 | 2 신규 + 1 수정 | PR Steps |
| 6 | 1 신규 + 1 수정 | Merge Step |
| 7 | 1 신규 + 4 수정 | Runner + 교체 |
| 8 | 1 신규 + 3 수정 | GitRepo 분해 |
| 9 | 1 신규 + 1 수정 | Daemon 정리 |
| 10 | 0 신규 + 5 수정 | 정리 |
| **합계** | **12 신규 + 22 수정** | |

---

## Phase별 커밋 전략

각 Phase 완료 시 1개 커밋:

```
refactor(autodev): define PipelineStep trait and StepResult types
refactor(autodev): split Gh trait into role-based sub-traits
refactor(autodev): add PipelineContext and ConsumerLogWriter
refactor(autodev): extract issue pipeline steps with verdict strategy
refactor(autodev): extract PR pipeline steps
refactor(autodev): extract merge pipeline step
refactor(autodev): integrate StepRunner and replace legacy pipeline
refactor(autodev): decompose GitRepository into focused modules
refactor(autodev): restructure daemon loop with DaemonLoop struct
refactor(autodev): remove legacy pipeline code and clean up
```
