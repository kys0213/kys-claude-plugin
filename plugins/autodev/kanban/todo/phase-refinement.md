# Phase 상태 세분화 (M-01)

> **Priority**: Medium — TUI 가시성 및 설계 정합성
> **Gap Report**: DESIGN-GAP-REPORT.md v2 §3 M-01
> **난이도**: 중간

## 배경

설계에서는 세밀한 phase 상태 전이를 정의하지만, 구현에서는 축소된 phase만 사용.
실행 중인 작업의 세부 상태(Analyzing vs Implementing 등)를 TUI/로그에서 구분할 수 없음.

## 항목

- [ ] **27. Issue phase 상수 확장**
  - `queue/task_queues.rs` — `issue_phase`에 `ANALYZING`, `IMPLEMENTING` 추가
  - 설계: `Pending → Analyzing → Ready → Implementing`

- [ ] **28. PR phase 상수 확장**
  - `queue/task_queues.rs` — `pr_phase`에 `REVIEWING`, `IMPROVING` 추가
  - 설계: `Pending → Reviewing → ReviewDone → Improving → Improved`

- [ ] **29. Merge phase 상수 확장**
  - `queue/task_queues.rs` — `merge_phase`에 `MERGING`, `CONFLICT` 추가
  - 설계: `Pending → Merging → Conflict`

- [ ] **30. Pipeline 상태 전이 리팩토링**
  - `pipeline/issue.rs` — pop(PENDING) → push(ANALYZING) → 분석 완료 → transit(ANALYZING→READY) → ...
  - `pipeline/pr.rs` — 동일 패턴 적용
  - `pipeline/merge.rs` — 동일 패턴 적용

## 대안: 설계 문서 갱신

구현의 인라인 처리 방식이 충분히 실용적이라 판단되면,
DESIGN.md §2 Phase 정의를 현행 구현에 맞게 갱신하는 것도 선택지.

## 현재 상태

```rust
// Issue: 2개만 (설계는 4개)
pub mod issue_phase {
    pub const PENDING: &str = "Pending";
    pub const READY: &str = "Ready";
}
// PR: 3개만 (설계는 5개)
pub mod pr_phase {
    pub const PENDING: &str = "Pending";
    pub const REVIEW_DONE: &str = "ReviewDone";
    pub const IMPROVED: &str = "Improved";
}
// Merge: 1개만 (설계는 3개)
pub mod merge_phase {
    pub const PENDING: &str = "Pending";
}
```

## 관련 파일

| 파일 | 변경 내용 |
|------|----------|
| `queue/task_queues.rs` | phase 상수 추가 |
| `pipeline/issue.rs` | 상태 전이 기반 리팩토링 |
| `pipeline/pr.rs` | 상태 전이 기반 리팩토링 |
| `pipeline/merge.rs` | 상태 전이 기반 리팩토링 |

## 완료 조건

- [ ] 설계 phase와 구현 phase가 1:1 매칭 (또는 설계 문서 갱신)
- [ ] TUI에서 현재 작업의 세부 상태 표시 가능
- [ ] 데몬 로그에 세밀한 상태 전이 이벤트 기록
