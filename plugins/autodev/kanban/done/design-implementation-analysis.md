# DESIGN.md vs 구현 정합성 분석 리포트

> **Date**: 2026-02-24
> **Scope**: `plugins/autodev/` 전체 (DESIGN.md ↔ cli/src/)
> **결론**: 설계와 구현은 대부분 일치하며, 핵심 아키텍처는 정확히 반영됨.
>           아래에 남아있는 차이점을 카테고리별로 정리.

---

## 1. 완전 일치 항목 (설계 = 구현)

아래 항목들은 DESIGN.md의 설계가 구현에 정확히 반영되어 있다.

| 설계 항목 | 구현 위치 | 상태 |
|-----------|----------|------|
| 3-Tier 상태 관리 (GitHub Labels + SQLite + In-Memory StateQueue) | `queue/`, `daemon/mod.rs` | ✅ 일치 |
| GitHub 라벨 = SSOT (autodev:wip, autodev:done, autodev:skip) | `queue/task_queues.rs` labels 모듈 | ✅ 일치 |
| 라벨 상태 전이 (없음 → wip → done/skip) | `pipeline/*.rs`, `scanner/*.rs` | ✅ 일치 |
| SQLite Schema (repositories, scan_cursors, consumer_logs만) | `queue/schema.rs` | ✅ 일치 |
| In-Memory StateQueue 구조 (HashMap<State, VecDeque<T>> + dedup index) | `queue/state_queue.rs` | ✅ 일치 |
| TaskQueues (issues + prs + merges + cross-queue contains) | `queue/task_queues.rs` | ✅ 일치 |
| WorkId 형식 ("{type}:{repo}:{number}") | `task_queues::make_work_id()` | ✅ 일치 |
| Phase 정의 (Issue: 4단계, PR: 5단계, Merge: 3단계) | `task_queues::{issue_phase,pr_phase,merge_phase}` | ✅ 일치 |
| Daemon 메인 루프 순서 (startup_reconcile → recovery → scan → consume → sleep) | `daemon/mod.rs::start()` | ✅ 일치 |
| Startup Reconciliation (bounded window, 라벨 기반 필터) | `daemon/mod.rs::startup_reconcile()` | ✅ 일치 |
| Recovery (orphan wip 정리) | `daemon/recovery.rs` | ✅ 일치 |
| Scanner (cursor 기반 incremental scan + dedup) | `scanner/issues.rs`, `scanner/pulls.rs` | ✅ 일치 |
| Issue Flow (Pending → Analyzing → Ready → Implementing → done) | `pipeline/issue.rs` | ✅ 일치 |
| PR Flow (Pending → Reviewing → ReviewDone → Improving → Improved → 재리뷰 반복) | `pipeline/pr.rs` | ✅ 일치 |
| Merge Flow (Pending → Merging → done / Conflict → 해결) | `pipeline/merge.rs` | ✅ 일치 |
| Infrastructure trait 추상화 (Gh, Git, Claude + mock/real) | `infrastructure/` | ✅ 일치 |
| Components (Workspace, Notifier, Reviewer, Merger, Verdict) | `components/` | ✅ 일치 |
| Workspace (git worktree 생명주기) | `components/workspace.rs` | ✅ 일치 |
| `[autodev]` 프롬프트 마커 삽입 | `pipeline/*.rs`, `components/merger.rs` | ✅ 일치 |
| Session Runner (claude -p + JSON output) | `infrastructure/claude/` | ✅ 일치 |
| CLI 서브커맨드 (start/stop/restart/status/dashboard/repo/logs) | `main.rs` | ✅ 일치 |
| PID 파일 기반 단일 인스턴스 보장 | `daemon/pid.rs` | ✅ 일치 |
| Config 구조체 매핑 (WorkflowConfig 5-section) | `config/models.rs` | ✅ 일치 |
| DaemonConfig 필드 (tick, reconcile_window, daily_report_hour, log_dir, retention) | `config/models.rs` | ✅ 일치 |
| Knowledge Extraction — per-task (done 전이 시) | `knowledge/extractor.rs` | ✅ 일치 |
| Knowledge Extraction — daily report (daemon.YYYY-MM-DD.log 파싱) | `knowledge/daily.rs` | ✅ 일치 |
| Knowledge PR 생성 (suggestions → branch → file write → PR + autodev:skip) | `knowledge/daily.rs::create_knowledge_prs()` | ✅ 일치 |
| suggest-workflow 교차 분석 (tool-frequency, filtered-sessions, repetition) | `knowledge/daily.rs::enrich_with_cross_analysis()` | ✅ 일치 |
| 로그 롤링 (일자별, retention_days) | `daemon/log.rs` | ✅ 일치 |
| Cargo.toml (dependencies, profile.release) | `cli/Cargo.toml` | ✅ 일치 |

---

## 2. 설계 대비 **추가** 구현된 항목

구현에는 있지만 DESIGN.md에 명시되지 않은 기능들. 대부분 운영 편의성 개선.

### 2-1. `infrastructure/suggest_workflow/` 모듈 (신규 인프라 레이어)

- **설계**: DESIGN.md §13에서 `suggest-workflow query` CLI 호출을 언급하지만, 별도 trait/mock 추상화는 미기재
- **구현**: `SuggestWorkflow` trait + `RealSuggestWorkflow` + `MockSuggestWorkflow` 구현체 존재
  - `query_tool_frequency()`, `query_filtered_sessions()`, `query_repetition()` 3개 메서드
- **영향**: DESIGN.md §3 디렉토리 구조에 `infrastructure/suggest_workflow/` 추가 필요
- **심각도**: **Low** — 설계 원칙(DIP, 테스트 가능성)에 부합하는 자연스러운 확장

### 2-2. `daemon/status.rs` (Status file 실시간 갱신)

- **설계**: 미기재 (TUI는 설계되었으나 daemon → TUI 간 상태 전달 메커니즘 미상세)
- **구현**: `daemon.status.json` 파일로 실시간 상태 export
  - `DaemonStatus` 구조체 (updated_at, uptime_secs, active_items, counters)
  - 매 tick마다 atomic write (tmp → rename)
  - `autodev status` CLI에서 이 파일을 읽어 표시
- **영향**: DESIGN.md §10 또는 §9에 status file 메커니즘 추가 필요
- **심각도**: **Low** — 운영 편의 기능

### 2-3. `infrastructure/claude/output.rs` (JSON 응답 파싱 모듈)

- **설계**: §8에서 `serde_json::from_str::<T>()` 정도만 언급
- **구현**: 별도 `output.rs` 모듈에서 `ClaudeJsonOutput`, `AnalysisResult`, `ReviewResult`, `ReviewVerdict` 등 구조화된 파싱 제공
- **영향**: 없음 (구현 디테일)
- **심각도**: **None**

### 2-4. Pipeline에서 Pre-flight check 존재

- **설계**: §5에서 "pre-flight API 호출 불필요 (scan 시점에 open 확인 완료)"라고 명시
- **구현**: `pipeline/issue.rs`에 `notifier.is_issue_open()`, `pipeline/pr.rs`에 `notifier.is_pr_reviewable()`, `pipeline/merge.rs`에 `notifier.is_pr_mergeable()` pre-flight 체크가 존재
- **영향**: 설계와 구현 간 **의도적 불일치**. scan과 consume 사이 시간 차로 인해 이미 close된 이슈를 처리하는 것을 방지하기 위한 방어적 구현
- **심각도**: **Medium** — 설계 문서를 업데이트하여 pre-flight check 추가 사유를 기록하거나, 설계 원문("pre-flight 불필요")을 수정해야 함

### 2-5. `knowledge/models.rs`의 `CrossAnalysis` 필드

- **설계**: §14 JSON Schema에서 `DailyReport`에 `cross_analysis` 필드 미기재
- **구현**: `DailyReport`에 `cross_analysis: Option<CrossAnalysis>` 필드 추가
  - `CrossAnalysis { tool_frequencies, anomalies, sessions }` 구조체
- **영향**: DESIGN.md §14 DailyReport JSON Schema 갱신 필요
- **심각도**: **Low**

### 2-6. Merge scan 소스의 차이

- **설계**: §6 scan() — "approved 상태" PR + "라벨 없는 것만"을 merge 대상으로 스캔
- **구현**: `scanner/pulls.rs::scan_merges()` — `autodev:done` 라벨이 붙은 open PR을 merge 대상으로 스캔 (done → wip 라벨 전환)
- **영향**: 설계는 "approved + 라벨 없는 PR"이 대상이라고 했으나, 구현은 "autodev:done 라벨이 붙은 PR" (= autodev가 리뷰 완료한 PR)이 대상. 논리적으로 더 안전한 방식 (리뷰 완료 PR만 merge)
- **심각도**: **Medium** — 설계 문서의 merge scan 설명을 실제 구현으로 갱신 필요

---

## 3. 설계에 있지만 **미구현** 항목

### 3-1. `components/analyzer.rs` (독립 Analyzer 컴포넌트)

- **설계**: §3에 `analyzer.rs — Analyzer { claude: &dyn Claude } — 이슈 분석` 명시
- **구현**: Analyzer가 독립 컴포넌트로 분리되지 않고 `pipeline/issue.rs::process_pending()` 내에 인라인으로 구현됨
  - 분석 프롬프트 생성, Claude 호출, 응답 파싱이 모두 pipeline 함수 안에 있음
- **영향**: SRP 관점에서 분석 로직을 `components/analyzer.rs`로 분리하면 테스트/재사용성 향상
- **심각도**: **Low** — 기능적으로는 완전히 동작하지만, 설계 의도(Components 레이어로 분리)와 불일치

### 3-2. CLI `queue` 서브커맨드 (queue list/retry/clear)

- **설계**: README.md에 `autodev queue list <repo>`, `autodev queue retry <id>`, `autodev queue clear <repo>` 문서화
- **구현**: `main.rs`에 `Queue` 서브커맨드 없음. REFACTORING-PLAN.md에서 "client/mod.rs는 이번 scope 제외 — IPC 설계 필요"로 명시
- **영향**: InMemory queue 전환 후 CLI에서 queue 접근 불가. IPC(Unix socket 등) 설계 필요
- **심각도**: **Medium** — 운영 시 queue 상태 확인/재시도를 CLI로 할 수 없음. `daemon.status.json`으로 읽기만 가능

### 3-3. `config show` / `config edit` 서브커맨드

- **설계**: §9에 `autodev config show`, `autodev config edit` 명시
- **구현**: `main.rs`에 Config 관련 서브커맨드 없음
- **영향**: 설정 확인/편집은 직접 YAML 파일 수정으로 대체
- **심각도**: **Low** — 편의 기능 미구현

### 3-4. Scan interval 경과 체크의 위치

- **설계**: §5 — `should_scan = db.cursor_should_scan(repo.id, scan_interval_secs)` 로 레포별 체크
- **구현**: `scanner/mod.rs::scan_all()`에서 정확히 이 패턴으로 구현됨 ✅
- (확인 결과 일치 — 초기 미구현 의심 후 확인 완료)

### 3-5. PR review의 `gh pr review` API 호출

- **설계**: §6 PR Flow — `approve` 시 `gh pr review --approve -b "{summary}"`, `request_changes` 시 `POST /pulls/{N}/reviews`
- **구현**: approve/request_changes 모두 GitHub 댓글(`issue_comment`)로 리뷰 결과를 게시. `gh pr review` API 또는 GitHub Reviews API 직접 호출은 없음
- **영향**: GitHub의 공식 "Pull Request Review" 상태(Approved/Changes Requested)가 설정되지 않음. 댓글만 달림
- **심각도**: **High** — merge scan이 `autodev:done` 라벨 기반이므로 기능적으로는 동작하지만, GitHub PR UI에서 리뷰 상태가 "Approved"로 표시되지 않는 UX 차이 존재. Gh trait에 `pr_review()` 메서드 추가 필요

### 3-6. Agent 파일 (issue-analyzer.md, pr-reviewer.md, conflict-resolver.md)

- **설계**: §11에 3개 에이전트 설계
  - `issue-analyzer.md` — Multi-LLM 병렬 분석
  - `pr-reviewer.md` — `/multi-review` 호출
  - `conflict-resolver.md` — Opus 모델 충돌 해결
- **구현**: `agents/` 디렉토리에 3개 파일 존재하지만, 실제 daemon/pipeline 코드에서 이 에이전트 파일을 직접 참조하지 않음. `claude -p` 호출 시 프롬프트에 에이전트 로직이 인라인됨
- **영향**: 에이전트 파일이 있지만 실제로 Claude Code의 에이전트 시스템으로 로드되는지는 별도 확인 필요
- **심각도**: **Low** — 설계 의도는 Claude Code 플러그인 시스템에서 에이전트 파일을 활용하는 것이나, daemon의 `claude -p` 호출에서는 직접 프롬프트 삽입 방식 사용

---

## 4. 구현 세부 차이

### 4-1. StateQueue의 dedup index 위치

- **설계**: `TaskQueues` 수준에서 단일 `index: HashMap<WorkId, State>` 관리
- **구현**: `StateQueue<T>` 각각이 자체 `index: HashMap<String, String>` 보유. `TaskQueues.contains()`는 3개 큐의 `contains()`를 OR 연산
- **영향**: 기능적으로 동일하지만, 설계의 "전체 dedup index O(1)"이 구현에서는 "3번 lookup O(1)"로 변환됨
- **심각도**: **None** — 성능 차이 무시 가능

### 4-2. ConsumerConfig에 설계에 없는 필드

- **설계**: §12에 명시된 ConsumerConfig 필드
- **구현**: 추가 필드 존재
  - `stuck_threshold_secs: u64` — 설계의 InMemory 전환 후 불필요할 수 있음 (DB 기반 stuck 탐지용이었으나 현재는 미사용)
  - `workspace_strategy: String` — 설계에 미기재
  - `gh_host: Option<String>` — GitHub Enterprise 지원용, 설계에 미기재
- **심각도**: **Low** — 추가 필드일 뿐 설계 위반 아님

### 4-3. Cursor 갱신 시점

- **설계**: §6 — 신규 아이템 발견 후 `cursor 전진: db.cursor_upsert(repo.id, "issues", latest_updated_at)`
- **구현**: `scanner/issues.rs` — 모든 이슈를 순회한 후 `latest_updated_at`으로 cursor 갱신. autodev 라벨이 있는 이슈의 `updated_at`도 cursor 전진에 반영됨
- **영향**: 설계에서는 "신규 아이템만" cursor를 전진시키는 것처럼 보이지만, 구현은 스캔된 모든 아이템 중 가장 최신의 `updated_at`으로 전진. 이는 올바른 구현 (autodev 라벨이 있는 이슈가 가장 최신이면 그 시점까지 커서를 전진시켜야 다음 스캔에서 중복 조회 방지)
- **심각도**: **None** — 구현이 설계보다 정확함

### 4-4. MergeItem에 `title` 필드

- **설계**: §Phase 1 models — `MergeItem`에 title 필드 없음
- **구현**: `MergeItem`에 `title: String` 필드 존재 (TUI 표시용)
- **심각도**: **None**

---

## 5. 요약 및 권장 조치

### 통계

| 카테고리 | 수 |
|----------|---:|
| 완전 일치 | 27 |
| 설계 대비 추가 구현 | 6 |
| 설계에 있지만 미구현 | 5 |
| 세부 차이 | 4 |

### 우선순위별 권장 조치

#### High Priority

| # | 항목 | 조치 |
|---|------|------|
| H-1 | PR review API 미호출 (§3-5) | Gh trait에 `pr_review()` 메서드 추가, approve 시 `gh pr review --approve` 호출 구현 |

#### Medium Priority

| # | 항목 | 조치 |
|---|------|------|
| M-1 | Pre-flight check 설계 불일치 (§2-4) | DESIGN.md §5의 "pre-flight API 호출 불필요" 문구를 실제 구현(방어적 pre-flight 포함)으로 갱신 |
| M-2 | Merge scan 소스 차이 (§2-6) | DESIGN.md §6의 merge scan 설명을 "autodev:done 라벨 기반"으로 갱신 |
| M-3 | CLI queue 서브커맨드 미구현 (§3-2) | IPC 설계 후 구현하거나, 불필요 시 README.md에서 제거 |

#### Low Priority

| # | 항목 | 조치 |
|---|------|------|
| L-1 | suggest_workflow 인프라 미기재 (§2-1) | DESIGN.md §3에 `infrastructure/suggest_workflow/` 추가 |
| L-2 | daemon.status.json 미기재 (§2-2) | DESIGN.md §9에 status file 메커니즘 추가 |
| L-3 | DailyReport cross_analysis 미기재 (§2-5) | DESIGN.md §14 JSON Schema 갱신 |
| L-4 | Analyzer 컴포넌트 미분리 (§3-1) | 리팩토링 검토 또는 설계 문서에서 인라인 방식으로 갱신 |
| L-5 | config show/edit 미구현 (§3-3) | 구현하거나 설계에서 제거 |
| L-6 | stuck_threshold_secs 잔여 (§4-2) | 미사용 확인 후 ConsumerConfig에서 제거 |
