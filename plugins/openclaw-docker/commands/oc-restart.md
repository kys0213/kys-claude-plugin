---
description: OpenClaw 게이트웨이를 재시작합니다
argument-hint: "[openclaw-project-dir]"
allowed-tools:
  - Bash
  - Read
---

# OpenClaw Restart

OpenClaw 게이트웨이를 안전하게 재시작합니다. Codex 인증 동기화가 자동으로 포함됩니다.

openclaw-ops 스킬을 참고하세요.

## 프로젝트 경로 결정

인자가 주어지면 해당 경로를, 아니면 `~/Documents/openclaw`를 `$PROJECT_DIR`로 사용합니다.

## 실행 순서

1. 게이트웨이 재시작 (인증 동기화 + force-recreate 포함):

```bash
$PROJECT_DIR/run.sh restart
```

> `run.sh restart`는 내부적으로 Codex 인증 동기화(`sync_codex_auth`) 후 `docker compose up -d --force-recreate`를 실행합니다.

2. 재시작 후 상태 확인:

```bash
$PROJECT_DIR/run.sh status
```

## 출력 형식

각 단계의 결과를 순서대로 보여줍니다:

```
1. 게이트웨이 재시작: ✅ 완료 (Codex 인증 동기화 포함)
2. 상태 확인: ✅ Running
```

재시작 실패 시:

1. 최근 로그 20줄을 확인합니다. `docker-compose.override.yml` 존재 여부를 먼저 확인하고, 존재하면 `-f` 옵션에 포함합니다:

```bash
COMPOSE_CMD="docker compose -f $PROJECT_DIR/docker-compose.yml"
[ -f "$PROJECT_DIR/docker-compose.override.yml" ] && \
  COMPOSE_CMD="$COMPOSE_CMD -f $PROJECT_DIR/docker-compose.override.yml"

$COMPOSE_CMD logs --tail 20 --no-color openclaw-gateway
```

2. 원인과 해결 방법을 제안합니다 (openclaw-ops 스킬의 트러블슈팅 섹션 참고).
