---
name: spec-write
description: 합의된 설계를 스펙 문서 계층으로 작성하는 스킬. "이 설계를 스펙 문서로 적어줘", "DESIGN.md 작성", "큰그림 스펙 적어줘", "컴포넌트 스펙 작성", "이 흐름 문서화" 같은 요청에 사용합니다. 설계를 대화로 합의하는 단계는 `grill` 스킬이 담당합니다. 여기서는 합의된 설계를 정해진 구조(DESIGN→concerns→flows)로 형식화합니다.
version: 1.0.0
---

# spec-write

합의된 설계를 **스펙 문서 계층으로 적는** 스킬입니다. 설계를 *생각하고 도전하는 대화*는 `grill`에서 끝내고, 여기서는 그 결과를 정해진 구조와 깊이로 형식화합니다.

이 skill 이 소유하는 것은 **무엇을 어떤 구조·깊이로 쓸 것인가의 기준**(DESIGN/concerns/flows 템플릿, 깊이 분리, `related_paths` 규약)입니다. 실제 파일 Write 는 `orchestrator` skill 이 위임하는 sub-agent 가 수행하며, 메인 에이전트는 Write 를 위임할 뿐 직접 실행하지 않습니다.

## 진입 라우팅 (의도 → 흐름)

| 사용자 의도 (예) | 흐름 | 산출물 |
|---|---|---|
| "이 설계 스펙 문서로", "DESIGN.md 작성", "큰그림 스펙 적어줘" | write (Big Picture) | `spec/DESIGN.md` |
| "컴포넌트 스펙 작성", "이 흐름 문서화", concerns/flows | write-detail (상세) | `spec/concerns/*.md`, `spec/flows/*.md` |

> **설계를 아직 합의하지 않았다면** 대화로 먼저 합의합니다 — 막연한 아이디어부터 기존 계획 심문까지 `grill` 스킬을 먼저 씁니다. spec-write 는 **합의된 설계를 문서로 형식화**하는 단계입니다.

입력 인자(설계 내용, 저장 경로 등)가 함께 오면 그대로 사용하고, 없으면 AskUserQuestion 으로 확인합니다.

## 작성 절차·형식

스펙 문서의 진입 전 맥락, 깊이 기준, 작성 원칙, 출력 구조(DESIGN/concerns/flows 템플릿), `related_paths` 규약은 `references/authoring.md` 에서 progressive disclosure 로 로드합니다.

| reference | 언제 로드 | 내용 |
|---|---|---|
| `references/authoring.md` | `write`/`write-detail` 수행 시 | 진입 전 맥락, 깊이 기준, 작성 원칙, 출력 구조(DESIGN/concerns/flows), related_paths |

## 위임 흐름 (orchestrator 연계)

`orchestrator` skill 은 문서 산출물 요청(스펙 작성)을 감지하면 실제 Write 를 sub-agent 에 위임한다. spec-write 는 그 sub-agent 가 따를 구조·깊이·형식 기준의 지식 출처다 — dispatch 프롬프트에 `references/authoring.md` 의 기준을 포함시켜 sub-agent 가 동일한 규약으로 작성하도록 한다.

## 흔한 실수

- ❌ 합의되지 않은 설계를 spec-write 단계에서 즉흥적으로 결정 → `grill` 로 돌아가 먼저 합의한다.
- ❌ write-detail 에서 Big Picture 내용을 다시 풀어씀 → DESIGN.md 를 링크로 참조하고 상세만 적는다.
- ❌ `related_paths` 를 추정으로 채움 → 확신 없으면 비워둔다. 틀린 경로는 없는 것보다 나쁘다.
- ❌ 승인 전에 파일을 먼저 저장 → 내용 제시 → 승인 → orchestrator 위임 Write 순서를 지킨다.

## 공통 원칙

- **합의 후 형식화** — 설계 결정은 `grill` 대화에서 합의한다. spec-write 는 합의된 내용을 구조화할 뿐, 새 설계 결정을 임의로 내리지 않는다.
- 작성 원칙(승인 전 저장 금지·`related_paths`·깊이 분리)과 출력 구조는 `references/authoring.md` 가 canonical 이다.
- **작성 주체 분리** — spec-write 는 구조·문체 기준의 단일 출처이고, 승인된 내용의 실제 파일 Write 는 `orchestrator` 가 조율하는 sub-agent 가 수행한다.
- **문서 계층 일관성** — DESIGN.md 의 상세 문서 표와 concerns/flows 의 관련 문서 표가 서로를 가리키는 양방향 링크를 유지한다.
