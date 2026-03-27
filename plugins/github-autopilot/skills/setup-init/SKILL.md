---
name: setup-init
description: autopilot 초기 설정 절차와 템플릿. setup command와 autopilot command에서 공유 참조
version: 1.0.0
---

# Setup Initialization

autopilot 초기 설정에 필요한 절차, 템플릿, 리소스 정의.

## 설정 파일 템플릿

`github-autopilot.local.md` 파일의 기본 템플릿:

```markdown
---
branch_strategy: "draft-main"
work_branch: ""               # 에이전트 작업 base 브랜치 (비어있으면 branch_strategy에 따라 결정)
auto_promote: true
label_prefix: "autopilot:"
spec_paths:
  - "spec/"
  - "docs/spec/"
loops:
  gap_watch:
    interval: "30m"
    enabled: true
  build_issues:
    interval: "15m"
    enabled: true
  merge_prs:
    interval: "10m"
    enabled: true
  ci_watch:
    interval: "20m"
    enabled: true
  ci_fix:
    interval: "15m"
    enabled: true
  qa_boost:
    interval: "1h"
    enabled: true
custom_loops: []              # 사용자 정의 루프 (예: [{name: "e2e", command: "/my-project:run-e2e", interval: "2h"}])
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

## Rules 파일

### autopilot-always-pull-first.md

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

### autopilot-draft-branch.md

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

## GitHub 라벨

autopilot이 사용하는 라벨 (label_prefix 기본값: `autopilot:`):

| 라벨 | 색상 | 설명 |
|------|------|------|
| `{label_prefix}ready` | `#0E8A16` | Autopilot 구현 대상 |
| `{label_prefix}wip` | `#FBCA04` | Autopilot 구현 진행 중 |
| `{label_prefix}ci-failure` | `#D93F0B` | CI 실패 이슈 |
| `{label_prefix}auto` | `#1D76DB` | Autopilot 자동 생성 PR |

라벨 생성 명령:

```bash
gh label create "{label_prefix}ready" --color "0E8A16" --description "Autopilot 구현 대상" 2>/dev/null || true
gh label create "{label_prefix}wip" --color "FBCA04" --description "Autopilot 구현 진행 중" 2>/dev/null || true
gh label create "{label_prefix}ci-failure" --color "D93F0B" --description "CI 실패 이슈" 2>/dev/null || true
gh label create "{label_prefix}auto" --color "1D76DB" --description "Autopilot 자동 생성 PR" 2>/dev/null || true
```

## PR Base Branch Guard Hook

`.claude/settings.local.json`에 추가하는 PreToolUse hook:

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
    }
  ]
}
```
