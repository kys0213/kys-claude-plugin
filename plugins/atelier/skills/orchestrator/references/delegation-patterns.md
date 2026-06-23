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

**예시**:
- "이 파일에 테스트 추가해줘" → 단발
- "PR diff를 보안 관점에서 리뷰해줘" → 단발
- 병렬 fan-out의 각 worker → 단발 (각각 독립)

### Agent team (`Agent`의 `name` 파라미터 — 실험 플래그)

**적합한 상황**:
- 여러 agent가 같은 작업 컨텍스트를 공유 (한 feature를 여러 역할로 협업)
- 진행 중 식별/제어가 필요 (이름으로 SendMessage)
- 장기 작업 → 중간에 사용자 결정을 주입할 수 있어야 함
- 결과물이 여러 단계로 누적

**예시**:
- 한 feature를 designer + implementer + reviewer 역할로 분리 → team
- 장기 마이그레이션 — 진행 중 사용자가 우선순위 변경 가능 → team
- 병렬 작업이지만 서로 결과를 참고해야 할 때 → team (다만 동기화 비용 주의)

### 결정 트리

```
작업이 단순 1회성이고 결과가 단일?
  Yes → 단발 sub-agent
  No  → 진행 중 개입(SendMessage)이 필요한가?
          Yes → agent team
          No  → 단발 sub-agent (병렬 fan-out도 단발 여러 개)
```

> **review→fix 반복이 예상되면 team으로 조율**(실험 플래그 시): 구현 → 리뷰 → 수정처럼 한 작업이 여러 라운드를 도는 경우, 매 라운드를 단발로 재위임하면 컨텍스트 손실·셋업 비용이 반복된다. reviewer teammate + implementer teammate를 한 team에 두고 내부 SendMessage로 수정 사이클을 돌리되, **실제 파일 편집은 implementer가 직접 하지 않고 `isolation:"worktree"` subagent에 위임**한다 (team은 공유 checkout이라 편집 격리가 없다). 실험 플래그가 없으면 team 대신 단발 subagent 재위임(실패 맥락 포함)으로 반복한다 — `autonomous-driving.md §위임 형태` 참조.

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

### 안티패턴

```
❌ "위에서 말한 파일을 수정해줘"
❌ "아까 본 그 함수처럼 처리해줘"
❌ "사용자가 원하는 대로 해줘"
```

**Edit 절대경로 트랩**: worktree 격리 sub-agent라도 Edit tool의 file_path가 부모 repo의 절대경로를 가리키면 메인 working tree가 직접 수정된다 — 실사례 3회 재현 (#783). Bash cwd가 worktree여도 Edit는 별개이므로, prompt에 worktree 격리 준수(위 7번)를 반드시 명시한다. sub-agent가 이를 어기고 부모 repo를 변형하면 메인 branch switch까지 이어질 수 있다.

### 좋은 예

```
목적: src/auth/login.ts의 토큰 만료 처리 버그 수정.
배경: 만료된 토큰이 401 대신 200을 반환하는 문제. 재현은 tests/auth/login.test.ts의
      "expired token" 케이스.
브랜치: 이 worktree는 epic/auth-rewrite 를 base로 한다. 결과는 epic 브랜치로
        머지될 예정이므로 main 기준 가정으로 작업하지 말 것.
범위: login.ts의 verifyToken 함수만 수정. 다른 파일 건드리지 말 것.
격리: 모든 Edit/Write/Bash는 이 worktree 경로 안에서만 수행. 부모 repo
      (메인 working tree) 경로의 파일은 절대 수정하지 말 것.
출력: 변경된 파일 + 테스트가 통과하는지 확인 결과.
검증: bun test tests/auth/login.test.ts 통과.
```

---

## isolation 결정

| 옵션 | 사용 시점 |
|------|-----------|
| 없음 (기본) | 읽기 전용 분석 sub-agent (epic 브랜치 자체에서 실행, 편집 X) |
| `isolation: "worktree"` | 코드를 변경하는 모든 sub-agent — epic 브랜치를 base로 한 worktree에서 작업 |

오케스트레이터 토폴로지에서는 **편집하는 sub-agent는 항상 `isolation: "worktree"`** 다. 메인이 epic 브랜치를 점유하고 있으므로 같은 working tree에서 sub-agent가 편집하면 메인 상태가 오염된다. isolation worktree는 변경이 없으면 자동 정리되고, 변경이 있으면 worktree 경로와 브랜치명이 결과에 포함된다. 자세한 머지/정리는 `worktree-lifecycle.md`.

이 표는 **단발 subagent에만 적용**된다. agent team teammate는 공유 checkout이라 `isolation` 인자로 격리되지 않으므로(아래 §Agent team 사용 패턴), 편집·격리가 필요하면 teammate가 아니라 isolated subagent를 쓴다.

---

## Agent team 사용 패턴

> **전제**: agent team은 실험 기능 — `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1`이 설정돼야 동작한다. 플래그가 없으면 teammate가 spawn되지 않는다. 과거의 `TeamCreate`/`TeamDelete` 도구는 제거됐고, `Agent`의 `team_name` 인자는 받지만 무시된다 — 세션마다 암묵적 team 하나가 있고 `name`으로 바로 spawn하며, session 종료 시 자동 정리된다.

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
- **편집 격리는 team이 아니라 subagent의 `isolation:"worktree"`가 보장한다.** teammate에게 worktree 이동을 위임하지 말 것 — 격리가 도구 보장에서 프롬프트 희망으로 격하되어 공유 checkout(메인 epic 브랜치)이 오염될 수 있다 (#783).

---

## 모델 선택

`Agent` 호출 시 `model` 옵션으로 sub-agent 모델을 지정할 수 있다.

| 작업 유형 | 권장 모델 |
|-----------|-----------|
| 복잡한 설계, 어려운 디버깅, 아키텍처 판단 | `opus` |
| 일반 구현, 코드 리뷰, 테스트 작성 | `sonnet` |
| 단순 분류, 포맷 변환, 짧은 추출 | `haiku` |

지정하지 않으면 부모 모델을 상속한다. 단순 작업에 opus 사용은 비용 낭비.

이 표는 **고정값이 아니라 시작 heuristic**이다. 모델이 더 똑똑해지면 같은 작업을 더 가벼운 tier로 내려 효율을 높일 수 있어야 하므로, 작업마다 "지금도 이 역량이 필요한가"를 재평가한다. 자율 루프에서의 작업별 모델 배분 원칙은 `autonomous-driving.md §모델 분배` 참조.

---

## 체크리스트

위임 직전 확인:

- [ ] prompt가 메인 대화 없이도 이해 가능한가? (자기완결성)
- [ ] prompt에 base epic 브랜치 이름이 포함되었는가?
- [ ] 출력 형식과 검증 기준이 명시되었는가?
- [ ] 단발/team 선택이 작업 성격과 맞는가?
- [ ] 편집하는 sub-agent라면 `isolation: "worktree"`를 켰는가?
- [ ] worktree dispatch라면 prompt에 worktree 격리 준수(경로 prefix 검증 + 부모 repo 수정 금지)를 명시했는가?
- [ ] 모델 선택이 작업 난이도와 맞는가?
- [ ] team의 경우 name이 의미 있고 유니크한가?
