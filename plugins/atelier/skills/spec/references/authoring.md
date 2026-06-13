# 스펙 문서 작성 컨벤션

합의된 설계를 스펙 문서로 적는 절차와 형식. **설계를 생각하고 도전하며 합의에 이르는 대화는 `interview`(brainstorm: 무에서 설계 / grill: 기존 계획 심문)에서 끝낸다.** 여기서는 그 결과를 정해진 구조로 형식화한다 — `write`(Big Picture DESIGN.md)와 `write-detail`(컴포넌트/플로우 상세)이 이 컨벤션을 공유한다.

## 진입 전 맥락 파악

- **write (Big Picture)**: 합의된 설계 내용(또는 사용자가 제시한 방향)을 입력으로 받는다. 없으면 먼저 `interview` 로 설계를 합의하도록 안내한다.
- **write-detail (컴포넌트/플로우)**: `spec/DESIGN.md` 또는 `**/DESIGN*.md` 를 Glob 으로 찾아 Read 한다. 없으면 AskUserQuestion: "Big Picture 스펙(DESIGN.md)을 찾을 수 없습니다. `write` 로 먼저 만들거나 기존 경로를 알려주세요." 기존 `spec/concerns/`·`spec/flows/` 및 관련 코드도 파악한다.

## 깊이 기준

- **write (Big Picture) → DESIGN.md** — 포함: 목표, 설계 철학(의견), 컴포넌트 목록과 책임, 컴포넌트 간 데이터 흐름, 확장 가능 지점. 제외: trait/인터페이스 시그니처, 의사코드, 상태 전이 다이어그램, DB 스키마, API 상세 (→ write-detail 영역).
- **write-detail** — "구현자가 코드 구조를 잡을 수 있지만, 구현 방법은 선택할 수 있는 수준". DESIGN.md 의 철학·원칙을 존중하며 세부 구체화. ❌ DESIGN.md 큰그림 중복 기술, ❌ 구현 코드 작성(의사코드까지만).

## 작성 원칙

- 저장 경로는 AskUserQuestion 으로 확인한다 (기본값은 아래 각 출력 구조 참조).
- **최종 승인 전까지 파일을 저장하지 않는다.** 내용을 제시하고 동의받은 뒤 Write.
- 기존 코드를 참조하되, 스펙이 기존 구조에 종속되지 않도록 한다.
- frontmatter `related_paths` 를 채운다 — 후속 `spec-review`·`gap-detect` 가 이 필드를 코드 영역 매핑 Hint 로 사용하므로 분석 정확도가 크게 오른다. 본문에서 언급된 모듈/디렉터리/식별자를 프로젝트 구조와 매칭하되, **확실한 경로만** 적는다(추정에 자신 없으면 비움). 신규 설계라 코드가 아직 없으면 비워둔다.

## 출력 구조

### write → DESIGN.md

저장 경로 기본: `spec/DESIGN.md` (AskUserQuestion 확인 후 Write).

```markdown
---
related_paths:
  - {추정 코드 경로}
---

# DESIGN

> **Date**: {오늘 날짜}
> **Status**: Draft

## 목표
{1-3문장: 이 시스템이 해결하는 문제, 설계 목표로 서술}

## 설계 철학
{번호 매긴 원칙들. 각 1-2문장. 서술이 아닌 의견/결정.}

### 1. {원칙 이름}
{설명}

## 전체 구조
{ASCII 다이어그램: 컴포넌트 배치와 데이터 흐름}

## 관심사 분리
| 레이어 | 책임 | 비고 |
|--------|------|------|

## OCP 확장점
{새 타입/시스템 추가 시 코어 변경 없이 확장 가능한 지점}

## 미결정 사항
{확정되지 않은 항목. write-detail 에서 구체화할 후보}

## 상세 문서
| 문서 | 설명 |
|------|------|
| [concerns/...](...) | ... |
| [flows/...](...) | ... |
```

### write-detail → concerns/{name}.md (컴포넌트)

저장 경로 기본: `spec/concerns/{component-name}.md` (AskUserQuestion 확인 후 Write). 저장 후 DESIGN.md 에 상세 문서 링크 추가를 제안한다.

```markdown
---
related_paths:
  - {추정 코드 경로}
---

# {컴포넌트 이름}

> {한 문장: 이 컴포넌트가 하는 것과 하지 않는 것}

## 역할
{핵심 책임}

## 인터페이스/Trait
{합의된 시그니처}

## 핵심 로직
{의사코드 — 합의된 수준}

## 에러 처리
{실패 시나리오와 대응 정책}

## 제약 조건
{불변 조건, 한계, 성능 제약}

## 관련 문서
| 문서 | 관계 |
|------|------|
| [DESIGN.md](../DESIGN.md) | 전체 구조에서의 위치 |
```

### write-detail → flows/{nn-scenario}.md (플로우)

저장 경로 기본: `spec/flows/{nn-scenario-name}.md`.

```markdown
---
related_paths:
  - {추정 코드 경로}
---

# Flow {번호}: {시나리오 이름}

> {한 문장 요약}

## 흐름 다이어그램
{ASCII 다이어그램}

## 단계별 설명
{각 단계: 트리거 → 컴포넌트 → 데이터 → 결과}

## 실패 경로
{각 단계에서 실패 시}

## 엣지 케이스
{비정상적이지만 유효한 시나리오}

## 관련 문서
| 문서 | 관계 |
|------|------|
| [DESIGN.md](../DESIGN.md) | 전체 구조 |
| [관련 concern](./concerns/xxx.md) | 관련 컴포넌트 상세 |
```
