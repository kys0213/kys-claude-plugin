# SQLite Queue → In-Memory StateQueue 리팩토링 - 완료

## 항목

- [x] **Phase 1: StateQueue + TaskQueues 구현**
  - `queue/state_queue.rs` — `StateQueue<T>` 제네릭 구조체 (HashMap + VecDeque + dedup index)
  - `queue/task_queues.rs` — `TaskQueues` (issues, prs, merges StateQueue + phase 상수)
  - 15+ unit tests

- [x] **Phase 2: Gh trait label_add 추가**
  - `infrastructure/gh/mod.rs` — `label_add()` 메서드 추가
  - `infrastructure/gh/real.rs`, `mock.rs` — 구현체

- [x] **Phase 3: Scanner → StateQueue 전환**
  - `scanner/issues.rs`, `scanner/pulls.rs` — `queues.contains()` 기반 dedup
  - `scanner/mod.rs` — `TaskQueues` 시그니처 전환

- [x] **Phase 4: Pipeline → StateQueue 전환 + 라벨 관리**
  - `pipeline/issue.rs` — `process_pending()`, `process_ready()` StateQueue 기반
  - `pipeline/pr.rs` — `process_pending()`, `process_review_done()`, `process_improved()`
  - `pipeline/merge.rs` — `process_pending()` StateQueue 기반

- [x] **Phase 5: Daemon 루프 + startup_reconcile**
  - `daemon/mod.rs` — `TaskQueues::new()` 생성, `startup_reconcile()` 구현
  - `daemon/recovery.rs` — `TaskQueues` 기반 orphan wip 복구

- [x] **Phase 6: SQLite 큐 테이블 제거**
  - `queue/schema.rs` — `issue_queue`, `pr_queue`, `merge_queue` 테이블 제거
  - `queue/repository.rs` — `IssueQueueRepository`, `PrQueueRepository`, `MergeQueueRepository` 제거

- [x] **Phase 7: PR 피드백 루프 구현**
  - `pipeline/pr.rs` — Pending → ReviewDone → Improved → (재리뷰 루프)

- [x] **Phase 8: 테스트 작성**
  - `queue/state_queue.rs` — 15 unit tests
  - `queue/task_queues.rs` — 6 unit tests
  - integration tests — daemon_consumer, daemon_scan, pipeline_e2e 등

## 검증일

- 2026-02-23: 전체 구현 완료 확인 (311 tests passing)
