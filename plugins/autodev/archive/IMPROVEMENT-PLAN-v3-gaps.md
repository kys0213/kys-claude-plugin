# DESIGN-v3 Gap 개선 계획

**Date**: 2026-03-11
**Base**: DESIGN-CODE-GAP-v3.md (6건의 Gap)

---

## 요구사항 정리

Gap 분석에서 발견된 6건을 심각도 순으로 수정한다.

| 순서 | Gap | 심각도 | 작업 유형 |
|------|-----|--------|-----------|
| 1 | Label add-first 원칙 미준수 | High | 코드 수정 |
| 2 | ExtractTask agent 실패 시 extract-failed 미부착 | Medium | 코드 수정 |
| 3 | ImplementTask preflight 검사 누락 | Medium | 코드 수정 |
| 4 | Improved → Pending 중간 단계 추가 | Low | 코드 수정 |
| 5 | 테스트 코멘트 오류 | Negligible | 코드 수정 |
| 6 | CONFIG-SCHEMA-v2 agent 필드 문서 | Low | 문서 수정 |

---

## 사이드이펙트 조사 결과

### Gap #1: Label add-first 순서 변경

- `git_repository.rs`의 `scan_approved_issues_adds_to_ready_queue` 테스트 — assertion이 `any()` 매처로 순서 무관 검증이므로 **테스트 변경 불필요**
- **추가 발견**: `recover_orphan_implementing` (git_repository.rs:584-592)에서도 remove-first 패턴 사용 중
  - `label_remove(IMPLEMENTING)` → `label_add(DONE)` — add-first로 수정 필요
  - 같은 함수의 PR open 분기 (line 610-616)와 None 분기 (line 634)는 단방향 제거(`label_remove`만)이므로 변경 불필요
- **추가 확인**: `recover_orphan_wip` — label 전이 없음 (wip 제거만 수행하거나 큐에 재삽입). 변경 불필요
- **추가 확인**: `startup_reconcile` — label 전이 없음 (기존 label 기반으로 큐에 push만 수행). 변경 불필요
- **대응**: Step 1 수정 목록에 `recover_orphan_implementing` 추가

### Gap #2: ExtractTask extract-failed

- `scan_done_merged()`가 `extracted` **와** `extract-failed` 모두 필터링하므로, `extract-failed` 부착 시 무한 재스캔 없음
- **대응**: 안전하게 분기 추가 가능

### Gap #3: ImplementTask preflight

- `scan_approved_issues`에서 이미 `implementing` 라벨을 부착한 상태
- preflight 실패 시 `implementing` 라벨 제거 + `done` 라벨 추가 필요 (AnalyzeTask 패턴 동일)
- **대응**: AnalyzeTask의 preflight 패턴을 복제

### Gap #4: Improved → Pending

- Pending에 넣어도 재스캔이나 중복은 발생하지 않음 (work_id dedup)
- **concurrency 계산 영향**: 현재 in-flight 계산이 `Reviewing + Improving`만 카운트하므로, Pending에 있는 아이템은 in-flight에 미포함. 이는 설계 의도에 부합 (Pending = 대기, in-flight 아님)
- **대응**: drain 순서를 `Pending → Reviewing` 하나로 통합. Improved → Pending으로 push하면 다음 tick에서 Pending → Reviewing으로 drain됨

---

## 구현 설계

### Step 1: Label add-first 유틸리티 도입 (Gap #1)

모든 label 전이 지점에서 순서를 역전하는 대신, 패턴의 일관성을 보장하기 위해 각 전이 지점을 개별 수정한다.

**변경 원칙**: `label_add(NEW)` → `label_remove(OLD)` 순서로 통일

**수정 파일 및 위치**:

| 파일 | 전이 | before | after |
|------|------|--------|-------|
| `tasks/analyze.rs` handle_analysis (wontfix) | wip→skip | remove→add | add→remove |
| `tasks/analyze.rs` handle_analysis (clarify) | wip→skip | remove→add | add→remove |
| `tasks/analyze.rs` handle_analysis (implement) | wip→analyzed | remove→add | add→remove |
| `tasks/analyze.rs` handle_fallback | wip→analyzed | remove→add | add→remove |
| `tasks/analyze.rs` before_invoke (closed) | wip→done | remove→add | add→remove |
| `tasks/analyze.rs` after_invoke (exit≠0) | remove wip only | — | 변경 없음 (단방향 제거) |
| `tasks/implement.rs` after_invoke (no PR) | implementing→impl-failed | remove→add | add→remove |
| `tasks/review.rs` before_invoke (closed) | wip→done + source issue | remove→add | add→remove |
| `tasks/review.rs` after_invoke approve | wip→done + source issue | remove→add | add→remove |
| `tasks/review.rs` after_invoke request_changes | wip→changes-requested | remove→add | add→remove |
| `tasks/review.rs` after_invoke max_iterations | wip+changes→skip | 복합 | add(skip)→remove(wip)→remove(changes) |
| `tasks/improve.rs` after_invoke (success) | changes-requested→wip | remove→add | add→remove |
| `domain/git_repository.rs` scan_approved_issues | approved→implementing | remove→add | add→remove |
| `domain/git_repository.rs` recover_orphan_implementing (closed/merged) | implementing→done | remove→add | add→remove |

**테스트 수정**: `git_repository.rs`의 `scan_approved_issues_adds_to_ready_queue` 테스트에서 label 순서 검증은 assertion이 순서 무관(`any()` 매처)이므로 **테스트 변경 불필요**.

**변경 불필요 확인**:
- `recover_orphan_implementing` PR open 분기 (line 610-616): 단방향 제거(`label_remove(IMPLEMENTING)`)만 수행
- `recover_orphan_implementing` None 분기 (line 634): 단방향 제거(`label_remove(IMPLEMENTING)`)만 수행
- `recover_orphan_wip`: label 전이 없음 (wip 제거 또는 큐 재삽입만)
- `startup_reconcile`: label 변경 없음 (기존 label 기반 큐 push만)

### Step 2: ExtractTask extract-failed 분기 (Gap #2)

**파일**: `tasks/extract.rs`

**변경**: `after_invoke` 마지막 부분에서 exit_code에 따라 라벨 분기

```rust
// before (현재)
self.gh.label_add(..., labels::EXTRACTED, ...).await;

// after (수정)
let final_label = if response.exit_code == 0 {
    labels::EXTRACTED
} else {
    labels::EXTRACT_FAILED
};
self.gh.label_add(..., final_label, ...).await;
```

**테스트 추가**: `after_invoke_agent_failure_adds_extract_failed_label` — exit_code=1일 때 `extract-failed` 라벨 확인

### Step 3: ImplementTask preflight 추가 (Gap #3)

**파일**: `tasks/implement.rs`

**변경**: `before_invoke` 시작 부분에 issue open 상태 확인 추가

```rust
// AnalyzeTask 패턴과 동일
let state = self.gh.api_get_field(..., "issues/{n}", ".state", ...).await;
if let Some(ref s) = state {
    if s != "open" {
        // add-first: DONE 먼저, IMPLEMENTING 제거
        self.gh.label_add(..., labels::DONE, ...).await;
        self.gh.label_remove(..., labels::IMPLEMENTING, ...).await;
        return Err(SkipReason::PreflightFailed(...));
    }
}
```

**테스트 추가**: `before_skips_closed_issue` — closed issue에서 skip 확인 + DONE 라벨 + IMPLEMENTING 제거

### Step 4: Improved → Pending 중간 단계 (Gap #4)

**파일**: `sources/github.rs`

**변경**: `drain_queue_items`에서 Improved → Reviewing 직접 전이를 제거하고, Improved → Pending으로 변경

```rust
// before (현재)
// PR: Improved → Reviewing (re-review)
let drained = repo.pr_queue.drain_to(pr_phase::IMPROVED, pr_phase::REVIEWING, pr_slots);

// after (수정)
// PR: Improved → Pending (설계 준수: re-review는 Pending에서 다시 시작)
let promoted = repo.pr_queue.drain_to(pr_phase::IMPROVED, pr_phase::PENDING, usize::MAX);
// Pending → Reviewing는 이미 위에서 처리됨
```

**주의**: Improved → Pending 전이는 concurrency 슬롯을 소비하지 않으므로 `usize::MAX`로 모두 이동. 실제 Reviewing 전이는 기존 `Pending → Reviewing` drain에서 처리됨.

**순서 변경 필요**: `drain_queue_items` 내에서 Improved → Pending이 Pending → Reviewing **이전에** 실행되어야 함. 그래야 같은 tick에서 Improved 아이템이 Pending → Reviewing까지 한번에 전이 가능.

```rust
// 수정된 drain 순서:
// 1. Improved → Pending (무제한, 큐 이동만)
// 2. Pending → Reviewing (concurrency 제한)
// 3. ReviewDone → Improving (concurrency 제한)
// 4. Extracting → pop (concurrency 제한)
```

**테스트 수정**: `drain_creates_review_task_from_improved_pr` (기존) → Improved가 Pending을 거쳐 Reviewing으로 전이되는지 확인

### Step 5: 테스트 코멘트 수정 (Gap #5)

**파일**: `tasks/review.rs:672`

```rust
// before
pr.review_iteration = 3; // default max is 3

// after
pr.review_iteration = 3; // exceeds default max (2)
```

### Step 6: CONFIG-SCHEMA-v2 문서 수정 (Gap #6)

**파일**: `CONFIG-SCHEMA-v2.md`

**변경**: `agent` 필드 관련 내용 제거 또는 deprecated 명시

```yaml
# before
workflows:
  analyze:
    agent: autodev:issue-analyzer

# after
workflows:
  analyze:
    command: /custom-analyze    # 커스텀 슬래시 커맨드 (선택)
```

---

## 구현 순서

```
Step 1: Label add-first (5개 파일, ~15개 전이 지점 + recovery 1개)
  └─ 기존 테스트 통과 확인 (테스트는 순서 무관 검증이므로 변경 불필요)

Step 2: ExtractTask extract-failed (1개 파일)
  └─ 테스트 1건 추가

Step 3: ImplementTask preflight (1개 파일)
  └─ 테스트 1건 추가

Step 4: Improved → Pending (1개 파일)
  └─ 기존 테스트 수정 + drain 순서 검증

Step 5: 테스트 코멘트 수정 (1개 파일)

Step 6: CONFIG-SCHEMA 문서 수정 (1개 파일)

최종: cargo fmt + cargo clippy + cargo test
```

---

## 리스크

| 리스크 | 확률 | 완화 |
|--------|------|------|
| Label add-first 변경으로 기존 테스트 실패 | 낮음 | 테스트가 순서 무관하게 검증하므로 영향 없음 |
| Improved → Pending에서 1 tick 지연 | 낮음 | drain 순서 조정으로 같은 tick 내 전이 가능 |
| ImplementTask preflight에서 네트워크 실패 시 오판 | 낮음 | AnalyzeTask와 동일한 패턴 (state가 None이면 통과) |
