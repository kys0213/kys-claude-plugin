# DESIGN-v2 최종 리뷰 리포트

> **Date**: 2026-02-28
> **Scope**: DESIGN-v2.md의 모든 섹션 vs 현재 구현 코드 전수 대조
> **Result**: **기능상 Gap 없음 (100% 구현 완료)**

---

## 리뷰 방법

DESIGN-v2.md의 10개 섹션을 구현 코드와 1:1 대조하고, 이전 갭 분석(2026-02-25)에서
식별된 3개 Open Gap(A/B/C)의 해결 여부를 코드 레벨에서 검증하였다.

---

## 섹션별 대조 결과

### 1. 변경 동기 / v2 목표 — ✅ 완전 충족

| 목표 | 구현 위치 | 상태 |
|------|----------|------|
| 분석 리뷰 게이트 (HITL) | `AnalyzeTask` → `analyzed` 라벨 → 사람 승인 → `approved-analysis` | ✅ |
| Issue-PR 연동 | `PrItem.source_issue_number` + `autodev:pr-link:XX` 마커 | ✅ |
| 세분화된 라벨 | `domain/labels.rs` — 7종 이슈 + 3종 PR 라벨 | ✅ |

### 2. Label Scheme v2 — ✅ 완전 일치

| 설계 라벨 | 코드 상수 | 전이 로직 위치 |
|-----------|----------|--------------|
| `autodev:analyze` | `labels::ANALYZE` | `git_repository.rs:scan_issues()` |
| `autodev:wip` | `labels::WIP` | 전 Task에서 사용 |
| `autodev:analyzed` | `labels::ANALYZED` | `analyze.rs:handle_analysis()` |
| `autodev:approved-analysis` | `labels::APPROVED_ANALYSIS` | `git_repository.rs:scan_approved_issues()` |
| `autodev:implementing` | `labels::IMPLEMENTING` | `scan_approved_issues()`, `implement.rs` |
| `autodev:done` | `labels::DONE` | `review.rs` approve, `merge.rs` success |
| `autodev:skip` | `labels::SKIP` | `analyze.rs` wontfix/clarify |

**Label-Positive 모델**: `scan_issues()`가 `labels=autodev:analyze` 파라미터로 필터링하여,
트리거 라벨이 명시적으로 있는 이슈만 처리. 크래시 안전성 확보. ✅

### 3. Issue Flow v2 — ✅ 전체 파이프라인 일치

```
Phase 1 (Analysis):
  scan_issues() → analyze 제거 + wip 추가 + Pending        ✅ git_repository.rs:214-228
  AnalyzeTask → 분석 코멘트 게시 + wip→analyzed             ✅ analyze.rs:170-201

Gate (Human Review):
  analyzed → 사람이 approved-analysis 추가 (수동)            ✅ (외부 동작, 코드 불필요)

Phase 2 (Implementation):
  scan_approved_issues() → approved/analyzed 제거
    + implementing 추가 + Ready                             ✅ git_repository.rs:265-303
  ImplementTask → PR 생성 + PR queue push                   ✅ implement.rs:141-273

Phase 3 (PR Review Loop):
  ReviewTask → approve/request_changes 분기                  ✅ review.rs:244-365
  ImproveTask → iteration++ + IMPROVED                      ✅ improve.rs:148-175
  IMPROVED → re-review (ReviewTask)                         ✅ github.rs:256-267

PR approve → source_issue implementing→done:
  review.rs:272-285에서 source_issue_number 확인 후
  IMPLEMENTING 제거 + DONE 추가                              ✅
```

### 4. Scan 구조 (Label-Positive) — ✅ 완전 일치

| 설계 | 구현 | 상태 |
|------|------|------|
| `issues::scan()` — `labels=autodev:analyze` | `git_repository.rs:scan_issues()` | ✅ |
| `issues::scan_approved()` — `labels=autodev:approved-analysis` | `git_repository.rs:scan_approved_issues()` | ✅ |
| `pulls::scan()` — since=cursor, no autodev label | `git_repository.rs:scan_pulls()` | ✅ |
| `pulls::scan_merges()` — `labels=autodev:done`, open | `git_repository.rs:scan_merges()` | ✅ |
| Safety Valve 불필요 | 미구현 (설계 의도대로) | ✅ |

### 5. Issue Phase 정의 — ✅ 완전 일치

| Phase | 설계 | 구현 상수 |
|-------|------|----------|
| trigger → Pending | analyze → wip | `issue_phase::PENDING` |
| Pending → Analyzing | queue drain | `issue_phase::ANALYZING` |
| exit → analyzed | 분석 완료 | queue에서 제거 |
| Ready | approved scan | `issue_phase::READY` |
| Ready → Implementing | queue drain | `issue_phase::IMPLEMENTING` |
| exit → implementing | PR 생성 후 | queue에서 제거 |
| done | PR approve 시 | ReviewTask에서 전이 |

### 6. Worktree & Branch Lifecycle — ✅ 불변식 3가지 모두 충족

- **불변식 1** (생성한 worktree를 자신이 제거): 모든 Task가 `cleanup_worktree()`를
  success/failure 양쪽 경로에서 호출. ✅
- **불변식 2** (head_branch는 remote에 존재): PR pipeline에서 branch를 remote에
  push한 후 worktree만 제거. ✅
- **불변식 3** (worktree 제거 시 branch 미삭제): `remove_worktree()`는 `git worktree remove`만 수행. ✅

### 7. Knowledge Extraction v2 — ✅ Per-Task + Daily 모두 구현

**Per-Task (ExtractTask):**

| 설계 항목 | 구현 위치 | 상태 |
|-----------|----------|------|
| 기존 레포 지식 수집 | `extractor.rs:collect_existing_knowledge()` | ✅ |
| suggest-workflow 세션 데이터 | `extractor.rs:build_suggest_workflow_section()` | ✅ |
| delta check | `before_invoke()` delta_section in prompt | ✅ |
| 이슈 코멘트로 게시 | `after_invoke()` → `format_knowledge_comment()` | ✅ |
| PR 생성 | `create_task_knowledge_prs()` | ✅ |

**Daily (daily.rs):**

| 설계 항목 | 구현 함수 | 상태 |
|-----------|----------|------|
| daemon 로그 파싱 | `parse_daemon_log()` | ✅ |
| 일간 per-task suggestions 집계 | `aggregate_daily_suggestions()` | ✅ |
| 교차 task 패턴 감지 | `detect_cross_task_patterns()` | ✅ |
| Claude 집계 → 우선순위 | `generate_daily_suggestions()` | ✅ |
| 일간 리포트 이슈 생성 | `post_daily_report()` | ✅ |
| knowledge PR 생성 | `create_knowledge_prs()` | ✅ |

### 8. Reconciliation (v2) — ✅ startup + per-tick recovery 모두 구현

**startup_reconcile (git_repository.rs:587-657):**

| 라벨 | 설계 처리 | 구현 | 상태 |
|------|----------|------|------|
| done/skip | skip | `is_terminal()` 체크 | ✅ |
| analyze | skip (다음 scan) | `is_analyze()` 체크 | ✅ |
| analyzed | skip (사람 리뷰 대기) | `is_analyzed()` 체크 | ✅ |
| approved-analysis | Ready 큐 적재 | `is_approved()` → Ready push | ✅ |
| implementing | skip (PR pipeline) | `is_implementing()` 체크 | ✅ |
| wip (orphan) | Pending 적재 | `is_wip()` → Pending push | ✅ |

**per-tick recovery (github.rs:run_recovery → git_repository.rs):**

| 시나리오 | 설계 처리 | 구현 | 상태 |
|----------|----------|------|------|
| wip + queue에 없음 | wip 제거 | `recover_orphan_wip()` | ✅ |
| implementing + PR merged/closed | → done | `recover_orphan_implementing()` | ✅ |
| implementing + PR open | skip | `recover_orphan_implementing()` — no-op | ✅ |
| implementing + 마커 없음 | implementing 제거 | `recover_orphan_implementing()` | ✅ |

### 9. End-to-End Flow (v2) — ✅ Daemon Loop 완전 일치

`GitHubTaskSource.poll()` (sources/github.rs:340-345):
```rust
async fn poll(&mut self) -> Vec<Box<dyn Task>> {
    self.sync_repos().await;       // repo 동기화
    self.run_recovery().await;     // 1. RECOVERY
    self.run_scans().await;        // 2. SCAN
    self.drain_queue_items()       // 3. CONSUME (Task 생성)
}
```

설계의 `RECOVERY → SCAN → CONSUME → sleep → loop` 구조와 정확히 일치. ✅

### 10. Status Transitions (v2) — ✅ 모든 전이 경로 구현

| Type | 설계 Phase Flow | 구현 확인 | 상태 |
|------|----------------|----------|------|
| Issue (분석) | trigger → Pending → Analyzing → exit | scan_issues → AnalyzeTask | ✅ |
| Issue (구현) | scan_approved → Ready → Implementing → exit | scan_approved → ImplementTask | ✅ |
| Issue (PR approved) | PR pipeline triggers | ReviewTask approve → source_issue done | ✅ |
| Issue (clarify/wontfix) | Analyzing → skip | AnalyzeTask → SKIP label | ✅ |
| Issue (reject) | analyzed → 사람 재트리거 | 라벨 제거 → analyze 재추가 (외부) | ✅ |
| PR (리뷰) | Pending → Reviewing → done | ReviewTask approve | ✅ |
| PR (피드백) | Reviewing → ReviewDone → Improving → Improved → re-review | ImproveTask + re-review | ✅ |
| Merge | Pending → Merging → done | MergeTask | ✅ |

---

## 이전 갭 분석(2026-02-25) Open Gap 해결 현황

| Gap | 심각도 | 내용 | 현재 상태 |
|-----|--------|------|----------|
| **A** | MEDIUM | `plugins/*/commands/*.md` skill 파일 미수집 | ✅ **해결** — `collect_existing_knowledge()`에서 `plugins/*/commands/*.md` 패턴 수집 |
| **B** | MEDIUM | `aggregate_daily_suggestions()` 미구현 | ✅ **해결** — `daily.rs:407`에 구현, daemon loop에서 호출 |
| **C** | LOW | Knowledge PR worktree 격리 미적용 | ✅ **해결** — `create_knowledge_prs()`에서 별도 worktree 생성/제거 |

---

## Quality Gate

- `cargo test`: **323 tests, 0 failures** ✅
- 모든 Task에 블랙박스 테스트 존재 (Mock 기반)
- `GitHubTaskSource` drain/apply 로직에 대한 단위 테스트 포함

---

## 결론

DESIGN-v2.md의 10개 섹션 모두 현재 구현과 기능상 Gap 없이 일치한다.
이전 갭 분석에서 식별된 3개 Open Gap(A/B/C)도 모두 해결되었다.
설계서 대비 구현 완성도: **100%**
