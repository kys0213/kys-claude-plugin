---
description: barrier-sync 플러그인의 SubagentStop hook을 등록합니다. 병렬 background Task 동기화를 위한 사전 설정.
allowed-tools:
  - Bash
  - Read
  - Write
  - AskUserQuestion
---

# Barrier Setup

barrier-sync 플러그인의 SubagentStop hook을 등록합니다.

## Step 1: 현재 상태 확인

프로젝트와 사용자 범위의 설정 파일을 확인합니다:

```bash
# 프로젝트 범위
cat .claude/settings.json 2>/dev/null || echo "{}"

# 사용자 범위
cat ~/.claude/settings.json 2>/dev/null || echo "{}"
```

SubagentStop hook에 `signal-done.cjs`가 이미 등록되어 있는지 확인합니다.

### 이미 등록된 경우

```
barrier-sync hook이 이미 등록되어 있습니다.

등록 위치: {settings_path}
등록 내용: SubagentStop → signal-done.cjs

변경이 필요하면 /hook-config를 사용하세요.
```

## Step 2: 등록 범위 선택

AskUserQuestion으로 등록 범위를 선택받습니다:

```
SubagentStop hook을 어디에 등록하시겠습니까?
```

옵션:
1. **프로젝트** - `.claude/settings.json` (이 프로젝트에서만 동작)
2. **사용자** - `~/.claude/settings.json` (모든 프로젝트에서 동작)

## Step 3: Hook 등록

선택된 범위의 `settings.json`을 읽어서 SubagentStop hook을 추가합니다.

### signal-done.cjs 경로 결정

플러그인 설치 경로를 기준으로 signal-done.cjs의 절대 경로를 결정합니다:

```bash
# 이 파일 기준으로 hooks/ 디렉토리 위치 결정
PLUGIN_DIR="$(cd "$(dirname "$0")/.." && pwd)"
HOOK_SCRIPT="${PLUGIN_DIR}/hooks/signal-done.cjs"
```

### settings.json 수정

기존 settings.json에 SubagentStop 항목을 추가합니다:

```json
{
  "hooks": {
    "SubagentStop": [
      {
        "matcher": "",
        "hooks": [
          {
            "command": "node /path/to/plugins/barrier-sync/hooks/signal-done.cjs",
            "timeout": 10
          }
        ]
      }
    ]
  }
}
```

**주의:** 기존 hooks 설정이 있으면 SubagentStop만 추가하고, 다른 hook은 건드리지 않습니다.

## Step 4: 검증

등록 후 설정을 다시 읽어서 올바르게 추가되었는지 확인합니다.

완료 메시지:

```
barrier-sync hook이 등록되었습니다!

등록 위치: {settings_path}
등록 내용: SubagentStop → signal-done.cjs

사용법:
  1. background Bash로 wait-for-tasks.sh 실행
  2. background Task 여러 개 실행
  3. wait-for-tasks.sh 결과를 Read로 확인

자세한 사용법은 barrier-sync 스킬을 참조하세요.
```
