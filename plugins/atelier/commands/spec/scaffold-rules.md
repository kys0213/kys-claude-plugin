---
description: 프로젝트의 spec/ 디렉토리를 분석하여 .claude/rules/spec-*.md 파일을 생성합니다
argument-hint: "[--gap-only]"
allowed-tools:
  - Glob
  - Read
  - AskUserQuestion
  - Bash
---

# Scaffold Spec Rules Command

> 프로젝트의 스펙 디렉토리 구조(`spec/`)를 분석하여 작성 컨벤션 룰(`.claude/rules/spec-*.md`)을 자동 생성합니다.

## Overview

스펙 문서를 다루는 프로젝트가 `common / concern / design / flow` 4종 컨벤션을 표준화하여 사용할 수 있도록, 공통 룰(common)은 spec 루트가 확정되면 항상 생성하고 도메인별 룰은 발견된 디렉토리 구조에 맞춰 생성합니다.

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

- `--gap-only` 옵션이고 `common/concern/design/flow` 4개가 모두 존재하면 **여기서 즉시 종료**합니다 (이후 단계 스킵).
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
  - common: spec_root가 확정되면 **항상 활성** (모든 spec 문서에 적용되는 공통 톤 정책)
  - concern: `{spec_root}/concerns/**/*.md` 매칭 ≥ 1
  - design: `{spec_root}/README.md` 또는 `{spec_root}/DESIGN.md` 매칭
  - flow: `{spec_root}/flows/**/*.md` 매칭 ≥ 1
  - 미발견 카테고리(common 제외)는 비활성 (사용자가 Step 2에서 강제 활성화 가능)
- **`{project}`** (단일 placeholder, `{project_name}` 사용 안 함): 우선순위 — `package.json#name` > `go.mod#module` 마지막 segment > `Cargo.toml#[package].name` > `pyproject.toml#[project].name` 또는 `[tool.poetry].name` > git remote / 디렉토리명. 모두 실패 시 placeholder 유지하고 Step 2에서 사용자 입력 요청.
- **`{concerns_path}`** = `{spec_root}/concerns`, **`{flows_path}`** = `{spec_root}/flows` (Step 4 frontmatter `paths:` 치환에 사용).

### Step 2: 감지 결과 확인 (HITL)

```
AskUserQuestion: |
  자동 감지 결과:
    - spec 루트: {spec_root}
    - 프로젝트명: {project}
    - 발견 카테고리: concern={yes/no}, design={yes/no}, flow={yes/no}
    - common: 항상 생성 (spec 전반 공통 톤 정책)
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
| common | `.claude/rules/spec-common.md` | `["{spec_root}/**/*.md"]` |
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

승인된 카테고리별로 `render-template` CLI 를 호출하여 placeholder 를 치환한 결과를 `.claude/rules/spec-*.md` 로 작성합니다. 에이전트는 변수 값 결정만 수행하고, 템플릿 Read·치환·Write 는 모두 CLI 에 위임합니다 (결정적 변환, 멱등).

| 카테고리 | 템플릿 | 출력 |
|---|---|---|
| common | `${CLAUDE_PLUGIN_ROOT}/templates/spec-common.md` | `.claude/rules/spec-common.md` |
| concern | `${CLAUDE_PLUGIN_ROOT}/templates/spec-concern.md` | `.claude/rules/spec-concern.md` |
| design | `${CLAUDE_PLUGIN_ROOT}/templates/spec-design.md` | `.claude/rules/spec-design.md` |
| flow | `${CLAUDE_PLUGIN_ROOT}/templates/spec-flow.md` | `.claude/rules/spec-flow.md` |

치환 변수 (Step 1-C에서 산출됨):

| 변수 | 값 |
|---|---|
| `{project}` | 프로젝트명 (`package.json` 등에서 추출, 실패 시 사용자 입력) |
| `{spec_root}` | 확정된 spec 루트 (`spec`, `docs/spec` 등) |
| `{concerns_path}` | `{spec_root}/concerns` |
| `{flows_path}` | `{spec_root}/flows` |

#### 호출 방식

승인된 카테고리마다 다음을 `Bash` 로 실행합니다 (카테고리 개수만큼 반복):

```bash
${CLAUDE_PLUGIN_ROOT}/../../common/scripts/render-template.sh \
  ${CLAUDE_PLUGIN_ROOT}/templates/spec-<category>.md \
  .claude/rules/spec-<category>.md \
  project=<project> \
  spec_root=<spec_root> \
  concerns_path=<concerns_path> \
  flows_path=<flows_path>
```

**출력 (stdout):** `rendered: .claude/rules/spec-<category>.md`
**Exit 0**: 성공. **Exit 1**: 템플릿 부재 또는 placeholder 인자 누락. **Exit 2**: 인자 형식 오류.

CLI 는 출력 디렉토리(`.claude/rules/`)가 없으면 `mkdir -p` 자동, 출력 파일이 존재하면 덮어쓰기 (Step 3 HITL 가 이미 overwrite 를 결정한 후 호출). 한국어 placeholder 비슷한 패턴(예: `{컴포넌트명}`, `{유스케이스명}`)은 정규식 `\{[a-zA-Z_][a-zA-Z0-9_]*\}` 비매칭이므로 본문 예시 자리에 그대로 보존됩니다.

concern/design/flow 세 파일은 같은 `## 다이어그램 분담` 섹션을 자체 포함합니다. 단독 로드 시에도 의미가 통하도록 의도된 중복이며, 컬럼 라벨은 모두 `종류 | 위치 | 기준`으로 통일합니다. common 은 도메인 무관 톤 정책이므로 다이어그램 분담을 포함하지 않습니다.

### Step 5: 결과 요약

생성된 파일과 paths를 출력합니다:

```
scaffold-rules 완료!

감지 결과:
  spec 루트: {spec_root}
  프로젝트명: {project}

생성된 룰 파일:
  .claude/rules/spec-common.md    (paths: {spec_root}/**/*.md)
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

- `render-template.sh` 가 출력 경로의 부모 디렉토리를 자동 `mkdir -p` 하므로 별도 처리 불필요

**`render-template.sh` exit 1 (placeholder 누락):**

- Step 1-C 산출 변수 4개(project, spec_root, concerns_path, flows_path) 중 하나가 비어 있다는 신호 — Step 1-C 로 돌아가 재시도하거나 사용자에게 누락 변수 직접 입력 요청
- stderr 메시지에 누락 키가 표시됨: `missing values for placeholder(s): <key>`

**`render-template.sh` exit 2 (인자 형식 오류):**

- 에이전트 호출 인자 조립 버그 — Bash 명령 구성을 점검하고 재시도

## Output Examples

### 성공 케이스

```
감지 결과:
  spec 루트: spec/
  프로젝트명: my-service
  카테고리: common (always), concern (yes), design (yes), flow (yes)

생성된 룰 파일:
  .claude/rules/spec-common.md    (paths: spec/**/*.md)
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
