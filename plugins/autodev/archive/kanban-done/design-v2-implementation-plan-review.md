# DESIGN-v2 + 구현계획 종합 검토 리포트

> **Date**: 2026-02-24
> **Reviewer**: claude
> **Scope**: `DESIGN-v2.md`, `IMPLEMENTATION-PLAN-v2.md`, `IMPROVEMENT-PLAN-v2-gaps.md`
> **참조**: `kanban/done/design-v2-review.md` (9개 gap 식별 결과)

---

## 1. 검토 요약

| 문서 | 평가 | 핵심 판단 |
|------|------|----------|
| DESIGN-v2.md | **양호** | 아키텍처 건전, 크래시 안전 고려 우수. 2건 설계 보완 필요 |
| IMPLEMENTATION-PLAN-v2.md | **양호** | 5-Phase 분해 적절, 의존성 분석 정확. 번호 오류 1건 |
| IMPROVEMENT-PLAN-v2-gaps.md | **양호** | 9개 gap을 체계적으로 해소. Phase 1이 핵심 — 즉시 실행 권장 |

**현재 코드 상태**: Phase A~C 구현 완료, Phase D 부분 구현 (핵심 gap 1건), Phase E 부분 구현 (3건 미구현)

---

## 2. DESIGN-v2.md 검토

### 2.1 강점

**상태 전이 설계가 견고하다.**

- HITL 게이트(분석 → 사람 리뷰 → 구현)는 v1의 "분석 품질이 낮아도 구현 진입" 문제를 정확히 해결한다.
- 라벨 전이 순서의 크래시 안전 설계가 돋보인다:
  ```
  scan_approved(): implementing 먼저 추가 → approved-analysis 제거
  → 크래시 시 "양쪽 다 있는" 상태 (안전) vs "둘 다 없는" 상태 (위험)
  ```
  이는 분산 시스템의 "at-least-once" 원칙을 잘 적용한 것이다.

**Safety Valve (재분석 무한 루프 방지)가 실용적이다.**

- 분석 코멘트 수 >= MAX_ANALYSIS_ATTEMPTS(3) → `autodev:skip` 전이
- 사람이 skip 해제 시 재시도 가능 → 완전 차단이 아닌 안전 밸브

**Issue-PR 연동 설계가 명확하다.**

- `PrItem.source_issue_number`로 단방향 링크
- PR approve 시점에 Issue done 전이 → 단일 책임 (PR pipeline만 done 전이 담당)
- `<!-- autodev:pr-link:{N} -->` 코멘트 마커로 recovery 시 PR 번호 추적 가능

**Worktree lifecycle invariant**가 명확하다.

- "모든 pipeline 함수는 생성한 worktree를 자신이 제거" — 리소스 누수 방지
- Knowledge PR은 별도 worktree 격리 → 구현 worktree 오염 방지

### 2.2 설계 이슈 (2건)

#### Issue 1: `scan_approved()` 라벨 제거 순서가 크래시 안전 원칙과 불일치 (Medium)

**설계 (Section 4)**:
```
implementing을 먼저 추가한 후 approved-analysis를 제거한다.
```

**현재 구현** (`scanner/issues.rs:193-198`):
```rust
gh.label_remove(repo_name, issue.number, labels::APPROVED_ANALYSIS, ...).await;
gh.label_remove(repo_name, issue.number, labels::ANALYZED, ...).await;
gh.label_add(repo_name, issue.number, labels::IMPLEMENTING, ...).await;
```

구현이 설계와 **반대 순서**로 되어 있다. approved-analysis를 먼저 제거한 후 implementing을 추가하므로, 두 API 호출 사이에 크래시 발생 시 "라벨 없음" 상태가 된다. 이 경우 다음 scan에서 재분석부터 시작되어 이미 승인된 분석이 낭비된다.

**권장**: 설계대로 `label_add(IMPLEMENTING)` → `label_remove(APPROVED_ANALYSIS)` → `label_remove(ANALYZED)` 순서로 수정.

#### Issue 2: `process_ready()` PR 번호 추출 실패 시 복구 전략 비효율 (Low)

**설계 (Section 5)**:
```
PR 번호 추출 실패 → implementing 라벨 제거 → queue 제거 (다음 scan에서 재시도)
```

implementing 라벨을 제거하면 이슈가 "라벨 없음" 상태가 되어 다음 scan에서 **재분석부터** 시작된다. 이미 분석이 완료되고 사람이 승인한 이슈를 다시 분석하는 것은 비효율적이다.

**대안 검토**: `approved-analysis` 라벨로 롤백하여 구현만 재시도하는 방안. 단, 이 경우 무한 구현 루프 가능성이 있으므로 구현 시도 횟수 제한 (분석 Safety Valve와 유사)이 필요하다. 현재 설계의 단순함이 가치 있으므로 v2 범위에서는 현재 전략을 유지하되, 향후 개선 사항으로 기록해두는 것이 적절하다.

### 2.3 설계 보완 제안 (3건)

#### Suggestion 1: Safety Valve 미구현 확인

설계 Section 4에 Safety Valve(`count_analysis_comments()`)가 명시되어 있으나, 현재 `scanner/issues.rs`의 `scan()` 함수에는 이 로직이 구현되어 있지 않다. scan 시 분석 코멘트 수를 확인하지 않으므로, reject → 재분석 무한 루프가 가능하다.

**영향**: reject된 이슈가 계속 재분석되어 API 리소스 소모
**권장**: IMPLEMENTATION-PLAN Phase B-3에 포함되어 있으므로 구현 시 누락하지 않도록 주의

#### Suggestion 2: `find_existing_pr()` GitHub API `head` 파라미터 형식

설계 Section 5의 `find_existing_pr()`에서 `("head", head_branch)` 로 PR을 조회하나, GitHub REST API의 `head` 파라미터는 `owner:branch` 형식이 필요할 수 있다. 실제 API 동작을 확인하고, 필요시 `repo_name`에서 owner를 추출하여 `{owner}:{branch}` 형식으로 전달해야 한다.

#### Suggestion 3: Knowledge PR `autodev:skip` 라벨의 의도치 않은 제거

Knowledge PR에 `autodev:skip` 라벨을 붙여 자기 자신이 리뷰하지 않도록 하는 설계는 적절하다. 그러나 사람이 실수로 skip 라벨을 제거하면 daemon이 자신이 생성한 PR을 리뷰하는 상황이 발생할 수 있다. PR body에 autodev 생성 마커를 남기고, scanner에서 이를 감지하여 skip하는 방어 로직을 고려할 수 있다.

---

## 3. IMPLEMENTATION-PLAN-v2.md 검토

### 3.1 강점

**Phase 분해와 의존성 분석이 정확하다.**

```
Phase A (기반) → Phase B (분석 리뷰 게이트)
              → Phase C (Approved Scan + 구현) → Phase D (Issue-PR 연동)
                                                         → Phase E (Knowledge v2)
```

- A가 모든 Phase의 기반 (라벨 상수 + PrItem 필드)
- B와 C의 순차 진행 권장 (같은 파일 `pipeline/issue.rs` 수정) → 올바른 판단
- 25개 항목의 세부 체크리스트로 추적 가능

**사이드이펙트 분석이 충실하다.**

- `PrItem.source_issue_number` 추가에 따른 cascading 수정 범위를 정확히 식별
  - `scanner/pulls.rs`, `daemon/mod.rs`, `knowledge/daily.rs`, 테스트 코드
- `IssueItem`은 변경 불필요 (`analysis_report` 필드 이미 존재) → 확인 완료

**위험 요소 사전 식별이 적절하다.**

- `regex` crate 의존성, `Gh` trait 메서드 존재 여부, DB 스키마 확인 등

### 3.2 이슈 (2건)

#### Issue 1: Phase D 번호 중복 (Minor)

D-2가 두 번 사용됨:
- D-2: PR approve → Issue done 전이
- D-2: `startup_reconcile()` 라벨 필터 확장

D-3, D-4로 번호를 재정리해야 한다. 현재 D-3(Recovery)은 실제로 D-4이고, D-5(테스트)는 D-5가 맞다.

#### Issue 2: Phase C의 `process_ready()` 변경에서 이슈 코멘트 미언급

Phase C-5에서 PR 생성 성공 시 `<!-- autodev:pr-link:{N} -->` 이슈 코멘트 게시가 명시되어 있으나, IMPROVEMENT-PLAN Phase 2에서도 이를 Phase 1 전제 조건으로 재언급한다. 두 문서 간 우선순위 혼란 가능성이 있다.

**권장**: IMPLEMENTATION-PLAN의 C-5 항목 설명에 "pr-link 코멘트는 Phase D recovery의 전제 조건"임을 명시적으로 추가

### 3.3 구현 현황 대비 (Phase별)

| Phase | 계획 항목 | 구현 완료 | 미완료 |
|-------|----------|----------|--------|
| A (1-4) | 라벨 상수, PrItem 필드, cascading 수정 | 4/4 | - |
| B (5-8) | format_analysis_comment, process_pending 변경, Safety Valve | 2/4 | Safety Valve, Safety Valve 테스트 |
| C (9-14) | scan_approved, extract_pr_number, process_ready | 5/6 | JSON pr_number fallback |
| D (15-19) | PR worktree 정리, Issue done 전이, reconcile, recovery | 2/5 | **process_ready 라벨 전이 오류**, recovery, PR worktree 정리 |
| E (20-25) | knowledge 확장, actionable PR, daily 패턴 | 2/6 | skills/hooks 수집, empty→None, actionable PR, daily 집계 |

---

## 4. IMPROVEMENT-PLAN-v2-gaps.md 검토

### 4.1 강점

**Gap 우선순위 분류가 정확하다.**

- Phase 1 (`process_ready()` 라벨 전이)을 최우선으로 식별 → 이 gap이 Phase D 전체를 무효화하므로 올바른 판단
- 인프라 현황 사전 조사로 "새 trait 메서드 추가 없이 해결 가능" 확인 → 구현 리스크 최소화

**Phase 간 의존성과 병렬 가능성이 명확하다.**

```
Phase 1 → (Phase 2, 3, 4 병렬) → Phase 5
```

**코드 수준의 변경 사항이 구체적이다.**

- 각 Phase에서 변경할 코드, 제거할 코드, 추가할 코드가 명확히 구분됨
- 테스트 케이스명까지 사전 정의

### 4.2 이슈 (2건)

#### Issue 1: Phase 1의 `process_ready()` 이슈 코멘트 추가가 필수인데 Phase 2에서 언급 (Clarity)

Phase 1-1에서 `implementing` 라벨 유지로 변경하는 것과 `<!-- autodev:pr-link:{N} -->` 코멘트 추가가 함께 진행되어야 Phase 2 recovery가 동작한다. 문서에서는 "Phase 1 체크리스트에 추가"라고 Phase 2-0에서 언급하지만, Phase 1 본문(1-1)에는 이 코멘트 로직이 포함되어 있지 않다.

**권장**: Phase 1-1 변경 사항에 pr-link 코멘트 로직을 명시적으로 포함. Phase 1 체크리스트 항목에도 반영.

#### Issue 2: Phase 5의 `extract_task_knowledge()` 시그니처 변경 영향 범위

`git: &dyn Git` + `workspace: &dyn Workspace` 파라미터 추가 시 호출부 2곳(`pipeline/pr.rs` process_pending, process_improved)만 언급하고 있으나, 테스트 코드에서 mock 구성도 변경이 필요하다. 위험 요소 표에 기재되어 있지만 Phase 5 항목 목록에는 누락되어 있다.

**권장**: Phase 5-2 항목에 "호출부 수정 + 테스트 mock 수정"을 명시

### 4.3 실행 순서 검증

현재 코드 상태에서 IMPROVEMENT-PLAN의 실행 순서를 검증한다:

**Phase 1 (process_ready 수정)** — **즉시 실행 가능, 최우선**

현재 `pipeline/issue.rs:432-474`에서 PR 생성 성공/실패 모두 `labels::DONE` 추가 + `labels::IMPLEMENTING` 제거를 한다. 이를 다음과 같이 변경:

- PR 생성 성공: `remove_from_phase()` + queue 제거만 (implementing 유지, done 미추가) + pr-link 코멘트
- PR 번호 추출 실패: `remove_from_phase()` + implementing 제거만 (done 미추가)

이 변경으로 `pipeline/pr.rs:179-192`의 source_issue done 전이 코드가 실제로 동작하게 된다.

**Phase 2 (recovery)** — Phase 1 완료 후 실행

`daemon/recovery.rs`에 `recover_orphan_implementing()` 추가. 현재 `recover_orphan_wip()`만 있으므로 구조를 참고하여 유사 패턴으로 구현 가능.

**Phase 3 (extract_pr_number)** — Phase 1과 병렬 가능

`output.rs:143-166`에 JSON `pr_number` fallback 추가. 독립적 변경.

**Phase 4 (knowledge 수집)** — 독립적

`knowledge/extractor.rs`의 `collect_existing_knowledge()`에 plugins/hooks/workflow 수집 추가. 독립적 변경.

**Phase 5 (actionable PR + daily)** — Phase 4 완료 후

`create_knowledge_pr()` 추가 + `extract_task_knowledge()` 시그니처 확장. cascading 영향 있음.

---

## 5. 종합 권장사항

### 즉시 실행 (Critical Path)

| 순번 | 항목 | 근거 |
|------|------|------|
| 1 | **`process_ready()` 라벨 전이 수정** (Gap Plan Phase 1) | Phase D의 핵심 설계 목표를 무효화하는 유일한 bug. PR 생성 시 implementing 유지 + done 미추가 + pr-link 코멘트 |
| 2 | **`scan_approved()` 라벨 순서 수정** | 크래시 안전 원칙 위반. `label_add(IMPLEMENTING)` → `label_remove(APPROVED_ANALYSIS)` → `label_remove(ANALYZED)` |
| 3 | **Safety Valve 구현** (Impl Plan Phase B-3) | 재분석 무한 루프 방지 미구현 |

### 후속 실행 (병렬 가능)

| 순번 | 항목 | 근거 |
|------|------|------|
| 4 | `recover_orphan_implementing()` (Gap Plan Phase 2) | Phase 1 수정 후 implementing 상태 이슈의 크래시 복구 필요 |
| 5 | `extract_pr_number()` JSON fallback (Gap Plan Phase 3) | PR 번호 파싱 강화 |
| 6 | `collect_existing_knowledge()` 수집 범위 확장 (Gap Plan Phase 4) | delta check 정확도 향상 |
| 7 | PR pipeline worktree 정리 (Impl Plan Phase D-1) | worktree 누적 방지 |

### 향후 검토 (v2 범위 외)

| 항목 | 근거 |
|------|------|
| `process_ready()` 실패 시 `approved-analysis` 롤백 + 구현 시도 횟수 제한 | 재분석 비용 절감 |
| Knowledge PR 자기 리뷰 방지 강화 (body 마커 기반) | skip 라벨 실수 제거 대비 |
| `find_existing_pr()` `head` 파라미터 `owner:branch` 형식 확인 | GitHub API 호환성 |
| 단일 daemon 인스턴스 보장 메커니즘 확인 | `scan_approved()` race condition 방지 |

---

## 6. 문서 품질

### DESIGN-v2.md
- 1,267행으로 충분한 상세 수준
- 코드 수준 pseudo-code가 포함되어 구현 참조로 사용 가능
- Section 10 (사이드이펙트) 영향 범위 테이블이 유용
- 라벨 상태 전이 다이어그램이 ASCII art로 명확하게 표현됨

### IMPLEMENTATION-PLAN-v2.md
- 343행으로 적절한 분량
- 25개 항목 체크리스트가 추적 가능
- Phase 간 의존성 다이어그램 제공
- Phase D 번호 중복 외 구조적 문제 없음

### IMPROVEMENT-PLAN-v2-gaps.md
- 514행으로 gap 분석에 충분
- 인프라 현황 사전 조사가 구현 리스크를 줄임
- Phase 1~5 순서가 논리적으로 타당
- 코드 수준 변경 사항이 copy-paste 수준으로 구체적

---

## 검토 결론

세 문서 모두 설계 품질이 양호하며, 핵심 아키텍처 결정(HITL 게이트, Issue-PR 연동, 크래시 안전 라벨 전이)이 견고하다. 현재 코드베이스와의 gap 중 **`process_ready()` 라벨 전이**가 유일한 critical issue이며, 이를 수정하면 Phase D의 설계 의도가 살아난다.

IMPROVEMENT-PLAN-v2-gaps.md의 Phase 1을 즉시 실행하고, `scan_approved()` 라벨 순서 + Safety Valve를 함께 수정하는 것을 권장한다.
