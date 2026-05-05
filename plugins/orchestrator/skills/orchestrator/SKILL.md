---
name: orchestrator
description: 메인 에이전트가 직접 코드를 편집하지 않고 sub-agent/agent-team/worktree에 위임해 복합 작업을 관리하는 오케스트레이터 모드. 독립적이거나 병렬화 가능한 복수의 작업을 분해·위임·추적·통합해야 할 때, 또는 장기 진행 작업에 식별/제어가 필요한 agent team을 다룰 때 사용. 메인이 Edit/Write로 직접 코드를 수정하려 할 때도 이 스킬을 먼저 검토하여 위임으로 전환할지 판단한다.
version: 0.1.0
---

# Orchestrator Skill

## 사고 모드 (Mental Model)

이 스킬을 트리거한 순간부터 메인 에이전트는 **편집자가 아니라 관리자**다.

```
❌ 메인이 Edit/Write로 코드 작성
✅ 메인은 Read/Bash로 상태 파악 + Agent로 위임 + SendMessage로 조율
```

### 메인 에이전트가 해도 되는 일
- `Read`, `Glob`, `Grep`, `Bash(git status / git log / git diff --stat)` — 작업 분해와 위험도 판단을 위한 조사
- `Agent`, `TeamCreate`, `SendMessage`, `Monitor` — 위임과 조율
- 결과물 취합 후 사용자에게 보고

### 메인 에이전트가 하면 안 되는 일
- `Edit`, `Write`, `NotebookEdit` — 코드 편집은 항상 sub-agent에 위임
- 코드 작성을 직접 수행 (sub-agent 실패 시에도 편집권을 가져오지 않음 → 사용자에게 보고)

---

## 표준 절차 (Workflow)

```
1. 분해 (Decompose)        → 작업을 독립 단위로 쪼갠다
2. 위험도 분석 (Analyze)    → 단위 간 파일/의존성 충돌 위험 식별
3. 실행 계획 (Plan)         → 병렬/순차 결정 + 위임 형태(단발/team) 결정
4. 위임 (Dispatch)          → Agent 호출, 필요 시 worktree isolation
5. 모니터링 (Monitor)       → 진행 추적, 정체 감지, 사용자 보고
6. 머지 조정 (Coordinate)   → 결과 통합 순서 결정 + 충돌 위임 + worktree 정리
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
| 여러 agent 협업 또는 장기 진행, 식별/제어 필요 | agent team | `TeamCreate` + `Agent({team_name, name})` + `SendMessage` |
| 파일 충돌 위험 있는 병렬 | worktree-isolated | `Agent({isolation: "worktree", ...})` |

자세한 판단 기준과 prompt 작성법은 `references/delegation-patterns.md`.

---

## References (필요할 때만 로드)

메인 컨텍스트 절약을 위해 아래 파일은 **명시적으로 필요한 단계에서만** Read한다.

| 파일 | 언제 읽을지 |
|------|-------------|
| `references/delegation-patterns.md` | 위임 형태(단발 vs team)를 결정하거나 sub-agent prompt를 작성할 때 |
| `references/worktree-lifecycle.md` | 병렬 dispatch 직전, 또는 worktree 정리/머지를 다룰 때 |
| `references/agent-monitor.md` | 백그라운드 agent를 띄웠고 진행 상황을 추적해야 할 때 |
| `references/merge-coordinator.md` | 병렬 결과를 통합할 때 (순서 결정, 충돌 처리) |

---

## 빠른 참조: 의사코드

### 병렬 fan-out (worktree-isolated)

```
# 1. 분해 + 위험도 분석
files_A = analyze_changes(task_A)
files_B = analyze_changes(task_B)
assert disjoint(files_A, files_B)

# 2. 병렬 dispatch
Agent({description: "task A", subagent_type: "general-purpose",
       isolation: "worktree", run_in_background: true,
       prompt: "<자기완결적 컨텍스트 + task A>"})
Agent({description: "task B", subagent_type: "general-purpose",
       isolation: "worktree", run_in_background: true,
       prompt: "<자기완결적 컨텍스트 + task B>"})

# 3. 완료 알림 수신 후 머지 조정
# (merge-coordinator.md 참조)
```

### 순차 (의존성 있음)

```
result_A = Agent({description: "task A", prompt: "..."})
# A 결과를 B 입력으로 전달
Agent({description: "task B", prompt: "<task B + A의 결과 요약>"})
```

### Agent team

```
TeamCreate({name: "feature-x"})
Agent({team_name: "feature-x", name: "designer", run_in_background: true, ...})
Agent({team_name: "feature-x", name: "implementer", run_in_background: true, ...})
# 진행 중 개입
SendMessage({to: "designer", message: "..."})
```

---

## 사용자 보고 원칙

- **시작 시**: 분해된 작업 목록 + 병렬/순차 결정 + 그 이유를 한 번에 보고
- **진행 중**: 정체/실패 감지 시에만 보고 (정상 진행은 침묵)
- **종료 시**: 머지된 결과, 미머지 항목, 사용자 결정이 필요한 충돌 요약

자동 개입(SendMessage 등으로 agent에 명령 주입)은 **하지 않는다**. 정체나 실패는 사용자에게 보고하고 결정을 받는다.

---

## 안티패턴

1. **편집권 회수**: sub-agent가 실패하면 메인이 직접 Edit로 마무리 → 금지. 다시 위임하거나 사용자에게 보고.
2. **충돌 위험 무시한 병렬화**: 시간 단축에 끌려 disjoint 검증 없이 병렬 → 머지 지옥. 의심스러우면 순차.
3. **컨텍스트 의존 prompt**: "위에서 말한 그 파일을" 같은 prompt → sub-agent는 메인 대화를 못 봄. 자기완결적으로 작성.
4. **Reference 일괄 로드**: 시작하자마자 4개 reference를 모두 Read → 컨텍스트 낭비. 단계별로 필요할 때만.
5. **무한 폴링**: `Bash sleep` 루프로 agent 상태 확인 → 금지. `run_in_background: true` + 완료 알림 사용.
