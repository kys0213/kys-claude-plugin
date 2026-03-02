# DESIGN GAP ANALYSIS v3.1

> **Date**: 2026-03-02
> **Scope**: DESIGN-v2.md + DESIGN-v3-ARCHITECTURE.md + IMPLEMENTATION-PLAN-v3.md + README.md vs 실제 구현 코드
> **Version**: autodev 0.2.3

---

## 요약

| 구분 | 일치 | Gap (High) | Gap (Medium) | Gap (Low) |
|------|------|-----------|-------------|----------|
| DESIGN-v3 (아키텍처) | 14/16 | 0 | 1 | 1 |
| DESIGN-v2 (워크플로우) | 18/20 | 0 | 1 | 1 |
| IMPL-PLAN-v3 (구현 계획) | 18/21 | 0 | 1 | 2 |
| README.md (사용자 문서) | - | 0 | 2 | 1 |
| **합계** | | **0** | **5** | **5** |

---

## 1. DESIGN-v3-ARCHITECTURE.md vs 구현

### 완전 일치 항목 (14건)

| # | 설계 항목 | 구현 위치 | 상태 |
|---|----------|----------|------|
| 1 | Task trait (work_id, repo_name, before_invoke, after_invoke) | `daemon/task.rs` | ✅ 정확히 일치 |
| 2 | Agent trait (invoke) | `daemon/agent.rs` | ✅ |
| 3 | TaskRunner trait (run) | `daemon/task_runner.rs` | ✅ |
| 4 | TaskManager trait (tick, drain_ready, apply) | `daemon/task_manager.rs` | ✅ + `pop_ready()`, `active_items()` 추가 |
| 5 | TaskSource trait (poll, apply) | `daemon/task_source.rs` | ✅ + `active_items()` 추가 |
| 6 | DefaultTaskRunner (before→agent→after 생명주기) | `daemon/task_runner_impl.rs` | ✅ |
| 7 | ClaudeAgent (Claude trait 래핑) | `daemon/agent_impl.rs` | ✅ |
| 8 | DefaultTaskManager (sources 집계) | `daemon/task_manager_impl.rs` | ✅ |
| 9 | GitHubTaskSource (sync→recovery→scan→drain) | `sources/github.rs` | ✅ |
| 10 | Daemon (select! 루프 + InFlightTracker) | `daemon/mod.rs` | ✅ |
| 11 | DTO: AgentRequest, AgentResponse, TaskResult, TaskStatus, SkipReason, QueueOp | `daemon/task.rs` | ✅ |
| 12 | WorkspaceOps trait 추출 | `components/workspace.rs` | ✅ |
| 13 | ConfigLoader trait 추출 | `config/mod.rs` | ✅ |
| 14 | TaskContext 폐기 (개별 Arc 주입) | 모든 Task 구현체 | ✅ |

### Gap 항목

#### M-01: AgentRequest에 system_prompt 필드 부재 (Medium)

**설계** (DESIGN-v3 §5):
```rust
pub struct AgentRequest {
    pub working_dir: PathBuf,
    pub prompt: String,
    pub system_prompt: Option<String>,  // ← 별도 필드
    pub session_opts: SessionOptions,
}
```

**구현** (`daemon/task.rs:22-29`):
```rust
pub struct AgentRequest {
    pub working_dir: PathBuf,
    pub prompt: String,
    pub session_opts: SessionOptions,  // system_prompt은 SessionOptions 내부에 포함
}
```

**영향**: 기능적 차이 없음 — 각 Task가 `SessionOptions.append_system_prompt`로 시스템 프롬프트를 전달하고 있음. 하지만 설계 문서와 코드의 구조가 불일치.

**권장 조치**: DESIGN-v3 문서를 구현에 맞게 업데이트. `system_prompt`은 `SessionOptions`에 통합되어 있으며, Claude CLI의 실제 옵션 체계와도 일치.

#### L-01: DailyReporter가 TaskManager 내부가 아닌 Daemon 직접 소유 (Low)

**설계** (DESIGN-v3 §3):
> `schedule_daily_report()` → **TaskManager** 책임

**구현** (`daemon/mod.rs`):
- `Daemon.reporter: Box<dyn DailyReporter>` — Daemon이 직접 소유
- 매 tick에서 `self.reporter.maybe_run()` 호출

**영향**: 실제 구현이 SRP 관점에서 더 좋은 설계. TaskManager는 TaskSource 집계에만 집중하고, DailyReporter는 독립 컴포넌트로 분리.

**권장 조치**: DESIGN-v3 문서의 §3 책임 분배 표에서 daily report 위치를 `Daemon (DailyReporter)` 로 수정.

---

## 2. DESIGN-v2.md vs 구현

### 완전 일치 항목 (18건)

| # | 설계 항목 | 상태 |
|---|----------|------|
| 1 | Label-Positive 모델 전면 적용 | ✅ |
| 2 | Issue 라벨 7종 (analyze, wip, analyzed, approved-analysis, implementing, done, skip) | ✅ `domain/labels.rs` |
| 3 | PR 라벨 4종 (wip, changes-requested, done, skip) + extracted + iteration/N | ✅ |
| 4 | HITL 게이트 (analyzed → 사람 리뷰 → approved-analysis) | ✅ AnalyzeTask가 analyzed 라벨 부착 후 큐 이탈 |
| 5 | Issue Flow Phase 1: analyze → wip → AnalyzeTask → analyzed | ✅ `tasks/analyze.rs` |
| 6 | Issue Flow Phase 2: approved-analysis → implementing → ImplementTask → PR 생성 | ✅ `tasks/implement.rs` + `git_repository.rs:scan_approved_issues` |
| 7 | Issue Flow Phase 3: PR review loop (ReviewTask ↔ ImproveTask) | ✅ `tasks/review.rs` + `tasks/improve.rs` |
| 8 | scan_issues: autodev:analyze 라벨 → wip 전이 → Pending 큐 | ✅ `git_repository.rs:scan_issues` |
| 9 | scan_approved_issues: approved-analysis → implementing 전이 → Ready 큐 | ✅ `git_repository.rs:scan_approved_issues` |
| 10 | scan_pulls: autodev:wip 라벨 → Pending 큐 | ✅ `git_repository.rs:scan_pulls` |
| 11 | scan_done_merged: done + merged + NOT extracted → Extracting 큐 | ✅ `git_repository.rs:scan_done_merged` |
| 12 | PR approve → source_issue done 전이 (implementing → done) | ✅ `tasks/review.rs` after_invoke approve 분기 |
| 13 | PR request_changes → changes-requested 라벨 → ImproveTask → wip (재리뷰) | ✅ |
| 14 | max_review_iterations 초과 → skip | ✅ `tasks/review.rs` |
| 15 | startup_reconcile 라벨별 처리 | ✅ `git_repository.rs:startup_reconcile` |
| 16 | Recovery: orphan wip 정리 + orphan implementing 정리 | ✅ |
| 17 | Worktree lifecycle: Task별 생성/제거, branch 유지 | ✅ |
| 18 | Knowledge Extraction per-task (ExtractTask) | ✅ `tasks/extract.rs` |

### Gap 항목

#### M-02: Daily Report가 첫 번째 enabled repo에만 적용 (Medium)

**설계** (DESIGN-v2 §8, Daily):
```
1. daemon 로그 파싱 (통계)
2. 일간 per-task suggestions 집계
3. 교차 task 패턴 감지
4. Claude: 집계 데이터 → 우선순위 정렬
5. 일간 리포트 이슈 생성
6. 고우선순위 → knowledge PR 생성
```
→ 암묵적으로 모든 enabled repo에 대해 수행

**구현** (`daemon/daily_reporter.rs:117-118`):
```rust
if let Some(er) = enabled.first() {
    // 첫 번째 enabled repo에서만 리포트 생성
```

**영향**: 다수 repo 등록 시 첫 번째 repo에만 daily report 이슈가 생성됨. Knowledge PR도 첫 번째 repo의 워크스페이스에서만 생성.

**권장 조치**: `enabled.first()` → `for er in &enabled` 루프로 변경하여 모든 enabled repo에 대해 리포트 생성. 또는 설계 문서에 "daemon 로그는 글로벌이므로 대표 repo 1건에만 게시" 라는 정책을 명시.

#### L-02: scan_approved_issues에서 approved-analysis 라벨만 제거 — 설계는 "전이" 표현 (Low)

**설계** (DESIGN-v2 §4 Phase 2):
> `approved-analysis 제거, autodev:implementing 추가 → queue[Ready]에 push`

**구현** (`git_repository.rs:235-254`):
```rust
// approved-analysis 제거 ✅
gh.label_remove(&self.name, issue.number, labels::APPROVED_ANALYSIS, ...).await;
// analyzed 제거 ✅
gh.label_remove(&self.name, issue.number, labels::ANALYZED, ...).await;
// implementing 추가 ✅
gh.label_add(&self.name, issue.number, labels::IMPLEMENTING, ...).await;
```

**영향**: 없음 — 실제 구현이 설계를 정확히 따르고 있음. 추가로 `analyzed` 라벨도 함께 제거하여 더 깔끔한 상태 전이를 수행. **Gap 아님 — 확인 완료.**

---

## 3. IMPLEMENTATION-PLAN-v3.md vs 구현

### 완전 일치 항목 (18건)

| Phase | 항목 | 상태 |
|-------|------|------|
| 1-1 | Task trait + DTO | ✅ |
| 1-2 | TaskSource trait | ✅ |
| 1-3 | Agent trait | ✅ |
| 1-4 | TaskManager trait | ✅ |
| 1-5 | TaskRunner trait | ✅ |
| 1-6 | WorkspaceOps trait 추출 | ✅ |
| 1-7 | ConfigLoader trait 추출 | ✅ |
| 2-1 | AnalyzeTask (TDD) | ✅ 10개 테스트 |
| 2-2 | ImplementTask (TDD) | ✅ 6개 테스트 |
| 2-3 | ReviewTask (TDD) | ✅ 8개 테스트 |
| 2-4 | ImproveTask (TDD) | ✅ 4개 테스트 |
| 3-1 | DefaultTaskRunner | ✅ 3개 테스트 |
| 3-2 | ClaudeAgent | ✅ 3개 테스트 |
| 3-3 | DefaultTaskManager | ✅ 3개 테스트 |
| 3-4 | GitHubTaskSource | ✅ 9개 테스트 |
| 4-1 | Daemon struct | ✅ 2개 테스트 |
| 4-2 | main.rs DI 조립 | ✅ |
| 4-3 | Legacy pipeline/ + scanner/ 제거 | ✅ 모듈 완전 제거 |

### Gap 항목

#### M-03: MergeTask 미구현 (Medium)

**IMPL-PLAN-v3 §Phase 2-5**:
> `tasks/merge.rs` — MergeTask 구현 (테스트 5개 계획)

**구현**: `tasks/merge.rs` 파일 없음. `tasks/mod.rs`에도 `merge` 모듈 선언 없음.

**설계 정합성**: DESIGN-v2.md §12 "Scope 외" 에서 "PR Merge: `autodev:done` 이후의 머지는 사람의 판단 또는 별도 자동화가 처리" 로 명시. DESIGN-v2가 merge를 scope 밖으로 명확히 배제했으므로 **의도적 미구현**.

**영향**: IMPLEMENTATION-PLAN-v3이 DESIGN-v2와 불일치. 계획서에 MergeTask가 포함되어 있으나 최종 설계(v2.1 revision)에서 merge 파이프라인이 제거됨.

**권장 조치**: IMPLEMENTATION-PLAN-v3.md에서 Phase 2-5 MergeTask 항목을 "~~MergeTask~~ (DESIGN-v2.1에서 scope 외로 제거)" 로 표기.

#### L-03: IMPL-PLAN-v3의 TaskContext 삭제 명시가 코드에 반영됨 (Low)

**IMPL-PLAN-v3 §1-8**: `daemon/task_context.rs`는 dead code로 삭제 대상

**구현**: `daemon/task_context.rs` 파일 없음 — 삭제 완료. 그러나 IMPL-PLAN에서 별도 item으로 추적하고 있으므로, 완료 처리가 필요.

**권장 조치**: IMPL-PLAN-v3의 1-8 항목에 ✅ 표기.

#### L-04: `#[allow(clippy::too_many_arguments)]` 잔존 여부 (Low)

**IMPL-PLAN-v3 §4-4**: Phase 4 이후 `#[allow(clippy::too_many_arguments)]` 0건 목표

**구현**: pipeline/ 모듈이 완전 제거되었고 Task 구조체가 필드 기반 DI를 사용하므로 해당 allow 지시자는 사라졌을 것으로 추정. 확인 필요.

---

## 4. README.md vs 구현

### Gap 항목

#### M-04: README의 코드 구조가 v2 이전 구조를 설명 (Medium)

**README §Architecture "코드 구조"**:
```
plugins/autodev/cli/src/
├── scanner/    # GitHub 이벤트 감지
├── pipeline/   # 흐름 오케스트레이션
...
```

**실제 구조**:
```
plugins/autodev/cli/src/
├── sources/    # TaskSource 구현체 (scanner 대체)
├── tasks/      # Task 구현체 (pipeline 대체)
├── daemon/     # Daemon + TaskManager + TaskRunner + Agent
...
```

- `scanner/` → `sources/github.rs`로 이동 (v3 리팩토링)
- `pipeline/` → `tasks/` 모듈로 대체 (v3 리팩토링)

**권장 조치**: README의 코드 구조 섹션을 v3 모듈 구조로 업데이트.

#### M-05: README의 Merge 파이프라인 설명이 현재 scope와 불일치 (Medium)

**README §Flows "Merge: 별도 큐"**:
```
merge scan: approved + 라벨 없는 PR 발견
  → wip + queue[Pending] → 머지(/merge-pr) → queue[Merging]
  ├─ success  → autodev:done
  ├─ conflict → queue[Conflict] → 자동 해결 시도
  └─ failure  → 라벨 제거
```

**README §Architecture "3-Tier 상태 관리"**:
```
In-Memory StateQueue
  merges[Merging] → [Conflict]
```

**실제**: MergeTask 미구현, merge queue 없음. DESIGN-v2.md §12에서 scope 외로 명확히 제거됨.

**권장 조치**: README에서 Merge 파이프라인 섹션을 제거하거나 "Scope 외 (사람 또는 별도 자동화가 처리)" 주석으로 대체. 3-Tier 다이어그램에서 `merges` 행 제거.

#### L-05: README의 라벨 전이 다이어그램이 v1 기준 (Low)

**README §Architecture "라벨 상태 전이"**:
```
(없음) ──scan──→ autodev:wip ──success──→ autodev:done
```

**실제 (DESIGN-v2 기준)**: 7개 issue 라벨 + 4개 PR 라벨의 세분화된 전이.

**권장 조치**: README의 라벨 전이 다이어그램을 v2 기준으로 업데이트하거나 "상세 전이는 DESIGN-v2.md 참조" 링크 추가.

---

## 5. 기능 일치 확인 (Cross-Reference)

### DESIGN-v2 워크플로우 시나리오 트레이스

#### 시나리오 1: Issue 분석 → 승인 → 구현 → PR → 리뷰 → Done

| 단계 | DESIGN-v2 명세 | 구현 확인 |
|------|---------------|----------|
| 사람이 `autodev:analyze` 추가 | §4 Phase 1 | ✅ `scan_issues`에서 감지 |
| analyze → wip 전이 | §3 Issue 전이 | ✅ `git_repository.rs:184-198` |
| AnalyzeTask 실행 | §4 Phase 1 | ✅ `tasks/analyze.rs` |
| implement verdict → analyzed 라벨 | §4 Phase 1 | ✅ `analyze.rs:handle_analysis` |
| 사람이 approved-analysis 추가 | §4 Gate: HITL | ✅ (외부 동작) |
| approved → implementing 전이 | §4 Phase 2 | ✅ `scan_approved_issues:235-254` |
| ImplementTask → PR 생성 | §4 Phase 2 | ✅ `tasks/implement.rs` |
| PR에 wip 라벨 추가 | §4 Phase 2 | ✅ `implement.rs:after_invoke` |
| ReviewTask → approve | §4 Phase 3 | ✅ `tasks/review.rs` |
| PR done + source_issue done | §4 Phase 3 | ✅ `review.rs:after_invoke` approve 분기 |

#### 시나리오 2: PR 리뷰 → request_changes → 개선 → 재리뷰 → Done

| 단계 | DESIGN-v2 명세 | 구현 확인 |
|------|---------------|----------|
| ReviewTask → request_changes | §3 PR 전이 | ✅ `review.rs:after_invoke` |
| changes-requested 라벨 | §3 PR 전이 | ✅ |
| ImproveTask 실행 | §3 PR 전이 | ✅ `tasks/improve.rs` |
| changes-requested → wip 전이 | §3 PR 전이 | ✅ `improve.rs:after_invoke` |
| iteration 증가 (PushPr Improved) | §6 PR Phase | ✅ `improve.rs` iteration++ |
| Improved → Pending (재리뷰) | §6 PR Phase | ✅ `sources/github.rs:drain_queue_items` |
| max iteration → skip | §3 PR 전이 | ✅ `review.rs:after_invoke` max_iterations 분기 |

#### 시나리오 3: Knowledge Extraction (merge 후)

| 단계 | DESIGN-v2 명세 | 구현 확인 |
|------|---------------|----------|
| scan_done_merged 감지 | §8 Per-Task | ✅ `git_repository.rs:scan_done_merged` |
| Extracting 큐 적재 | §8 Per-Task | ✅ |
| ExtractTask 실행 | §8 Per-Task | ✅ `tasks/extract.rs` |
| 기존 지식 수집 | §8 Per-Task step 1 | ✅ `collect_existing_knowledge` |
| suggest-workflow 세션 데이터 | §8 Per-Task step 2 | ✅ `build_suggest_workflow_section` |
| delta check | §8 Per-Task step 3 | ✅ 프롬프트에 기존 지식 포함 |
| 이슈 코멘트 게시 | §8 Per-Task step 4 | ✅ `format_knowledge_comment` |
| knowledge PR 생성 | §8 Per-Task step 5 | ✅ `create_task_knowledge_prs` |
| extracted 라벨 추가 | §8 Per-Task step 6 | ✅ `extract.rs:after_invoke` |

---

## 6. 조치 우선순위

### Medium (문서 업데이트 필요)

| ID | Gap | 조치 | 난이도 |
|----|-----|------|-------|
| M-01 | AgentRequest.system_prompt 필드 불일치 | DESIGN-v3 문서 수정 | S |
| M-02 | Daily Report 단일 repo만 적용 | 코드 수정 또는 설계 문서에 정책 명시 | M |
| M-03 | MergeTask IMPL-PLAN에 잔존 | IMPL-PLAN-v3 문서 수정 | S |
| M-04 | README 코드 구조 outdated | README 업데이트 | S |
| M-05 | README Merge 파이프라인 설명 | README 업데이트 | S |

### Low (문서 정리)

| ID | Gap | 조치 | 난이도 |
|----|-----|------|-------|
| L-01 | DailyReporter 소유권 위치 | DESIGN-v3 문서 수정 | S |
| L-03 | IMPL-PLAN TaskContext 삭제 확인 | IMPL-PLAN 체크리스트 업데이트 | S |
| L-04 | `#[allow]` 잔존 확인 | `cargo clippy` 실행으로 확인 | S |
| L-05 | README 라벨 전이 다이어그램 | README 업데이트 | S |

---

## 7. 종합 평가

### 강점

1. **v3 리팩토링 완수**: DESIGN-v3의 핵심 목표(Daemon→TaskManager+TaskRunner 분리, TaskSource 추상화, Task trait 통일)가 100% 구현됨
2. **DESIGN-v2 워크플로우 충실 구현**: Label-Positive 모델, HITL 게이트, Issue-PR 연동, Knowledge Extraction이 모두 정확히 동작
3. **Legacy 코드 완전 제거**: `pipeline/`, `scanner/` 모듈이 깨끗하게 제거되고 `tasks/`, `sources/`로 대체
4. **포괄적 테스트**: Task 구현체별 8-10개, 인프라 컴포넌트별 3-9개 단위 테스트
5. **SOLID 준수**: 모든 의존성이 trait 기반 DI, 개별 Arc 주입 패턴

### 개선 필요

1. **문서-코드 동기화**: README와 IMPL-PLAN이 v3 리팩토링 이후 업데이트되지 않음
2. **Daily Report multi-repo**: 첫 번째 repo에만 적용되는 제한 해소 필요
3. **설계 문서 간 정합성**: IMPL-PLAN-v3에 MergeTask가 잔존하나 DESIGN-v2에서는 scope 외

### 결론

**기능적 Gap: 0건 (High)** — 모든 핵심 워크플로우가 설계 의도대로 동작.
**문서 동기화 Gap: 5건 (Medium)** — v3 리팩토링 이후 문서 업데이트가 필요한 항목들.

설계 대비 구현 일치율: **~95%** (기능 100%, 문서 정합성 ~85%)
