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

### Agent team (`TeamCreate` + `Agent({team_name, name})`)

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

---

## Prompt 작성 원칙

sub-agent는 **메인 대화 히스토리를 보지 못한다**. prompt는 자기완결적이어야 한다.

### 필수 포함 요소

1. **목적**: 무엇을 달성해야 하는가
2. **컨텍스트**: 작업 배경, 관련 파일 경로 (전체 경로), 이미 알려진 제약
3. **범위**: 무엇을 하고 무엇을 하지 말 것
4. **출력 형식**: 결과를 어떤 형태로 돌려줄지 (파일 변경? 요약? JSON?)
5. **검증 기준**: 완료를 어떻게 확인할지 (테스트, 빌드, 특정 체크 등)

### 안티패턴

```
❌ "위에서 말한 파일을 수정해줘"
❌ "아까 본 그 함수처럼 처리해줘"
❌ "사용자가 원하는 대로 해줘"
```

### 좋은 예

```
목적: src/auth/login.ts의 토큰 만료 처리 버그 수정.
배경: 만료된 토큰이 401 대신 200을 반환하는 문제. 재현은 tests/auth/login.test.ts의
      "expired token" 케이스.
범위: login.ts의 verifyToken 함수만 수정. 다른 파일 건드리지 말 것.
출력: 변경된 파일 + 테스트가 통과하는지 확인 결과.
검증: bun test tests/auth/login.test.ts 통과.
```

---

## isolation 결정

| 옵션 | 사용 시점 |
|------|-----------|
| 없음 (기본) | 읽기 전용 분석, 또는 같은 worktree에서 순차 작업 |
| `isolation: "worktree"` | 병렬 코드 변경 — 각 agent가 다른 worktree에서 작업 |

isolation worktree는 변경이 없으면 자동 정리되고, 변경이 있으면 worktree 경로와 브랜치명이 결과에 포함된다. 자세한 머지/정리는 `worktree-lifecycle.md`.

---

## TeamCreate 사용 패턴

```
TeamCreate({name: "feature-auth-rewrite"})

# 역할별 agent
Agent({
  team_name: "feature-auth-rewrite",
  name: "designer",
  run_in_background: true,
  description: "Auth design",
  prompt: "<자기완결적 design 작업>"
})

Agent({
  team_name: "feature-auth-rewrite",
  name: "implementer",
  run_in_background: true,
  isolation: "worktree",
  description: "Auth implementation",
  prompt: "<designer 결과를 input으로 받는 implementation 작업>"
})

# 중간 개입
SendMessage({to: "implementer", message: "<우선순위 변경 또는 수정 지시>"})
```

### Team 사용 시 주의

- `name`이 식별자다. 같은 team 안에서 유니크해야 한다.
- `run_in_background: true`로 띄워야 SendMessage로 개입할 수 있다.
- Team은 명시적으로 `TeamDelete`하지 않으면 남는다 — 작업 종료 시 정리.

---

## 모델 선택

`Agent` 호출 시 `model` 옵션으로 sub-agent 모델을 지정할 수 있다.

| 작업 유형 | 권장 모델 |
|-----------|-----------|
| 복잡한 설계, 어려운 디버깅, 아키텍처 판단 | `opus` |
| 일반 구현, 코드 리뷰, 테스트 작성 | `sonnet` |
| 단순 분류, 포맷 변환, 짧은 추출 | `haiku` |

지정하지 않으면 부모 모델을 상속한다. 단순 작업에 opus 사용은 비용 낭비.

---

## 체크리스트

위임 직전 확인:

- [ ] prompt가 메인 대화 없이도 이해 가능한가? (자기완결성)
- [ ] 출력 형식과 검증 기준이 명시되었는가?
- [ ] 단발/team 선택이 작업 성격과 맞는가?
- [ ] 병렬이라면 isolation: "worktree"를 켰는가?
- [ ] 모델 선택이 작업 난이도와 맞는가?
- [ ] team의 경우 name이 의미 있고 유니크한가?
