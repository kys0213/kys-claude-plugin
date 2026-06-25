---
name: grill
description: Interview the user relentlessly about a plan or design until reaching shared understanding, resolving each branch of the decision tree. Use when user wants to stress-test a plan, get grilled on their design, or mentions "grill me".
version: 1.0.0
---

# grill

이미 손에 든 계획·설계·접근안이 있을 때, 코드를 건드리기 전에 그 계획을 사람과의 대화로 집요하게 심문해 빈틈을 드러낸다. 무에서 새 설계를 만드는 일이 아니라 **있는 것을 무너뜨려 보는** 활동이다.

## 전제

이 스킬은 검토 대상(계획/설계/접근안)이 **이미 존재한다**고 가정한다. 아직 구체안이 없고 막연한 아이디어에서 출발한다면 이 스킬이 아니라 `brainstorm` 스킬로 간다 — 거기서 설계를 만든 뒤, 그 설계를 심문하고 싶을 때 다시 grill 로 핸드오프한다.

## 자세 (posture)

이 계획의 모든 측면에 대해 **공유된 이해에 도달할 때까지 집요하게 인터뷰하라**. 설계 트리의 각 가지를 내려가며, 결정 사이의 의존성을 하나씩 해소하라. 각 질문마다 추천 답을 함께 제시하라.

- **질문은 한 번에 하나씩만** 하라. 여러 질문으로 압도하지 않는다.
- 질문이 코드베이스 탐색으로 답해질 수 있다면, 묻지 말고 코드베이스를 탐색하라.
- 빈틈·미검증 가정·암묵적 trade-off 를 드러내는 데 집중한다. 동의가 아니라 이해가 목표다.
- 유연하게: 답이 새 가지를 열면 그 가지로 내려간다. 정해진 체크리스트를 강제하지 않는다 — 그 rigid 한 9단계 프로세스는 `brainstorm` 의 몫이고, grill 은 계획의 형태에 맞춰 움직이는 자세(posture)다.

## 종료와 핸드오프

모든 가지가 해소되면(또는 사용자가 충분하다고 하면) 합의된 결정 목록을 요약한다. 그다음:

- 코드 변경이 필요하면 **Plan Mode** 로 — *어떻게* 바꿀지 구현 계획을 세운다.
- 합의된 설계를 장기 스펙 문서로 남겨야 하면 **`spec-write`** 로 — 정해진 구조(DESIGN/concerns/flows)로 형식화한다.

합의 전에는 구현을 시작하지 않는다.

## 책임 경계

| 대상 | 차이 | 핸드오프 |
|---|---|---|
| `brainstorm` | grill 은 *있는 계획*을 심문한다(수렴·비평), brainstorm 은 *무에서 설계*를 생성한다(발산→수렴) | 검토할 구체안이 없으면 `brainstorm` 으로. brainstorm 이 설계를 만든 뒤 심문이 필요하면 grill 로 |
| `spec-write` | **대화 ≠ 문서**. grill 은 설계를 *대화로 도전*하고, `spec-write` 는 *합의된 설계를 스펙 문서로 형식화*(DESIGN/concerns/flows)한다 | 합의된 설계를 장기 스펙 문서로 남길 땐 `spec-write`, 단일 작업 구현은 Plan Mode 로 |
| `spec-review` | 작성된 스펙을 *코드와 대조 분석*(L1/L2/audit)·품질 평가하는 단계. grill 의 설계 도전과 다른 활동 | 스펙 작성 후 코드 정합 확인이 필요하면 `spec-review` 로 |
| Plan Mode | grill 은 *무엇을/왜*(의도·설계 합의), Plan Mode 는 *어떻게*(코드 변경 단계) | 의도가 확정되고 코드 변경이 필요하면 Plan Mode 로 넘긴다 |

## 출처

`grill` 은 [obra/superpowers](https://github.com/obra/superpowers) (MIT License, © 2025 Jesse Vincent) 의 `grill-me` 스킬이 원본입니다. 원본 지시문(집요한 인터뷰 자세·한 번에 한 질문·코드베이스 우선 탐색)을 보존했고, 핸드오프만 atelier 생태계(Plan Mode / `spec-write` / `brainstorm`)로 바인딩했습니다.
