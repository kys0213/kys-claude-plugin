# DESIGN v3: Issue-PR Workflow (Auto-Approve + Label-Positive)

> **Date**: 2026-03-05
> **Revision**: v3.0 — Auto-approve, impl-failed, changes-requested 정규 스캔, sync_default_branch, add-first 라벨 전이
> **Base**: [DESIGN-v2.md](./DESIGN-v2.md) — Analysis Review + Label-Positive 모델

---

## 1. 변경 동기

### v2의 한계

```
v2: analyze → analyzed → [HITL 승인] → implementing → done
                              ↑
                          항상 사람 개입 필요
```

- 분석 confidence가 높아도 매번 사람 승인을 기다려야 함 → 불필요한 병목
- changes-requested PR이 recovery 경로에서만 감지됨 → 정규 스캔 누락
- PR 생성 실패 시 worktree가 삭제되어 구현 결과 유실
- orphan implementing 복구 시 implementing 라벨 미제거 → 무한 복구 루프

### v3 목표

1. **Auto-approve**: confidence 기반 자동 구현 전환 (HITL은 불확실한 경우에만)
2. **impl-failed 복구 경로**: PR 생성 실패 시 worktree 보존 + 수동 복구 안내
3. **changes-requested 정규 스캔**: recovery 의존 제거
4. **sync_default_branch**: git pull 대체로 브랜치 오염 방지
5. **add-first 라벨 전이**: 라벨 전이 시 추가를 먼저 수행하여 유실 방지

### v2 → v3 주요 차이

| | v2 | v3 |
|---|---|---|
| analyzed → approved | HITL only | HITL 또는 daemon (auto-approve) |
| PR 생성 실패 | worktree 삭제 + 라벨 제거 | worktree 보존 + impl-failed 라벨 + 복구 코멘트 |
| changes-requested 스캔 | recovery에서만 | `scan_pulls()`에서 정규 스캔 |
| 기본 클론 동기화 | `git pull --ff-only` | `sync_default_branch` (fetch+checkout+reset) |
| 라벨 전이 순서 | 미규정 | add-first 원칙 |
| orphan implementing 복구 | wip 추가만 | wip 추가 + implementing 제거 |

---

## 2. Label Scheme

### Issue 라벨

| 라벨 | 의미 | 전이 주체 |
|------|------|----------|
| `autodev:analyze` | **트리거** — 분석 요청 | HITL |
| `autodev:wip` | 분석 진행중 | daemon |
| `autodev:analyzed` | 분석 완료, **리뷰 대기** | daemon |
| `autodev:approved-analysis` | 분석 승인, **구현 대기** | HITL 또는 daemon (auto-approve) |
| `autodev:implementing` | PR 생성됨, **PR 리뷰 진행중** | daemon |
| `autodev:impl-failed` | 구현 완료했으나 PR 생성/감지 실패, **수동 복구 필요** | daemon |
| `autodev:done` | 완료 | daemon (PR approve 시) |
| `autodev:skip` | 제외 | daemon (clarify/wontfix) 또는 HITL |

### PR 라벨

| 라벨 | 의미 | 전이 주체 | 비고 |
|------|------|----------|------|
| `autodev:wip` | **트리거** — 리뷰 대기/진행중 | daemon (ImplementTask) | Issue 공유 |
| `autodev:changes-requested` | 피드백 반영중 | daemon (ReviewTask) | PR 전용 |
| `autodev:done` | approve 완료 | daemon (ReviewTask) | Issue 공유 |
| `autodev:skip` | 제외 | HITL | Issue 공유 |

### 공유 라벨 정책

Issue와 PR은 GitHub에서 독립된 엔티티이므로, 동일한 라벨명을 사용해도 충돌하지 않는다.
라벨의 의미는 대상(Issue/PR)에 따라 다르게 해석된다:

| 라벨 | Issue 의미 | PR 의미 |
|------|-----------|---------|
| `autodev:wip` | 분석 진행중 | 리뷰 대기/진행중 |
| `autodev:done` | 완료 | approve 완료 |
| `autodev:skip` | 제외 | 제외 |

PR 전용 라벨은 `autodev:changes-requested` 1개만 존재한다.

### Label-Positive 모델

Issue와 PR 모두 Label-Positive 모델을 따른다:

```
Label-Positive: 특정 라벨이 있는 항목만 scan 대상
- autodev 라벨이 없으면 → 무시 (안전)
- 크래시로 라벨 유실 → 재처리 위험 없음
- 사람이 명시적으로 트리거해야만 워크플로우 진입
```

Issue는 사람이 `autodev:analyze`를 추가해야 시작.
PR은 `ImplementTask`가 PR 생성 시 `autodev:wip`를 자동 추가하여 시작.
외부에서 생성된 PR은 사람이 수동으로 `autodev:wip`를 추가해야 리뷰 대상이 됨.

---

## 3. 라벨 상태 전이

### Issue 전이

```
                         ┌─────────────────┐
                    HITL │  사람이 라벨 추가  │
                         └────────┬────────┘
                                  │
                          autodev:analyze
                                  │
                    ──────────────┼──────────────
                                  │
                         ┌────────▼────────┐
                  daemon │  scanner 감지    │
                         │  analyze → wip  │
                         └────────┬────────┘
                                  │
                           autodev:wip
                                  │
                         ┌────────▼────────┐
                  daemon │  AnalyzeTask     │
                         └────────┬────────┘
                                  │
                     ┌────────────┼────────────┐
                     │            │            │
              wontfix/clarify   성공        실패
                     │            │            │
                     ▼            ▼            ▼
               autodev:skip  autodev:analyzed  (라벨 제거)
                                  │
                    ┌─────────────┴─────────────┐
                    │                           │
             auto_approve &&              그 외 (HITL 대기)
          confidence >= threshold               │
                    │                  ─────────┼──────────
                    │                           │
                    │                  ┌────────▼────────┐
                    │             HITL │  사람이 검토      │
                    │                  └────────┬────────┘
                    │                           │
                    │                 ┌─────────┴─────────┐
                    │               승인                거부
                    │                 │                   │
                    ▼                 ▼                   ▼
            autodev:approved-analysis (daemon)    (analyzed 제거)
                                  │               재트리거 시
                    ──────────────┤               analyze 재추가
                                  │                 (HITL)
                         ┌────────▼────────┐
                  daemon │  scanner 감지    │
                         │  → implementing │
                         └────────┬────────┘
                                  │
                          autodev:implementing
                                  │
                         ┌────────▼────────┐
                  daemon │  ImplementTask   │
                         │  PR 생성         │
                         └────────┬────────┘
                                  │
                        ┌─────────┴─────────┐
                     PR 생성 성공      PR 감지 실패
                        │                   │
                        ▼                   ▼
             ──────────┼──────     autodev:impl-failed
                        │          (worktree 보존,
             PR pipeline으로 이관    수동 복구 안내)
                        │
                        ▼  PR approve 시
                  autodev:done
```

### PR 전이

```
               ┌─────────────────────────┐
        daemon │  ImplementTask가 PR 생성 │
               │  + autodev:wip 추가      │
               └────────────┬────────────┘
                            │
                    autodev:wip ◄────────────────┐
                            │                       │
                   ┌────────▼────────┐              │
            daemon │  ReviewTask      │              │
                   └────────┬────────┘              │
                            │                       │
            ┌───────────────┼──────────────┐        │
            │               │              │        │
         approve    request_changes   max iter      │
            │               │              │        │
            ▼               ▼              ▼        │
     autodev:done  autodev:changes   autodev:skip   │
            │      -requested                       │
            │               │                       │
            │      ┌────────▼────────┐              │
            │      │  ImproveTask     │              │
            │      └────────┬────────┘              │
            │               │                       │
            │        ┌──────┴──────┐                │
            │      성공          실패               │
            │        │             │                │
            │        │        (라벨 제거)           │
            │        │                              │
            │        └──────────────────────────────┘
            │         autodev:wip
            │         (iteration +1)
            │
    ────────┼──────── queue 이탈, merge 대기
            │
            ▼  PR merged 시 (scan_done_merged 감지)
   ┌────────────────┐
   │  ExtractTask    │ 지식 추출
   └────────┬───────┘
            │
            ▼
     queue 제거 (완료)
```

### HITL 요약

| 전이 | 대상 | 누가 |
|------|------|------|
| (없음) → `analyze` | Issue | **HITL** |
| `analyzed` → `approved-analysis` | Issue | **HITL** (auto_approve 비활성 시) |
| `analyzed` → `approved-analysis` | Issue | **daemon** (auto_approve 활성 + confidence ≥ threshold) |
| `analyzed` → (제거) | Issue | **HITL** |
| (수동) → `wip` | PR (외부 PR) | **HITL** |
| (수동) → `skip` | Both | **HITL** |
| **그 외 모든 전이** | Both | **daemon** |

---

## 4. Issue Flow

```
┌─────────────────────────────────────────────────────────────────────┐
│  Phase 1: Analysis (트리거 라벨 기반)                                 │
│                                                                     │
│  사람: 이슈에 autodev:analyze 라벨 추가                               │
│  Scanner: autodev:analyze 라벨 감지                                  │
│  → analyze 제거 + autodev:wip 추가 + queue[Pending]                 │
│  → AnalyzeTask → 분석 리포트를 이슈 코멘트로 게시                     │
│  → autodev:wip → autodev:analyzed                                  │
│                                                                     │
│  Auto-approve 분기:                                                 │
│    auto_approve=true + confidence ≥ threshold:                      │
│      → autodev:approved-analysis 자동 추가 (daemon)                  │
│      → Phase 2로 즉시 진행                                           │
│    그 외:                                                            │
│      → queue에서 제거 (사람 리뷰 대기)                                │
└─────────────────────────────────┬───────────────────────────────────┘
                                  │
              ┌───────────────────▼──────────────────────┐
              │  Gate: Human Review (HITL)                │
              │  ※ auto_approve 시 이 단계를 건너뜀         │
              │                                           │
              │  사람이 분석 리포트를 검토:                  │
              │    ✅ 승인 → autodev:approved-analysis 추가 │
              │    ❌ 거부 → analyzed 라벨 제거 + 피드백     │
              │              (재분석 시 autodev:analyze 재추가) │
              └───────────────────┬──────────────────────┘
                                  │
┌─────────────────────────────────▼───────────────────────────────────┐
│  Phase 2: Implementation (자동)                                      │
│                                                                      │
│  Scanner: autodev:approved-analysis 라벨 감지                        │
│  → approved-analysis 제거, autodev:implementing 추가                  │
│  → queue[Ready]에 push                                               │
│  → ImplementTask → PR 생성 (body에 Closes #N 포함)                   │
│                                                                      │
│  PR 생성 성공:                                                        │
│  → PR에 autodev:wip 라벨 + PR queue[Pending]에 직접 push            │
│  → worktree 제거 + queue에서 issue 제거 (PR 리뷰 대기)               │
│                                                                      │
│  PR 감지 실패:                                                        │
│  → autodev:impl-failed 라벨 추가                                     │
│  → worktree 보존 + 복구 안내 코멘트 게시                               │
│  → queue에서 issue 제거                                               │
└─────────────────────────────────┬───────────────────────────────────┘
                                  │
┌─────────────────────────────────▼───────────────────────────────────┐
│  Phase 3: PR Review Loop (자동)                                      │
│                                                                      │
│  PR queue[Pending] → ReviewTask → verdict 분기                       │
│    approve → autodev:done (PR) + source_issue → done                │
│    request_changes → autodev:changes-requested                       │
│      → ImproveTask → 피드백 반영 → autodev:wip (re-review)        │
│                                                                      │
│  PR approve 시:                                                      │
│    source_issue_number가 있으면 →                                     │
│      Issue: autodev:implementing → autodev:done                      │
└──────────────────────────────────────────────────────────────────────┘
```

---

## 5. Scan 구조 (Label-Positive)

```
scan_all():
  issues::scan()            — labels=autodev:analyze → Pending (분석 대기)
  issues::scan_approved()   — labels=autodev:approved-analysis → Ready (구현 대기)
  pulls::scan()             — labels=autodev:wip → Pending (리뷰 대기)
                            — labels=autodev:changes-requested → ReviewDone (피드백 반영 대기)
  pulls::scan_done_merged() — labels=autodev:done, NOT autodev:extracted/extract-failed, state=merged → Extracting
```

- `issues::scan()`: `autodev:analyze` 라벨이 있는 open 이슈만 감지
- `issues::scan_approved()`: 승인된 이슈를 감지하여 구현 큐에 적재 (HITL 또는 auto-approve)
- `pulls::scan()`: 두 가지 라벨을 정규 스캔:
  - `autodev:wip` → Pending (리뷰 대기)
  - `autodev:changes-requested` → ReviewDone (피드백 반영 대기)
- `pulls::scan_done_merged()`: `autodev:done` 라벨 + merged 상태 + `autodev:extracted`/`autodev:extract-failed` 라벨이 없는 PR 감지
- Safety Valve 불필요: Label-Positive 모델에서는 무한루프 방지 로직이 필요 없음

---

## 6. Queue Phase 정의

### Issue Phase

```
  (trigger)     → 사람이 autodev:analyze 라벨 추가
  Pending       → scan에서 트리거 감지 (analyze→wip 전이, 분석 대기)
  Analyzing     → 분석 프롬프트 실행중
  (exit queue)  → autodev:analyzed 라벨
                  auto_approve 시: approved-analysis도 자동 추가 → 다음 scan에서 Ready 진입
                  그 외: 사람 리뷰 대기
  Ready         → approved scan에서 등록됨 (구현 대기)
  Implementing  → 구현 프롬프트 실행중 + PR 생성
  (exit queue)  → autodev:implementing 라벨 (PR 리뷰 대기)
                  또는 autodev:impl-failed (PR 감지 실패, 수동 복구)
  (done)        → PR approve 시 자동 전이
```

### PR Phase

```
  (trigger)     → ImplementTask가 PR 생성 + autodev:wip 라벨 추가
  Pending       → scan에서 wip 라벨 감지 (또는 ImplementTask가 직접 push)
  Reviewing     → ReviewTask 실행중
  (exit: approve)       → autodev:done + queue 제거 (merge 대기)
  (exit: request_changes) → autodev:changes-requested
  ReviewDone    → 리뷰 verdict 파싱 완료 (피드백 반영 대기)
                  ※ scan에서 changes-requested 라벨로 직접 감지 가능
  Improving     → ImproveTask 실행중
  Improved      → 피드백 반영 완료 → autodev:wip + Pending으로 재진입

  --- merge 후 ---
  Extracting    → scan_done_merged에서 감지 (done + merged + NOT extracted/extract-failed)
  (exit)        → ExtractTask 완료 → autodev:extracted 추가 + queue 제거
                → worktree 실패 → autodev:extract-failed 추가 (라벨 제거로 재시도 가능)
```

---

## 7. Worktree & Branch Lifecycle

### 원칙

- **Worktree**: 각 Task에서 생성하고, Task 완료 시 **반드시 제거**
- **Branch**: remote에 push된 branch는 PR이 closed/merged 될 때까지 **유지**
- Worktree 제거 != Branch 삭제
- **기본 클론 동기화**: `sync_default_branch` (fetch → symbolic-ref로 기본 브랜치 감지 → checkout → reset --hard)
  - `git pull --ff-only` 대체: 다른 브랜치에 체크아웃된 상태에서도 안전하게 동기화

### Task별 Lifecycle

```
Issue Tasks:
  AnalyzeTask:    create_worktree → 분석 실행 → remove_worktree
  ImplementTask:  create_worktree → 구현 + git push → PR 생성 → remove_worktree
  ※ branch는 remote에 유지 → PR pipeline이 이후 사용

PR Tasks:
  ReviewTask:     create_worktree → 리뷰 실행 → remove_worktree
  ImproveTask:    create_worktree → 피드백 반영 + push → remove_worktree
  ※ branch는 remote에 유지, 다음 단계에서 재생성 가능
```

### 불변식

1. 모든 Task는 생성한 worktree를 자신이 제거한다 (success/failure 모두)
   - **예외**: ImplementTask에서 PR 생성/감지 실패 시 worktree를 보존하여 수동 복구 가능
   - 수동 PR 생성 완료 후 `autodev:impl-failed` 라벨을 제거하면 다음 poll에서 worktree가 정리됨
2. PR의 `head_branch`는 remote에 존재하므로 다음 단계에서 항상 재생성 가능
3. Worktree 제거 시 branch를 삭제하지 않는다
4. **라벨 전이 순서 (add-first)**: 라벨을 전이할 때 새 라벨을 먼저 추가한 후 이전 라벨을 제거한다.
   크래시 발생 시 라벨이 유실되지 않고, recovery에서 감지할 수 있다.
5. **source_issue_number 보존**: reconcile/preflight 경로에서 PR 아이템을 복구할 때
   원본 이슈 번호를 반드시 함께 복원한다. 누락 시 PR approve → Issue done 전이가 실패한다.

---

## 8. Knowledge Extraction

### Per-Task (PR merge 후)

트리거 조건: `autodev:done` 라벨 + PR merged 상태 + `autodev:extracted`/`autodev:extract-failed` 라벨 없음

```
┌─────────────────────────────────────────────────────┐
│  scan_done_merged() 감지                              │
│  → Extracting 큐 적재                                │
│                                                      │
│  ExtractTask:                                        │
│  1. 기존 레포 지식 수집 (CLAUDE.md, rules, skills)    │
│  2. suggest-workflow 세션 데이터                       │
│  3. Claude: delta check (기존 지식과 비교)             │
│     └─ 차이 없음 → skip (no noise)                   │
│     └─ 차이 있음 → suggestions                       │
│  4. 이슈 코멘트로 게시                                │
│  5. skill/subagent → PR 생성 (autodev:skip 라벨)     │
│  6. autodev:extracted 라벨 추가 (중복 방지)           │
│     └─ worktree 실패 시 autodev:extract-failed 추가  │
└─────────────────────────────────────────────────────┘
```

> **왜 merge 후인가?**
> approve됐지만 merge되지 않은 PR에서 지식을 추출하면 낭비될 수 있다.
> merge된 코드만이 실제로 레포에 반영된 확정 지식이다.
>
> **중복 방지**: `autodev:extracted` 라벨로 이미 처리된 PR을 scan에서 제외.
> worktree 생성 실패 등 preflight 오류 시 `autodev:extract-failed` 라벨을 추가하여 무한 재스캔을 방지.
> 라벨을 수동 제거하면 다음 스캔에서 재시도된다.
> Label-Positive 모델과 일관되며 GitHub UI에서도 추출 상태를 확인할 수 있다.
>
> **보안**: Knowledge PR 생성 시 `target_file` 경로를 검증하여 path traversal을 방지한다.
> 허용된 디렉토리 밖으로의 파일 생성/수정을 차단한다.

### Daily (일간 집계)

```
┌─────────────────────────────────────────────────────┐
│  1. daemon 로그 파싱 (통계)                           │
│  2. 일간 per-task suggestions 집계                   │
│  3. 교차 task 패턴 감지                               │
│  4. Claude: 집계 데이터 → 우선순위 정렬               │
│  5. 일간 리포트 이슈 생성                             │
│  6. 고우선순위 → knowledge PR 생성                    │
└─────────────────────────────────────────────────────┘
```

---

## 9. Reconciliation

### startup_reconcile 라벨 처리

| 라벨 | 처리 |
|------|------|
| `autodev:done` / `autodev:skip` | skip |
| `autodev:analyze` | skip (다음 scan에서 처리) |
| `autodev:analyzed` | skip (사람 리뷰 대기, auto_approve 시 다음 scan에서 approved 감지) |
| `autodev:approved-analysis` | Ready 큐 적재 |
| `autodev:implementing` | skip (PR pipeline이 처리) |
| `autodev:impl-failed` | skip (수동 복구 대기) |
| `autodev:wip` (orphan Issue) | Pending 적재 (분석 재개) |
| `autodev:wip` (PR) | Pending 적재 (리뷰 재개) |
| `autodev:changes-requested` (PR) | ReviewDone 적재 (피드백 반영 재개) |
| autodev 라벨 없음 | 무시 (Label-Positive) |

> **source_issue_number**: reconcile에서 PR 아이템 복구 시
> 이슈 코멘트의 `autodev:pr-link` 마커에서 source_issue_number를 추출하여 반드시 복원한다.

### recovery 추가 로직

```
autodev:wip (orphan Issue) 감지 →
  queue에 없음 → wip 라벨 제거 (다음 scan에서 재처리)

autodev:implementing 이슈 감지 →
  이슈 코멘트에서 pr-link 마커로 연결 PR 번호 추출 →
  연결 PR이 merged/closed → implementing → done
  연결 PR이 아직 open → implementing 제거 + PR에 wip 추가 (PR pipeline이 처리)
  연결 PR 마커 없음 → implementing 라벨 제거 (재시도)
  그 외 PR 상태 → warn 로그 (상태값 포함) + skip

autodev:wip (orphan PR) 감지 →
  queue에 없음 → Pending 적재 (리뷰 재개)

autodev:changes-requested (orphan PR) 감지 →
  queue에 없음 → ReviewDone 적재 (피드백 반영 재개)
```

---

## 10. End-to-End Flow

```
┌──────────────────────────────────────────────────────────────────────┐
│                        DAEMON LOOP                                    │
│                                                                      │
│  ┌───────────────────────────────────────────────────────────────┐  │
│  │ 1. RECOVERY                                                   │  │
│  │    Issue: autodev:wip + queue에 없음 → wip 라벨 제거           │  │
│  │    Issue: autodev:implementing + PR merged → done              │  │
│  │    Issue: autodev:implementing + PR open                      │  │
│  │      → implementing 제거 + PR에 wip 추가                      │  │
│  │    PR: autodev:wip + queue에 없음 → Pending 적재            │  │
│  │    PR: autodev:changes-requested + queue에 없음 → ReviewDone  │  │
│  └───────────────────────────────────────────────────────────────┘  │
│                            │                                        │
│  ┌───────────────────────────────────────────────────────────────┐  │
│  │ 2. SCAN                                                       │  │
│  │    2a. issues::scan()           — analyze 라벨 → Pending      │  │
│  │    2b. issues::scan_approved()  — approved → Ready            │  │
│  │    2c. pulls::scan()            — wip 라벨 → Pending (리뷰)   │  │
│  │                                 — changes-requested → ReviewDone│  │
│  │    2d. pulls::scan_done_merged()— done+merged-extracted      │  │
│  └───────────────────────────────────────────────────────────────┘  │
│                            │                                        │
│  ┌───────────────────────────────────────────────────────────────┐  │
│  │ 3. CONSUME                                                    │  │
│  │                                                               │  │
│  │  Issues:                                                      │  │
│  │    Pending → Analyzing:                                       │  │
│  │      OK → 분석 코멘트 + autodev:analyzed                      │  │
│  │        auto_approve + confidence ≥ threshold:                 │  │
│  │          → autodev:approved-analysis 자동 추가 + exit queue   │  │
│  │        그 외: exit queue (HITL 대기)                           │  │
│  │      clarify/wontfix → autodev:skip                          │  │
│  │                                                               │  │
│  │    Ready → Implementing:                                      │  │
│  │      OK + PR 생성 → PR에 autodev:wip + PR queue push         │  │
│  │      OK + PR 감지 실패 → autodev:impl-failed (worktree 보존) │  │
│  │      Err → autodev:impl-failed (라벨 추가 + 실패 코멘트)      │  │
│  │                                                               │  │
│  │  PRs:                                                         │  │
│  │    Pending → Reviewing:                                       │  │
│  │      approve → autodev:done (PR) + source_issue → done       │  │
│  │      request_changes → autodev:changes-requested              │  │
│  │      Err → autodev:review-failed (라벨 추가 + 실패 코멘트)   │  │
│  │                                                               │  │
│  │    ReviewDone → Improving:                                    │  │
│  │      OK → autodev:wip + Pending (re-review)                  │  │
│  │      Err → autodev:improve-failed (라벨 추가 + 실패 코멘트)  │  │
│  │                                                               │  │
│  │    Extracting:                                                │  │
│  │      ExtractTask → 지식 추출 + autodev:extracted → 제거      │  │
│  │      (worktree 실패 → autodev:extract-failed → 제거)         │  │
│  │                                                               │  │
│  └───────────────────────────────────────────────────────────────┘  │
│                            │                                        │
│                      sleep(tick) → loop                              │
└──────────────────────────────────────────────────────────────────────┘
```

---

## 11. Status Transitions 요약

| Type | Phase Flow | 라벨 전이 |
|------|-----------|----------|
| Issue (분석) | `(trigger) → Pending → Analyzing → (exit)` | `analyze → wip → analyzed` |
| Issue (auto-approve) | `Analyzing → (exit) → (scan_approved) → Ready` | `analyzed + approved-analysis (auto)` |
| Issue (승인 → 구현) | `(scan_approved) → Ready → Implementing → (exit)` | `approved-analysis → implementing` |
| Issue (구현 실패) | `Implementing → (PR 감지 실패)` | `implementing → impl-failed` |
| Issue (PR approved) | `(PR pipeline triggers)` | `implementing → done` |
| Issue (clarify/wontfix) | `Pending → Analyzing → skip` | `wip → skip` |
| Issue (analysis reject) | `analyzed → (사람이 다시 트리거)` | `analyzed → (없음) → analyze → ...` |
| PR (리뷰) | `Pending → Reviewing → (approve)` | `wip → done` |
| PR (리뷰 + 피드백) | `Pending → Reviewing → ReviewDone → Improving → Improved → Pending` | `wip → changes-requested → wip` |
| PR (max iteration) | `Pending → Reviewing → (skip)` | `wip → skip` |
| PR (지식 추출) | `(scan_done_merged) → Extracting → (exit)` | `done (merged) → done + extracted` |

---

## 12. Configuration

### Auto-approve 설정

```yaml
sources:
  github:
    auto_approve: false           # 자동 구현 전환 활성화 (기본: false)
    auto_approve_threshold: 0.8   # 자동 전환 최소 confidence (기본: 0.8, 범위: 0.0~1.0)
    confidence_threshold: 0.7     # 분석 신뢰도 기준 (기본: 0.7)
```

| 설정 | 기본값 | 설명 |
|------|--------|------|
| `auto_approve` | `false` | 분석 완료 후 자동 구현 전환 활성화 |
| `auto_approve_threshold` | `0.8` | `auto_approve=true`일 때 자동 전환 최소 confidence. 0.0~1.0 범위로 clamping됨 |
| `confidence_threshold` | `0.7` | 분석 신뢰도 기준 (clarify/implement 분류에 사용) |

---

## 13. Scope 외

다음은 v3 플로우 범위에 포함되지 않으며, 별도 운영 결정 사항이다:

- **PR Merge**: `autodev:done` 이후의 머지는 사람의 판단 또는 별도 자동화가 처리
- **Branch 정리**: merged PR의 branch 삭제는 GitHub settings 또는 별도 자동화
- **외부 PR 자동 리뷰**: 외부 PR에 `autodev:wip` 라벨을 자동 추가하는 정책은 별도 결정
- **라벨 자동 등록**: `/auto-setup` 시 autodev 라벨 일괄 생성은 별도 기능으로 관리
