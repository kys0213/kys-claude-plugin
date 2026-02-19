---
description: develop-workflow 플러그인 초기 설정. SessionStart hook 등을 사용자/프로젝트 단위로 설정합니다.
allowed-tools:
  - Read
  - Bash
  - Write
  - AskUserQuestion
---

# Develop Workflow Setup

이 커맨드는 develop-workflow 플러그인의 hook을 설정합니다. HITL(Human-in-the-Loop)로 설치 범위를 선택합니다.

## Step 1: 환경 확인

먼저 git 저장소인지 확인합니다:

```bash
git rev-parse --is-inside-work-tree 2>/dev/null
```

**git 저장소가 아닌 경우:**

```
이 디렉토리는 git 저장소가 아닙니다.
develop-workflow는 git 저장소에서만 동작합니다.

먼저 git init을 실행하거나, git 저장소 디렉토리에서 다시 실행해주세요.
```

> 설정을 중단합니다.

**git 저장소인 경우**, git-utils CLI 존재 여부를 확인합니다:

```bash
which git-utils 2>/dev/null || echo "NOT_FOUND"
```

**git-utils가 없는 경우:**

```
git-utils CLI가 설치되어 있지 않습니다.
먼저 git-utils /setup을 실행해주세요.
```

> 설정을 중단합니다.

## Step 2: 기존 설정 확인

이미 hook이 설치되어 있는지 확인합니다:

```bash
git-utils hook list SessionStart
git-utils hook list SessionStart --project-dir="$HOME"
```

detect-ralph-state.sh 관련 hook이 이미 있으면 사용자에게 알립니다:

```
기존 SessionStart hook이 발견되었습니다.
└─ detect-ralph-state.sh: 등록됨

옵션:
1. 덮어쓰기 - 최신 설정으로 재등록
2. 취소 - 기존 설정 유지
```

AskUserQuestion으로 선택받습니다.

## Step 3: 설치 범위 선택

AskUserQuestion으로 설치 범위를 선택받습니다:

```
hook 설치 범위를 선택하세요.
```

옵션:
1. **프로젝트 (Recommended)** - 이 프로젝트에만 적용 (.claude/settings.json). 팀과 공유 가능
2. **사용자** - 모든 프로젝트에 적용 (~/.claude/settings.json). 개인 설정

### 범위별 차이

| 항목 | 프로젝트 | 사용자 |
|------|----------|--------|
| settings.json 위치 | `.claude/settings.json` | `~/.claude/settings.json` |
| hook 스크립트 경로 | 상대경로 (`./plugins/...`) | 절대경로 bake-in |
| 팀 공유 | git commit으로 공유 가능 | 불가 (개인 설정) |
| 적용 범위 | 이 프로젝트만 | 모든 프로젝트 (state.json 없으면 자동 스킵) |

## Step 4: Hook 등록

### 프로젝트 범위 선택 시

```bash
git-utils hook register SessionStart "*" \
  "bash ./plugins/develop-workflow/hooks/detect-ralph-state.sh" \
  --timeout=5
```

### 사용자 범위 선택 시

플러그인의 절대 경로를 bake-in합니다:

```bash
PLUGIN_DIR=$(cd "$(dirname "$0")/.." && pwd)
```

```bash
git-utils hook register SessionStart "*" \
  "bash $PLUGIN_DIR/hooks/detect-ralph-state.sh" \
  --timeout=5 \
  --project-dir="$HOME"
```

## Step 5: 완료 메시지

### 프로젝트 범위

```
develop-workflow hook이 프로젝트에 설정되었습니다!

설정된 hook:
├─ SessionStart → detect-ralph-state.sh
└─ .claude/settings.json (hook 등록됨)

SessionStart Hook:
├─ 세션 시작 시 .develop-workflow/state.json 감지
├─ 진행 중인 워크플로우가 있으면 상태를 표시
└─ state.json이 없으면 자동 스킵

.claude/ 디렉토리를 git에 커밋하면 팀과 설정을 공유할 수 있습니다.
```

### 사용자 범위

```
develop-workflow hook이 사용자 설정에 등록되었습니다!

설정된 hook:
├─ SessionStart → detect-ralph-state.sh
└─ ~/.claude/settings.json (hook 등록됨)

SessionStart Hook:
├─ 세션 시작 시 .develop-workflow/state.json 감지
├─ 진행 중인 워크플로우가 있으면 상태를 표시
└─ state.json이 없으면 자동 스킵 (git 저장소가 아니어도 안전)
```

## 예시 실행 흐름

### 프로젝트 범위로 신규 설정

```
프로젝트 환경을 분석 중...

감지된 환경:
├─ Git 저장소: 확인
└─ git-utils CLI: 확인

hook 설치 범위를 선택하세요.
[프로젝트 (Recommended)] [사용자]

> 프로젝트 선택

develop-workflow hook이 프로젝트에 설정되었습니다!

설정된 hook:
├─ SessionStart → detect-ralph-state.sh
└─ .claude/settings.json (hook 등록됨)
```

### 기존 설정이 있는 경우

```
프로젝트 환경을 분석 중...

기존 SessionStart hook이 발견되었습니다.
└─ detect-ralph-state.sh: 등록됨

[덮어쓰기] [취소]

> 덮어쓰기 선택

hook 설치 범위를 선택하세요.
[프로젝트 (Recommended)] [사용자]

> 프로젝트 선택

develop-workflow hook이 프로젝트에 설정되었습니다!
```
