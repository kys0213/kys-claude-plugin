---
description: "github-autopilot 플러그인 초기 설정. 설정 파일 생성 및 hook/CLI 설치 (user scope)"
argument-hint: ""
allowed-tools: ["Bash", "Read", "Write", "Glob", "AskUserQuestion"]
---

# Setup

github-autopilot 플러그인의 초기 설정을 수행합니다. 모든 hook은 user scope(`~/.claude/settings.json`)에 설치됩니다.

## 사용법

```bash
/github-autopilot:setup
```

## 작업 프로세스

### Step 1: 현재 상태 확인

프로젝트에 이미 설정이 있는지 확인합니다:
- `github-autopilot.local.md` 존재 여부

### Step 2: 설정 파일 생성

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
event_mode: "hybrid"          # "hybrid" (이벤트 드리븐 + cron) | "cron" (기존 폴링 전용)
monitor:
  poll_sec: 60                # Events API 폴링 간격 (초). 서버의 X-Poll-Interval이 더 크면 자동 존중
default_intervals:
  gap_watch: "30m"            # cron 모드에서만 사용
  analyze_issue: "20m"        # cron 모드에서만 사용
  build_issues: "15m"         # hybrid/cron 모두 사용
  merge_prs: "10m"            # cron 모드에서만 사용
  ci_watch: "20m"             # cron 모드에서만 사용
  ci_fix: "15m"               # cron 모드에서만 사용
  qa_boost: "1h"              # cron 모드에서만 사용
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

## event_mode

autopilot 루프의 실행 모드를 지정합니다.

- **hybrid** (기본): Monitor 기반 이벤트 드리븐. main 브랜치 변경, CI 완료, 새 이슈 등의 이벤트가 발생할 때만 해당 커맨드를 트리거합니다. build-issues와 test-watch만 CronCreate로 동작합니다.
- **cron**: 기존 CronCreate 기반 폴링. 모든 커맨드가 고정 간격으로 실행됩니다.

## monitor

hybrid 모드에서 Events API 폴링 간격(초)을 설정합니다.
ETag 기반 conditional request를 사용하므로 변경이 없으면 rate limit을 소비하지 않습니다.
서버의 `X-Poll-Interval` 헤더 값이 설정보다 크면 자동으로 존중합니다.
```

### Step 3: Hook 설치 (user scope)

`~/.claude/settings.json`에 아래 hook들을 추가합니다.

> 모든 hook은 `github-autopilot.local.md`가 없는 프로젝트에서는 자동 skip됩니다.

#### PreToolUse: PR Base Branch Guard

```json
{
  "matcher": "mcp__github__create_pull_request",
  "hooks": [
    {
      "type": "command",
      "command": "bash ${CLAUDE_PLUGIN_ROOT}/hooks/guard-pr-base.sh",
      "timeout": 5
    }
  ]
},
{
  "matcher": "Bash",
  "hooks": [
    {
      "type": "command",
      "command": "bash ${CLAUDE_PLUGIN_ROOT}/hooks/guard-pr-base.sh",
      "timeout": 5
    }
  ]
}
```

#### SessionStart: CLI Version Check

```json
{
  "hooks": [
    {
      "type": "command",
      "command": "bash ${CLAUDE_PLUGIN_ROOT}/hooks/check-cli-version.sh",
      "timeout": 10
    }
  ]
}
```

#### Hook 설명

| Hook | 이벤트 | 동작 |
|------|--------|------|
| `guard-pr-base.sh` | PreToolUse | 설정된 base branch 외 PR 생성 차단 |
| `check-cli-version.sh` | SessionStart | CLI 버전이 과거이면 업데이트 안내 |

#### 주의사항

- **이미 설치되어 있으면 skip**: 각 hook이 이미 등록되어 있으면 중복 추가하지 않습니다.
- user scope이므로 한번 설치하면 모든 프로젝트에서 동작합니다.

### Step 4: autopilot CLI 설치

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

### Step 5: GitHub 라벨 생성

autopilot이 사용하는 라벨을 레포에 일괄 생성합니다.
`label_prefix`는 Step 2에서 생성한 설정 파일의 값을 사용합니다 (기본값: `autopilot:`).

```bash
# 이미 존재하는 라벨은 skip (--force 없음)
gh label create "{label_prefix}ready" --color "0E8A16" --description "Autopilot 구현 대상" 2>/dev/null || true
gh label create "{label_prefix}wip" --color "FBCA04" --description "Autopilot 구현 진행 중" 2>/dev/null || true
gh label create "{label_prefix}ci-failure" --color "D93F0B" --description "CI 실패 이슈" 2>/dev/null || true
gh label create "{label_prefix}auto" --color "1D76DB" --description "Autopilot 자동 생성 PR" 2>/dev/null || true
gh label create "{label_prefix}qa-suggestion" --color "C5DEF5" --description "QA 테스트 제안 (검토 후 ready로 전환)" 2>/dev/null || true
gh label create "{label_prefix}spec-needed" --color "BFD4F2" --description "역방향 갭 — 스펙 정의 필요" 2>/dev/null || true
```

### Step 6: 결과 보고

설치된 항목 목록을 출력합니다:

```
## Setup 완료

### 설정 파일
- github-autopilot.local.md ✅ (생성됨 / 이미 존재)

### Hook (user scope: ~/.claude/settings.json)
- PreToolUse → guard-pr-base ✅
- SessionStart → check-cli-version ✅

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
3. 개별 커맨드를 실행하세요 (예: `/github-autopilot:ci-watch`)
```
