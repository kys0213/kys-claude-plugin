# DESIGN v2.1 Gap Analysis

> **Date**: 2026-03-01
> **Base**: DESIGN-v2.md v2.1 revision (2026-03-01)
> **Scope**: v2.1 디자인 변경(Merge 파이프라인 제거, PR 라벨 세분화, Label-Positive 전면 적용)이 구현에 반영되었는지 조사

---

## 요약

| Severity | 개수 | 주요 테마 |
|----------|------|----------|
| **Critical** | 8 | PR 라벨 생명주기 누락, PR scan 모델 불일치, 지식추출 타이밍/중복방지 |
| **Medium** | 5 | Merge 파이프라인 잔존, 복구 로직 불완전 |
| **Low** | 2 | 큐 phase 경로, dead code |

### 핵심 테마 3가지

1. **PR 라벨 생명주기 누락** (GAP 1, 3, 7, 8, 16): `autodev:changes-requested` 라벨이 코드에 전혀 없어서, PR review-improve 사이클의 GitHub 상태가 보이지 않음
2. **PR scan 모델 불일치** (GAP 5, 9): PR 스캔이 cursor 기반 자동수집이며, 디자인의 Label-Positive (HITL 트리거) 원칙에 위배
3. **지식추출 타이밍/중복방지** (GAP 2, 4, 12, 15): approve 시점에 추출 실행(merge 후가 아님), `scan_done_merged` 미구현, `autodev:extracted` 라벨 없음

---

## Critical Gaps

### GAP 1: `autodev:changes-requested` 라벨 미구현

| 항목 | 내용 |
|------|------|
| **카테고리** | Label scheme |
| **디자인** | Section 2: `autodev:changes-requested`는 PR 전용 라벨, daemon(ReviewTask)이 설정 |
| **구현** | `labels.rs`에 상수 없음. ReviewTask request_changes 시 라벨 추가 로직 없음 |
| **파일** | `cli/src/domain/labels.rs` (상수 누락), `cli/src/tasks/review.rs:326-423` |

**영향**: GitHub UI에서 "리뷰 피드백 반영중" 상태를 구분할 수 없음. startup_reconcile에서 해당 PR 복구 불가 (GAP 3).

---

### GAP 2: `autodev:extracted` 라벨 미구현

| 항목 | 내용 |
|------|------|
| **카테고리** | Label scheme / Knowledge extraction |
| **디자인** | Section 5: `scan_done_merged`는 `done + merged + NOT extracted`로 필터. Section 8: 추출 완료 시 `autodev:extracted` 추가 |
| **구현** | `labels.rs`에 상수 없음. ExtractTask 완료 시 라벨 추가 없음 |
| **파일** | `cli/src/domain/labels.rs` (상수 누락), `cli/src/tasks/extract.rs:235` (QueueOp::Remove만) |

**영향**: daemon 재시작 시 이미 추출된 PR이 중복 처리될 수 있음.

---

### GAP 3: startup_reconcile에서 `autodev:changes-requested` PR 미처리

| 항목 | 내용 |
|------|------|
| **카테고리** | Reconciliation |
| **디자인** | Section 9: `autodev:changes-requested (PR) → ReviewDone 큐 적재` |
| **구현** | `startup_reconcile()`은 `wip` PR만 복구 (`.filter(\|p\| p.is_wip())`). `changes-requested` 처리 없음 |
| **파일** | `cli/src/domain/git_repository.rs:659-687` |

**영향**: daemon 재시작 시 피드백 반영 대기중인 PR이 유실됨.

---

### GAP 4: `scan_done_merged()` 함수 미구현

| 항목 | 내용 |
|------|------|
| **카테고리** | Scan structure / Knowledge extraction |
| **디자인** | Section 5: `pulls::scan_done_merged()` — `done + merged + NOT extracted → Extracting` |
| **구현** | 해당 함수 자체가 없음. 지식추출은 ReviewTask approve 시 인라인으로만 트리거 |
| **파일** | `cli/src/domain/git_repository.rs` (함수 없음) |

**영향**: merge 후 지식추출 트리거 경로가 없음. approve만으로 추출이 실행됨 (GAP 15와 연결).

---

### GAP 5: PR scan이 Label-Positive가 아닌 cursor 기반 자동수집

| 항목 | 내용 |
|------|------|
| **카테고리** | Scan structure |
| **디자인** | Section 5: `pulls::scan()` — `labels=autodev:wip, state=open → Pending` (Label-Positive) |
| **구현** | `scan_pulls()`는 cursor 기반 증분 스캔. `since` 파라미터로 모든 업데이트 PR 수집, autodev 라벨 없는 PR에 자동으로 `wip` 추가 |
| **파일** | `cli/src/domain/git_repository.rs:318-403` (특히 326-331 cursor params, 357 `has_autodev_label` skip) |

**영향**: 디자인의 핵심 원칙 "autodev 라벨이 없으면 → 무시 (안전)"에 위배. 모든 외부 PR이 자동으로 리뷰 큐에 진입.

```
디자인: autodev:wip 라벨 있는 PR만 → scan 대상
구현:   모든 업데이트 PR → autodev 라벨 없으면 자동으로 wip 추가 → scan 대상
```

---

### GAP 7: ReviewTask request_changes 시 `autodev:changes-requested` 라벨 미추가

| 항목 | 내용 |
|------|------|
| **카테고리** | State transition |
| **디자인** | Section 3: `request_changes → autodev:changes-requested`. Section 11: `wip → changes-requested` |
| **구현** | ReviewTask request_changes 분기에서 `REVIEW_DONE` push만 하고, 라벨 변경 없음. `wip`이 그대로 유지 |
| **파일** | `cli/src/tasks/review.rs:370-423` |

**영향**: PR의 GitHub 라벨이 실제 워크플로우 상태를 반영하지 못함. "리뷰 대기"와 "피드백 반영중" 구분 불가.

---

### GAP 9: PR scan이 외부 PR에 자동으로 `autodev:wip` 추가

| 항목 | 내용 |
|------|------|
| **카테고리** | Scan structure / State transition |
| **디자인** | Section 2: "외부에서 생성된 PR은 사람이 수동으로 `autodev:wip`를 추가해야 리뷰 대상이 됨" |
| **구현** | `scan_pulls()`가 autodev 라벨 없는 모든 PR에 `wip` 자동 추가 (line 388-389) |
| **파일** | `cli/src/domain/git_repository.rs:388-389` |

**영향**: GAP 5와 연결. 외부 PR의 HITL 안전장치가 무력화.

---

### GAP 12/15: 지식추출이 merge 후가 아닌 approve 후에 실행

| 항목 | 내용 |
|------|------|
| **카테고리** | Knowledge extraction |
| **디자인** | Section 8: merge 후 추출. "merge된 코드만이 실제로 레포에 반영된 확정 지식" |
| **구현** | ReviewTask approve → 즉시 `PushPr(EXTRACTING)`. merge 여부와 무관하게 실행 |
| **파일** | `cli/src/tasks/review.rs:319-324` |

**영향**: approve 후 merge되지 않는 PR에서도 지식추출이 실행됨. 또한 `autodev:extracted` 라벨 없이 (GAP 2) 추출 완료 표시 불가.

---

## Medium Gaps

### GAP 6: Merge 파이프라인이 아직 존재 (디자인 scope 외)

| 항목 | 내용 |
|------|------|
| **카테고리** | Scope |
| **디자인** | Section 12: "PR Merge: scope 외" |
| **구현** | `tasks/merge.rs`, `MergeItem`, `merge_queue`, `merge_phase`, `scan_merges()` 모두 존재/활성 |
| **파일** | 다수 (tasks/merge.rs, task_queues.rs:55-109, git_repository.rs:408-481, github.rs:287-297) |

---

### GAP 8: ImproveTask에서 `changes-requested → wip` 라벨 전이 누락

| 항목 | 내용 |
|------|------|
| **카테고리** | State transition |
| **디자인** | Section 3: ImproveTask 성공 → `autodev:wip` + Pending 재진입 |
| **구현** | ImproveTask는 iteration 라벨만 관리. `changes-requested` 제거 / `wip` 추가 로직 없음 (GAP 1로 인해 원래 `wip`이 유지중이라 기능적으로는 동작) |
| **파일** | `cli/src/tasks/improve.rs:148-175` |

---

### GAP 10: orphan wip PR 복구가 재큐잉 대신 라벨 제거

| 항목 | 내용 |
|------|------|
| **카테고리** | Reconciliation |
| **디자인** | Section 10: `PR: autodev:wip + queue에 없음 → Pending 적재` |
| **구현** | `recover_orphan_wip()`는 wip 라벨을 **제거**함. "다음 scan에서 재발견" 의도이지만 Label-Positive에서는 라벨 없으면 재발견 불가 |
| **파일** | `cli/src/domain/git_repository.rs:509-523` |

---

### GAP 11: `scan_merges()`가 존재 (디자인 scope 외)

| 항목 | 내용 |
|------|------|
| **카테고리** | Scan structure |
| **디자인** | Section 5: scan 구조에 merge scan 없음 |
| **구현** | `scan_merges()`가 `autodev:done` open PR을 스캔하여 merge 큐에 적재 |
| **파일** | `cli/src/domain/git_repository.rs:408-481` |

---

### GAP 16: `RepoPull`에 `is_changes_requested()` 메서드 누락

| 항목 | 내용 |
|------|------|
| **카테고리** | Domain model |
| **디자인** | `autodev:changes-requested` 라벨 기반 필터링 필요 |
| **구현** | `is_wip()`, `is_done()`, `is_terminal()` 메서드만 존재 |
| **파일** | `cli/src/domain/models.rs:128-139` |

---

## Low Gaps

### GAP 13: Improved phase가 Pending을 건너뛰고 바로 Reviewing으로 전이

| 항목 | 내용 |
|------|------|
| **카테고리** | Queue phase |
| **디자인** | Section 6: `Improved → autodev:wip + Pending으로 재진입` |
| **구현** | `drain_queue_items()`에서 `IMPROVED → REVIEWING` 직접 전이 + ReviewTask 생성 |
| **파일** | `cli/src/sources/github.rs:256-268` |

**참고**: 기능적으로는 동일 (Pending → Reviewing이 같은 drain 사이클에서 발생). 하지만 dedup/scan 재검증 로직이 있다면 건너뛸 수 있음.

---

### GAP 14: `merge_phase::CONFLICT` 상수가 dead code

| 항목 | 내용 |
|------|------|
| **카테고리** | Scope |
| **디자인** | Merge pipeline 없음 |
| **구현** | `merge_phase::CONFLICT = "Conflict"` 정의되어 있으나 어디서도 사용되지 않음 |
| **파일** | `cli/src/queue/task_queues.rs:108` |

---

## 구현 우선순위 제안

### Phase A: PR Label-Positive 전환 (Critical, GAP 5 + 9)

가장 근본적인 아키텍처 불일치. 나머지 gap의 전제조건.

```
scan_pulls() 리팩토링:
  현재: cursor 기반 + autodev 라벨 없는 PR 자동수집
  목표: labels=autodev:wip 파라미터로 wip 라벨 PR만 수집
```

### Phase B: PR 라벨 생명주기 구현 (Critical, GAP 1 + 7 + 8 + 3 + 16)

```
1. labels.rs에 CHANGES_REQUESTED 상수 추가
2. ReviewTask request_changes → wip 제거 + changes-requested 추가
3. ImproveTask 성공 → changes-requested 제거 + wip 추가
4. RepoPull에 is_changes_requested() 추가
5. startup_reconcile에 changes-requested PR → ReviewDone 복구 추가
```

### Phase C: 지식추출 타이밍 수정 (Critical, GAP 2 + 4 + 12 + 15)

```
1. labels.rs에 EXTRACTED 상수 추가
2. scan_done_merged() 구현 (done + merged + NOT extracted)
3. ReviewTask approve에서 EXTRACTING push 제거
4. ExtractTask 완료 시 autodev:extracted 라벨 추가
5. scan_targets에 "done_merged" 추가
```

### Phase D: Merge 파이프라인 제거 (Medium, GAP 6 + 11 + 14)

```
1. tasks/merge.rs 삭제
2. MergeItem, merge_queue, merge_phase 제거
3. scan_merges() 제거
4. drain_queue_items()에서 merge 관련 코드 제거
5. auto_merge config 제거 또는 deprecated 표시
```

### Phase E: Recovery 일관성 (Medium, GAP 10)

```
recover_orphan_wip() PR 분기:
  현재: wip 라벨 제거
  목표: Pending 큐에 재적재 (Label-Positive 모델과 일관)
```
