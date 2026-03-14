---
description: HUD status line을 설치하거나 업데이트합니다
allowed-tools:
  - Bash
  - Read
  - Write
  - AskUserQuestion
---

# HUD Setup

Claude Code status line을 설치합니다. 플러그인의 `scripts/statusline.sh`를 `~/.claude/statusline-command.sh`로 복사하고 `~/.claude/settings.json`에 등록합니다.

## Step 1: 현재 상태 확인

기존 statusline 설정을 확인합니다:

```bash
cat ~/.claude/settings.json 2>/dev/null | jq '.statusLine // empty'
```

### 이미 설정된 경우

AskUserQuestion으로 덮어쓸지 확인합니다:

```
기존 statusLine 설정이 발견되었습니다.
덮어쓰시겠습니까?
```

옵션:
1. **덮어쓰기** - 최신 HUD 스크립트로 교체
2. **취소** - 기존 설정 유지

취소 선택 시 설정을 중단합니다.

## Step 2: 스크립트 복사

플러그인 디렉토리에서 statusline 스크립트를 복사합니다:

```bash
# 플러그인 scripts/statusline.sh 경로 결정
PLUGIN_DIR="이 커맨드 파일 기준으로 ../scripts"
```

`scripts/statusline.sh`를 `~/.claude/statusline-command.sh`로 복사하고 실행 권한을 부여합니다:

```bash
cp "${PLUGIN_DIR}/scripts/statusline.sh" ~/.claude/statusline-command.sh
chmod +x ~/.claude/statusline-command.sh
```

## Step 3: settings.json에 statusLine 등록

`~/.claude/settings.json`을 읽어서 `statusLine` 항목을 추가합니다.

기존 settings.json의 다른 항목(permissions, hooks 등)은 유지하고 statusLine만 추가/업데이트합니다:

```json
{
  "statusLine": {
    "type": "command",
    "command": "~/.claude/statusline-command.sh"
  }
}
```

**주의:** 기존 설정은 건드리지 않고 `statusLine` 키만 추가/업데이트합니다.

## Step 4: 검증

설치된 스크립트가 정상 동작하는지 테스트합니다:

```bash
echo '{"workspace":{"current_dir":"'$(pwd)'"},"model":{"display_name":"Test"},"context_window":{"used_percentage":50}}' | ~/.claude/statusline-command.sh
```

## Step 5: 완료 메시지

```
HUD status line이 설치되었습니다!

설치된 파일:
├─ ~/.claude/statusline-command.sh (스크립트)
└─ ~/.claude/settings.json (statusLine 등록)

표시 항목:
├─  디렉토리 (VS Code 링크)
├─ [repo:branch] (GitHub 링크)
├─  모델
└─ ████░░░░░░ 컨텍스트 (동적 색상)

업데이트하려면 /hud:setup을 다시 실행하세요.
```
