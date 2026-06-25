---
name: orchestrator
description: Use this skill when delegating work to multiple sub-agents, agent-teams, or worktrees — parallel fan-out, sequential pipelines, long-running agent teams, or any moment the main agent is about to use Edit/Write directly (delegate instead). Triggers include "여러 작업 병렬로", "동시에 처리", "에이전트 나눠서", "worktree로 분리", "위임해서", "팀으로 작업", "delegate", "parallel agents", "fan-out", "agent team", "sub-agent", "dispatch multiple", "split into tasks", "run in parallel".
version: 0.1.0
---

# Orchestrator Skill

## When to use (트리거 케이스)

이 스킬을 트리거해야 하는 상황:

- 사용자가 **2개 이상의 독립 작업**을 한 번에 요청 ("A랑 B랑 C 같이 해줘", "동시에 처리해줘")
- **병렬 fan-out**이 가능해 보일 때 ("여러 파일 동시에", "병렬로", "parallel", "in parallel")
- **sub-agent / agent-team / worktree 위임**을 명시적으로 요청 ("나눠서", "팀으로", "에이전트 여러 개", "delegate", "dispatch")
- **장기 진행 작업**에 식별 가능한 agent team이 필요할 때 (designer/implementer/reviewer 등)
- **머지 조정**이 필요한 다중 변경 (여러 worktree 결과 통합, 충돌 해결 위임)
- **메인 에이전트가 Edit/Write/NotebookEdit로 직접 코드를 수정하려는 모든 순간** — 위임으로 전환할지 먼저 검토

트리거하면 안 되는 상황:
- 단일 파일의 단순 편집 (오버헤드만 늘어남)
- 사용자가 직접 메인이 처리하라고 명시한 경우
- 1턴 안에 끝나는 read-only 조사

## 사고 모드 (Mental Model)

이 스킬을 트리거한 순간부터 메인 에이전트는 **편집자가 아니라 관리자**다.

```
❌ 메인이 Edit/Write로 코드 작성
✅ 메인은 Read/Bash로 상태 파악 + Agent로 위임 + SendMessage로 조율
```

### 메인 에이전트가 해도 되는 일
- `Read`, `Glob`, `Grep`, `Bash(git status / git log / git diff --stat)` — 작업 분해와 위험도 판단을 위한 조사
- `Agent`, `SendMessage`, `Monitor` — 위임과 조율 (agent team은 `Agent`의 `name`으로 spawn — 실험 플래그 필요, `TeamCreate`는 제거됨)
- `TaskCreate` / `TaskList` / `TaskGet` / `TaskUpdate` — 다중 작업의 분배·의존성·상태 추적 (선택; `references/agent-monitor.md §Task 시스템`)
- 결과물 취합 후 사용자에게 보고

### 메인 에이전트가 하면 안 되는 일
- `Edit`, `Write`, `NotebookEdit` — 코드 편집은 항상 sub-agent에 위임
- 코드 작성을 직접 수행 (sub-agent 실패 시에도 편집권을 가져오지 않음 → 사용자에게 보고)
- `EnterWorktree` / `git checkout <other-branch>` 로 worktree 또는 다른 브랜치로 진입 — 메인은 epic 브랜치에서만 동작

---

## 진입 절차 (Entry Procedure)

**오케스트레이터는 반드시 epic 브랜치 전략으로 동작한다.** 메인 에이전트는 worktree가 아니라 epic 브랜치에 체크아웃된 상태로 작업하고, agent team으로 위임된 sub-agent들만 worktree로 격리한다. 이 토폴로지를 어기면 머지 경로가 꼬이고 메인이 직접 편집하게 되어 오케스트레이터 원칙이 깨진다.

### 토폴로지

```
main
  └─ epic/<name>   ← 메인 에이전트 (read + dispatch + report)
       ├─ worktree A (sub-agent A: 격리된 작업 브랜치, base = epic/<name>)
       ├─ worktree B (sub-agent B: 격리된 작업 브랜치, base = epic/<name>)
       └─ worktree C (sub-agent C: ...)
```

- **메인 = epic 브랜치 자체**. 절대 worktree로 들어가지 않는다.
- **sub-agent = epic 브랜치를 base로 한 worktree**. 결과는 epic 브랜치로 머지한다.
- **epic 브랜치 → main 머지는 이 스킬 범위 밖** (사용자 결정 / 별도 release 절차).

### 진입 시 체크

오케스트레이터 트리거 직후, 위임을 시작하기 전에 메인이 확인할 것:

1. **현재 브랜치가 epic 브랜치인가?**
   - `git branch --show-current` 확인
   - `main` / 일반 feature 브랜치라면 epic 브랜치를 먼저 만들거나 사용자에게 어떤 epic 브랜치로 진입할지 물어본다 (`git` skill 의 브랜치 생성 또는 plain `git checkout -b epic/<name>`).
2. **현재 메인이 다른 worktree 안에 있지 않은가?**
   - `git rev-parse --show-toplevel` 가 repo의 메인 working tree여야 함
   - worktree 안에서 오케스트레이터를 시작했다면 즉시 메인 working tree로 빠져나오도록 사용자에게 보고
3. **이후 모든 sub-agent dispatch는 `isolation: "worktree"` 로** — base는 현재 epic 브랜치 (Agent isolation이 자동으로 현재 HEAD를 base로 worktree를 만든다)

### 메인의 동작 범위

```
epic 브랜치 위에서 메인이 하는 일:
  - Read / Glob / Grep / Bash(git status, git log, git diff)
  - Agent(isolation: "worktree", ...) 로 sub-agent dispatch
  - 결과 수령 → 머지 순서 결정 → 충돌은 위임 → epic 브랜치로 머지
  - 사용자 보고

epic 브랜치 위에서 메인이 하지 않는 일:
  - Edit / Write / NotebookEdit
  - EnterWorktree / git checkout <다른 브랜치>
  - 직접 코드 작성, 직접 충돌 해결
```

---

## 표준 절차 (Workflow)

```
0. 진입 확인 (Entry)        → 현재가 epic 브랜치 + 메인 working tree인지 확인
1. 분해 (Decompose)        → 작업을 독립 단위로 쪼갠다
2. 위험도 분석 (Analyze)    → 단위 간 파일/의존성 충돌 위험 식별
3. 실행 계획 (Plan)         → 병렬/순차 결정 + 위임 형태(단발/team) 결정
4. 위임 (Dispatch)          → Agent 호출 (worktree isolation, base = epic 브랜치)
5. 모니터링 (Monitor)       → 진행 추적, 정체 감지, 사용자 보고
6. 머지 조정 (Coordinate)   → 결과를 epic 브랜치로 통합 + 충돌 위임 + worktree 정리
7. 보고 (Report)            → 사용자에게 결과 요약
```

각 단계의 상세 패턴은 아래 references에 있다.

---

## 병렬 vs 순차 결정 트리

오케스트레이터의 가장 중요한 판단. **머지 시 충돌이 가장 적고 안정적인 쪽**을 선택한다.

```
작업 A, B의 변경 파일 집합을 식별
  │
  ├─ disjoint (겹치는 파일 없음)
  │    └─ 의존성 없음? → 병렬 (각자 worktree-isolated agent)
  │       의존성 있음? → 순차 (A 결과 → B 입력)
  │
  └─ overlap (같은 파일 수정)
       └─ 같은 라인 영역 가능성? → 순차 (단일 worktree에서 직렬)
          명확히 다른 영역? → 순차 권장 (안전), 병렬은 경험상 안전한 경우만
```

판단 근거:
- **병렬의 이득**: 시간 단축, 독립 컨텍스트
- **병렬의 비용**: 머지 시 충돌 → 사람 개입 필요
- **기본 규칙**: 의심스러우면 순차. 병렬은 disjoint가 명백할 때만.

---

## 위임 형태 결정

| 상황 | 형태 | 도구 |
|------|------|------|
| 1회성 독립 작업, 결과물 단일 | 단발 sub-agent | `Agent({...})` |
| 여러 agent 협업·식별/제어 필요 (read-only 조율) | agent team | `Agent({name, ...})` + `SendMessage` (실험 플래그 필요·`team_name` 무시·편집 격리는 subagent) |
| 파일 충돌 위험 있는 병렬 | worktree-isolated | `Agent({isolation: "worktree", ...})` |

> **격리는 subagent만 보장**: agent team teammate는 공유 checkout이라 worktree 격리가 없다. 편집·격리는 team이 아니라 `isolation:"worktree"` subagent로, team은 조율 전용이다.

자세한 판단 기준과 prompt 작성법은 `references/delegation-patterns.md`.

---

## References (필요할 때만 로드)

메인 컨텍스트 절약을 위해 아래 파일은 **명시적으로 필요한 단계에서만** Read한다.

| 파일 | 언제 읽을지 |
|------|-------------|
| `references/delegation-patterns.md` | 위임 형태(단발 vs team)를 결정하거나 sub-agent prompt를 작성할 때 |
| `references/worktree-lifecycle.md` | 병렬 dispatch 직전, 또는 worktree 정리/머지를 다룰 때 |
| `references/agent-monitor.md` | 백그라운드 agent 진행 추적, 또는 Task 시스템으로 다중 작업 상태·의존성을 추적할 때 |
| `references/merge-coordinator.md` | 병렬 결과를 통합할 때 (순서 결정, 충돌 처리) |
| `references/autonomous-driving.md` | 자율 루프(분해→위임→머지 self-drive)를 돌릴 때 — **오케스트레이터 기본 동작**. 계약·가드레일·종료 조건·에스컬레이션 (단발 fan-out 1회면 불필요) |
| `references/spec-driven-review.md` | 자율주행이 **spec 문서를 입력으로 구현**할 때 — 팀 모드로 검토자(spec↔구현)·QA 매니저(spec↔테스트)를 상주시켜 worktree 코드를 계속 리뷰·개선하는 게이트 (spec 입력이 없으면 불필요) |

---

## 빠른 참조: 의사코드

### 병렬 fan-out (worktree-isolated)

```
# 0. 진입 확인
assert current_branch.startswith("epic/")
assert in_main_working_tree()

# 1. 분해 + 위험도 분석
files_A = analyze_changes(task_A)
files_B = analyze_changes(task_B)
assert disjoint(files_A, files_B)

# 2. 병렬 dispatch (worktree base = 현재 epic 브랜치)
Agent({description: "task A", subagent_type: "general-purpose",
       isolation: "worktree", run_in_background: true,
       prompt: "<자기완결적 컨텍스트 + task A + base=epic/<name>>"})
Agent({description: "task B", subagent_type: "general-purpose",
       isolation: "worktree", run_in_background: true,
       prompt: "<자기완결적 컨텍스트 + task B + base=epic/<name>>"})

# 3. 완료 알림 수신 후 결과를 epic 브랜치로 머지
# (merge-coordinator.md 참조)
```

### 순차 (의존성 있음)

```
result_A = Agent({description: "task A", prompt: "..."})
# A 결과를 B 입력으로 전달
Agent({description: "task B", prompt: "<task B + A의 결과 요약>"})
```

### Agent team (조율 전용 — 실험 플래그 `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1` 필요)

```
# team은 공유 checkout — 편집 격리 없음. 편집은 isolated subagent에 위임.
Agent({name: "reviewer", run_in_background: true,        # read-only 조율/리뷰
       prompt: "<epic diff 검토, 편집 X>"})
Agent({name: "implementer", run_in_background: true,     # 편집은 직접 X
       prompt: "<실제 편집은 Agent({isolation:'worktree'}) subagent로 위임>"})
# 진행 중 개입 (name으로 식별, team_name은 무시됨)
SendMessage({to: "reviewer", message: "..."})
```

---

## 사용자 보고 원칙

오케스트레이터는 **기본적으로 자율 주행**한다 — 진입 시 자율 계약을 1회 보고하고, 가드레일(종료 조건·예산·자동 중단) 안에서 자동 재위임·머지·충돌 해결을 사람 개입 없이 진행한다. 자율 계약·루프·에스컬레이션 규칙은 `references/autonomous-driving.md` 가 단일 소유한다.

- **시작 시**: 분해된 작업 목록 + 병렬/순차 결정 + 자율 계약(종료 조건·예산·hard stop·결정 기록 위치)을 한 번에 보고
- **진행 중**: 침묵 (정상 루프는 보고하지 않음) — hard stop / 에스컬레이션 발생 시에만 즉시 보고
- **종료 시**: 종료 사유(완료/예산 소진/에스컬레이션) + 머지 결과 + 미해결 항목 + 의사결정 요약

단, 에스컬레이션 조건(되돌리기 어려운 행위·토폴로지 위반·도메인 의미 충돌·예산 소진 등)은 자율 모드라도 **항상** 멈추고 보고한다 (`references/autonomous-driving.md §에스컬레이션`).

**opt-out — 휴먼-인-더-루프**: 사용자가 단계별 확인을 명시하면(예: "확인받으면서", "단계마다 물어봐", "babysit", "자동으로 머지하지 마") 자율 주행을 끄고 휴먼-인-더-루프로 전환한다. 이때는 자동 개입(SendMessage 명령 주입·자동 머지·자동 충돌 해결)을 하지 않고, 정체·실패·머지 결정을 사용자에게 보고하고 결정을 받는다 (`agent-monitor.md` / `merge-coordinator.md` 의 HITL 규칙).

---

## 안티패턴

1. **편집권 회수**: sub-agent가 실패하면 메인이 직접 Edit로 마무리 → 금지. 다시 위임하거나 사용자에게 보고.
2. **충돌 위험 무시한 병렬화**: 시간 단축에 끌려 disjoint 검증 없이 병렬 → 머지 지옥. 의심스러우면 순차.
3. **컨텍스트 의존 prompt**: "위에서 말한 그 파일을" 같은 prompt → sub-agent는 메인 대화를 못 봄. 자기완결적으로 작성.
4. **Reference 일괄 로드**: 시작하자마자 4개 reference를 모두 Read → 컨텍스트 낭비. 단계별로 필요할 때만.
5. **무한 폴링**: `Bash sleep` 루프로 agent 상태 확인 → 금지. `run_in_background: true` + 완료 알림 사용.
6. **메인이 worktree에서 시작**: 메인을 worktree에 진입시킨 채 오케스트레이션 → 머지 경로 꼬임. 메인은 epic 브랜치의 메인 working tree에서만 동작.
7. **epic 브랜치 우회**: main 또는 임의 feature 브랜치에서 sub-agent를 바로 dispatch → 결과를 어디로 모을지 모호. 반드시 epic 브랜치를 만들고 거기서 dispatch.
