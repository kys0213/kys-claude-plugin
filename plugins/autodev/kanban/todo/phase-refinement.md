# Phase 상태 세분화 (M-01)

> **Priority**: Medium — TUI 가시성 및 설계 정합성
> **Gap Report**: DESIGN-GAP-REPORT.md v2 §3 M-01
> **난이도**: 중간

## 배경

설계에서는 세밀한 phase 상태 전이를 정의하지만, 구현에서는 축소된 phase만 사용.
실행 중인 작업의 세부 상태(Analyzing vs Implementing 등)를 TUI/로그에서 구분할 수 없음.

## 항목

- [x] **27. Issue phase 상수 확장**
  - `queue/task_queues.rs` — `issue_phase`에 `ANALYZING`, `IMPLEMENTING` 추가
  - 설계: `Pending → Analyzing → Ready → Implementing`

- [x] **28. PR phase 상수 확장**
  - `queue/task_queues.rs` — `pr_phase`에 `REVIEWING`, `IMPROVING` 추가
  - 설계: `Pending → Reviewing → ReviewDone → Improving → Improved`

- [x] **29. Merge phase 상수 확장**
  - `queue/task_queues.rs` — `merge_phase`에 `MERGING`, `CONFLICT` 추가
  - 설계: `Pending → Merging → Conflict`

- [x] **30. Pipeline 상태 전이 리팩토링**
  - `pipeline/issue.rs` — pop(PENDING) → push(ANALYZING) → 분석 완료 → remove → push(READY)
  - `pipeline/pr.rs` — 동일 패턴 적용 (REVIEWING, IMPROVING)
  - `pipeline/merge.rs` — 동일 패턴 적용 (MERGING, CONFLICT)

## 추가 수정

- [x] **TUI 색상 매핑 버그 수정** (`tui/views.rs`)
  - 기존: lowercase 비교 (`"pending"`) — PascalCase 상수와 불일치하여 항상 DarkGray 표시
  - 수정: PascalCase 매칭 + 새 중간 상태(Analyzing, Reviewing 등) 색상 추가

## 완료 상태

```rust
// Issue: 4개 (설계 1:1 매칭)
pub mod issue_phase {
    pub const PENDING: &str = "Pending";
    pub const ANALYZING: &str = "Analyzing";
    pub const READY: &str = "Ready";
    pub const IMPLEMENTING: &str = "Implementing";
}
// PR: 5개 (설계 1:1 매칭)
pub mod pr_phase {
    pub const PENDING: &str = "Pending";
    pub const REVIEWING: &str = "Reviewing";
    pub const REVIEW_DONE: &str = "ReviewDone";
    pub const IMPROVING: &str = "Improving";
    pub const IMPROVED: &str = "Improved";
}
// Merge: 3개 (설계 1:1 매칭)
pub mod merge_phase {
    pub const PENDING: &str = "Pending";
    pub const MERGING: &str = "Merging";
    pub const CONFLICT: &str = "Conflict";
}
```

## 관련 파일

| 파일 | 변경 내용 |
|------|----------|
| `queue/task_queues.rs` | phase 상수 6개 추가 + 테스트 5개 추가 |
| `pipeline/issue.rs` | 상태 전이 기반 리팩토링 (Analyzing, Implementing) |
| `pipeline/pr.rs` | 상태 전이 기반 리팩토링 (Reviewing, Improving) |
| `pipeline/merge.rs` | 상태 전이 기반 리팩토링 (Merging, Conflict) |
| `tui/views.rs` | 색상 매핑 PascalCase 수정 + 신규 상태 추가 |

## 완료 조건

- [x] 설계 phase와 구현 phase가 1:1 매칭
- [x] TUI에서 현재 작업의 세부 상태 표시 가능
- [x] 데몬 로그에 세밀한 상태 전이 이벤트 기록
