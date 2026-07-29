---
name: orchestrator
description: Use this skill when delegating work to multiple sub-agents, agent-teams, or worktrees — parallel fan-out, sequential pipelines, long-running agent teams, document deliverables (writing reports, specs, or write-up docs is also delegated Write work), or any moment the main agent is about to use Edit/Write directly (delegate instead). Triggers include "여러 작업 병렬로", "동시에 처리", "에이전트 나눠서", "worktree로 분리", "위임해서", "팀으로 작업", "리포트 작성", "보고서로 정리", "스펙 문서 작성", "분석 결과 문서화", "delegate", "parallel agents", "fan-out", "agent team", "sub-agent", "dispatch multiple", "split into tasks", "run in parallel", "write a report", "draft a spec", "write up findings".
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
- **문서 산출물 작업** — 리포트·스펙·정리 문서 작성 요청 ("리포트로 정리해줘", "스펙 문서 만들어줘", "분석 보고서 작성") — 문서 작성도 Write 작업이므로 위임 대상
- **메인 에이전트가 Edit/Write/NotebookEdit로 직접 코드를 수정하려는 모든 순간** — 위임으로 전환할지 먼저 검토

트리거하면 안 되는 상황:
- 단일 파일의 단순 편집 (오버헤드만 늘어남)
- 사용자가 직접 메인이 처리하라고 명시한 경우
- 1턴 안에 끝나는 read-only 조사

## 사고 모드 (Mental Model)

이 스킬을 트리거한 순간부터 메인 에이전트는 **편집자가 아니라 관리자**다 — Edit/Write로 직접 코드를 작성하지 않고, Read/Bash로 상태를 파악하고 Task로 일감을 분리·관리하며 Agent로 위임하고 SendMessage로 조율한다.

### 메인 에이전트가 해도 되는 일
- `Read`, `Glob`, `Grep`, `Bash(git status / git log / git diff --stat)` — 작업 분해와 위험도 판단을 위한 조사
- `Agent`, `SendMessage`, `Monitor` — 위임과 조율 (agent team은 `Agent`의 `name`으로 spawn — 가용 판정은 아래 §진입 시 체크 4, `TeamCreate`는 제거됨)
- `TaskCreate` / `TaskList` / `TaskGet` / `TaskUpdate` — 일감을 분리하고 상태를 관리하는 것은 **메인 에이전트의 핵심 룰**이다. 편집을 위임하는 관리자로서 메인의 본업은 일감을 추적 가능한 Task로 쪼개고 상태를 갱신하는 것 — 다중 작업이면 항상 적용하고 단발 1회만 예외다 (`references/agent-monitor.md §Task 시스템`)
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
4. **agent team이 이번 세션에서 가용한가?** — 판정 신호는 둘이고 **우선순위가 있다**:
   1. **1차 (권위): `Agent` 도구 스키마에 `name` 파라미터가 노출되는가.** teammate spawn을 실제로 결정하는 것은 런타임이 노출한 스키마이므로 이쪽이 사실이다.
   2. **2차 (보조): `printenv CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS`** (read-only Bash). Bash 도구는 메인과 다른 프로세스로 뜨므로 메인 프로세스의 env를 반영한다는 보장이 없다 — **단독 판정 근거로 쓰지 않는다**.
   - **둘이 엇갈리면 1차(스키마)를 따른다.** env가 비어 있어도 스키마에 `name`이 있으면 **가용**이다 — 이 false negative가 team 전제 경로 전체를 조용히 죽이는 가장 흔한 실패다.
   - 판정 결과와 **어느 신호로 판정했는지**를 진입 보고 1줄 + decision log에 남긴다. 근거 없는 "플래그 off"는 판정이 아니다.
   - 이 한 번의 확인으로 team이 전제인 경로들의 가용성을 **진입 시 확정**한다 (트리거 시점에 확인하면 이미 다른 경로를 다 태운 뒤라 늦다). 단, 필수 등급 경로의 **첫 spawn이 teammate로 뜨지 않으면 1회 재판정**한다 (아래 §team mode 강제 등급).
   - **off 확정 시의 반응은 경로마다 다르다** — 아래 §team mode 강제 등급이 단일 출처다.

---

## 표준 절차 (Workflow)

```
0. 진입 확인 (Entry)        → 현재가 epic 브랜치 + 메인 working tree인지 확인
1. 분해 (Decompose)        → 작업을 독립 단위로 쪼갠다 — 복잡·모호한 요구는 아키텍트 협의체
                             (설계 생성 → 별도 agent 의 심문·검증)에 위임해 검증된 task 를 도출
                             (`references/architect-council.md`)
2. 위험도 분석 (Analyze)    → 단위 간 파일/의존성 충돌 위험 식별
3. 실행 계획 (Plan)         → 병렬/순차 결정 + 위임 형태(단발/team) 결정
4. 위임 (Dispatch)          → Agent 호출 (worktree isolation, base = epic 브랜치)
5. 모니터링 (Monitor)       → 진행 추적, 정체 감지, 사용자 보고
6. 검토·QA 게이트 (Gate)    → 작업마다 검토 에이전트 + QA 에이전트(검증 테스트 추가) 필수 + DB 접촉 작업은
                             DBA 에이전트 추가, 전부 pass여야 머지
7. 머지 조정 (Coordinate)   → 게이트 통과분만 epic 브랜치로 통합 + 충돌 위임 + worktree 정리
8. 보고 (Report)            → 사용자에게 결과 요약
```

각 단계의 상세 패턴은 아래 references에 있다.

### 일감을 Task로 분리·관리하는 것은 메인의 핵심 룰

메인 에이전트는 편집자가 아니라 **관리자**다(위 *사고 모드*). 관리자의 본업은 일감을 추적 가능한 단위로 쪼개고 그 상태를 끝까지 관리하는 것이다. 따라서 1단계에서 분해한 일감 목록을 메인의 머릿속이나 대화 흐름에만 두지 말고 **Task 시스템(`TaskCreate`/`TaskList`/`TaskGet`/`TaskUpdate`)으로 분리·등록·갱신**하는 것은 선택이 아니라 메인의 핵심 룰이다. 일감이 목록이 아니라 추적 가능한 상태 객체가 되어야 메인과 사용자 모두 진행 상황을 한눈에 본다.

- **분리**: 분해된 각 독립 단위 = Task 1개. dispatch 전에 등록해 "무엇을 할 것인가"를 먼저 가시화한다.
- **상태 추적**: dispatch 시 `in_progress`, 머지/완료 시 `completed`로 갱신해 진행률이 목록에 드러나게 한다.
- **의존성·소유자**: 순차 의존이 있으면 Task의 의존성(blocked-by)으로, 어느 worktree/agent가 맡았는지는 owner로 표기해 병렬 상태를 추적한다.
- **적용 범위**: 다중 작업이거나 의존성이 있으면 **항상** 적용한다. 단발 1회 작업만 오버헤드라 예외로 생략한다.

상세 사용법(필드·의존성·owner)은 `references/agent-monitor.md §Task 시스템`이 단일 출처다. 자율 모드에서의 Task 추적 규칙은 `references/autonomous-driving.md`를 따른다.

### 작업 케이스마다 검토 에이전트·QA 에이전트는 필수 (Review & QA Gate)

코드를 바꾸는 각 작업(work case)은 구현이 끝나면 머지 전에 전용 게이트 에이전트를 **반드시** 거친다 — Task 분리가 핵심 룰인 것과 동급의 필수 규칙이다. 게이트가 references에 묻혀 누락되지 않도록, **적용 여부**만 본문에 항상 로드되는 규칙으로 둔다.

- **검토 에이전트 (reviewer)** — `구현 ↔ 요구사항`, **QA 에이전트 (qa)** — `요구사항 ↔ 테스트`(누락 시 검증 테스트를 추가·보강), **DBA 에이전트 (dba)** — DB 접촉 작업만 조건부로 `구현 ↔ DB 안전성`.
- **AND 게이트**: 전부 `pass`여야 머지 후보로 승급. 하나라도 `reject`면 findings를 실어 재위임한다.
- 게이트 에이전트들은 **구현 sub-agent와 다른 agent**다 — 자기 코드 자기 검증 금지.
- QA의 테스트 추가도 편집이므로 **`isolation:"worktree"` subagent로 위임**한다 (메인은 직접 편집하지 않는다 — *사고 모드*).
- 게이트 역할의 tier는 아래 §모델 라우팅 전략에 따라 dispatch 시점에 정한다 — 자동 머지의 유일한 안전장치라 보통 더 높은 역량을 둘 가치가 있다.
- 예외는 Task 룰과 동일하게 **단발 1회·read-only 작업만**이다.

역할별 입력·검증 질문·출력 계약, DB 접촉 판정, 게이트 거부의 재위임 예산·기록 등 세부 규칙은 `references/autonomous-driving.md §리뷰어·QA 게이트`가 단일 출처다. spec 문서를 입력으로 구현하는 경우만 `references/spec-driven-review.md`(검토자=spec↔구현, QA 매니저=spec↔테스트)로 특수화된다.

### 병렬 fan-out 복원력 — 실패한 조각도 누락 없이 (Resilience)

대규모 fan-out(예: 15개 이상 agent)에서는 일부 agent가 504 Gateway Time-out·API 에러로 죽는 것을 정상 케이스로 전제한다. 실패가 조용히 누락되면 최종 취합 리포트의 수치 일관성이 깨진다.

- **체크포인트**: 각 agent 결과는 완료 즉시 파일로 저장한다 — 전체 완료를 기다리지 않는다 (저장 주체는 agent 자신 — 메인의 Edit/Write 금지 유지).
- **재시도**: gateway/API 에러로 실패한 agent는 같은 prompt로 N회(기본 3회)까지 재시도한다.
- **폴백**: 재시도 소진 시 해당 조각은 메인의 read-only 직접 분석 또는 조건을 바꾼 새 agent 재위임으로 대체해 취합을 완성한다 — 미완성 상태로 종료하지 않는다. 폴백도 편집이 필요하면 위임한다 (*사고 모드*).
- **투명 보고**: 최종 보고에 어떤 agent가 실패/재시도/폴백되었는지 명시한다 — "실패한 건도 누락 없이 보고" 원칙의 fan-out 구체화다.

상세 절차(체크포인트 파일 규약·재시도 예산·폴백 판단)는 `references/agent-monitor.md §fan-out 복원력`이 단일 출처다.

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
| 여러 agent 협업·식별/제어 필요 (read-only 조율) | agent team | `Agent({name, ...})` + `SendMessage` (가용 판정 §진입 시 체크 4·`team_name` 무시·편집 격리는 subagent) |
| 파일 충돌 위험 있는 병렬 | worktree-isolated | `Agent({isolation: "worktree", ...})` |

> **격리는 subagent만 보장**: agent team teammate는 공유 checkout이라 worktree 격리가 없다 — 편집·격리는 `isolation:"worktree"` subagent, team은 조율 전용 (`references/delegation-patterns.md §Agent team 사용 패턴`이 단일 출처).

자세한 판단 기준과 prompt 작성법은 `references/delegation-patterns.md`.

### team mode 강제 등급 (단일 출처)

team을 "쓰면 좋다"로 두면 폴백이 사실상 기본값이 되어 team 전제 경로가 조용히 사라진다. 경로별 **강제 등급을 2단계로 확정**하고, 등급별 반응을 이 절이 단일 소유한다.

**등급 기준**: 경로의 본질이 **read-only 조율(편집 없음)**이면 `필수`다 — team의 유일한 실질 비용인 공유 checkout 오염 위험이 애초에 없으므로 강제하지 못할 이유가 없고, 왕복 조율이라는 실익만 남는다. 반대로 **편집이 개입**하는 경로는 `선호`다 — 격리를 보장하는 것은 team이 아니라 `isolation:"worktree"` subagent이므로 단발 폴백이 본질을 잃지 않는다.

| 경로 | 등급 | team 가용인데 단발로 대체 | team 비가용 |
|------|------|---------------------------|-------------|
| 자문 조회 (`references/advisory-consult.md`) | **필수** | **위반** — 그 왕복은 자문이 아니다. 결과를 채택하지 않고 위반을 기록 | 자문 생략하고 원래 하려던 에스컬레이션으로 진행 |
| 아키텍트 협의체 (`references/architect-council.md`) | **필수** | **위반** — "협의체를 돌렸다"는 기록만 남는다 | 폴백 없이 즉시 에스컬레이션 |
| spec 검토·QA 게이트 (`references/spec-driven-review.md`) | 선호 | 허용 — 단발 subagent 2개로 두 검증 차원 유지 | 동일 폴백 |
| review→fix 루프 (`references/autonomous-driving.md`) | 선호 | 허용 — 단발 격리 subagent 재위임 | 동일 폴백 |

**필수 등급에는 가드 두 개가 붙는다** (없으면 강제가 아니라 권고다):

1. **spawn 확인** — 필수 경로의 첫 `Agent({name, ...})` 직후, 그 agent가 실제 teammate로 떴는지 확인한다(`SendMessage`로 도달 가능한 식별자인가). teammate가 아니면 **진입 판정을 1회 재판정**하고, 재판정도 비가용이면 폴백하지 말고 위 표의 "team 비가용" 열대로 처리한다.
2. **decision log 필수 필드** — 필수 경로의 결정 기록에 **`실행 형태`(teammate / subagent)와 `판정 근거`(스키마 / env)**를 반드시 남긴다 (`references/autonomous-driving.md §의사결정 기록`). 이 두 필드가 없으면 폴백 여부를 사후에 판별할 수 없어 감사 자체가 성립하지 않는다.

---

## 모델 라우팅 전략 (Model Routing)

모델 라우팅에는 **역할 기준 원칙**과 그 안에서의 **작업별 tier heuristic**이 있다. 원칙은 이 절이 단일 출처다.

### 역할 기준 원칙 (단일 출처)

- **오케스트레이터(메인)는 집행 위임되는 어떤 sub-agent보다 낮은 역량 tier로 내려가지 않는다.** 분해·위임·조율·머지 판단이 스웜 전체 결과의 상한을 결정하기 때문이다. 이 원칙은 **집행 위임**(코드·문서를 만드는 sub-agent)에 적용되며, 아래 §자문 조회가 유일한 예외다.
- **위임 dispatch에는 항상 `model`을 명시한다** — 상속에 맡기면 메인의 tier가 그대로 번져 배분 자체가 무의미해진다.
- **tier 선택은 고정 매핑이 아니라 heuristic**이다 — 작업의 난이도·리스크·되돌리기 비용에 맞춰 매 dispatch 재평가하고, 표준을 벗어난 선택은 근거와 함께 decision log에 남긴다.
- **특정 모델명을 문서에 박지 않는다** — 모델 세대가 바뀌어도 이 원칙이 그대로 성립해야 한다. 역량 수준(최상위/중간/경량)과 실제 모델명의 매핑은 dispatch 시점 판단에 맡긴다.

작업 유형 → 시작 tier 표는 `references/delegation-patterns.md §모델 선택`이 단일 출처다 (여기서 중복 정의하지 않는다). 자율 루프의 작업별 배분 원칙은 `references/autonomous-driving.md §모델 분배`.

### 자문 조회 — 상위 tier 예외 (단일 출처)

위임에는 두 종류가 있다. 위 역할 기준 원칙은 **집행 위임**의 규칙이고, **자문 조회**는 tier 방향이 반대인 별도 경로다.

| | 집행 위임 (executive) | 자문 조회 (advisory) |
|---|---|---|
| 산출물 | 코드·문서 (편집) | 권고 + 근거 (read-only) |
| 결정권 | sub-agent 결과를 메인이 머지 판단 | **메인에 100% 잔류** — advisor는 `pass`/`reject` 권한이 없다 |
| tier | 메인 ≥ sub-agent | **메인 < advisor 허용** (유일한 예외) |
| 실행 형태 | `isolation:"worktree"` subagent | **team member 전용** (단발 subagent 폴백 금지) |

예외가 원칙을 훼손하지 않는 이유: 역할 기준 원칙의 근거는 "메인의 판단이 스웜 결과의 **상한**을 결정한다"이고, 자문은 그 상한을 **올리는** 방향이기 때문이다. 결정권이 메인을 떠나는 순간 이 근거가 무너진다.

- **역할 제한은 계약이지 도구 보장이 아니다**: teammate는 도구 권한을 제한할 수단이 없다(agent 정의 지정은 단발 subagent의 경로다). 편집·`SendMessage`·재위임 금지는 **패킷의 금지 계약**으로 걸고, 위반은 **자문 소집 전후의 토폴로지 가드로 사후 탐지**한다. 왕복 조율(자문의 실질)과 도구 보장은 현재 동시에 얻을 수 없으며, 이 경로는 왕복을 택했다 — 없는 보장을 있다고 적지 않는다.

- **사용 조건**: 진입 시 확정한 team 가용 판정(위 §진입 시 체크 4). 자문은 **team 필수 등급**이다 — advisor는 편집하지 않는 read-only 역할이라 공유 checkout 비용이 없고 왕복 조율이라는 실익만 남기 때문이다(위 §team mode 강제 등급). 비가용이면 자문 경로는 사용 불가이며, 자문 없이 원래 에스컬레이션으로 진행한다. 가용인데 단발 subagent로 대체하는 것은 **위반**이다.
- **고정하는 것은 계약, 관점·인원은 런타임 주입**: 어떤 렌즈로 몇 명을 붙일지는 문제를 보고 메인이 정한다 — 관점을 나눠 **자문 스웜**으로 소집할 수 있다 (협의체와 같은 정책·메커니즘 분리).
- **상시 경로가 아니다**: 자문은 에스컬레이션 직전 한 단계다. 소집 트리거·패킷 계약·출력 계약·수명은 `references/advisory-consult.md`가 단일 출처다.

### 역할별 모델 제약 — 지시로 받고, 설정 파일을 만들지 않는다

"이 모델은 리뷰어로 쓰지 마라" 같은 제약은 **사용자 지시**로 받는다. 받은 제약은 런 내내 일관되게 적용하고 근거와 함께 decision log에 남긴다.

- **설정 파일 규약을 두지 않는다.** 파싱·검증하는 코드가 없는 설정 파일은 오타·스키마 위반을 조용히 삼켜 "걸린 줄 알았는데 안 걸린" 상태를 만든다. 강제력 없는 규약에 파일을 붙이면 강제력이 있는 것처럼 보이기만 한다.
- **세션을 넘겨 유지해야 하는 제약**은 이미 로드되는 문서(프로젝트 `CLAUDE.md`, `.claude/rules/*`)에 적는다 — 실제로 읽히는 경로에만 규약을 얹는다.
- **제약은 tier 선택 범위만 좁힌다**: 어떤 제약도 역할 기준 원칙을 뒤집지 못한다. 오케스트레이터가 집행 위임 sub-agent보다 낮은 tier로 내려가거나 `model` 미지정으로 상속하는 배정은 허용되지 않는다.
- 제약 때문에 표준 heuristic의 모델을 쓸 수 없으면 **인접 tier로 대체**한다 — 판정 품질이 게이트인 역할(reviewer/QA/DBA)은 인접 상위 우선, 그 외는 인접 하위 우선. 대체는 비표준 선택이므로 근거와 함께 기록한다.

---

## References (필요할 때만 로드)

메인 컨텍스트 절약을 위해 아래 파일은 **명시적으로 필요한 단계에서만** Read한다.

| 파일 | 언제 읽을지 |
|------|-------------|
| `references/architect-council.md` | 분해(1단계) 시 요구가 복잡·모호해 아키텍트 협의체(설계 생성 ↔ 심문 검증)로 분석·검증 후 task 를 도출할 때 |
| `references/delegation-patterns.md` | 위임 형태(단발 vs team)를 결정하거나 sub-agent prompt를 작성할 때 — **작업 유형 → tier 표의 단일 출처** (역할 기준 원칙·역할별 모델 제약은 위 §모델 라우팅 전략이 단일 출처) |
| `references/worktree-lifecycle.md` | 병렬 dispatch 직전, 또는 worktree 정리/머지를 다룰 때 |
| `references/agent-monitor.md` | 백그라운드 agent 진행 추적, Task 시스템으로 다중 작업 상태·의존성을 추적할 때, 또는 대규모 fan-out에서 실패 대비 복원력(체크포인트·재시도·폴백) 절차를 적용할 때 |
| `references/merge-coordinator.md` | 병렬 결과를 통합할 때 (순서 결정, 충돌 처리) |
| `references/autonomous-driving.md` | 자율 루프(분해→위임→머지 self-drive)를 돌릴 때 — **오케스트레이터 기본 동작**. 계약·가드레일·종료 조건·에스컬레이션 + **작업마다 필수인 리뷰어·QA 게이트**(검토 + 검증 테스트 추가)의 단일 출처 (단발 fan-out 1회면 불필요) |
| `references/advisory-consult.md` | 상위 tier 자문을 소집할 때 (협의체 예산 소진 tie-break, 게이트 재위임 루프, 되돌리기 어려운 결정, 사용자 요청) — **소집 트리거·패킷 계약·출력 계약·수명의 단일 출처** (집행/자문 분리와 tier 예외 원칙 자체는 위 §모델 라우팅 전략이 단일 출처). 진입 시 team 비가용으로 확정됐으면 읽을 필요 없다 |
| `references/spec-driven-review.md` | 검토·QA 게이트가 **spec 문서를 입력으로 구현**하는 경우의 특수화 — 팀 모드로 검토자(spec↔구현)·QA 매니저(spec↔테스트)를 상주시켜 worktree 코드를 계속 리뷰·개선 (spec 입력이 없으면 일반 게이트 사용) |

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
8. **자문 흉내**: 자문 경로가 비활성인데 단발 subagent 1회 왕복이나 메인 자신의 판단을 "자문 결과"로 포장 → 실질 없이 기록만 남는다. 경로가 없으면 없는 대로 진행하고, 그 시점의 판단은 **메인 자신의 판단으로 명시**한다. 필수 등급 경로의 폴백은 decision log의 `실행 형태` 필드로 사후 탐지된다 (§team mode 강제 등급).
9. **고무도장 메인**: 상위 tier라는 이유로 권고를 검토 없이 채택 → 실질 오케스트레이터가 advisor가 되고 메인은 전달자로 전락한다. 결정권은 메인에 있고, 채택도 기각도 사유와 함께 기록한다.
10. **env 한 줄로 team 비가용 단정**: `printenv`가 비었다는 이유만으로 team 경로를 닫음 → Bash는 메인과 다른 프로세스라 false negative가 구조적으로 발생하고, 판정 하나가 틀리면 team 전제 경로가 동시에 죽는다. 권위 있는 신호는 `Agent` 스키마의 `name`이다 (§진입 시 체크 4).
