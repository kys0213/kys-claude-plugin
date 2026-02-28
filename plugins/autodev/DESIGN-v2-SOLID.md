# DESIGN v2: SOLID 원칙 기반 아키텍처 개선

> **Date**: 2026-02-28
> **Base**: 현재 구현 코드 (DESIGN-v2 + Gap 개선 완료 상태)
> **목표**: SOLID 원칙 위반 식별 및 구조적 개선 설계

---

## 1. 현재 아키텍처 요약

```
main.rs (CLI + DI)
  ├── daemon/mod.rs        — 이벤트 루프, spawn, reconcile
  ├── pipeline/
  │     ├── issue.rs       — analyze_one, implement_one + legacy batch
  │     ├── pr.rs          — review_one, improve_one, re_review_one + legacy batch
  │     └── merge.rs       — merge_one + legacy batch
  ├── scanner/             — issues.rs, pulls.rs (GitHub 감지)
  ├── components/
  │     ├── analyzer.rs    — Claude 분석 래퍼
  │     ├── reviewer.rs    — Claude 리뷰 래퍼
  │     ├── merger.rs      — Claude 머지 래퍼
  │     ├── notifier.rs    — GitHub 상태 확인 + 댓글
  │     ├── workspace.rs   — Git worktree 관리
  │     └── verdict.rs     — 코멘트 포맷팅
  ├── infrastructure/
  │     ├── gh/            — Gh trait + real/mock
  │     ├── git/           — Git trait + real/mock
  │     ├── claude/        — Claude trait + real/mock + output parsing
  │     └── suggest_workflow/ — SuggestWorkflow trait + real/mock
  ├── domain/
  │     ├── git_repository.rs      — aggregate (scan + reconcile + queue)
  │     ├── git_repository_factory.rs
  │     ├── labels.rs      — 라벨 상수
  │     ├── models.rs      — DB 모델
  │     └── repository.rs  — DB trait
  ├── queue/               — StateQueue, TaskQueues, Database
  ├── knowledge/           — extractor.rs, daily.rs, models.rs
  └── config/              — loader, models
```

---

## 2. SOLID 원칙별 위반 분석

### 2.1. SRP (단일 책임 원칙) 위반

#### V-SRP-1: `pipeline/issue.rs` — 거대 함수 + 이중 구현

**현상**: `process_pending()` (legacy batch) 와 `analyze_one()` (spawnable) 가 **동일한 로직을 중복 구현**. 두 함수 모두 ~250줄이며, 분석/라벨 전이/코멘트/worktree 관리/로깅을 모두 포함.

```
process_pending()  — 316줄, 책임 6개 (worktree, 분석, verdict 분기, 라벨, 코멘트, DB 로그)
analyze_one()      — 230줄, 동일 책임을 TaskOutput 패턴으로 재구현
process_ready()    — 226줄, 책임 5개 (worktree, 구현, PR 추출, 라벨, 코멘트)
implement_one()    — 200줄, 동일 책임 재구현
```

**변경의 이유가 6개**: worktree 정책 변경, 분석 프롬프트 변경, verdict 분기 추가, 라벨 전이 규칙 변경, 코멘트 포맷 변경, 로깅 정책 변경 → SRP 위반.

#### V-SRP-2: `pipeline/pr.rs` — 동일한 패턴의 극단적 중복

**현상**: `process_pending()` + `review_one()`, `process_review_done()` + `improve_one()`, `process_improved()` + `re_review_one()` 가 각각 동일 로직의 이중 구현. 파일 전체가 **~1000줄**.

#### V-SRP-3: `daemon/mod.rs` — 오케스트레이터가 세부 로직까지 관리

**현상**: `start()` 함수 (~250줄) 가 이벤트 루프, 레포 동기화, 스캔, 스폰, daily report 생성, 로그 정리를 모두 포함.

```rust
// daemon/mod.rs:482-543 — daily report 로직이 daemon 루프 안에 인라인
if knowledge_extraction {
    let now = chrono::Local::now();
    // ... 60줄의 daily report 로직
}
```

#### V-SRP-4: `domain/git_repository.rs` — God Object

**현상**: `GitRepository` 가 scan, reconcile, recovery, queue 관리를 모두 포함하는 God Object (~700줄). 스캔 로직(GitHub API 호출 + JSON 파싱)과 도메인 로직(라벨 전이)과 큐 관리가 하나의 구조체에 혼재.

---

### 2.2. OCP (개방-폐쇄 원칙) 위반

#### V-OCP-1: Pipeline verdict 분기 — 새 verdict 추가 시 기존 코드 수정 필수

**현상**: `process_pending()`/`analyze_one()` 의 verdict 분기가 `match` 문으로 하드코딩.

```rust
match res.analysis {
    Some(ref a) if a.verdict == Verdict::Wontfix => { /* 40줄 */ }
    Some(ref a) if a.verdict == Verdict::NeedsClarification => { /* 40줄 */ }
    Some(ref a) => { /* 30줄 — implement */ }
    None => { /* 25줄 — fallback */ }
}
```

새로운 verdict (예: `Duplicate`, `Deferred`) 추가 시 **4곳** (process_pending, analyze_one, 그리고 pr.rs의 2곳)을 모두 수정해야 함.

#### V-OCP-2: Pipeline 타입별 분기 — issue/pr/merge 각각 별도 함수

**현상**: `spawn_ready_tasks()`에서 issue/pr/merge 각 타입별 while 루프가 반복. 새 파이프라인 타입 추가 시 `spawn_ready_tasks`와 `apply_queue_ops` 모두 수정 필요.

#### V-OCP-3: scan_targets 분기 — 하드코딩된 match

```rust
// daemon/mod.rs:428-473
match target.as_str() {
    "issues" => { /* scan + scan_approved */ }
    "pulls" => { /* scan_pulls */ }
    "merges" => { /* scan_merges */ }
    _ => {}
}
```

새로운 scan target 추가 시 이 match 문을 수정해야 함.

---

### 2.3. LSP (리스코프 치환 원칙) — 양호

현재 trait 구현체들(`RealGh`/`MockGh`, `RealGit`/`MockGit`, `RealClaude`/`MockClaude`)은 계약을 잘 준수. **심각한 LSP 위반 없음**.

단, `SuggestWorkflow` trait이 PR pipeline에서 파라미터로 전달되지만 일부 함수에서 사용되지 않음 (`_sw` prefix).

---

### 2.4. ISP (인터페이스 분리 원칙) 위반

#### V-ISP-1: `Gh` trait — 거대 인터페이스

**현상**: `Gh` trait이 9개 메서드를 가지며, 사용처마다 일부만 사용.

```
Scanner:   api_paginate, label_add, label_remove
Pipeline:  label_add, label_remove, issue_comment, pr_review
Notifier:  api_get_field
Knowledge: create_issue, create_pr
Recovery:  api_get_field, api_paginate, label_add, label_remove
```

모든 mock이 9개 메서드를 전부 구현해야 하며, 테스트에서 사용하지 않는 메서드도 stub 필요.

#### V-ISP-2: Pipeline 함수 시그니처 — `#[allow(clippy::too_many_arguments)]`

**현상**: pipeline 함수들이 `db, env, workspace, notifier, gh, claude, sw, queues` 등 7-8개 파라미터. `too_many_arguments` clippy 경고를 `#[allow]`로 억제.

이는 ISP 위반의 결과 — 함수가 필요 이상의 의존성을 주입받고 있음.

---

### 2.5. DIP (의존성 역전 원칙) 위반

#### V-DIP-1: `Workspace`/`Notifier` — 구조체 직접 생성 (부분 위반)

**현상**: `Workspace`와 `Notifier`는 trait이 아닌 **구조체**로 정의. pipeline 함수에서 직접 생성.

```rust
// pipeline/issue.rs:560
let workspace = Workspace::new(git, env);
let notifier = Notifier::new(gh);
```

테스트에서 Workspace/Notifier의 동작을 개별 mock할 수 없음. Git/Gh의 mock을 통해 간접적으로만 테스트 가능.

#### V-DIP-2: `Database` — 구체 타입에 직접 의존

**현상**: pipeline 함수들이 `&Database` 구체 타입을 직접 받음. `ConsumerLogRepository` trait이 존재하지만 pipeline 코드에서는 `Database`를 직접 사용.

```rust
// pipeline/issue.rs:81
pub async fn process_pending(
    db: &Database,  // ← 구체 타입
    ...
```

#### V-DIP-3: `config::loader::load_merged()` — 정적 함수 호출

**현상**: pipeline 함수 내부에서 `config::loader::load_merged(env, None)` 을 직접 호출하여 설정을 로드. 설정이 pipeline의 파라미터가 아닌 내부 의존성.

---

## 3. 개선 설계

### 3.1. Pipeline Handler 패턴 (SRP + OCP 해결)

#### 목표
- `process_pending()` + `analyze_one()` 이중 구현 제거
- verdict 분기를 전략 패턴으로 전환
- 함수당 책임을 1-2개로 축소

#### 설계

**핵심 아이디어**: Pipeline의 각 phase를 `PipelineHandler` trait으로 추상화. legacy batch와 spawnable의 로직을 **하나의 handler 구현체로 통합**.

```rust
// pipeline/handler.rs — 새 파일

/// Pipeline 단계의 실행 결과
pub struct StepResult {
    /// 라벨 조작 (순서 보장)
    pub label_ops: Vec<LabelOp>,
    /// 이슈/PR 코멘트
    pub comments: Vec<Comment>,
    /// 큐 조작
    pub queue_ops: Vec<QueueOp>,
    /// 로그 엔트리
    pub log: Option<NewConsumerLog>,
}

pub enum LabelOp {
    Add { number: i64, label: &'static str },
    Remove { number: i64, label: &'static str },
}

pub struct Comment {
    pub number: i64,
    pub body: String,
}

/// Pipeline 핵심 로직만 담당 (부수 효과 없음)
/// SRP: 하나의 phase 처리 로직만 책임
#[async_trait]
pub trait PipelineStep: Send + Sync {
    /// phase 이름 (로깅/디버깅용)
    fn phase_name(&self) -> &str;

    /// 핵심 로직 실행 (부수 효과 없이 결과만 반환)
    async fn execute(&self, ctx: &StepContext) -> Result<StepResult>;
}
```

**StepContext**: 실행에 필요한 모든 의존성을 묶은 구조체 (ISP 해결).

```rust
pub struct StepContext {
    pub gh: Arc<dyn Gh>,
    pub git: Arc<dyn Git>,
    pub claude: Arc<dyn Claude>,
    pub env: Arc<dyn Env>,
    pub repo_name: String,
    pub repo_url: String,
    pub gh_host: Option<String>,
}
```

**StepRunner**: handler 실행 + 부수 효과(라벨/코멘트/로그) 적용을 담당하는 공통 러너. Worktree 생명주기를 보장.

```rust
// pipeline/runner.rs — 새 파일

/// StepRunner: PipelineStep의 결과를 실제 GitHub/DB에 반영
/// SRP: 부수 효과 적용만 담당
pub struct StepRunner {
    gh: Arc<dyn Gh>,
    db: Arc<Database>,
}

impl StepRunner {
    /// handler.execute() → StepResult → 라벨/코멘트/로그 적용
    pub async fn run(
        &self,
        step: &dyn PipelineStep,
        ctx: &StepContext,
    ) -> TaskOutput {
        // 1. Worktree 생성
        // 2. step.execute(ctx)
        // 3. StepResult의 label_ops 순서대로 적용
        // 4. comments 게시
        // 5. log DB 기록
        // 6. Worktree 정리 (success/failure 모두)
        // 7. queue_ops를 TaskOutput으로 변환
    }
}
```

#### Issue 분석 handler 예시

```rust
// pipeline/steps/analyze_issue.rs — 새 파일

pub struct AnalyzeIssueStep;

impl PipelineStep for AnalyzeIssueStep {
    fn phase_name(&self) -> &str { "Analyzing" }

    async fn execute(&self, ctx: &StepContext) -> Result<StepResult> {
        let analyzer = Analyzer::new(&*ctx.claude);
        let prompt = build_analysis_prompt(&ctx.item);
        let res = analyzer.analyze(&ctx.wt_path, &prompt, Some(AGENT_SYSTEM_PROMPT)).await?;

        // verdict → StepResult 변환 (부수 효과 없음)
        Ok(match_verdict(&ctx.item, &res))
    }
}
```

#### Verdict 전략 패턴 (OCP 해결)

```rust
// pipeline/verdict_strategy.rs — 새 파일

/// Verdict 처리 전략
/// OCP: 새 verdict 추가 시 기존 코드 수정 없이 새 함수만 추가
fn match_verdict(item: &IssueItem, res: &AnalyzerOutput) -> StepResult {
    match &res.analysis {
        Some(a) if a.verdict == Verdict::Wontfix => wontfix_result(item, a),
        Some(a) if a.verdict == Verdict::NeedsClarification => clarify_result(item, a),
        Some(a) => analyzed_result(item, a),
        None => fallback_analyzed_result(item, &res.stdout),
    }
}

fn wontfix_result(item: &IssueItem, a: &AnalysisResult) -> StepResult {
    StepResult {
        label_ops: vec![
            LabelOp::Remove { number: item.github_number, label: labels::WIP },
            LabelOp::Add { number: item.github_number, label: labels::SKIP },
        ],
        comments: vec![Comment {
            number: item.github_number,
            body: verdict::format_wontfix_comment(a),
        }],
        queue_ops: vec![QueueOp::Remove],
        log: None,
    }
}

// 새 verdict 추가 시: 여기에 함수만 추가하면 됨
// fn deferred_result(...) -> StepResult { ... }
```

#### 이중 구현 제거

legacy batch 함수(`process_pending`)와 spawnable 함수(`analyze_one`)를 **하나의 handler + runner로 통합**:

```rust
// 기존 analyze_one() — 제거
// 기존 process_pending() — 제거

// 대체:
pub async fn analyze_one(item: IssueItem, ctx: &StepContext) -> TaskOutput {
    let step = AnalyzeIssueStep;
    let runner = StepRunner::new(ctx.gh.clone(), ctx.db.clone());
    runner.run(&step, ctx).await
}

// legacy batch도 동일한 step을 사용:
pub async fn process_pending_batch(...) -> Result<()> {
    let step = AnalyzeIssueStep;
    for item in queues.issues.drain(issue_phase::PENDING) {
        let result = step.execute(&ctx).await?;
        apply_result(&result, gh, db, queues).await;
    }
}
```

---

### 3.2. Gh trait 분리 (ISP 해결)

#### 목표
- 사용처별로 필요한 메서드만 의존
- mock 작성 부담 감소

#### 설계

```rust
// infrastructure/gh/mod.rs

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

/// 전체 Gh 기능 (후방 호환, 점진 마이그레이션용)
pub trait Gh: GhLabels + GhQuery + GhInteract + GhCreate {}
impl<T: GhLabels + GhQuery + GhInteract + GhCreate> Gh for T {}
```

**후방 호환**: 기존 `&dyn Gh`를 사용하는 코드는 변경 없이 동작. 새 코드에서만 `&dyn GhLabels` 등 세분화된 trait을 사용.

---

### 3.3. Pipeline 파라미터 정리 (ISP + DIP 해결)

#### 목표
- `#[allow(clippy::too_many_arguments)]` 제거
- `Database` 구체 타입 의존 제거
- 설정을 외부에서 주입

#### 설계: PipelineContext

```rust
// pipeline/context.rs — 새 파일

/// Pipeline 실행에 필요한 모든 의존성을 묶는 컨텍스트.
/// DIP: trait 참조만 보유, 구체 타입 없음.
pub struct PipelineContext {
    pub gh: Arc<dyn Gh>,
    pub git: Arc<dyn Git>,
    pub claude: Arc<dyn Claude>,
    pub env: Arc<dyn Env>,
    pub log_writer: Arc<dyn ConsumerLogWriter>,
    pub config: PipelineConfig,
}

/// ConsumerLogRepository의 write-only subset (ISP)
#[async_trait]
pub trait ConsumerLogWriter: Send + Sync {
    fn write_log(&self, log: &NewConsumerLog);
}

/// Pipeline에 필요한 설정만 추출 (DIP: config 로딩 로직에 의존하지 않음)
pub struct PipelineConfig {
    pub issue_concurrency: usize,
    pub pr_concurrency: usize,
    pub merge_concurrency: usize,
    pub confidence_threshold: f64,
    pub max_review_iterations: u32,
    pub issue_workflow: String,
    pub pr_workflow: String,
}
```

**기존 시그니처**:
```rust
pub async fn process_pending(
    db: &Database, env: &dyn Env, workspace: &Workspace<'_>,
    notifier: &Notifier<'_>, gh: &dyn Gh, claude: &dyn Claude,
    queues: &mut TaskQueues,
) -> Result<()>
```

**개선 시그니처**:
```rust
pub async fn analyze_one(item: IssueItem, ctx: &PipelineContext) -> TaskOutput
```

---

### 3.4. GitRepository 분해 (SRP 해결)

#### 목표
- God Object인 `GitRepository` (~700줄)를 역할별로 분해
- 각 역할이 독립적으로 테스트 가능

#### 설계

```rust
// domain/git_repository.rs — 구조 분해

/// GitRepository: 순수 데이터 컨테이너 + 큐 보유 (SRP: 상태 보유만)
pub struct GitRepository {
    id: String,
    name: String,
    url: String,
    gh_host: Option<String>,
    pub issue_queue: StateQueue<IssueItem>,
    pub pr_queue: StateQueue<PrItem>,
    pub merge_queue: StateQueue<MergeItem>,
}

// 기존 scan 로직 → scanner 모듈로 이동
// domain/scanner.rs — 새 파일 또는 scanner/ 모듈 확장
pub struct RepoScanner;

impl RepoScanner {
    /// Issue scan (autodev:analyze 라벨 기반)
    pub async fn scan_issues(
        repo: &mut GitRepository,
        gh: &dyn GhQuery,
        labels_gh: &dyn GhLabels,
        ignore_authors: &[String],
        filter_labels: &[String],
    ) -> Result<u64> { ... }

    /// Approved issue scan
    pub async fn scan_approved(
        repo: &mut GitRepository,
        gh: &dyn GhQuery,
        labels_gh: &dyn GhLabels,
    ) -> Result<u64> { ... }
}

// 기존 reconcile 로직 → recovery 모듈로 이동
// domain/reconciler.rs 또는 daemon/recovery.rs 확장
pub struct RepoReconciler;

impl RepoReconciler {
    pub async fn startup_reconcile(
        repo: &mut GitRepository,
        gh: &dyn Gh,
    ) -> u64 { ... }

    pub async fn recover_orphan_wip(
        repo: &mut GitRepository,
        gh: &dyn Gh,
    ) -> u64 { ... }
}
```

---

### 3.5. Daemon 루프 정리 (SRP 해결)

#### 목표
- `start()` 함수에서 daily report 로직 분리
- 각 tick 단계를 별도 함수로 추출

#### 설계

```rust
// daemon/mod.rs — 리팩토링

/// Daemon 이벤트 루프의 각 단계
struct DaemonLoop {
    repos: HashMap<String, GitRepository>,
    tracker: InFlightTracker,
    join_set: JoinSet<TaskOutput>,
    ctx: PipelineContext,
    db: Database,
}

impl DaemonLoop {
    /// Tick: 한 주기의 모든 작업 수행
    async fn tick(&mut self) {
        self.sync_repos().await;
        self.run_recovery().await;
        self.run_scans().await;
        self.spawn_ready_tasks();
    }

    /// 레포 동기화 (DB ↔ HashMap)
    async fn sync_repos(&mut self) { ... }

    /// Recovery: orphan 정리
    async fn run_recovery(&mut self) { ... }

    /// Scan: per-repo 스캔
    async fn run_scans(&mut self) { ... }

    /// Task spawn
    fn spawn_ready_tasks(&mut self) { ... }
}

// Daily report: 별도 모듈로 분리
// daemon/daily_scheduler.rs — 새 파일
pub struct DailyScheduler {
    report_hour: u32,
    last_report_date: String,
    knowledge_enabled: bool,
}

impl DailyScheduler {
    /// 일간 리포트 트리거 여부 확인 + 실행
    pub async fn maybe_run(&mut self, ctx: &DailyContext) { ... }
}
```

---

## 4. 변경 요약

| 위반 ID | SOLID | 심각도 | 개선 | 영향 범위 |
|---------|-------|--------|------|----------|
| V-SRP-1 | SRP | **높음** | Pipeline Handler 패턴 | pipeline/issue.rs 전체 |
| V-SRP-2 | SRP | **높음** | Pipeline Handler 패턴 | pipeline/pr.rs 전체 |
| V-SRP-3 | SRP | 중간 | Daemon 루프 분리 | daemon/mod.rs |
| V-SRP-4 | SRP | **높음** | GitRepository 분해 | domain/git_repository.rs |
| V-OCP-1 | OCP | 중간 | Verdict 전략 패턴 | pipeline/issue.rs, pr.rs |
| V-OCP-2 | OCP | 낮음 | (Phase 2에서) | daemon/mod.rs |
| V-OCP-3 | OCP | 낮음 | (Phase 2에서) | daemon/mod.rs |
| V-ISP-1 | ISP | 중간 | Gh trait 분리 | infrastructure/gh/ |
| V-ISP-2 | ISP | 중간 | PipelineContext | pipeline/ 전체 |
| V-DIP-1 | DIP | 낮음 | (Pipeline Handler가 해결) | components/ |
| V-DIP-2 | DIP | 중간 | ConsumerLogWriter trait | pipeline/ → queue/ |
| V-DIP-3 | DIP | 낮음 | PipelineConfig 주입 | pipeline/ |

---

## 5. 개선 효과 예측

### Before (현재)

```
pipeline/issue.rs:  ~1000줄 (4개 함수, 각 200-300줄)
pipeline/pr.rs:     ~1000줄 (6개 함수, 각 150-200줄)
pipeline/merge.rs:   ~340줄 (2개 함수)
domain/git_repository.rs: ~700줄 (God Object)
daemon/mod.rs:       ~750줄 (모든 책임 혼재)
```

### After (개선 후)

```
pipeline/
  handler.rs:             ~50줄 (trait + 타입 정의)
  runner.rs:              ~80줄 (부수 효과 적용)
  context.rs:             ~40줄 (의존성 묶기)
  verdict_strategy.rs:    ~80줄 (verdict별 결과 생성)
  steps/
    analyze_issue.rs:     ~50줄 (분석 핵심 로직)
    implement_issue.rs:   ~60줄 (구현 핵심 로직)
    review_pr.rs:         ~60줄 (리뷰 핵심 로직)
    improve_pr.rs:        ~50줄 (개선 핵심 로직)
    merge_pr.rs:          ~50줄 (머지 핵심 로직)
domain/
  git_repository.rs:      ~100줄 (순수 데이터)
  scanner.rs:             ~200줄 (scan 로직)
  reconciler.rs:          ~150줄 (recovery 로직)
daemon/
  mod.rs:                 ~200줄 (이벤트 루프)
  daily_scheduler.rs:     ~100줄 (daily report)
infrastructure/gh/
  mod.rs:                 4개 sub-trait + blanket impl
```

### 정량 비교

| 지표 | Before | After | 개선 |
|------|--------|-------|------|
| 최대 함수 크기 | ~316줄 | ~80줄 | **75% 감소** |
| 코드 중복 (issue.rs) | 2x (batch + spawnable) | 1x | **중복 제거** |
| 코드 중복 (pr.rs) | 2x (batch + spawnable) | 1x | **중복 제거** |
| pipeline/ 총 LOC | ~2340줄 | ~520줄 | **78% 감소** |
| mock 메서드 수 (Gh) | 9개 전부 | 역할별 2-3개 | **66% 감소** |
| `#[allow(too_many_arguments)]` | 5곳 | 0곳 | **제거** |
| `#[allow(dead_code)]` pipeline | 3곳 | 0곳 | **제거** |

---

## 6. 비적용 항목 (의도적 제외)

| 항목 | 제외 이유 |
|------|----------|
| `Workspace`/`Notifier` trait 화 | Pipeline Handler의 `StepRunner`가 이 역할을 흡수하므로 별도 trait 불필요 |
| `StateQueue` 리팩토링 | 현재 구현이 SOLID 잘 준수. 변경 불필요 |
| `config/` 리팩토링 | 설정 모듈은 DIP만 PipelineConfig로 해결하면 충분 |
| `tui/` 리팩토링 | 별도 scope |
| `knowledge/` 리팩토링 | 현재 구조가 적절. 별도 개선 시 분리 |
| LSP 관련 | 현재 위반 없음 |
