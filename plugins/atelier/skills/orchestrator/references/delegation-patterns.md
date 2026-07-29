---
name: delegation-patterns
description: 단발 sub-agent vs agent team 결정과 자기완결 prompt 작성 패턴. orchestrator 스킬 내부 참조 문서.
version: 0.1.0
user-invocable: false
---

# Delegation Patterns

위임 형태 결정과 prompt 작성을 다룬다. 메인 에이전트가 sub-agent 또는 agent team에 작업을 넘길 때 참조.

## 단발 sub-agent vs agent team

### 단발 sub-agent (`Agent({...})` 한 번 호출)

**적합한 상황**:
- 결과물이 단일 (코드 변경, 리뷰 보고서, 분석 요약 등)
- 작업이 독립적이고 외부 개입 없이 끝남
- 한 번의 prompt → 한 번의 결과

### Agent team (`Agent`의 `name` 파라미터 — 가용 시)

**적합한 상황**:
- 여러 agent가 같은 작업 컨텍스트를 공유 (한 feature를 여러 역할로 협업)
- 진행 중 식별/제어가 필요 (이름으로 SendMessage)
- 장기 작업 → 중간에 사용자 결정을 주입할 수 있어야 함
- 결과물이 여러 단계로 누적

### 결정 트리

```
작업이 단순 1회성이고 결과가 단일?
  Yes → 단발 sub-agent
  No  → 진행 중 개입(SendMessage)이 필요한가?
          Yes → agent team
          No  → 단발 sub-agent (병렬 fan-out도 단발 여러 개)
```

> **review→fix 반복이 예상되면 team으로 조율**(team 가용 시 — 선호 등급): 구현 → 리뷰 → 수정처럼 한 작업이 여러 라운드를 도는 경우, 매 라운드를 단발로 재위임하면 컨텍스트 손실·셋업 비용이 반복된다. reviewer teammate + implementer teammate를 한 team에 두고 내부 SendMessage로 수정 사이클을 돌리되, **실제 파일 편집은 implementer가 직접 하지 않고 `isolation:"worktree"` subagent에 위임**한다 (team은 공유 checkout이라 편집 격리가 없다). team이 비가용이면 단발 subagent 재위임(실패 맥락 포함)으로 반복한다 — `autonomous-driving.md §위임 형태` 참조.

---

## Prompt 작성 원칙

sub-agent는 **메인 대화 히스토리를 보지 못한다**. prompt는 자기완결적이어야 한다.

### 필수 포함 요소

1. **목적**: 무엇을 달성해야 하는가
2. **컨텍스트**: 작업 배경, 관련 파일 경로 (전체 경로), 이미 알려진 제약
3. **브랜치**: base epic 브랜치 이름 (sub-agent는 자기 worktree만 보지 메인 대화의 epic 브랜치를 모름)
4. **범위**: 무엇을 하고 무엇을 하지 말 것
5. **출력 형식**: 결과를 어떤 형태로 돌려줄지 (파일 변경? 요약? JSON?)
6. **검증 기준**: 완료를 어떻게 확인할지 (테스트, 빌드, 특정 체크 등)
7. **worktree 격리 준수** (`isolation: "worktree"` dispatch 시 필수): 모든 Edit/Write의 file_path와 Bash cwd가 자기 worktree 경로(`.claude/worktrees/agent-...`) 안인지 매 호출 전 검증하고, 부모 repo(메인 working tree)의 파일을 절대 직접 수정하지 말 것. 부모 repo에 의도치 않은 변경을 만들었음을 발견하면 직접 reset/checkout 하지 말고 변경을 stash로 보존한 뒤 보고할 것.
8. **재위임 금지**: "이 작업을 다른 agent에게 재위임하지 말고 직접 수행하라. 막히면 실패 사유와 함께 종료하라"를 prompt에 포함할 것 (단, team teammate의 편집 격리용 isolated subagent 위임 1단계는 예외 — §위임 깊이 제한 참조).

### 안티패턴

```
❌ "위에서 말한 파일을 수정해줘"
```

**Edit 절대경로 트랩**: worktree 격리 sub-agent라도 Edit tool의 file_path가 부모 repo의 절대경로를 가리키면 격리를 우회해 메인 working tree가 직접 수정된다. Bash cwd가 worktree여도 Edit는 별개 경로 판정이므로, prompt에 worktree 격리 준수(위 7번)를 반드시 명시한다. 부모 repo가 변형되면 메인 branch switch까지 이어질 수 있다.

---

## isolation 결정

| 옵션 | 사용 시점 |
|------|-----------|
| 없음 (기본) | 읽기 전용 분석 sub-agent (epic 브랜치 자체에서 실행, 편집 X) |
| `isolation: "worktree"` | 코드를 변경하는 모든 sub-agent — epic 브랜치를 base로 한 worktree에서 작업 |

오케스트레이터 토폴로지에서는 **편집하는 sub-agent는 항상 `isolation: "worktree"`** 다. 메인이 epic 브랜치를 점유하고 있으므로 같은 working tree에서 sub-agent가 편집하면 메인 상태가 오염된다. isolation worktree는 변경이 없으면 자동 정리되고, 변경이 있으면 worktree 경로와 브랜치명이 결과에 포함된다. 자세한 머지/정리는 `worktree-lifecycle.md`.

이 표는 **단발 subagent에만 적용**된다. agent team teammate는 공유 checkout이라 `isolation` 인자로 격리되지 않으므로(아래 §Agent team 사용 패턴), 편집·격리가 필요하면 teammate가 아니라 isolated subagent를 쓴다.

---

## 위임 깊이 제한 (flat delegation)

위임 계층은 **오케스트레이터 → 작업 agent**, 그리고 team 모드에 한해 **teammate → 편집 격리용 isolated subagent 1단계**까지만 허용한다. 후자는 아래 §Agent team 사용 패턴이 의도한 설계다(teammate는 공유 checkout이라 편집을 직접 하지 않고 isolated subagent에 위임) — 새 제약이 아니라 기존 패턴의 상한을 명시한 것이다.

**작업 agent는 문제 해결을 다른 agent에게 재위임하지 않는다.** 막히면 또 다른 agent를 spawn하지 말고 실패 사유와 함께 종료한다. 재위임 여부 판단은 오케스트레이터의 몫이다 (`agent-monitor.md §재위임 판단 기준` 참조).

근거: 구현 agent가 스스로 중첩 재위임 체인을 만들면 오케스트레이터가 위임 트리 전체를 파악하지 못해 상태 추적·개입 경로를 잃는다. 위임 깊이를 평평하게 유지해야 오케스트레이터가 각 agent를 직접 추적·중단할 수 있다.

---

## 계획 우선 게이트 (plan-first)

리스크가 크거나 되돌리기 어려운 편집(스키마 변경, 광범위 리팩토링, 공개 API 변경 등)은 곧장 편집을 위임하지 말고 **계획을 먼저 받아 승인한 뒤 편집을 재위임**한다. worktree를 다 고쳐놓고 방향이 틀렸음을 발견하는 비용을 막는 사전 게이트다.

```
# 1. 계획만 (편집 금지) — read-only 분석 subagent
plan = Agent({
  description: "auth 리팩토링 계획",
  prompt: "<목표/제약>. 구현하지 말고 변경 계획만 반환하라:
           바꿀 파일·함수, 순서, 회귀 위험, 검증 방법.",
})

# 2. 메인(또는 자율 모드 가드)이 계획 검토 → 승인 / 수정 요청

# 3. 승인된 계획을 input으로 편집을 격리 subagent에 재위임
Agent({
  description: "auth 리팩토링 구현",
  isolation: "worktree", run_in_background: true,
  prompt: "<승인된 계획 요약>을 그대로 구현하라. 계획을 벗어나면 중단하고 보고.",
})
```

- **도구 의존 없음**: `mode:"plan"` 같은 파라미터에 의존하지 않으므로 런타임 차이와 무관하게 동작한다. (team이 가용하면 teammate의 plan 승인 기능으로 대체 가능 — 단 보장 경로는 위 2-스텝 패턴이다.)
- **자율 모드 연계**: 리스크 큰 작업의 사전 게이트로, 계획 승인 실패는 hard stop / 에스컬레이션으로 처리한다 (`autonomous-driving.md`).
- 단순·저위험 작업엔 쓰지 않는다 (왕복 비용만 늘어남) — isolation·Monitor와 같은 절제 원칙.

---

## Agent team 사용 패턴

> **전제**: agent team은 실험 기능이라 세션마다 가용 여부가 다르다. **가용 판정의 권위 신호는 `Agent` 도구 스키마에 `name` 파라미터가 노출되는지**이며, `printenv CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS`는 보조 신호다(Bash는 메인과 다른 프로세스라 단독 근거가 못 된다) — 판정 절차는 `SKILL.md §진입 시 체크 4`, 경로별 강제 등급은 `SKILL.md §team mode 강제 등급`이 단일 출처다. 과거의 `TeamCreate`/`TeamDelete` 도구는 제거됐고, `Agent`의 `team_name` 인자는 받지만 무시된다 — 세션마다 암묵적 team 하나가 있고 `name`으로 바로 spawn하며, session 종료 시 자동 정리된다.
>
> **`name`을 넘겼다고 teammate로 떴다는 보장은 없다** — `team_name`이 조용히 무시되는 선례가 이미 있다. team 필수 등급 경로는 첫 spawn 직후 `SendMessage`로 도달 가능한 식별자인지 **spawn 확인**을 실행한다 (`advisory-consult.md §spawn 확인`).

> **team은 공유 checkout이다 — per-teammate worktree 격리가 없다.** 같은 파일을 두 teammate가 편집하면 덮어쓴다. 따라서 **편집·격리가 필요한 작업은 teammate가 직접 하지 않고 isolated subagent에 위임**한다. team은 read-only 조율/리뷰만 맡는다.

```
# 조율 전용 teammate (read-only). 편집은 nested isolated subagent로.
Agent({
  name: "reviewer",
  run_in_background: true,
  description: "Auth review",
  prompt: "<epic 브랜치 diff를 검토. 편집하지 말 것.>"
})

Agent({
  name: "implementer",
  run_in_background: true,
  description: "Auth implementation (조율)",
  prompt: "<설계 입력을 받아, 실제 편집은 Agent({isolation:'worktree'})
           단발 subagent로 위임하라. 이 teammate 자신은 공유 checkout을
           직접 편집하지 말 것.>"
})

# 중간 개입 (name으로 식별)
SendMessage({to: "implementer", message: "<우선순위 변경 또는 수정 지시>"})
```

### Team 사용 시 주의

- `name`이 식별자다. 세션 내에서 유니크해야 한다 (`team_name`은 무시되므로 쓰지 않는다).
- `run_in_background: true`로 띄워야 SendMessage로 개입할 수 있다.
- team은 session 종료 시 **자동 정리**된다 (`TeamDelete` 없음). 별도 정리 단계 불필요.
- **편집 격리는 team이 아니라 subagent의 `isolation:"worktree"`가 보장한다.** teammate에게 worktree 이동을 위임하지 말 것 — 격리가 도구 보장에서 프롬프트 희망으로 격하되어 공유 checkout(메인 epic 브랜치)이 오염될 수 있다.
- **teammate의 권한은 도구로 제한할 수 없다 — 계약 + 사후 탐지로만 관리한다.** 별도 agent 정의를 지정해 tools를 좁히는 경로(`subagent_type`)는 **단발 subagent의 것**이고, team spawn과 결합한 선례가 없다. 따라서 read-only teammate라도 실제로는 편집·`SendMessage`가 가능하다. 금지는 prompt에 계약으로 명시하고, 위반은 **토폴로지 가드로 탐지**한다(사전 차단 아님 — `merge-coordinator.md §토폴로지 가드`). 도구 수준 보장이 꼭 필요한 역할이면 team이 아니라 단발 subagent를 고른다.

---

## 모델 선택

`Agent` 호출 시 `model` 옵션으로 sub-agent 모델을 지정한다. 역할 기준 원칙(오케스트레이터는 위임 sub-agent보다 낮은 tier로 내려가지 않는다 / 위임 dispatch에는 항상 `model` 명시 / 특정 모델명을 문서에 박지 않는다)은 **`SKILL.md §모델 라우팅 전략`이 단일 출처**다 — 여기서 재서술하지 않는다.

작업 유형 → 시작 tier 표는 **이 절이 단일 출처**다:

| 작업 유형 | 시작 tier (역량 수준) |
|-----------|----------------------|
| 요구사항 분해·설계 심문 (아키텍트 협의체 — `architect-council.md`), 복잡한 설계·어려운 디버깅·아키텍처 판단 | 최상위 |
| 일반 구현, 코드 리뷰, 테스트 작성 | 중간 |
| **리서치·조사(discovery)** — 코드베이스 파악, 영향 범위 수집, 자료 조사 | **경량**에서 시작. 판단이 섞이면(방안 비교, 설계 함의 해석) 중간 |
| 단순 분류, 포맷 변환, 짧은 추출 | 경량 |

- **discovery 를 최상위 tier 로 올리지 않는다.** 넓게 읽고 추려 오는 일은 역량보다 **범위와 병렬성**이 결과를 좌우한다 — 같은 비용이면 상위 tier 하나보다 경량 여러 갈래가 더 넓게 훑는다. 조사 결과를 **해석**하는 것은 메인의 몫이고, 거기서 판단이 막히면 그때 별도 경로(자문 조회)를 쓴다.
- **역량 수준 ↔ 실제 모델명 매핑은 dispatch 시점 판단**이다. 세대가 바뀌면 같은 작업이 더 가벼운 tier로 내려갈 수 있어야 하므로 문서에 모델명을 고정하지 않는다.
- 이 표는 고정값이 아니라 **시작 heuristic**이다 — 작업마다 "지금도 이 역량이 필요한가"를 재평가하고, 벗어난 선택은 근거와 함께 decision log에 남긴다. 자율 루프에서의 배분 원칙은 `autonomous-driving.md §모델 분배` 참조.
- 단순 작업에 최상위 tier를 쓰는 것은 비용 낭비다.
- 이 표는 **집행 위임**(코드·문서를 만드는 sub-agent)의 tier heuristic이다. 메인보다 상위 tier에 의견만 구하는 **자문 조회**는 이 표 밖의 별도 경로다 — 원칙은 `SKILL.md §자문 조회`, 절차는 `advisory-consult.md`가 단일 출처다.

---

## 체크리스트

위임 직전 확인:

- [ ] prompt가 메인 대화 없이도 이해 가능한가? (자기완결성)
- [ ] prompt에 base epic 브랜치 이름이 포함되었는가?
- [ ] 출력 형식과 검증 기준이 명시되었는가?
- [ ] 단발/team 선택이 작업 성격과 맞는가?
- [ ] 편집하는 sub-agent라면 `isolation: "worktree"`를 켰는가?
- [ ] worktree dispatch라면 prompt에 worktree 격리 준수(경로 prefix 검증 + 부모 repo 수정 금지)를 명시했는가?
- [ ] prompt에 재위임 금지 문구(§위임 깊이 제한)를 포함했는가? (team teammate → isolated subagent 편집 위임 1단계는 예외)
- [ ] 모델 선택이 작업 난이도와 맞는가? (dispatch에 `model`을 명시했는가 — 상속 금지)
- [ ] 사용자가 준 역할별 모델 제약이 있으면 반영했는가? (제약으로 막히면 인접 tier 대체 + decision log)
- [ ] 이 dispatch가 **집행 위임**(= 자문 4트리거가 아닌 전부, 리서치·조사 포함)이라면 tier가 메인을 넘지 않는가? (상위 tier는 자문 조회만 예외 — `SKILL.md §자문 조회`)
- [ ] 역할을 지목한 사용자 제약(예: "자문은 X 모델")을 **그 역할 밖으로 번지게** 하지 않았는가?
- [ ] read-only teammate라면 금지를 prompt 계약으로 명시하고, 위반 탐지를 토폴로지 가드에 맡겼는가? (도구 제한은 team에서 불가)
- [ ] team의 경우 name이 의미 있고 유니크한가?
