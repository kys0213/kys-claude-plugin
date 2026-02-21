---
description: OpenClaw Docker 환경의 현재 상태를 확인합니다
argument-hint: "[openclaw-project-dir]"
allowed-tools:
  - Bash
  - Read
---

# OpenClaw Status

OpenClaw Docker 환경의 현재 상태를 확인하고 간결한 테이블로 정리합니다.

openclaw-ops 스킬을 참고하세요.

## 프로젝트 경로 결정

인자가 주어지면 해당 경로를, 아니면 `~/Documents/openclaw`를 `$PROJECT_DIR`로 사용합니다.
해당 경로에 `run.sh`가 있는지 확인합니다.

## 실행 순서

1. 컨테이너 상태 확인:

```bash
$PROJECT_DIR/run.sh status
```

2. 인증 상태 확인 - `check-auth.sh` 스크립트로 위임합니다:

```bash
bash $PLUGIN_DIR/scripts/check-auth.sh $PROJECT_DIR
```

**보안 주의**: 토큰 값은 절대 표시하지 않습니다. 만료 시각과 유효 여부만 표시합니다.

3. 최근 로그 5줄 확인 (에러 유무만 빠르게 확인):

`docker-compose.override.yml` 존재 여부를 먼저 확인하고, 존재하면 `-f` 옵션에 포함합니다.

```bash
COMPOSE_CMD="docker compose -f $PROJECT_DIR/docker-compose.yml"
[ -f "$PROJECT_DIR/docker-compose.override.yml" ] && \
  COMPOSE_CMD="$COMPOSE_CMD -f $PROJECT_DIR/docker-compose.override.yml"

$COMPOSE_CMD logs --tail 5 --no-color openclaw-gateway
```

## 출력 형식

결과를 다음 형식의 테이블로 정리합니다:

```
| 항목 | 상태 |
|------|------|
| 게이트웨이 | ✅ Running (Up 3시간) 또는 ❌ Stopped |
| 인증 | ✅ openai-codex (45분 후 갱신) 또는 ❌ 만료됨 |
| 최근 로그 | ✅ 정상 또는 ⚠️ 오류 감지 |
```

오류가 감지되면 `/oc-diagnose` 실행을 제안합니다.
