---
description: "github-autopilot 플러그인 초기 설정. 프로젝트에 rules 설치 및 설정 파일 생성"
argument-hint: ""
allowed-tools: ["Bash", "Read", "Write", "Glob", "AskUserQuestion"]
---

# Setup

github-autopilot 플러그인의 초기 설정을 수행합니다.

## 사용법

```bash
/github-autopilot:setup
```

## Context

- Setup 절차: !`cat ${CLAUDE_PLUGIN_ROOT}/skills/setup-init/SKILL.md`

## 작업 프로세스

### Step 1: 현재 상태 확인

프로젝트에 이미 설정이 있는지 확인합니다:
- `.claude/rules/autopilot-*.md` 존재 여부
- `github-autopilot.local.md` 존재 여부

### Step 2: Rules 설치

setup-init 스킬의 "Rules 파일" 섹션을 참조하여 `.claude/rules/` 디렉토리에 규칙 파일을 설치합니다.

이미 존재하는 파일은 AskUserQuestion으로 덮어쓸지 확인합니다.

### Step 3: 설정 파일 생성

`github-autopilot.local.md`가 없으면 setup-init 스킬의 "설정 파일 템플릿" 섹션을 참조하여 생성합니다.

### Step 4: PR Base Branch Guard Hook 설치

setup-init 스킬의 "PR Base Branch Guard Hook" 섹션을 참조하여 `.claude/settings.local.json`에 hook을 추가합니다.

주의사항:
- **settings.local.json 사용**: 프로젝트 공용 `settings.json`이 아닌 로컬 전용 파일에 설치
- **기존 Bash matcher와 병합**: 이미 Bash matcher가 있으면 `settings.local.json`의 hook이 추가로 실행됨
- **이미 설치되어 있으면 skip**: `guard-pr-base.sh`가 이미 등록되어 있으면 중복 추가하지 않음

### Step 5: GitHub 라벨 생성

setup-init 스킬의 "GitHub 라벨" 섹션을 참조하여 라벨을 일괄 생성합니다.

### Step 6: 결과 보고

```
## Setup 완료

### 설치된 Rules
- .claude/rules/autopilot-always-pull-first.md ✅
- .claude/rules/autopilot-draft-branch.md ✅

### 설정 파일
- github-autopilot.local.md ✅ (생성됨 / 이미 존재)

### Hook
- .claude/settings.local.json → guard-pr-base hook ✅ (설치됨 / 이미 존재)

### GitHub 라벨
- autopilot:ready ✅
- autopilot:wip ✅
- autopilot:ci-failure ✅
- autopilot:auto ✅

### 다음 단계
1. `github-autopilot.local.md`의 설정을 프로젝트에 맞게 수정하세요
2. `/github-autopilot:autopilot`으로 전체 루프를 시작하거나
3. 개별 커맨드를 1회 실행하세요 (예: `/github-autopilot:gap-watch`)
```
