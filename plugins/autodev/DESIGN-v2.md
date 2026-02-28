# DESIGN v2: Implement Workflow (Analysis Review + Issue-PR Linkage)

> **Date**: 2026-02-24
> **Base**: DESIGN.md (v1) — 3-Tier 상태 관리, 메인 루프, workspace 등 기존 아키텍처 유지
> **변경 범위**: Issue Flow에 분석 리뷰 게이트 추가, Issue-PR 연동, 라벨 세분화

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
3. **세분화된 라벨**: 이슈의 현재 상태를 GitHub UI에서 명확히 파악 가능

---

## 2. Label Scheme v2

### Issue 라벨

| 라벨 | 의미 | 전이 조건 |
|------|------|----------|
| `autodev:analyze` | **트리거** — 분석 요청 | 사람이 라벨 추가 |
| `autodev:wip` | 분석 진행중 | scanner가 트리거 라벨 감지 |
| `autodev:analyzed` | 분석 완료, **사람 리뷰 대기** | 분석 성공 시 |
| `autodev:approved-analysis` | 사람이 분석 승인, **구현 대기** | 사람이 라벨 변경 |
| `autodev:implementing` | PR 생성됨, **PR 리뷰 진행중** | 구현 + PR 생성 성공 시 |
| `autodev:done` | 완료 | PR approve 시 (자동) |
| `autodev:skip` | HITL 대기/제외 | clarify/wontfix |

### PR 라벨 (v1과 동일)

| 라벨 | 의미 |
|------|------|
| `autodev:wip` | 리뷰 진행중 |
| `autodev:done` | 리뷰 approve 완료 |
| `autodev:skip` | 제외 |

### 라벨 상태 전이

```
Issue (Label-Positive 모델):
                    ┌────────────────────────┐
                    │  사람이 트리거 라벨 추가  │
                    │  autodev:analyze        │
                    └────────┬───────────────┘
                             │
                    ┌────────▼───────────────┐
                    │  scan()이 감지           │
                    │  analyze 제거 + wip 추가 │
                    └────────┬───────────────┘
                             │
              autodev:wip ───┤
                    │        │
                    ├──skip──→ autodev:skip
                    │
                    ▼ (분석 완료)
              autodev:analyzed ← 사람 리뷰 대기
                    │
                    │  사람이 라벨 변경:
                    │  analyzed 제거 + approved-analysis 추가
                    │
                    ▼
              autodev:approved-analysis
                    │
                    │  scan_approved()이 감지
                    │
                    ▼
              autodev:implementing ← PR 생성됨
                    │
                    │  PR approve 시 PR pipeline이 전이
                    │
                    ▼
              autodev:done

PR:
(없음) ─scan─→ autodev:wip ─approve─→ autodev:done
                    │                      │
                    └──failure──→ (없음)    └─→ source_issue도 done 전이

사람이 분석을 reject하는 경우:
autodev:analyzed → (사람이 코멘트 + analyzed 라벨 제거)
                 → 사람이 다시 autodev:analyze 라벨 추가 시 재분석
                 → 사람이 라벨을 추가하지 않으면 아무 일도 안 일어남 (안전)

크래시 안전성:
  Label-Positive 모델이므로 크래시로 라벨이 유실되어도 재분석 위험 없음.
  사람이 autodev:analyze를 명시적으로 추가해야만 scan() 대상이 됨.
```

---

## 3. Issue Flow v2

```
┌─────────────────────────────────────────────────────────────────────┐
│  Phase 1: Analysis (트리거 라벨 기반)                                 │
│                                                                     │
│  사람: 이슈에 autodev:analyze 라벨 추가                               │
│  Scanner: autodev:analyze 라벨 감지                                  │
│  → analyze 제거 + autodev:wip 추가 + queue[Pending]                 │
│  → Analyze → 분석 리포트를 이슈 코멘트로 게시                         │
│  → autodev:wip → autodev:analyzed                                  │
│  → queue에서 제거 (사람 리뷰 대기)                                    │
└─────────────────────────────────┬───────────────────────────────────┘
                                  │
              ┌───────────────────▼──────────────────────┐
              │  Gate: Human Review (수동)                 │
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
│  → Implement → PR 생성 (body에 Closes #N 포함)                       │
│  → PR에 autodev:wip 라벨 + PR queue[Pending]에 직접 push             │
│  → queue에서 issue 제거 (PR 리뷰 대기)                                │
└─────────────────────────────────┬───────────────────────────────────┘
                                  │
┌─────────────────────────────────▼───────────────────────────────────┐
│  Phase 3: PR Review Loop (자동, 기존 v1 메커니즘)                     │
│                                                                      │
│  PR queue[Pending] → Reviewing → verdict 분기                        │
│    approve → autodev:done (PR)                                       │
│    request_changes → ReviewDone → Improving → Improved → re-review   │
│                                                                      │
│  PR approve 시:                                                      │
│    source_issue_number가 있으면 →                                     │
│      Issue: autodev:implementing → autodev:done                      │
└──────────────────────────────────────────────────────────────────────┘
```

---

## 4. Scan 구조 (Label-Positive)

```
scan_all():
  issues::scan()            — labels=autodev:analyze → Pending (분석 대기)
  issues::scan_approved()   — labels=autodev:approved-analysis → Ready (구현 대기)
  pulls::scan()             — since=cursor, no autodev label → Pending (리뷰 대기)
  pulls::scan_merges()      — labels=autodev:done, open → merge Pending
```

- `scan()`: `autodev:analyze` 라벨이 있는 open 이슈만 감지 (Label-Positive)
- `scan_approved()`: 사람이 승인한 이슈를 감지하여 구현 큐에 적재
- Safety Valve 불필요: Label-Positive 모델에서는 무한루프 방지 로직이 필요 없음

---

## 5. Issue Phase 정의

```
Issue Phase (v2):
  (trigger)     → 사람이 autodev:analyze 라벨 추가
  Pending       → scan에서 트리거 감지 (analyze→wip 전이, 분석 대기)
  Analyzing     → 분석 프롬프트 실행중
  (exit queue)  → autodev:analyzed 라벨 (사람 리뷰 대기)
  Ready         → approved scan에서 등록됨 (구현 대기)
  Implementing  → 구현 프롬프트 실행중 + PR 생성
  (exit queue)  → autodev:implementing 라벨 (PR 리뷰 대기)
  (done)        → PR approve 시 자동 전이
```

| | v1 | v2 |
|---|---|---|
| Analyzing → Ready | 내부 자동 전이 | queue 이탈 → 사람 리뷰 → scanner 재진입 |
| Ready → done | 구현 성공 시 즉시 done | PR 생성 후 queue 이탈 → PR approve 시 done |
| Issue-PR 연결 | 없음 | `PrItem.source_issue_number` |

---

## 6. Worktree & Branch Lifecycle

### 원칙

- **Worktree**: 각 pipeline 단계에서 생성하고, 단계 완료 시 **반드시 제거**
- **Branch**: remote에 push된 branch는 PR이 closed/merged 될 때까지 **유지**
- Worktree 제거 != Branch 삭제

### Pipeline별 Lifecycle

```
Issue Pipeline (process_ready):
  create_worktree → 구현 + git push → PR 생성
  → PR queue push → remove_worktree
  ※ branch는 remote에 유지 → PR pipeline이 이후 사용

PR Pipeline (process_pending → process_review_done → process_improved):
  각 단계마다: create_worktree → 작업 수행 → remove_worktree
  ※ branch는 remote에 유지, 다음 단계에서 재생성 가능

Knowledge PR:
  create_worktree("main") → branch 생성 + 파일 작성 + PR 생성
  → remove_worktree
```

### 불변식

1. 모든 pipeline 함수는 생성한 worktree를 자신이 제거한다 (success/failure 모두)
2. PR의 `head_branch`는 remote에 존재하므로 다음 단계에서 항상 재생성 가능
3. Worktree 제거 시 branch를 삭제하지 않는다

---

## 7. Knowledge Extraction v2

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
│     - 같은 skill 부족이 3개 task에서 반복              │
│     - 동일 파일 반복 수정 패턴                         │
│  4. Claude: 집계 데이터 → 우선순위 정렬               │
│  5. 일간 리포트 이슈 생성                             │
│  6. 고우선순위 → knowledge PR 생성                    │
└─────────────────────────────────────────────────────┘
```

---

## 8. Reconciliation (v2)

### startup_reconcile 라벨 처리

| 라벨 | 처리 |
|------|------|
| `autodev:done` / `autodev:skip` | skip |
| `autodev:analyze` | skip (다음 scan에서 처리) |
| `autodev:analyzed` | skip (사람 리뷰 대기) |
| `autodev:approved-analysis` | Ready 큐 적재 |
| `autodev:implementing` | skip (PR pipeline이 처리) |
| `autodev:wip` (orphan) | Pending 적재 (분석 재개) |
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

## 9. End-to-End Flow (v2)

```
┌──────────────────────────────────────────────────────────────────────┐
│                        DAEMON LOOP (v2)                              │
│                                                                      │
│  ┌───────────────────────────────────────────────────────────────┐  │
│  │ 1. RECOVERY                                                   │  │
│  │    autodev:wip + queue에 없음 → wip 라벨 제거                  │  │
│  │    autodev:implementing + PR merged → implementing → done      │  │
│  └───────────────────────────────────────────────────────────────┘  │
│                            │                                        │
│  ┌───────────────────────────────────────────────────────────────┐  │
│  │ 2. SCAN                                                       │  │
│  │    2a. issues::scan()         — analyze 라벨 → Pending (분석)  │  │
│  │    2b. issues::scan_approved()— approved → Ready (구현)        │  │
│  │    2c. pulls::scan()          — 새 PR → Pending (리뷰)        │  │
│  │    2d. pulls::scan_merges()   — approved PR → merge Pending   │  │
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
│  │      OK + PR 생성 → PR queue push + autodev:implementing     │  │
│  │      Err → 라벨 제거 + 재시도                                 │  │
│  │                                                               │  │
│  │  PRs (리뷰):                                                  │  │
│  │    Reviewing → approve → knowledge(done) + autodev:done (PR)  │  │
│  │                         + source_issue → done                  │  │
│  │    Reviewing → request_changes → ReviewDone → Improving       │  │
│  │                                    → Improved → re-review     │  │
│  │                                                               │  │
│  │  Merges:                                                      │  │
│  │    Pending → Merging → done | Conflict → 재시도               │  │
│  │                                                               │  │
│  └───────────────────────────────────────────────────────────────┘  │
│                            │                                        │
│                      sleep(tick) → loop                              │
└──────────────────────────────────────────────────────────────────────┘
```

---

## 10. Status Transitions (v2)

| Type | Phase Flow | 라벨 전이 |
|------|-----------|----------|
| Issue (분석) | `(trigger) → Pending → Analyzing → (exit)` | `analyze → wip → analyzed` |
| Issue (승인 → 구현) | `(scan_approved) → Ready → Implementing → (exit)` | `approved-analysis → implementing` |
| Issue (PR approved) | `(PR pipeline triggers)` | `implementing → done` |
| Issue (clarify/wontfix) | `Pending → Analyzing → skip` | `wip → skip` |
| Issue (analysis reject) | `analyzed → (사람이 다시 트리거)` | `analyzed → (없음) → analyze → ...` |
| PR (리뷰) | `Pending → Reviewing → approve → done` | `(없음) → wip → done` |
| PR (리뷰 + 피드백) | `Pending → Reviewing → ReviewDone → Improving → Improved → Reviewing` | `wip` 유지 |
| Merge | `Pending → Merging → done` | `(없음) → wip → done` |
