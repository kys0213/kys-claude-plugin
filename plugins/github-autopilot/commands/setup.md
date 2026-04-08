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
spec_quality_threshold: "C"   # 최소 스펙 품질 등급 (A/B/C/D), preflight에서 검증
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

### Step 4: PR Base Branch Guard Hook 설치

프로젝트의 `.claude/settings.local.json`에 PreToolUse hook을 추가하여,
autopilot agent가 설정된 base branch 외의 브랜치로 PR을 생성하는 것을 차단합니다.

> 이 hook은 `github-autopilot.local.md` 설정 파일이 존재할 때만 동작합니다.
> 설정 파일이 없는 프로젝트에서는 자동으로 skip됩니다.

#### 설치 방법

`.claude/settings.local.json` 파일의 `hooks.PreToolUse` 배열에 아래 항목들을 추가합니다:

```json
{
  "matcher": "mcp__github__create_pull_request",
  "hooks": [
    {
      "type": "command",
      "command": "bash plugins/github-autopilot/hooks/guard-pr-base.sh",
      "timeout": 5
    }
  ]
},
{
  "matcher": "Bash",
  "hooks": [
    {
      "type": "command",
      "command": "bash plugins/github-autopilot/hooks/guard-pr-base.sh",
      "timeout": 5
    },
    {
      "type": "command",
      "command": "bash plugins/github-autopilot/hooks/check-cli-version.sh",
      "timeout": 5
    }
  ]
}
```

#### Hook 설명

| Hook | 트리거 | 동작 |
|------|--------|------|
| `guard-pr-base.sh` | Bash, MCP PR 생성 | 설정된 base branch 외 PR 생성 차단 |
| `check-cli-version.sh` | Bash | 세션당 1회, CLI 버전이 과거이면 업데이트 안내 |

#### 주의사항

- **settings.local.json 사용**: 프로젝트 공용 `settings.json`이 아닌 로컬 전용 파일에 설치합니다.
  autopilot hook은 autopilot을 사용하는 환경에서만 필요하기 때문입니다.
- **기존 Bash matcher와 병합**: 이미 Bash matcher가 `.claude/settings.json`에 있는 경우,
  `settings.local.json`의 hook이 추가로 실행됩니다.
- **이미 설치되어 있으면 skip**: 각 hook이 이미 등록되어 있으면 중복 추가하지 않습니다.

### Step 5: autopilot CLI 설치

autopilot CLI 바이너리를 빌드하여 `~/.local/bin/autopilot`에 설치합니다.

```bash
bash ${CLAUDE_PLUGIN_ROOT}/scripts/ensure-binary.sh
```

이미 최신 버전이 설치되어 있으면 skip합니다.

> `~/.local/bin`이 `$PATH`에 포함되어 있는지 확인합니다. 포함되어 있지 않으면 안내합니다:
> ```
> ~/.local/bin이 PATH에 없습니다. 셸 설정에 추가해주세요:
> export PATH="$HOME/.local/bin:$PATH"
> ```

### Step 6: GitHub 라벨 생성

autopilot이 사용하는 라벨을 레포에 일괄 생성합니다.
`label_prefix`는 Step 3에서 생성한 설정 파일의 값을 사용합니다 (기본값: `autopilot:`).

```bash
# 이미 존재하는 라벨은 skip (--force 없음)
gh label create "{label_prefix}ready" --color "0E8A16" --description "Autopilot 구현 대상" 2>/dev/null || true
gh label create "{label_prefix}wip" --color "FBCA04" --description "Autopilot 구현 진행 중" 2>/dev/null || true
gh label create "{label_prefix}ci-failure" --color "D93F0B" --description "CI 실패 이슈" 2>/dev/null || true
gh label create "{label_prefix}auto" --color "1D76DB" --description "Autopilot 자동 생성 PR" 2>/dev/null || true
gh label create "{label_prefix}qa-suggestion" --color "C5DEF5" --description "QA 테스트 제안 (검토 후 ready로 전환)" 2>/dev/null || true
gh label create "{label_prefix}spec-needed" --color "BFD4F2" --description "역방향 갭 — 스펙 정의 필요" 2>/dev/null || true
```

### Step 7: 결과 보고

설치된 파일 목록을 출력합니다:

```
## Setup 완료

### 설치된 Rules
- .claude/rules/autopilot-always-pull-first.md ✅
- .claude/rules/autopilot-draft-branch.md ✅

### 설정 파일
- github-autopilot.local.md ✅ (생성됨 / 이미 존재)

### Hook
- .claude/settings.local.json → guard-pr-base hook ✅ (설치됨 / 이미 존재)
- .claude/settings.local.json → check-cli-version hook ✅ (설치됨 / 이미 존재)

### CLI
- autopilot CLI ✅ (~/.local/bin/autopilot)

### GitHub 라벨
- autopilot:ready ✅
- autopilot:wip ✅
- autopilot:ci-failure ✅
- autopilot:auto ✅
- autopilot:qa-suggestion ✅
- autopilot:spec-needed ✅

### 다음 단계
1. `github-autopilot.local.md`의 설정을 프로젝트에 맞게 수정하세요
2. `/github-autopilot:autopilot`으로 전체 루프를 시작하거나
3. 개별 커맨드를 실행하세요 (예: `/github-autopilot:ci-watch 20m`)
```
