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

## 작업 프로세스

### Step 1: 현재 상태 확인

프로젝트에 이미 설정이 있는지 확인합니다:
- `.claude/rules/autopilot-*.md` 존재 여부
- `github-autopilot.local.md` 존재 여부

### Step 2: Rules 설치

`.claude/rules/` 디렉토리에 autopilot 규칙 파일을 설치합니다.

이미 존재하는 파일은 AskUserQuestion으로 덮어쓸지 확인합니다.

#### autopilot-always-pull-first.md

```markdown
---
paths:
  - "**"
---

# Always Pull First (github-autopilot)

github-autopilot의 모든 agent와 command는 작업 전 반드시 최신 변경사항을 가져와야 합니다.

## 규칙

작업 시작 시 아래 명령을 실행합니다:

\`\`\`bash
git fetch origin
\`\`\`

현재 브랜치가 remote tracking 브랜치가 있는 경우:

\`\`\`bash
git pull --rebase origin $(git branch --show-current)
\`\`\`

## 이유

autopilot은 주기적으로 실행되므로, 이전 실행 이후 변경된 내용을 반영하지 않으면 충돌이나 중복 작업이 발생합니다.
```

#### autopilot-draft-branch.md

```markdown
---
paths:
  - "**"
---

# Draft Branch Convention (github-autopilot)

## 브랜치 네이밍

| 용도 | 패턴 | remote push |
|------|------|-------------|
| `draft/*` | agent 작업용 | 금지 (로컬 only) |
| `feature/*` | PR 생성용 | 허용 |

## 금지 사항

- draft/* 브랜치를 `git push`하지 않는다
- main, develop 브랜치에 직접 커밋하지 않는다
- 기존 feature/* 브랜치를 덮어쓰지 않는다 (이미 존재하면 skip)

## 승격 조건

- Quality gate (fmt, lint, test) 통과 후에만 승격
- 승격 후 draft 브랜치는 즉시 삭제
- PR 라벨에 autopilot 접두사 포함 필수
```

### Step 3: 설정 파일 생성

`github-autopilot.local.md`가 없으면 템플릿을 생성합니다:

```markdown
---
branch_strategy: "draft-main"
work_branch: ""               # 에이전트 작업 base 브랜치 (비어있으면 branch_strategy에 따라 결정)
auto_promote: true
label_prefix: "autopilot:"
spec_paths:
  - "spec/"
  - "docs/spec/"
default_intervals:
  gap_watch: "30m"
  build_issues: "15m"
  merge_prs: "10m"
  ci_watch: "20m"
  ci_fix: "15m"
  qa_boost: "1h"
notification: ""              # skip 이슈 알림 방법 (자연어, 예: "Slack DM으로 @irene에게 알려줘")
quality_gate_command: ""      # 커스텀 quality gate 명령어 (비어있으면 자동 감지)
max_consecutive_failures: 3   # 연속 실패 허용 횟수, 초과 시 에스컬레이션
max_ci_fix_retries: 3         # CI fix 루프 최대 재시도 횟수
test_watch: []                # 테스트 스위트 정의 (예: [{name: "e2e", command: "npm run test:e2e", interval: "2h"}])
---

# github-autopilot Configuration

이 파일은 github-autopilot 플러그인의 설정 파일입니다.
위 YAML frontmatter의 값을 프로젝트에 맞게 수정하세요.

## work_branch

에이전트가 작업할 base 브랜치를 지정합니다.
설정하면 모든 에이전트가 이 브랜치에서 draft 브랜치를 분기하고, PR의 base도 이 브랜치가 됩니다.
비어있으면 branch_strategy에 따라 자동 결정됩니다 (draft-main → main, draft-develop-main → develop).

예시: `work_branch: "alpha"` → alpha에서 분기, PR base도 alpha
```

### Step 4: GitHub 라벨 생성

autopilot이 사용하는 라벨을 레포에 일괄 생성합니다.
`label_prefix`는 Step 3에서 생성한 설정 파일의 값을 사용합니다 (기본값: `autopilot:`).

```bash
# 이미 존재하는 라벨은 skip (--force 없음)
gh label create "{label_prefix}ready" --color "0E8A16" --description "Autopilot 구현 대상" 2>/dev/null || true
gh label create "{label_prefix}wip" --color "FBCA04" --description "Autopilot 구현 진행 중" 2>/dev/null || true
gh label create "{label_prefix}ci-failure" --color "D93F0B" --description "CI 실패 이슈" 2>/dev/null || true
gh label create "{label_prefix}auto" --color "1D76DB" --description "Autopilot 자동 생성 PR" 2>/dev/null || true
```

### Step 5: 결과 보고

설치된 파일 목록을 출력합니다:

```
## Setup 완료

### 설치된 Rules
- .claude/rules/autopilot-always-pull-first.md ✅
- .claude/rules/autopilot-draft-branch.md ✅

### 설정 파일
- github-autopilot.local.md ✅ (생성됨 / 이미 존재)

### GitHub 라벨
- autopilot:ready ✅
- autopilot:wip ✅
- autopilot:ci-failure ✅
- autopilot:auto ✅

### 다음 단계
1. `github-autopilot.local.md`의 설정을 프로젝트에 맞게 수정하세요
2. `/github-autopilot:autopilot`으로 전체 루프를 시작하거나
3. 개별 커맨드를 실행하세요 (예: `/github-autopilot:ci-watch 20m`)
```
