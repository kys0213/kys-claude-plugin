# DESIGN.md vs 구현 Gap 분석 리포트

> **Date**: 2026-02-22
> **Scope**: `plugins/autodev/DESIGN.md` ↔ `plugins/autodev/cli/src/` 전체
> **목적**: 설계 문서와 실제 구현의 차이를 식별하고, 수정 방향을 제시

---

## Executive Summary

DESIGN.md는 **3-Tier 상태 관리** (GitHub Labels → SQLite → In-Memory StateQueue)를 핵심 아키텍처로 정의하고 있으나, 실제 구현은 **SQLite 중심의 영속 큐**로 대체되었다. 이는 단순한 구현 차이가 아니라 **상태 관리 철학 자체가 다른** 근본적 gap이다.

| 심각도 | 건수 | 설명 |
|--------|------|------|
| **Fundamental** | 2건 | 아키텍처 근간이 다름 (큐 구조, 라벨 SSOT) |
| **Major** | 4건 | 설계된 기능이 미구현 (PR 피드백 루프 등) |
| **Minor** | 4건 | 모듈 구조/네이밍 차이 |

---

## 1. Fundamental Gaps (아키텍처 근간)

### F-01: 작업 큐가 In-Memory가 아닌 SQLite에 영속화됨

**DESIGN (line 19, 132)**:
> "작업 큐 테이블 없음: issue_queue, pr_queue, merge_queue는 In-Memory StateQueue로 대체"

**구현 (`schema.rs:24-75`)**:
`issue_queue`, `pr_queue`, `merge_queue` 3개 테이블이 SQLite에 존재하며, 모든 상태 전이가 DB UPDATE로 처리된다.

**영향**:
- 설계의 `StateQueue<T>`(HashMap + VecDeque) 자체가 구현되지 않음
- `active.rs`의 `ActiveItems`는 scan 중복 방지 용도로만 사용 (설계의 `index: HashMap<WorkId, State>`와 역할이 다름)
- DB 큐 방식은 pre-flight API 호출이 필요해짐 (설계는 "pre-flight 불필요" 명시)

**gap 원인 추정**: 재시작 안전성을 위해 영속 큐를 선택한 것으로 보이나, 설계는 이를 reconciliation으로 해결하도록 의도했음

---

### F-02: GitHub 라벨이 SSOT로 사용되지 않음

**DESIGN (line 18, 33-38, 76-87)**:
> "GitHub 라벨 = SSOT: 작업 완료 상태의 유일한 영속 마커"

설계는 라벨 상태 전이를 명확히 정의:
```
(없음) → autodev:wip → autodev:done
                     → autodev:skip
```

**구현**:
- `label_remove`만 존재 (`gh/mod.rs:45`), **`label_add`가 trait에 없음**
- scan 시 `autodev:wip` 라벨을 **설정하지 않음** (`scanner/issues.rs:108`)
- done/skip 전이 시 `autodev:done`/`autodev:skip` 라벨을 **설정하지 않음**
- recovery에서 `autodev:wip` 제거만 수행 (`daemon/recovery.rs`)

**영향**:
- 재시작 시 GitHub 기반 reconciliation 불가능
- 외부에서 (GitHub UI 등) 작업 상태 확인 불가
- 설계의 startup_reconcile()이 구현 불가 (라벨이 없으므로 필터링 기준 자체가 없음)

---

## 2. Major Gaps (미구현 기능)

### M-01: startup_reconcile() 미구현

**DESIGN (line 404-453)**:
데몬 시작 시 `cursor - 24h` 범위로 GitHub API 조회 → 라벨 기반 필터 → 메모리 큐 복구

**구현 (`daemon/mod.rs:47-59`)**:
`queue_reset_stuck()` + `queue_auto_retry_failed()`로 DB의 stuck/failed 항목만 복구. GitHub 기반 reconciliation 없음.

**gap 원인**: F-01, F-02의 결과. 큐가 DB에 있으므로 reconciliation 대신 DB 복구로 대체함.

---

### M-02: PR 피드백 루프 미구현

**DESIGN (line 631-682)**:
```
Reviewing → approve → done
         → request_changes → ReviewDone → Improving → Improved → Reviewing (반복)
```

**구현 (`pipeline/pr.rs`)**:
```
pending → reviewing → review_done (1회 리뷰로 종료)
```

- `ReviewDone → Improving → Improved → Reviewing` 사이클 자체가 없음
- 리뷰 후 피드백 반영 → 재리뷰 로직 미구현
- `review_done` 이후 다음 action이 없음 (dead end)

---

### M-03: Merge conflict phase 불완전

**DESIGN (line 684-718)**:
```
Merging → conflict → 충돌 해결 시도 → 성공: 재머지 / 실패: 재시도
```

**구현 (`pipeline/merge.rs:114-128`)**:
Conflict 감지 및 해결 시도는 존재하나, 해결 후 **재머지가 아닌 바로 done 처리**. 설계의 "Merging → Conflict → Merging (재시도)" 사이클이 없음.

---

### M-04: Knowledge Extraction 미구현

**DESIGN (line 939-1133)**:
- Per-task: done 전이 시 해당 세션 분석 → 즉시 피드백
- Daily: 매일 전일 로그 전체 분석 → 일일 리포트 + 크로스 태스크 패턴

**구현**: 관련 코드 없음. pipeline에서 done 전이 후 knowledge extraction 호출이 없고, daily batch 트리거도 없음.

---

## 3. Minor Gaps (구조/네이밍)

### m-01: queue/ 모듈 구조 차이

**DESIGN (line 261-267)**:
```
queue/
  ├── models.rs        # WorkId, IssueItem, ...
  ├── schema.rs        # SQLite DDL
  ├── state_queue.rs   # In-Memory StateQueue<T>
  ├── repo_store.rs    # RepositoryStore
  ├── cursor_store.rs  # CursorStore
  └── log_store.rs     # LogStore
```

**구현**:
```
queue/
  ├── models.rs        # 데이터 모델 ✅
  ├── schema.rs        # SQLite DDL ✅
  ├── repository.rs    # 모든 DB 연산 통합 (repo_store + cursor_store + log_store + queue CRUD)
  └── mod.rs           # Database wrapper
```

- `state_queue.rs` 없음 (F-01의 결과)
- store가 분리되지 않고 `repository.rs`에 통합 (3000+ lines 추정)

---

### m-02: components/analyzer.rs 미분리

**DESIGN (line 239)**:
```
components/
  ├── analyzer.rs      # Analyzer { claude: &dyn Claude }
```

**구현**: `analyzer.rs` 없음. 분석 로직이 `pipeline/issue.rs`의 `process_pending()` 안에 인라인으로 존재.

---

### m-03: consumer → pipeline 네이밍 변경

**DESIGN (line 245-249)**: `pipeline/` 디렉토리
**구현**: `pipeline/` 디렉토리 ✅ (일치)

다만 DESIGN은 daemon 루프를 `consume()`로 호출하지만, 구현은 `process_all()`을 사용. 기능상 동일.

---

### m-04: Gh trait 메서드 차이

**DESIGN (line 225)**: `label_add, label_remove, list_issues, ...`

**구현 (`gh/mod.rs`)**:
- `label_add` **없음** (F-02의 원인)
- `label_remove` ✅
- `list_issues` 없음 → `api_paginate`로 대체
- `api_get_field` 추가 (pre-flight 용)
- `issue_comment` 추가

---

## 4. 구현이 설계보다 나은 부분

| 항목 | 설계 | 구현 | 평가 |
|------|------|------|------|
| Pre-flight check | scan에서 open 확인했으므로 불필요 | pipeline에서 매번 GitHub API로 재확인 | **구현이 안전** — scan 후 시간 경과로 상태 변경 가능 |
| Retry / stuck recovery | 설계에 없음 | `stuck_threshold_secs`, `auto_retry_failed`, `retry_count` | **구현이 실용적** |
| Concurrency control | 설계에 없음 | `issue_concurrency`, `pr_concurrency`, `merge_concurrency` | **구현이 실용적** |
| Merge conflict detection | 설계는 phase만 정의 | `MergeOutcome` enum + `resolve_conflicts()` | **구현이 구체적** |

---

## 5. Gap 해소 방향

### 옵션 A: 설계대로 리팩토링 (In-Memory 큐 + 라벨 SSOT)

```
변경 범위: queue/, scanner/, pipeline/, daemon/, infrastructure/gh
예상 작업량: 대규모 (schema 변경, 모든 pipeline 수정)
```

장점:
- API 비용 절감 (pre-flight 제거)
- GitHub UI에서 상태 확인 가능
- 재시작 시 깔끔한 복구 (reconciliation)

단점:
- 현재 동작하는 코드를 대규모 변경
- retry_count, stuck recovery 등 실용적 기능을 새 구조에 재구현 필요

### 옵션 B: 현재 구현 기준으로 DESIGN.md 갱신

```
변경 범위: DESIGN.md만
예상 작업량: 소규모 (문서 수정)
```

장점:
- 코드 변경 없이 설계-구현 동기화
- 현재 코드의 실용적 개선사항 반영

단점:
- 라벨 SSOT의 장점 포기
- pre-flight API 비용 계속 발생

### 옵션 C: 하이브리드 (라벨만 추가, 큐는 유지)

```
변경 범위: infrastructure/gh (label_add), scanner/, pipeline/
예상 작업량: 중규모
```

현재 SQLite 큐를 유지하면서 라벨 관리만 추가:
1. `Gh` trait에 `label_add` 추가
2. scan 시 `autodev:wip` 라벨 설정
3. done/skip 시 라벨 전이
4. startup_reconcile()은 DB + 라벨 교차 검증

장점:
- 코드 변경 최소화
- 외부 가시성 확보 (GitHub UI)
- 재시작 안전성 강화

---

## 6. 요약: Gap 전체 목록

| ID | 심각도 | 카테고리 | 설계 | 구현 | 방향 |
|----|--------|---------|------|------|------|
| F-01 | Fundamental | Queue 구조 | In-Memory StateQueue | SQLite 테이블 | A or B |
| F-02 | Fundamental | 상태 SSOT | GitHub Labels | DB status 컬럼 | A or C |
| M-01 | Major | Startup | reconcile(cursor-24h) | stuck/failed DB 복구 | F-01에 종속 |
| M-02 | Major | PR Flow | 리뷰→개선→재리뷰 사이클 | 1회 리뷰로 종료 | 별도 구현 필요 |
| M-03 | Major | Merge Flow | Conflict→재머지 사이클 | 해결 후 바로 done | 구현 보완 |
| M-04 | Major | Knowledge | per-task + daily 추출 | 미구현 | Phase 후순위 |
| m-01 | Minor | Module | store 분리 | repository.rs 통합 | 리팩토링 |
| m-02 | Minor | Component | analyzer.rs 분리 | pipeline에 인라인 | 리팩토링 |
| m-03 | Minor | Naming | consume() | process_all() | 사소 |
| m-04 | Minor | Gh Trait | label_add 포함 | label_add 없음 | F-02에 종속 |
