---
name: interview
description: 작업 착수 전 계획·설계를 사람과 대화로 다지는 인터뷰 메타스킬. "grill me / 이 계획 심문해줘 / 스트레스 테스트해줘", "같이 brainstorm 하자 / 설계 같이 잡자 / 막연한데 방향 잡아줘" 같은 요청에 사용합니다. 슬래시로 직접 호출하거나 모델이 모호한 요청을 감지하면 사용을 제안합니다. 기본은 기존 계획을 심문(grill)하고, 무에서 설계를 시작할 땐 brainstorm 으로 전환합니다.
version: 1.0.0
---

# interview

코드를 건드리기 전에 계획·설계를 사람과 대화로 다진다. 두 모드가 있다.

| 의도 | 모드 | 동작 |
|---|---|---|
| 이미 계획/설계/접근안이 있고 빈틈을 찾고 싶다 (기본) | grill | 아래 인라인 지시 |
| 아직 구체안이 없어 무에서 설계를 시작한다 | brainstorm | `references/brainstorm.md` 로드 |

요청이 "막연한 아이디어 → 설계"면 brainstorm, "이 계획 검증/심문"이면 grill 이다. 헷갈리면 사용자에게 한 번 확인한다.

## grill (기본)

이 계획의 모든 측면에 대해 **공유된 이해에 도달할 때까지 집요하게 인터뷰하라**. 설계 트리의 각 가지를 내려가며, 결정 사이의 의존성을 하나씩 해소하라. 각 질문마다 추천 답을 함께 제시하라.

질문은 한 번에 하나씩만 하라.

질문이 코드베이스 탐색으로 답해질 수 있다면, 묻지 말고 코드베이스를 탐색하라.

### 종료와 핸드오프

모든 가지가 해소되면(또는 사용자가 충분하다고 하면) 합의된 결정 목록을 요약한다. 코드 변경이 필요하면 Plan Mode 로, 아키텍처 규모면 `spec:design` 으로 핸드오프한다. 합의 전에는 구현을 시작하지 않는다.

## 책임 경계

| 대상 | 차이 | 핸드오프 |
|---|---|---|
| `spec:design` | 의도·요구사항이 아직 모호하면 interview, 의도는 합의됐고 아키텍처 구조(다중 컴포넌트)를 잡는 단계면 spec:design. brainstorm 도 설계 문서를 산출하지만 **단일 작업 범위**이며 `related_paths` frontmatter 로 spec 스킬이 소비 가능한 형식으로 쓴다 | brainstorm 중 아키텍처/다중 컴포넌트 규모로 판명되면 `spec:design` 으로 에스컬레이트 |
| Plan Mode | interview 는 *무엇을/왜*(의도·설계 합의), Plan Mode 는 *어떻게*(코드 변경 단계) | 의도가 확정되고 코드 변경이 필요하면 Plan Mode 로 넘긴다 |
| `autopilot` | 자율 루프와 무관, 사람↔에이전트 대화 전용 | 해당 없음 |

## 출처

`grill`·`brainstorm` 은 [obra/superpowers](https://github.com/obra/superpowers) (MIT License, © 2025 Jesse Vincent) 의 `grill-me`·`brainstorming` 스킬이 원본입니다. grill 은 원본 지시문을 보존했고, brainstorm 은 생태계 바인딩 4곳(writing-plans→Plan Mode, browser visual companion→AskUserQuestion·markdown 다이어그램, 체크리스트→TaskCreate, 문서 위치→프로젝트 spec 컨벤션)만 치환한 충실 포팅입니다. MIT 고지는 `references/brainstorm.md` 머리에 명시되어 있습니다.
