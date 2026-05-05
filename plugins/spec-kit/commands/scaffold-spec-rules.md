---
description: 프로젝트의 spec/ 디렉토리를 분석하여 .claude/rules/spec-*.md 파일을 생성합니다
argument-hint: "[--gap-only]"
allowed-tools:
  - Glob
  - Read
  - Write
  - AskUserQuestion
  - Bash
---

# Scaffold Spec Rules Command

> 프로젝트의 스펙 디렉토리 구조(`spec/`)를 분석하여 작성 컨벤션 룰(`.claude/rules/spec-*.md`)을 자동 생성합니다.

## Overview

스펙 문서를 다루는 프로젝트가 `concern / design / flow` 3종 컨벤션을 표준화하여 사용할 수 있도록, 발견된 디렉토리 구조에 맞는 룰 파일만 골라 생성합니다.

5단계 워크플로우:

1. **spec 디렉토리 감지**: 후보 경로 + 내부 구조 + 프로젝트명 추출
2. **감지 결과 확인**: 사용자 승인 (HITL)
3. **룰 파일 구조 제안**: 카테고리별 paths/생성 여부 승인 (HITL)
4. **룰 파일 생성**: `.claude/rules/spec-*.md` 일괄 작성
5. **결과 요약**

`$ARGUMENTS`에 `--gap-only`가 포함된 경우, 이미 존재하는 `spec-*.md`는 건너뛰고 누락된 카테고리만 생성합니다.

## Execution Steps

### Step 1: spec 디렉토리 감지

후보 경로를 `Glob`으로 탐색합니다:

```
Glob: spec/
Glob: specs/
Glob: docs/spec/
Glob: .spec/
```

가장 먼저 발견된 디렉토리를 `spec_root`로 채택합니다. 여러 개가 발견되면 Step 2에서 사용자에게 선택을 요청합니다.

발견된 `spec_root` 내부에서 다음을 확인합니다:

| 후보 | 매칭 카테고리 |
|---|---|
| `{spec_root}/concerns/**/*.md` | **concern** |
| `{spec_root}/README.md` 또는 `{spec_root}/DESIGN.md` | **design** |
| `{spec_root}/flows/**/*.md` | **flow** |

발견되지 않은 카테고리는 후속 단계에서 기본 비활성으로 표시됩니다 (사용자가 명시 추가 가능).

#### 프로젝트명 추출

다음 우선순위로 프로젝트명을 추출합니다:

1. `package.json`의 `name`
2. `go.mod`의 `module` 마지막 segment
3. `Cargo.toml`의 `[package].name`
4. `pyproject.toml`의 `[project].name` 또는 `[tool.poetry].name`
5. 위가 없으면 git remote 또는 디렉토리명

추출 실패 시 placeholder `{project}`로 둔 채 Step 2에서 사용자에게 직접 묻습니다.

#### 기존 룰 파일 감지

`Glob: .claude/rules/spec-*.md`를 실행하여 이미 존재하는 룰 파일 목록을 확보합니다. `--gap-only` 옵션이면 이 목록에 포함된 카테고리는 후속 단계에서 자동 skip됩니다.

### Step 2: 감지 결과 확인 (HITL)

```
AskUserQuestion: |
  자동 감지 결과:
    - spec 루트: {spec_root}
    - 프로젝트명: {project_name}
    - 발견 카테고리: {concern: yes/no, design: yes/no, flow: yes/no}
    - 기존 룰 파일: {existing_files}

  이대로 진행할까요?
    1. yes — 진행
    2. spec 루트 변경
    3. 프로젝트명 수정
    4. 카테고리 강제 활성화 (감지 안 된 카테고리도 생성)
    5. cancel
```

후보 경로가 0개이면 `AskUserQuestion`으로 직접 입력을 받거나 종료합니다.

### Step 3: 룰 파일 구조 제안 (HITL)

활성 카테고리별로 다음 구조를 제안합니다:

| 카테고리 | 파일 | paths frontmatter (예시) |
|---|---|---|
| concern | `.claude/rules/spec-concern.md` | `["{spec_root}/concerns/**/*.md"]` |
| design | `.claude/rules/spec-design.md` | `["{spec_root}/README.md", "{spec_root}/DESIGN.md"]` |
| flow | `.claude/rules/spec-flow.md` | `["{spec_root}/flows/**/*.md"]` |

```
AskUserQuestion: |
  생성 후보:
    - [x] .claude/rules/spec-concern.md   (paths: spec/concerns/**/*.md)
    - [x] .claude/rules/spec-design.md    (paths: spec/README.md, spec/DESIGN.md)
    - [ ] .claude/rules/spec-flow.md      (감지 안 됨)

  옵션:
    - "yes": 체크된 항목 생성
    - 번호 (예: "1,3"): 선택만 생성
    - "no": 취소
    - paths 수정 요청 가능
```

### Step 4: 룰 파일 생성

승인된 카테고리별로 아래 템플릿을 사용해 `.claude/rules/spec-*.md`를 `Write`합니다.

`{project}`, `{spec_root}`, `{concerns_path}`, `{flows_path}`는 Step 1/2에서 결정된 값으로 치환합니다. 결정 안 된 경우 placeholder를 그대로 둡니다 (사용자가 후속 편집).

세 파일 모두 상단에 **다이어그램 분담 표**를 공통 인용하여 단독 로드되어도 의미가 통하게 합니다.

#### 공통 헤더 — 다이어그램 분담 표

스펙 트리 전반에서 시각화 위치를 종류별로 분담합니다.

| 다이어그램 종류 | 위치 | 판단 기준 |
|---|---|---|
| 시스템 전체 아키텍처 (블록, 책임 분배) | **design** | 시스템 전체 그림인가 |
| 단일 컴포넌트 정적 구조 (ERD, 상태 머신, 관계도) | **concern** | 시간축 없는 정적 구조인가 |
| 시스템 간 시간축 인터랙션 (요청·응답 흐름) | **flow** | 시간축이 있는 시퀀스인가 |

#### 템플릿 1 — `spec-concern.md`

```markdown
---
name: spec-concern
description: 스펙 문서 작성 컨벤션 — 단일 컴포넌트 관심사 (concern)
paths: ["{concerns_path}/**/*.md"]
---

# Spec — Concern

> 단일 컴포넌트의 관심사를 기술하는 스펙 파일 컨벤션. 시스템 전체 그림은 design, 시간축 시퀀스는 flow에 위임한다.

## 다이어그램 분담

| 종류 | 위치 | 기준 |
|---|---|---|
| 시스템 전체 아키텍처 | design | 시스템 전체 그림인가 |
| 정적 구조 (ERD, 상태 머신) | **concern (이 파일군)** | 시간축 없는 정적 구조인가 |
| 시간축 인터랙션 시퀀스 | flow | 시간축이 있는가 |

## 필수 구조

\`\`\`markdown
# {project} — {컴포넌트명}

> 한 줄 요약

---

## 역할
(이 컴포넌트가 무엇을 하는지)

## (본문 섹션)
(인터페이스, 처리 흐름, 규칙)

---

## 참조 (외부 문서 링크)
## 미결 사항 (TBD 항목)
\`\`\`

## 작성 원칙

- **하나의 파일 = 하나의 관심사** — 여러 관심사가 섞이면 분리한다
- **역할 섹션 필수** — "무엇을 하는가"를 본문 어떤 항목보다 먼저 정의한다
- **결정된 것만 본문에** — 미확정은 "미결 사항" 섹션으로 격리
- **교차 참조는 상대 경로**로 작성한다
- **네이밍**: 파일명 kebab-case

## 본문 vs 코드 SSOT

**본문에 담는 것**

- API 엔드포인트 명세 (요청·응답 schema, 에러 코드)
- 도메인 추상화 인터페이스 (메서드 시그니처와 계약)
- 정책의 케이스별 정리 (환경 분기, 사건×결과 표, 상태 머신)
- 스키마 정의 (DDL, enum)
- 무결성 규칙 / 라이프사이클 invariant
- 구조 시각화 (ERD, 상태 머신, 컴포넌트 관계도) — 정적 구조만
- 슈도코드 (흐름 묘사용 의사 표기)

**코드가 SSOT인 영역 (spec에 옮겨 적지 않는다)**

- 비즈니스 로직 구현 → 코드
- 마이그레이션 SQL → 마이그레이션 파일
- 트랜잭션 시퀀스 SQL → flow의 시퀀스 다이어그램으로 추상화
- DAO / 트랜잭션 구현 → repository 코드
- 테스트 시나리오 → 테스트 파일

> spec과 코드를 두 곳에 두면 drift 위험. spec은 "코드가 만족해야 할 계약"만 정의한다.

## 금지 사항

- 시간축 인터랙션 시퀀스 다이어그램 → flow에 작성한다
- 컴파일되는 언어 코드 본문 직접 포함 (함수 body, 클래스 구현)
- 다른 concern 파일 내용 중복 기술 → 교차 참조로 대체

## 필드 표기 규칙 (선택)

같은 개념을 레이어별로 다르게 표기. 동일 정보를 여러 방식으로 적지 않고 1:1 변환을 따른다.

| 위치 | 표기 컨벤션 |
|---|---|
| API JSON 필드 | camelCase |
| DB 컬럼명 (DDL) | snake_case |
| 구조체 필드 | 언어별 컨벤션 |
```

#### 템플릿 2 — `spec-design.md`

```markdown
---
name: spec-design
description: 스펙 문서 작성 컨벤션 — 시스템 아키텍처 + 라우팅 진입점 (design)
paths: ["{spec_root}/README.md", "{spec_root}/DESIGN.md"]
---

# Spec — Design

> 시스템 전체 그림과 진입점/라우팅을 다루는 스펙 파일 컨벤션. 단일 컴포넌트 상세는 concern에 위임한다.

## 다이어그램 분담

| 종류 | 위치 | 기준 |
|---|---|---|
| 시스템 전체 아키텍처 | **design (이 파일군)** | 시스템 전체 그림인가 |
| 정적 구조 (ERD, 상태 머신) | concern | 시간축 없는 정적 구조인가 |
| 시간축 인터랙션 시퀀스 | flow | 시간축이 있는가 |

## README — 라우팅 + 변경 노트

- **역할**: 라우팅 테이블 + 변경 노트(시간순 누적) + Archive 링크
- **위임**: 컴포넌트 상세는 concern, 처리 흐름은 flow에 위임 — README에 본문 상세를 옮겨 적지 않는다
- **필수**: `concerns/`, `flows/` 내 모든 문서를 라우팅 테이블에 포함한다
- **새 문서 추가 시**: README 라우팅 테이블에 반드시 항목을 추가
- **변경 노트**: 메이저/마이너 개정 시 한 단락씩 누적, 이전 노트 보존. 본문 상세는 별도 문서가 SSOT이며 변경 노트는 진입점만 안내

## DESIGN — 시스템 아키텍처

- **역할**: 시스템 전체 그림, 역할 분리, 연결 경로
- **포함**: 아키텍처 다이어그램, 책임 분배 표, namespace 개념
- **미포함**: 개별 컴포넌트 상세 → concern에 위임한다

## 새 스펙 문서 추가 체크리스트

1. `concerns/` 또는 `flows/`에 파일을 생성한다
2. README 라우팅 테이블에 항목을 추가한다
3. 필요 시 DESIGN의 아키텍처 다이어그램을 갱신한다

## 금지 사항

- 컴포넌트 상세 본문을 README/DESIGN에 직접 옮겨 적지 않는다 → concern 위임
- 시간축 인터랙션 시퀀스를 design에 두지 않는다 → flow 위임
```

#### 템플릿 3 — `spec-flow.md`

```markdown
---
name: spec-flow
description: 스펙 문서 작성 컨벤션 — 시간축 유스케이스 시퀀스 (flow)
paths: ["{flows_path}/**/*.md"]
---

# Spec — Flow

> 시간축 유스케이스 시퀀스를 기술하는 스펙 파일 컨벤션. 시스템 전체 그림은 design, 단일 컴포넌트 정적 구조는 concern에 위임한다.

## 다이어그램 분담

| 종류 | 위치 | 기준 |
|---|---|---|
| 시스템 전체 아키텍처 | design | 시스템 전체 그림인가 |
| 정적 구조 (ERD, 상태 머신) | concern | 시간축 없는 정적 구조인가 |
| 시간축 인터랙션 시퀀스 | **flow (이 파일군)** | 시간축이 있는가 |

## 필수 구조

\`\`\`markdown
# Flow N: {유스케이스명}

> 시스템 개요는 [../DESIGN.md](../DESIGN.md) 참조.

> 한 줄 요약

> **관여 시스템:**
> - **{시스템A}:** 역할
> - **{시스템B}:** 역할

## 시퀀스
(ASCII 시퀀스 다이어그램)
\`\`\`

## 작성 원칙

- **하나의 파일 = 하나의 유스케이스 흐름** — 서브 플로우는 같은 파일에 포함 가능
- **관여 시스템 필수** — 어떤 시스템이 어떤 역할인지 먼저 명시
- **시간축 시퀀스 다이어그램** — ASCII 코드 블록으로 작성
- **DESIGN 역참조 필수** — 첫 줄에 명시한다

## 네이밍

- 파일명: `{번호}-{kebab-case}.md` (예: `01-deployment.md`)
- 번호는 논리적 순서 (등록 → 권한 → 조회 → 사용)

## 금지 사항

- 컴포넌트 상세 스펙을 flow에 넣지 않는다 → concern 위임
- 인터페이스/필드 정의를 flow에 넣지 않는다 → 흐름만 기술
- 실제 SQL/코드 스니펫 금지 — ASCII 시퀀스 또는 슈도코드만
```

### Step 5: 결과 요약

생성된 파일과 paths를 출력합니다:

```
scaffold-spec-rules 완료!

감지 결과:
  spec 루트: {spec_root}
  프로젝트명: {project_name}

생성된 룰 파일:
  .claude/rules/spec-concern.md   (paths: {concerns_path}/**/*.md)
  .claude/rules/spec-design.md    (paths: {spec_root}/README.md, {spec_root}/DESIGN.md)
  .claude/rules/spec-flow.md      (paths: {flows_path}/**/*.md)

다음 단계:
  - 생성된 파일의 placeholder({project} 등)를 프로젝트에 맞게 보강
  - 새 스펙 문서를 추가할 때마다 README 라우팅 테이블 갱신
```

`--gap-only`로 일부만 생성된 경우 skip된 파일도 함께 보고합니다.

## 에러 처리

**spec 디렉토리가 발견되지 않는 경우:**

- `AskUserQuestion`으로 사용자에게 spec 루트 직접 입력 요청
- 사용자가 cancel하면 종료하고 "spec 디렉토리가 없어 생성 대상이 없음"을 보고

**프로젝트명 추출 실패:**

- placeholder `{project}`로 두고 사용자에게 직접 입력 요청

**기존 룰 파일 충돌:**

- 사용자에게 덮어쓸지 / skip할지 / merge할지 선택 요청
- `--gap-only` 옵션은 충돌 시 자동 skip

**`.claude/rules/` 디렉토리 부재:**

- `Bash: mkdir -p .claude/rules`로 생성 후 진행

## Output Examples

### 성공 케이스

```
감지 결과:
  spec 루트: spec/
  프로젝트명: my-service
  카테고리: concern (yes), design (yes), flow (yes)

생성된 룰 파일:
  .claude/rules/spec-concern.md   (paths: spec/concerns/**/*.md)
  .claude/rules/spec-design.md    (paths: spec/README.md, spec/DESIGN.md)
  .claude/rules/spec-flow.md      (paths: spec/flows/**/*.md)
```

### `--gap-only` 케이스

```
기존 파일 보존:
  .claude/rules/spec-concern.md   (skip)

새로 생성:
  .claude/rules/spec-design.md
  .claude/rules/spec-flow.md
```

### spec 디렉토리 부재

```
spec 디렉토리를 찾을 수 없습니다. 생성 대상이 없어 종료합니다.
스펙 디렉토리를 만든 뒤 다시 실행해주세요.
```
