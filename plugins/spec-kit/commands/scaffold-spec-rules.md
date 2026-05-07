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

#### 1-A. 기존 룰 파일 감지 (early-exit 체크)

`Glob: .claude/rules/spec-*.md`로 이미 존재하는 룰 파일 목록을 확보합니다.

- `--gap-only` 옵션이고 `concern/design/flow` 3개가 모두 존재하면 **여기서 즉시 종료**합니다 (이후 단계 스킵).
- 그 외에는 결과를 Step 3 충돌 해결 입력으로 전달합니다.

#### 1-B. 후보 경로 + 내부 구조 + 프로젝트명 (병렬 수집)

다음 호출을 **하나의 메시지에 묶어 병렬로 실행**합니다 (의존 관계 없음):

| 호출 | 목적 |
|---|---|
| `Glob: {spec,specs,docs/spec,.spec}/{README,DESIGN}.md` | spec 루트 후보 + design 카테고리 |
| `Glob: {spec,specs,docs/spec,.spec}/concerns/**/*.md` | concern 카테고리 |
| `Glob: {spec,specs,docs/spec,.spec}/flows/**/*.md` | flow 카테고리 |
| `Read: package.json` | 프로젝트명 후보 1 |
| `Read: go.mod` | 프로젝트명 후보 2 |
| `Read: Cargo.toml` | 프로젝트명 후보 3 |
| `Read: pyproject.toml` | 프로젝트명 후보 4 |

존재하지 않는 파일에 대한 `Read` 에러는 무시합니다.

#### 1-C. 결과 합성

- **`spec_root`**: Glob 결과의 첫 번째 경로 prefix(`spec` / `specs` / `docs/spec` / `.spec`). 여러 prefix가 동시에 잡히면 Step 2에서 사용자에게 enumerated 선택지로 제시.
- **카테고리 활성 여부**:
  - concern: `{spec_root}/concerns/**/*.md` 매칭 ≥ 1
  - design: `{spec_root}/README.md` 또는 `{spec_root}/DESIGN.md` 매칭
  - flow: `{spec_root}/flows/**/*.md` 매칭 ≥ 1
  - 미발견 카테고리는 비활성 (사용자가 Step 2에서 강제 활성화 가능)
- **`{project}`** (단일 placeholder, `{project_name}` 사용 안 함): 우선순위 — `package.json#name` > `go.mod#module` 마지막 segment > `Cargo.toml#[package].name` > `pyproject.toml#[project].name` 또는 `[tool.poetry].name` > git remote / 디렉토리명. 모두 실패 시 placeholder 유지하고 Step 2에서 사용자 입력 요청.
- **`{concerns_path}`** = `{spec_root}/concerns`, **`{flows_path}`** = `{spec_root}/flows` (Step 4 frontmatter `paths:` 치환에 사용).

### Step 2: 감지 결과 확인 (HITL)

```
AskUserQuestion: |
  자동 감지 결과:
    - spec 루트: {spec_root}
    - 프로젝트명: {project}
    - 발견 카테고리: concern={yes/no}, design={yes/no}, flow={yes/no}
    - 기존 룰 파일: {existing_files}

  이대로 진행할까요?
    1. yes — 진행
    2. spec 루트 변경
    3. 프로젝트명 수정
    4. 카테고리 강제 활성화 (감지 안 된 카테고리도 생성)
    5. cancel
```

**spec 루트 후보가 여러 개**일 때(예: `spec/`와 `docs/spec/` 둘 다 존재)는 위 질문 대신 enumerated 선택지로 먼저 단일 루트를 확정합니다:

```
AskUserQuestion: |
  여러 spec 루트가 감지되었습니다. 사용할 루트를 선택하세요:
    1. spec/
    2. docs/spec/
    ...
```

**후보 경로가 0개**이면 `AskUserQuestion`으로 사용자에게 spec 루트 직접 입력을 요청하거나 종료합니다.

### Step 3: 룰 파일 구조 제안 + 충돌 해결 (HITL)

활성 카테고리별로 다음 구조를 제안합니다:

| 카테고리 | 파일 | paths frontmatter (예시) |
|---|---|---|
| concern | `.claude/rules/spec-concern.md` | `["{concerns_path}/**/*.md"]` |
| design | `.claude/rules/spec-design.md` | `["{spec_root}/README.md", "{spec_root}/DESIGN.md"]` |
| flow | `.claude/rules/spec-flow.md` | `["{flows_path}/**/*.md"]` |

Step 1-A에서 감지한 기존 룰 파일이 있으면 같은 질문에서 **충돌 해결 옵션을 함께 노출**합니다 (`--gap-only`이면 자동 skip되어 이 질문이 표시되지 않음).

```
AskUserQuestion: |
  생성 후보:
    1. [new]      .claude/rules/spec-concern.md   (paths: spec/concerns/**/*.md)
    2. [overwrite] .claude/rules/spec-design.md   (기존 파일 존재)
    3. [skip]     .claude/rules/spec-flow.md      (감지 안 됨)

  옵션:
    - "yes": 체크된 항목 생성 (기존 파일은 overwrite)
    - 번호 (예: "1,2"): 선택만 생성
    - 항목별 skip/merge 변경 가능 (예: "2: skip" — 2번을 skip으로 변경)
    - "no": 취소
    - paths 수정 요청 가능
```

### Step 4: 룰 파일 생성

승인된 카테고리별로 아래 템플릿을 사용해 `.claude/rules/spec-*.md`를 `Write`합니다.

치환 변수 (Step 1-C에서 산출됨):

| 변수 | 값 |
|---|---|
| `{project}` | 프로젝트명 (`package.json` 등에서 추출, 실패 시 사용자 입력) |
| `{spec_root}` | 확정된 spec 루트 (`spec`, `docs/spec` 등) |
| `{concerns_path}` | `{spec_root}/concerns` |
| `{flows_path}` | `{spec_root}/flows` |

세 파일은 같은 `## 다이어그램 분담` 섹션을 자체 포함합니다. 단독 로드 시에도 의미가 통하도록 의도된 중복이며, 컬럼 라벨은 모두 `종류 | 위치 | 기준`으로 통일합니다.

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
  프로젝트명: {project}

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

- Step 3 HITL에서 항목별 `[overwrite]/[skip]/[merge]` 선택 (기본 overwrite)
- `--gap-only` 옵션은 충돌 시 자동 skip되어 Step 3에서 표시되지 않음

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
