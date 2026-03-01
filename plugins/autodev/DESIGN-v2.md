# DESIGN v2: Issue-PR Workflow (Analysis Review + Label-Positive)

> **Date**: 2026-03-01
> **Revision**: v2.1 — Merge 파이프라인 제거, PR 라벨 세분화, PR scan Label-Positive 전환
> **Base**: DESIGN.md (v1) — 3-Tier 상태 관리, 메인 루프, workspace 등 기존 아키텍처 유지

---

## 1. 변경 동기

### v1의 한계

```
v1: Pending → Analyzing → Ready → Implementing → done
                 (자동)      (자동)     (자동)
```

- 분석 품질이 낮아도 곧바로 구현에 진입 → 잘못된 방향의 구현 → 리소스 낭비
- 구현 결과(PR)와 원본 이슈의 연결 고리가 없음
- PR 리뷰가 끝나도 이슈 상태는 수동으로 관리해야 함

### v2 목표

1. **분석 리뷰 게이트 (HITL)**: 분석 완료 후 사람이 검토/승인해야 구현 진행
2. **Issue-PR 연동**: 이슈에서 생성된 PR이 approve되면 이슈도 자동으로 done
3. **세분화된 라벨**: 이슈/PR의 현재 상태를 GitHub UI에서 명확히 파악 가능
4. **Label-Positive 전면 적용**: Issue와 PR 모두 라벨 기반 트리거 (자동 수집 없음)

### v1 → v2 주요 차이

| | v1 | v2 |
|---|---|---|
| Analyzing → Ready | 내부 자동 전이 | queue 이탈 → 사람 리뷰 → scanner 재진입 |
| Ready → done | 구현 성공 시 즉시 done | PR 생성 후 queue 이탈 → PR approve 시 done |
| Issue-PR 연결 | 없음 | `PrItem.source_issue_number` |
| PR scan | cursor 기반 (모든 PR 자동 수집) | Label-Positive (`autodev:review` 라벨만) |
| Merge 파이프라인 | 없음 | 없음 (scope 외) |

---

## 2. Label Scheme

### Issue 라벨

| 라벨 | 의미 | 전이 주체 |
|------|------|----------|
| `autodev:analyze` | **트리거** — 분석 요청 | HITL |
| `autodev:wip` | 분석 진행중 | daemon |
| `autodev:analyzed` | 분석 완료, **사람 리뷰 대기** | daemon |
| `autodev:approved-analysis` | 사람이 분석 승인, **구현 대기** | HITL |
| `autodev:implementing` | PR 생성됨, **PR 리뷰 진행중** | daemon |
| `autodev:done` | 완료 | daemon (PR approve 시) |
| `autodev:skip` | 제외 | daemon (clarify/wontfix) 또는 HITL |

### PR 라벨

| 라벨 | 의미 | 전이 주체 |
|------|------|----------|
| `autodev:review` | **트리거** — 리뷰 대기/진행중 | daemon (ImplementTask) |
| `autodev:changes-requested` | 피드백 반영중 | daemon (ReviewTask) |
| `autodev:done` | approve 완료 | daemon (ReviewTask) |
| `autodev:skip` | 제외 | HITL |

### Label-Positive 모델

Issue와 PR 모두 Label-Positive 모델을 따른다:

```
Label-Positive: 특정 라벨이 있는 항목만 scan 대상
- autodev 라벨이 없으면 → 무시 (안전)
- 크래시로 라벨 유실 → 재처리 위험 없음
- 사람이 명시적으로 트리거해야만 워크플로우 진입
```

Issue는 사람이 `autodev:analyze`를 추가해야 시작.
PR은 `ImplementTask`가 PR 생성 시 `autodev:review`를 자동 추가하여 시작.
외부에서 생성된 PR은 사람이 수동으로 `autodev:review`를 추가해야 리뷰 대상이 됨.

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
                    ──────────────┼──────────────
                                  │
                         ┌────────▼────────┐
                    HITL │  사람이 검토      │
                         └────────┬────────┘
                                  │
                        ┌─────────┴─────────┐
                      승인                거부
                        │                   │
                        ▼                   ▼
            autodev:approved-analysis    (analyzed 제거)
                        │                재트리거 시
                        │                analyze 재추가
              ──────────┼──────────        (HITL)
                        │
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
              ──────────┼────────── PR pipeline으로 이관
                        │
                        ▼  PR approve 시
                  autodev:done
```

### PR 전이

```
               ┌─────────────────────────┐
        daemon │  ImplementTask가 PR 생성 │
               │  + autodev:review 추가   │
               └────────────┬────────────┘
                            │
                    autodev:review ◄────────────────┐
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
                   -requested                       │
                            │                       │
                   ┌────────▼────────┐              │
            daemon │  ImproveTask     │              │
                   └────────┬────────┘              │
                            │                       │
                     ┌──────┴──────┐                │
                   성공          실패               │
                     │             │                │
                     │        (라벨 제거)           │
                     │                              │
                     └──────────────────────────────┘
                      autodev:review
                      (iteration +1)
```

### HITL 요약

| 전이 | 대상 | 누가 |
|------|------|------|
| (없음) → `analyze` | Issue | **HITL** |
| `analyzed` → `approved-analysis` | Issue | **HITL** |
| `analyzed` → (제거) | Issue | **HITL** |
| (수동) → `review` | PR (외부 PR) | **HITL** |
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
│  → queue에서 제거 (사람 리뷰 대기)                                    │
└─────────────────────────────────┬───────────────────────────────────┘
                                  │
              ┌───────────────────▼──────────────────────┐
              │  Gate: Human Review (HITL)                │
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
│  → PR에 autodev:review 라벨 + PR queue[Pending]에 직접 push          │
│  → queue에서 issue 제거 (PR 리뷰 대기)                                │
└─────────────────────────────────┬───────────────────────────────────┘
                                  │
┌─────────────────────────────────▼───────────────────────────────────┐
│  Phase 3: PR Review Loop (자동)                                      │
│                                                                      │
│  PR queue[Pending] → ReviewTask → verdict 분기                       │
│    approve → autodev:done (PR) + source_issue → done                │
│    request_changes → autodev:changes-requested                       │
│      → ImproveTask → 피드백 반영 → autodev:review (re-review)        │
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
  pulls::scan()             — labels=autodev:review → Pending (리뷰 대기)
```

- `issues::scan()`: `autodev:analyze` 라벨이 있는 open 이슈만 감지
- `issues::scan_approved()`: 사람이 승인한 이슈를 감지하여 구현 큐에 적재
- `pulls::scan()`: `autodev:review` 라벨이 있는 open PR만 감지
- Safety Valve 불필요: Label-Positive 모델에서는 무한루프 방지 로직이 필요 없음

---

## 6. Queue Phase 정의

### Issue Phase

```
  (trigger)     → 사람이 autodev:analyze 라벨 추가
  Pending       → scan에서 트리거 감지 (analyze→wip 전이, 분석 대기)
  Analyzing     → 분석 프롬프트 실행중
  (exit queue)  → autodev:analyzed 라벨 (사람 리뷰 대기)
  Ready         → approved scan에서 등록됨 (구현 대기)
  Implementing  → 구현 프롬프트 실행중 + PR 생성
  (exit queue)  → autodev:implementing 라벨 (PR 리뷰 대기)
  (done)        → PR approve 시 자동 전이
```

### PR Phase

```
  (trigger)     → ImplementTask가 PR 생성 + autodev:review 라벨 추가
  Pending       → scan에서 review 라벨 감지 (또는 ImplementTask가 직접 push)
  Reviewing     → ReviewTask 실행중
  (exit: approve)       → autodev:done + queue 제거
  (exit: request_changes) → autodev:changes-requested
  ReviewDone    → 리뷰 verdict 파싱 완료 (피드백 반영 대기)
  Improving     → ImproveTask 실행중
  Improved      → 피드백 반영 완료 → autodev:review + Pending으로 재진입
```

---

## 7. Worktree & Branch Lifecycle

### 원칙

- **Worktree**: 각 Task에서 생성하고, Task 완료 시 **반드시 제거**
- **Branch**: remote에 push된 branch는 PR이 closed/merged 될 때까지 **유지**
- Worktree 제거 != Branch 삭제

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
2. PR의 `head_branch`는 remote에 존재하므로 다음 단계에서 항상 재생성 가능
3. Worktree 제거 시 branch를 삭제하지 않는다

---

## 8. Knowledge Extraction

### Per-Task (done 전이 시)

```
┌─────────────────────────────────────────────────────┐
│  1. 기존 레포 지식 수집 (CLAUDE.md, rules, skills)    │
│  2. suggest-workflow 세션 데이터                       │
│  3. Claude: delta check (기존 지식과 비교)             │
│     └─ 차이 없음 → skip (no noise)                   │
│     └─ 차이 있음 → suggestions                       │
│  4. 이슈 코멘트로 게시                                │
│  5. skill/subagent → PR 생성 (autodev:skip 라벨)     │
└─────────────────────────────────────────────────────┘
```

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
| `autodev:analyzed` | skip (사람 리뷰 대기) |
| `autodev:approved-analysis` | Ready 큐 적재 |
| `autodev:implementing` | skip (PR pipeline이 처리) |
| `autodev:wip` (orphan Issue) | Pending 적재 (분석 재개) |
| `autodev:review` (PR) | Pending 적재 (리뷰 재개) |
| `autodev:changes-requested` (PR) | ReviewDone 적재 (피드백 반영 재개) |
| autodev 라벨 없음 | 무시 (Label-Positive) |

### recovery 추가 로직

```
autodev:implementing 이슈 감지 →
  이슈 코멘트에서 pr-link 마커로 연결 PR 번호 추출 →
  연결 PR이 merged/closed → implementing → done
  연결 PR이 아직 open → skip (PR pipeline이 처리)
  연결 PR 마커 없음 → implementing 라벨 제거 (재시도)
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
│  │    PR: autodev:review + queue에 없음 → Pending 적재            │  │
│  │    PR: autodev:changes-requested + queue에 없음 → ReviewDone  │  │
│  └───────────────────────────────────────────────────────────────┘  │
│                            │                                        │
│  ┌───────────────────────────────────────────────────────────────┐  │
│  │ 2. SCAN                                                       │  │
│  │    2a. issues::scan()         — analyze 라벨 → Pending (분석)  │  │
│  │    2b. issues::scan_approved()— approved → Ready (구현)        │  │
│  │    2c. pulls::scan()          — review 라벨 → Pending (리뷰)   │  │
│  └───────────────────────────────────────────────────────────────┘  │
│                            │                                        │
│  ┌───────────────────────────────────────────────────────────────┐  │
│  │ 3. CONSUME                                                    │  │
│  │                                                               │  │
│  │  Issues:                                                      │  │
│  │    Pending → Analyzing:                                       │  │
│  │      OK → 분석 코멘트 + autodev:analyzed + exit queue         │  │
│  │      clarify/wontfix → autodev:skip                          │  │
│  │                                                               │  │
│  │    Ready → Implementing:                                      │  │
│  │      OK + PR 생성 → PR에 autodev:review + PR queue push      │  │
│  │      Err → 라벨 제거 + 재시도                                 │  │
│  │                                                               │  │
│  │  PRs:                                                         │  │
│  │    Pending → Reviewing:                                       │  │
│  │      approve → autodev:done (PR) + source_issue → done       │  │
│  │      request_changes → autodev:changes-requested              │  │
│  │                                                               │  │
│  │    ReviewDone → Improving:                                    │  │
│  │      OK → autodev:review + Pending (re-review)               │  │
│  │      Err → 라벨 제거                                          │  │
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
| Issue (승인 → 구현) | `(scan_approved) → Ready → Implementing → (exit)` | `approved-analysis → implementing` |
| Issue (PR approved) | `(PR pipeline triggers)` | `implementing → done` |
| Issue (clarify/wontfix) | `Pending → Analyzing → skip` | `wip → skip` |
| Issue (analysis reject) | `analyzed → (사람이 다시 트리거)` | `analyzed → (없음) → analyze → ...` |
| PR (리뷰) | `Pending → Reviewing → (approve)` | `review → done` |
| PR (리뷰 + 피드백) | `Pending → Reviewing → ReviewDone → Improving → Improved → Pending` | `review → changes-requested → review` |
| PR (max iteration) | `Pending → Reviewing → (skip)` | `review → skip` |

---

## 12. Scope 외

다음은 v2 플로우 범위에 포함되지 않으며, 별도 운영 결정 사항이다:

- **PR Merge**: `autodev:done` 이후의 머지는 사람의 판단 또는 별도 자동화가 처리
- **Branch 정리**: merged PR의 branch 삭제는 GitHub settings 또는 별도 자동화
- **외부 PR 자동 리뷰**: 외부 PR에 `autodev:review` 라벨을 자동 추가하는 정책은 별도 결정
