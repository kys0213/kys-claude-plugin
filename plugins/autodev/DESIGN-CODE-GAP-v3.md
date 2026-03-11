# DESIGN-v3 ↔ Code Gap Analysis

**Date**: 2026-03-11
**Scope**: DESIGN-v3.md + DESIGN-v3-ARCHITECTURE.md vs. 실제 코드 (`cli/src/`)

---

## 1. Label Transition Ordering: add-first 원칙 미준수

**설계**: "Label transition ordering (add-first): New labels added before removing old ones to prevent loss on crashes"

**코드**: 모든 Task 구현체에서 **remove-first** 패턴을 사용하고 있음.

| Task | 전이 | 코드 순서 | 설계 순서 |
|------|------|-----------|-----------|
| AnalyzeTask (wontfix) | wip → skip | `remove(WIP)` → `add(SKIP)` | `add(SKIP)` → `remove(WIP)` |
| AnalyzeTask (implement) | wip → analyzed | `remove(WIP)` → `add(ANALYZED)` | `add(ANALYZED)` → `remove(WIP)` |
| AnalyzeTask (clarify) | wip → skip | `remove(WIP)` → `add(SKIP)` | `add(SKIP)` → `remove(WIP)` |
| AnalyzeTask (closed) | wip → done | `remove(WIP)` → `add(DONE)` | `add(DONE)` → `remove(WIP)` |
| ImplementTask (no PR) | implementing → impl-failed | `remove(IMPLEMENTING)` → `add(IMPL_FAILED)` | `add(IMPL_FAILED)` → `remove(IMPLEMENTING)` |
| ReviewTask (approve) | wip → done | `remove(WIP)` → `add(DONE)` | `add(DONE)` → `remove(WIP)` |
| ReviewTask (request_changes) | wip → changes-requested | `remove(WIP)` → `add(CHANGES_REQUESTED)` | `add(CHANGES_REQUESTED)` → `remove(WIP)` |
| ImproveTask (success) | changes-requested → wip | `remove(CHANGES_REQUESTED)` → `add(WIP)` | `add(WIP)` → `remove(CHANGES_REQUESTED)` |
| scan_approved_issues | approved → implementing | `remove(APPROVED)` → `remove(ANALYZED)` → `add(IMPLEMENTING)` | `add(IMPLEMENTING)` → `remove(...)` |

**영향도**: High — 크래시 시 라벨이 모두 제거된 상태가 되어 아이템을 잃을 수 있음.

**위치**: `tasks/analyze.rs`, `tasks/implement.rs`, `tasks/review.rs`, `tasks/improve.rs`, `domain/git_repository.rs`

---

## 2. ExtractTask: agent 실패 시 `extract-failed` 라벨 미부착

**설계**: "Idempotency: Adds `autodev:extracted` or `autodev:extract-failed` (retry by removing label)"

**코드** (`tasks/extract.rs:248-256`): `after_invoke`에서 agent exit_code와 관계없이 항상 `autodev:extracted` 라벨을 추가함.

```rust
// agent 실패(exit_code != 0) 시에도 실행됨
self.gh.label_add(..., labels::EXTRACTED, ...).await;
```

- Worktree 생성 실패 시: `extract-failed` 라벨 정상 부착 ✅ (`before_invoke`)
- Agent 호출 실패 시: `extracted` 라벨 부착 ❌ (설계상 `extract-failed`이어야 함)

**영향도**: Medium — agent 실패한 extraction을 재시도할 수 없음 (수동으로 `extracted` 제거 필요).

---

## 3. ImplementTask: preflight 검사 누락

**설계**: AnalyzeTask와 동일하게 issue가 open 상태인지 preflight 검사가 필요.

**코드** (`tasks/implement.rs:100-147`): `before_invoke`에서 issue 상태를 확인하지 않음. Workspace 준비만 수행.

- AnalyzeTask: `gh.api_get_field("issues/{n}", ".state")` → closed면 skip ✅
- ImplementTask: 상태 확인 없이 바로 worktree 생성 ❌

**영향도**: Low-Medium — 이미 닫힌 이슈에 대해 불필요한 구현 작업이 실행될 수 있음.

---

## 4. PR Queue 상태 머신: Improved → Reviewing vs. Improved → Pending

**설계** (DESIGN-v3.md):
```
ReviewDone → Improving → Improved → Pending (re-review)
```

**코드** (`sources/github.rs:281-295`):
```rust
// PR: Improved → Reviewing (re-review)
repo.pr_queue.drain_to(pr_phase::IMPROVED, pr_phase::REVIEWING, pr_slots);
```

Improved 상태에서 Pending을 거치지 않고 바로 Reviewing으로 전이됨.

**영향도**: Low — 기능적으로는 올바르게 동작하지만, 문서와 코드의 상태 머신이 불일치.

---

## 5. ReviewStage `max_iterations` 기본값 — 테스트 코멘트 오류

**코드** (`config/models.rs:134-139`):
```rust
impl Default for ReviewStage {
    fn default() -> Self {
        Self { command: None, max_iterations: 2 }
    }
}
```

**테스트** (`tasks/review.rs:672`):
```rust
pr.review_iteration = 3; // default max is 3  ← 잘못된 코멘트
```

실제 기본값은 `2`이며 테스트 로직 자체는 정상 동작 (3 >= 2 → skip).

**영향도**: Negligible — 코멘트만 부정확, 기능에 영향 없음.

---

## 6. CONFIG-SCHEMA-v2의 `agent` 필드 — deprecated 전이 불완전

**설계** (CONFIG-SCHEMA-v2.md):
```yaml
workflows:
  analyze:
    agent: autodev:issue-analyzer
  review:
    agent: autodev:pr-reviewer
```

**코드**: `WorkflowStage`에 `agent` 필드가 없으며, `deny_unknown_fields` 미적용으로 YAML에 있어도 무시됨.

- `deprecated_agent_field_is_silently_ignored` 테스트가 이를 의도적으로 검증 ✅
- CONFIG-SCHEMA-v2.md 문서가 현재 코드 구조를 반영하지 않음 ❌

**영향도**: Low — 사용자가 `agent` 필드를 설정해도 무시될 뿐 에러는 발생하지 않음. 문서 업데이트 필요.

---

## Gap Summary

| # | Gap | 심각도 | 수정 난이도 |
|---|-----|--------|------------|
| 1 | Label add-first 원칙 미준수 (전체 Task) | **High** | Medium — 모든 label 전이 순서 역전 필요 |
| 2 | ExtractTask agent 실패 시 extract-failed 미부착 | **Medium** | Low — 조건 분기 1곳 추가 |
| 3 | ImplementTask preflight 검사 누락 | **Medium** | Low — AnalyzeTask 패턴 복제 |
| 4 | Improved → Reviewing (설계: Improved → Pending) | **Low** | Low — 문서 또는 코드 중 택 1 수정 |
| 5 | 테스트 코멘트 "default max is 3" 오류 | **Negligible** | Trivial |
| 6 | CONFIG-SCHEMA-v2 agent 필드 문서 미업데이트 | **Low** | Trivial — 문서 수정 |

---

## 일치 항목 (설계 ↔ 코드 일치)

| 항목 | 상태 |
|------|------|
| Task trait 구조 (work_id, repo_name, before_invoke, after_invoke) | ✅ |
| Agent trait (invoke → AgentResponse) | ✅ |
| TaskManager trait (tick, drain_ready, pop_ready, apply, active_items) | ✅ |
| TaskRunner trait (run) | ✅ |
| TaskSource trait (poll, apply, active_items) | ✅ |
| DTO: AgentRequest, AgentResponse, TaskResult, QueueOp | ✅ |
| Daemon event loop (4-arm select: completion, tick, heartbeat, shutdown) | ✅ |
| Module 구조 (daemon/, tasks/, sources/, components/, infrastructure/) | ✅ |
| InFlightTracker (per-repo + global max) | ✅ |
| Label 상수 (모든 label 정의) | ✅ |
| Issue queue phases (Pending → Analyzing → Ready → Implementing) | ✅ |
| PR queue phases (Pending → Reviewing → ReviewDone → Improving → Improved → Extracting) | ✅ |
| Auto-approve (confidence threshold 기반) | ✅ |
| impl-failed recovery (worktree 보존 + label) | ✅ |
| changes-requested 정규 스캔 (recovery-only 아님) | ✅ |
| sync_default_branch (workspace.ensure_cloned에서 호출) | ✅ |
| Knowledge extraction (per-task: PR done + merged + NOT extracted) | ✅ |
| Per-repo concurrency (issue_concurrency, pr_concurrency) | ✅ |
| Startup reconcile (bounded recovery) | ✅ |
| Config deep merge (global + repo override) | ✅ |
| 3-tier state management (GitHub Labels → SQLite → In-Memory Queue) | ✅ |
