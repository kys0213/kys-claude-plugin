# Autodev: DESIGN.md vs 구현 Gap 분석 리포트 (v2)

> **Date**: 2026-02-23 (v2 — 코드 기반 재검증)
> **Previous**: 2026-02-22 (v1)
> **Scope**: `plugins/autodev/DESIGN.md` ↔ `plugins/autodev/cli/src/` 전체 소스코드 1:1 대조
> **방법**: v1 리포트의 각 gap을 현재 코드에서 직접 검증하여 해소/잔존 상태 갱신

---

## 1. Executive Summary

v1 리포트(2026-02-22)에서 식별한 12건의 gap 중 **5건이 해소**, **7건이 잔존**한다.
특히 H-01(reconcile_window_hours), H-02(Config 구조), H-03(PR verdict), M-02(merge scan)는
v1 이후 수정되어 설계와 일치한다.

### v1 → v2 변경 요약

| v1 ID | 심각도 | v1 상태 | v2 상태 | 변경 사유 |
|-------|--------|---------|---------|----------|
| H-01 | High | Gap | **해소** | `DaemonConfig.reconcile_window_hours` 필드 존재, daemon에서 설정값 사용 |
| H-02 | High | Gap | **해소** | `DaemonConfig` 구조체 분리됨 (`tick_interval_secs`, `reconcile_window_hours`, `daily_report_hour`) |
| H-03 | High | Gap | **해소** | `ReviewResult { verdict, summary, comments }` JSON 파싱 구현됨 (`output.rs:59-108`) |
| M-01 | Medium | Gap | **잔존** | Phase 상수 여전히 축소 (Analyzing/Implementing/Reviewing/Merging/Conflict 없음) |
| M-02 | Medium | Gap | **해소** | `scan_merges()` 함수 구현됨 (`scanner/pulls.rs:139-222`) |
| M-03 | Medium | Gap | **잔존** | suggest-workflow 통합 여전히 미구현 |
| M-04 | Medium | Gap | **부분 해소** | `reqwest` 불필요 확인 (gh CLI 기반), `tracing-appender` 여전히 누락 |
| M-05 | Medium | Gap | **잔존** | 로그 롤링/보존 여전히 미구현 |
| L-01 | Low | Gap | **잔존 (승격: Medium)** | TUI `query_active_items()`, `query_label_counts()` 빈 값 반환 |
| L-02 | Low | Gap | **해소** | `ReviewOutput.verdict: Option<ReviewVerdict>` 필드 존재 |
| L-03 | Low | Gap | **잔존** | `session/output.rs` → `infrastructure/claude/output.rs` 모듈명 불일치 |
| L-04 | Low | Gap | **잔존** | GAP-ANALYSIS.md 미갱신 |

### 현재 요약

| 심각도 | 건수 | 설명 |
|--------|------|------|
| **High** | 0건 | v1의 3건 모두 해소 |
| **Medium** | 5건 | 기능 누락 또는 부분 구현 (L-01 승격 포함) |
| **Low** | 2건 | 문서 정합성, 모듈명 불일치 |

---

## 2. 해소된 Gap (5건)

### ~~H-01~~: `reconcile_window_hours` — **해소**

`config/models.rs:18-32`:
```rust
pub struct DaemonConfig {
    pub tick_interval_secs: u64,
    pub reconcile_window_hours: u32,    // ← 필드 존재
    pub daily_report_hour: u32,
}
// Default: reconcile_window_hours: 24
```

`daemon/mod.rs:58`:
```rust
let reconcile_window_hours = cfg.daemon.reconcile_window_hours;  // ← 설정에서 읽음
```

---

### ~~H-02~~: Config 구조 — **해소**

설계의 `DaemonConfig`와 `ConsumerConfig` 분리가 구현에 반영됨.

`config/models.rs`:
```rust
pub struct WorkflowConfig {
    pub consumer: ConsumerConfig,  // scan, concurrency, model 등
    pub daemon: DaemonConfig,      // tick_interval, reconcile_window, daily_report
    pub workflow: WorkflowRouting,
    pub commands: CommandsConfig,
    pub develop: DevelopConfig,
}
```

**차이점** (설계 갱신 권장):
- 설계는 `repos[]` 배열로 per-repo 설정을 정의하나, 구현은 글로벌 `ConsumerConfig` + 워크스페이스별 `.develop-workflow.yaml` 오버라이드
- `log_dir`, `log_retention_days` 필드는 `DaemonConfig`에 아직 없음 (M-05와 연관)

---

### ~~H-03~~: PR 리뷰 verdict — **해소**

`infrastructure/claude/output.rs:59-108`:
```rust
pub struct ReviewResult {
    pub verdict: ReviewVerdict,      // ← JSON "approve" | "request_changes"
    pub summary: String,
    pub comments: Vec<ReviewComment>,
}

pub fn parse_review(stdout: &str) -> Option<ReviewResult> { ... }
```

`pipeline/pr.rs:127-174`:
```rust
match output.verdict {
    Some(ReviewVerdict::Approve) => { /* approve → done */ }
    Some(ReviewVerdict::RequestChanges) | None => { /* feedback loop */ }
}
```

approve와 request_changes를 JSON verdict 기반으로 정확히 분기함.

---

### ~~M-02~~: Merge scan — **해소**

`scanner/pulls.rs:139-222`: `scan_merges()` 구현됨.
`scanner/mod.rs:78-87`: `auto_merge` 설정 확인 후 `scan_merges()` 호출.

---

### ~~L-02~~: Reviewer 반환 타입 — **해소**

`ReviewOutput`에 `verdict: Option<ReviewVerdict>` 필드가 존재하여 구조화된 verdict 전달 가능.

---

## 3. 잔존 Gap — Medium (5건)

### M-01: Phase 상태 축소 — Analyzing/Implementing/Reviewing/Merging/Conflict 미구현

| | 설계 (DESIGN.md §2) | 구현 (`task_queues.rs:83-100`) |
|---|------|------|
| Issue | `Pending → Analyzing → Ready → Implementing` (4) | `Pending → Ready` (2) |
| PR | `Pending → Reviewing → ReviewDone → Improving → Improved` (5) | `Pending → ReviewDone → Improved` (3) |
| Merge | `Pending → Merging → Conflict` (3) | `Pending` (1) |

**코드 증거** (`task_queues.rs:83-100`):
```rust
pub mod issue_phase {
    pub const PENDING: &str = "Pending";
    pub const READY: &str = "Ready";
    // Analyzing, Implementing 없음
}
pub mod pr_phase {
    pub const PENDING: &str = "Pending";
    pub const REVIEW_DONE: &str = "ReviewDone";
    pub const IMPROVED: &str = "Improved";
    // Reviewing, Improving 없음
}
pub mod merge_phase {
    pub const PENDING: &str = "Pending";
    // Merging, Conflict 없음
}
```

**원인**: 분석/구현/리뷰/머지 실행이 `pop()` 후 인라인으로 수행됨. 상태 전이가 아닌 함수 내 동기 처리.

**영향**:
- TUI에서 "현재 무슨 작업 중인지" 세부 상태 표시 불가
- 데몬 로그의 상태 전이 이벤트가 설계보다 거침

**권장**: 설계를 구현에 맞게 갱신하거나, TUI 가시성이 필요하면 phase 추가

---

### M-03: suggest-workflow 통합 미구현

| | 설계 (DESIGN.md §13) | 구현 |
|---|------|------|
| 데이터 소스 A | daemon.YYYY-MM-DD.log | ✅ `knowledge/daily.rs:25-91` — `parse_daemon_log()` |
| 데이터 소스 B | suggest-workflow index.db | ❌ 미구현 |
| Per-task | daemon.log + suggest-workflow 교차 분석 | Claude 세션 분석만 (`knowledge/extractor.rs:14-64`) |
| Daily | suggest-workflow query 3종 호출 | daemon.log 파싱 + Claude 분석만 (`knowledge/daily.rs`) |

**코드 증거** (`knowledge/extractor.rs:23-34`):
```rust
let prompt = format!(
    "[autodev] knowledge: per-task {task_type} #{github_number}\n\n\
     Analyze the completed {task_type} task..."
);
let result = claude.run_session(wt_path, &prompt, None).await;
// ← suggest-workflow CLI 호출 없음
```

설계에서 기대하는 `suggest-workflow query --perspective tool-frequency --session-filter ...` 등의 호출이 전혀 없음.

**영향**: Knowledge Extraction이 daemon.log + Claude 추론에만 의존. 도구 사용 패턴, 파일 수정 이상치 등 정량적 인사이트 누락.

**권장**: 별도 이슈로 분리. suggest-workflow CLI wrapper를 infrastructure에 추가 후 knowledge 모듈에서 호출.

---

### M-04 (축소): Cargo.toml — `tracing-appender` 누락

| 의존성 | 설계 (DESIGN.md §4) | 구현 (Cargo.toml) | 판정 |
|--------|------|------|------|
| `reqwest` | 있음 | 없음 | **설계 갱신** (gh CLI로 대체됨) |
| `tracing-appender` | 있음 | 없음 | **구현 필요** (M-05와 연관) |
| `async-trait` | 없음 | 있음 | 설계 갱신 (async trait에 필요) |
| `libc` | 없음 | 있음 | 설계 갱신 (PID/signal에 필요) |
| `serde_yaml` | 없음 | 있음 | 설계 갱신 (YAML config에 필요) |

실질적 gap은 `tracing-appender` 1건. 나머지는 설계 문서 갱신 사항.

---

### M-05: 로그 롤링/보존 미구현

| | 설계 (DESIGN.md §9) | 구현 |
|---|------|------|
| 롤링 | `tracing-appender::rolling::daily()` | ❌ `tracing_subscriber::fmt()` → stderr |
| 보존 | `log_retention_days` 설정 + 자동 삭제 | ❌ 미구현 |
| 로그 파일 | `~/.autodev/logs/daemon.YYYY-MM-DD.log` | ❌ 파일 생성 없음 |

**코드 증거** (`main.rs:76-81`):
```rust
tracing_subscriber::fmt()
    .with_env_filter(...)
    .init();
// ← rolling appender 설정 없음, stderr 출력
```

**영향**: Daily Report(`daemon/mod.rs:108-111`)가 `daemon.{yesterday}.log` 파일을 읽으려 하지만:
```rust
let log_path = home.join(format!("daemon.{yesterday}.log"));
if log_path.exists() { ... }  // ← 파일이 존재하지 않으므로 항상 skip
```
**Daily Report가 사실상 작동하지 않음.**

**권장**: `tracing-appender` 도입 + `DaemonConfig`에 `log_dir`, `log_retention_days` 추가

---

### M-06 (신규, L-01 승격): TUI 대시보드 데이터 표시 불가

`tui/views.rs:121-130`:
```rust
pub fn query_active_items(_db: &Database) -> Vec<ActiveItem> {
    // Active items are now tracked in daemon memory (TaskQueues).
    // TUI will show them when daemon status file is implemented.
    Vec::new()  // ← 항상 빈 배열
}

pub fn query_label_counts(_db: &Database) -> LabelCounts {
    // Label counts are managed on GitHub, not in local DB.
    LabelCounts::default()  // ← 항상 0
}
```

**영향**: TUI 대시보드의 Active Items, Labels Summary 패널이 항상 비어있음.

**권장**: daemon에서 주기적으로 `~/.autodev/daemon.status.json`에 TaskQueues 스냅샷을 저장하고, TUI가 이를 읽어 표시.

---

## 4. 잔존 Gap — Low (2건)

### L-03: 모듈 경로 불일치

설계 `§3`: `session/output.rs`
구현: `infrastructure/claude/output.rs`

기능적 영향 없음. 설계 문서 갱신 사항.

### L-04: GAP-ANALYSIS.md 미갱신

`GAP-ANALYSIS.md`는 "모든 gap 해소" 상태로 남아있으나, 본 리포트에서 7건의 잔존 gap 확인.
리팩토링 scope 내 gap과 설계 전체 scope gap의 구분이 필요.

---

## 5. 구현이 설계보다 나은 부분

| 항목 | 설계 | 구현 | 평가 |
|------|------|------|------|
| Pre-flight check | 불필요 (scan에서 확인) | `is_issue_open()`, `is_pr_reviewable()`, `is_pr_mergeable()` | **안전** — scan-consume 시차 보정 |
| Verdict enum | String 암시 | `Verdict { Implement, NeedsClarification, Wontfix }` + serde | **타입 안전** |
| confidence_threshold | 설정 존재 암시 | `ConsumerConfig.confidence_threshold: f64` (0.7) | **실용적** |
| Concurrency 제어 | 미정의 | `issue/pr/merge_concurrency` 설정 | **실용적** |
| review max_iterations | 미정의 | `ReviewConfig.max_iterations: u32` (2) | **안전** — 무한 루프 방지 |
| Config 오버라이드 | per-repo YAML | 글로벌 + 워크스페이스별 `.develop-workflow.yaml` 딥머지 | **유연** |
| Knowledge PR 생성 | §13에서 설계 | `knowledge/daily.rs:230-300` 완전 구현 | 설계 충족 |
| Daily Report 이슈 게시 | §13에서 설계 | `knowledge/daily.rs:207-221` 완전 구현 | 설계 충족 |

---

## 6. Gap별 수정 난이도 (잔존 7건)

| ID | 내용 | 난이도 | 추정 수정 범위 |
|----|------|--------|-------------|
| M-01 | Phase 세분화 | 중간 | phase 상수 추가 + pipeline 상태 전이 리팩토링, 또는 설계 갱신 |
| M-03 | suggest-workflow 통합 | 높음 | infrastructure 모듈 추가 + knowledge 모듈 연동 |
| M-04 | tracing-appender 추가 | 낮음 | Cargo.toml + main.rs 로깅 설정 변경 |
| M-05 | 로그 롤링/보존 | 중간 | tracing-appender 설정 + DaemonConfig 필드 + retention 로직 |
| M-06 | TUI 데이터 표시 | 중간 | daemon status file 생성 + TUI 읽기 로직 |
| L-03 | 모듈명 불일치 | 낮음 | DESIGN.md §3 경로 수정 |
| L-04 | GAP-ANALYSIS.md 갱신 | 낮음 | 문서 갱신 |

---

## 7. 권장 사항

### 즉시 수정 (Daily Report 작동에 필수)
1. **M-04 + M-05**: `tracing-appender` 도입 → `daemon.YYYY-MM-DD.log` 자동 생성
   - 이것이 해결되지 않으면 Daily Report 기능이 **완전히 작동하지 않음**
   - `DaemonConfig`에 `log_dir: PathBuf`, `log_retention_days: u32` 추가

### 단기 개선
2. **M-06**: daemon status file 구현 → TUI 대시보드 데이터 표시
3. **L-03, L-04**: 문서 정합성 갱신

### 중기 개선 (별도 이슈)
4. **M-01**: Phase 세분화 여부 결정 (설계 갱신 vs 구현 확장)
5. **M-03**: suggest-workflow 통합 (scope 큼, 별도 설계 필요)

---

## 8. 결론

v1(2026-02-22) 대비 **핵심 아키텍처 gap 3건(H-01~H-03)이 모두 해소**되었다.
- `DaemonConfig` 분리, `reconcile_window_hours` 설정화, PR verdict JSON 파싱, merge scan 모두 구현 완료
- 핵심 아키텍처(In-Memory StateQueue + GitHub Labels SSOT)는 설계와 완전히 일치

**잔존 gap 7건**은 아키텍처 불일치가 아닌 **기능 완성도** 수준:
- **Critical**: 로그 롤링 미구현으로 Daily Report가 작동하지 않음 (M-04/M-05)
- **Important**: TUI 데이터 표시 불가 (M-06), suggest-workflow 미연동 (M-03)
- **Nice-to-have**: Phase 세분화 (M-01), 문서 정합성 (L-03/L-04)

전체적으로 구현 완성도는 **설계 대비 ~85%** 수준이며, 로그 롤링만 해결하면 ~90%에 도달한다.
