---
name: spec-common
description: 스펙 문서 작성 컨벤션 — 공통 톤 정책 (모든 spec 문서에 적용)
paths: ["{spec_root}/**/*.md"]
---

# Spec Common Rules

> `{spec_root}/` 전반(README/DESIGN/concerns/flows/openapi)에 공통으로 적용되는 톤 정책과 spec ↔ code 정합성 정책.

## 독자 친화 톤

### 핵심 원칙

| 항목 | 정책 |
|------|------|
| 독자 대상 | 특성화 고등학생도 이해 가능한 수준 |
| 문체 | 친근한 해요체 ("~해요", "~예요", "~돼요") 단문 위주 |
| 길이 | 한 문장은 짧게. 한 단락 = 한 주제. 구구절절 설명 금지 |
| 시각화 | 텍스트 설명보다 다이어그램·표·예시가 우선 |
| 용어 | 영어 약어/전문용어는 처음 등장 시 한국어 풀이 한 번 — 이후 약어만 |

### 작성 형식

- 헤딩은 짧게. 질문형 권장 ("어떻게 동작해요?", "왜 이렇게 만들었어요?")
- 문단 시작에 한 줄 요약 (TL;DR 느낌)
- 본문은 bullet / 표 / 다이어그램 우선
- 긴 prose 블록은 분할
- 동작 흐름은 ASCII 다이어그램 또는 mermaid 로
- 정책 케이스는 표로
- 상태 머신은 다이어그램으로
- 인터페이스 책임은 표로 (단, 함수 시그니처 표는 여전히 금지)

### 시각화 우선 원칙

다음은 무조건 시각화 (표/다이어그램):

| 대상 | 형태 |
|------|------|
| 트리거 / 처리 / 응답 흐름 | ASCII flow 또는 mermaid sequence |
| 케이스별 정책 (입력 → 결정) | 표 |
| 컴포넌트 간 관계 | ASCII 박스 또는 mermaid graph |
| 상태 라이프사이클 | 상태 머신 다이어그램 |
| 라벨 / cardinality 정책 | 표 |

### 톤 변환 예시

✗ Before (딱딱한 톤)

> `AccessAuthorizer` port 의 실제 반환값은 `{Allowed, AccessLevel, Reason}` 이며, `ManagerAuthAuthorizer` 가 일률 `AccessLevel=write` 부여하는 임시 정책으로, 차등 모델 재도입은 별도 설계로 분리한다.

✓ After (독자 친화 톤)

> AccessAuthorizer 는 세 가지를 알려줘요.
>
> | 필드 | 의미 |
> |------|------|
> | Allowed | 접근 가능? (true/false) |
> | AccessLevel | 권한 수준 (지금은 무조건 `write`) |
> | Reason | 거부 사유 (감사 로그용) |
>
> AccessLevel 이 `write` 고정인 이유? **차등 모델은 아직 안 정해졌어요**. 정해지면 별도 설계로 다시 다룰 예정이에요.
