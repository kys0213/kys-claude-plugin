---
description: 컴포넌트 상세 또는 시나리오 흐름을 대화형으로 설계합니다
argument-hint: "<컴포넌트|flow 시나리오>"
allowed-tools: ["AskUserQuestion", "Read", "Glob", "Write"]
---

# 상세 설계 커맨드 (/design-detail)

Big Picture 스펙(DESIGN.md)을 기반으로, 컴포넌트별 세부 설계(concerns/) 또는 시나리오별 흐름(flows/)을 대화형 핑퐁 세션으로 만들어갑니다.

> 핑퐁 루프·6개 관점·관점 전환·의사코드 처리·안티패턴은 `/design` 과 동일하게 `spec-workflow` skill 의 `references/design-protocol.md` 를 따릅니다. 이 커맨드는 세션 준비와 **컴포넌트/플로우 고유의 출력 구조**만 담습니다.

## 사용법

```bash
/design-detail daemon
/design-detail "queue-state-machine"
/design-detail flow "온보딩"
/design-detail flow "실패 복구"
```

| 인자 | 필수 | 설명 |
|------|------|------|
| 컴포넌트명 | No | DESIGN.md에 정의된 컴포넌트 이름 |
| flow 시나리오명 | No | "flow" 키워드 + 시나리오 이름 |

인자가 없으면 DESIGN.md에서 컴포넌트/시나리오 목록을 보여주고 선택을 요청한다.

## 세션 준비

1. **맥락 파악**: `spec/DESIGN.md` 또는 `**/DESIGN*.md` 를 Glob 으로 찾아 Read. 없으면 AskUserQuestion 으로 `/design` 선행 또는 경로 안내. 기존 `spec/concerns/`·`spec/flows/` 및 관련 코드도 파악.
2. **세션 타입 결정**: `flow` 키워드 → 플로우 세션, 그 외 → 컴포넌트 세션. 인자 없으면 목록 제시 후 선택.
3. **초안 제안**: DESIGN.md 의 해당 컴포넌트/시나리오 정보 + 기존 코드 구조를 반영해 구체적 초안 먼저 제안. 이후 핑퐁 루프(`references/design-protocol.md`)를 수렴까지 반복.

## 컴포넌트 세션 (concerns/)

### 핑퐁 초점
- 인터페이스/Trait 시그니처: AI 제안 → 사용자 단순화/수정
- 핵심 로직 의사코드 / 상태 전이 / 에러 케이스 / 다른 컴포넌트와의 의존(경계·계약)

### 수렴 판단
6개 관점 중 **3개 이상** 다룸(컴포넌트 단위라 `/design` 의 4개보다 낮음) + 인터페이스/역할 합의 + 핵심 로직 방향 결정 + 주요 에러 케이스 + 2-3회 연속 확인.

### 출력 구조

```markdown
---
related_paths:
  - {추정 코드 경로}
---

# {컴포넌트 이름}

> {한 문장: 이 컴포넌트가 하는 것과 하지 않는 것}

## 역할
{핵심 책임을 불릿으로}

## 인터페이스/Trait
```{language}
{합의된 시그니처}
```

## 핵심 로직
```
{의사코드 — 사용자와 합의된 수준}
```

## 에러 처리
{실패 시나리오와 대응 정책}

## 제약 조건
{불변 조건, 한계, 성능 제약}

## 관련 문서
| 문서 | 관계 |
|------|------|
| [DESIGN.md](../DESIGN.md) | 전체 구조에서의 위치 |
| [다른 concern](./other.md) | 의존/협력 관계 |
```

## 플로우 세션 (flows/)

### 초안 제안
DESIGN.md 의 컴포넌트 구조와 기존 concerns/ 를 읽고: 시나리오 요약(트리거→결과 한 문단) + 흐름 다이어그램 초안(ASCII) + 관련 관점 도전 하나.

### 핑퐁 초점
단계별 흐름(컴포넌트 관여 순서) / 분기점(성공·실패·엣지) / 데이터 변환 / 컴포넌트 간 상호작용.

### 수렴 판단
6개 관점 중 **3개 이상** + 주요 흐름(happy path) 합의 + 핵심 분기점 + 2-3회 연속 확인.

### 출력 구조

```markdown
---
related_paths:
  - {추정 코드 경로}
---

# Flow {번호}: {시나리오 이름}

> {한 문장 요약}

## 흐름 다이어그램
```
{ASCII 다이어그램}
```

## 단계별 설명
{각 단계: 트리거 → 컴포넌트 → 데이터 → 결과}

## 실패 경로
{각 단계에서 실패 시 어떻게 되는가}

## 엣지 케이스
{비정상적이지만 유효한 시나리오}

## 관련 문서
| 문서 | 관계 |
|------|------|
| [DESIGN.md](../DESIGN.md) | 전체 구조 |
| [관련 concern](./concerns/xxx.md) | 관련 컴포넌트 상세 |
```

## frontmatter `related_paths` 권고

`/design-detail` 은 `/design` 의 후속이므로, 부모 DESIGN.md frontmatter 가 채워져 있으면 그 값을 출발점으로 삼고 **이 컴포넌트/플로우에 한정된 더 좁은 경로로 보강**한다 (예: DESIGN.md 가 `crates/foo/` → 컴포넌트 spec 은 `crates/foo/src/daemon.rs`). 채우는 기준은 `references/design-protocol.md` 참조.

## 저장

수렴 시 AskUserQuestion 으로 정리 의사 확인 → 기본 경로 제안(컴포넌트: `spec/concerns/{name}.md`, 플로우: `spec/flows/{nn-scenario}.md`) → 경로 confirm → Write → DESIGN.md 에 상세 문서 링크 추가 제안.

## 안티패턴 / 주의사항

`references/design-protocol.md` 안티패턴에 추가로:
- ❌ DESIGN.md 의 큰그림 정보를 중복 기술
- ❌ 구현 코드 작성 (의사코드까지만)

깊이 기준: "구현자가 코드 구조를 잡을 수 있지만, 구현 방법은 선택할 수 있는 수준". DESIGN.md 의 철학·원칙을 존중하며 세부를 구체화하고, 기존 concerns/ 와 인터페이스 정합성을 확인한다.
