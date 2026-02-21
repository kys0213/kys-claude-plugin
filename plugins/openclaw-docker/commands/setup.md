---
description: OpenClaw Docker 플러그인 초기 설정. run.sh 경로 확인 및 Bash 권한을 설정합니다.
allowed-tools:
  - Bash
  - Read
  - Write
  - AskUserQuestion
---

# OpenClaw Docker Plugin Setup

이 커맨드는 OpenClaw Docker 환경에서 `run.sh`를 사용할 수 있도록 사전 요구사항을 확인하고 권한을 설정합니다.

## Step 1: 프로젝트 경로 확인

AskUserQuestion으로 OpenClaw 프로젝트 경로를 확인합니다:

```
OpenClaw 프로젝트 디렉토리 경로를 확인합니다.
```

옵션:
1. **~/Documents/openclaw (Recommended)** - 기본 경로
2. **직접 입력** - 다른 경로 지정

확인한 경로를 `$PROJECT_DIR`로 사용합니다.

## Step 2: 환경 검증

`check-env.sh` 스크립트로 모든 필수 항목을 한 번에 검증합니다:

```bash
bash $PLUGIN_DIR/scripts/check-env.sh $PROJECT_DIR
```

출력에서 `FAIL` 항목이 없으면 Step 4로 진행합니다.

## Step 3: FAIL 항목 해결

FAIL 항목별 안내:
- `DOCKER_INSTALLED=FAIL` → Docker Desktop 설치 안내
- `DOCKER_RUNNING=FAIL` → Docker Desktop 실행 안내
- `RUN_SH=FAIL` → 경로 확인 또는 `chmod +x $PROJECT_DIR/run.sh`
- `ENV_LOCAL=FAIL` → `.env.local` 파일 생성 필요
- `REQUIRED_VARS=FAIL` → 누락된 변수를 `.env.local`에 추가 안내

모든 FAIL 해결 후 Step 2를 다시 실행하여 확인합니다.

## Step 4: 권한 설정 안내

OpenClaw 프로젝트의 `.claude/settings.local.json`에 다음 Bash 권한이 필요함을 안내합니다:

```json
{
  "permissions": {
    "allow": [
      "Bash(./run.sh:*)",
      "Bash(docker compose:*)"
    ]
  }
}
```

이미 설정되어 있으면 건너뜁니다.

## Step 5: 동작 확인 및 완료

```bash
$PROJECT_DIR/run.sh status
```

정상이면 완료 메시지를 표시합니다:

```
OpenClaw Docker 플러그인이 설정되었습니다!

환경:
├─ OpenClaw 프로젝트: $PROJECT_DIR
├─ .env.local: 확인됨
├─ run.sh: 실행 가능
└─ Docker: 실행 중

사용 가능한 커맨드:
├─ /oc-status   → 현재 상태 요약
├─ /oc-restart  → 게이트웨이 재시작
└─ /oc-diagnose → 환경 진단

openclaw-ops 스킬에서 run.sh 명령어 레퍼런스를 확인할 수 있습니다.
```
