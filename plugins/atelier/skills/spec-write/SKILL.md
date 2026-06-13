---
name: spec-write
description: 합의된 설계를 스펙 문서 계층으로 작성하는 스킬. "이 설계를 스펙 문서로 적어줘", "DESIGN.md 작성", "큰그림 스펙 적어줘", "컴포넌트 스펙 작성", "이 흐름 문서화" 같은 요청에 사용합니다. 설계를 대화로 합의하는 단계는 `interview` 스킬, 작성된 스펙을 코드와 대조 분석하는 단계는 `spec-review` 스킬이 담당합니다. 여기서는 합의된 설계를 정해진 구조(DESIGN→concerns→flows)로 형식화합니다.
version: 1.0.0
---

# spec-write

합의된 설계를 **스펙 문서 계층으로 적는** 스킬입니다. 설계를 *생각하고 도전하는 대화*는 `interview`(brainstorm/grill)에서 끝내고, 여기서는 그 결과를 정해진 구조와 깊이로 형식화합니다. 메인 에이전트가 직접 문서를 작성합니다(sub-agent 분석 아님).

## 진입 라우팅 (의도 → 흐름)

| 사용자 의도 (예) | 흐름 | 산출물 |
|---|---|---|
| "이 설계 스펙 문서로", "DESIGN.md 작성", "큰그림 스펙 적어줘" | write (Big Picture) | `spec/DESIGN.md` |
| "컴포넌트 스펙 작성", "이 흐름 문서화", concerns/flows | write-detail (상세) | `spec/concerns/*.md`, `spec/flows/*.md` |

> **설계를 아직 합의하지 않았다면** (막연한 아이디어, 큰그림 잡기, 기존 계획 도전) `interview` 스킬을 먼저 씁니다. spec-write 는 **합의된 설계를 문서로 형식화**하는 단계입니다.
>
> **작성한 스펙이 실제 코드와 맞는지 확인**하려면 `spec-review` 스킬(spec↔code 갭 분석)을 씁니다.

입력 인자(설계 내용, 저장 경로 등)가 함께 오면 그대로 사용하고, 없으면 AskUserQuestion 으로 확인합니다.

## 작성 절차·형식

스펙 문서의 진입 전 맥락, 깊이 기준, 작성 원칙, 출력 구조(DESIGN/concerns/flows 템플릿), `related_paths` 규약은 `references/authoring.md` 에서 progressive disclosure 로 로드합니다.

| reference | 언제 로드 | 내용 |
|---|---|---|
| `references/authoring.md` | `write`/`write-detail` 수행 시 | 진입 전 맥락, 깊이 기준, 작성 원칙, 출력 구조(DESIGN/concerns/flows), related_paths |

## 공통 원칙

- **합의 후 형식화** — 설계 결정은 interview 에서 합의한다. spec-write 는 합의된 내용을 구조화할 뿐, 새 설계 결정을 임의로 내리지 않는다.
- 작성 원칙(승인 전 저장 금지·`related_paths`·깊이 분리)과 출력 구조는 `references/authoring.md` 가 canonical 이다.
