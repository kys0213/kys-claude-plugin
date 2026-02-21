---
description: OpenClaw Docker 환경을 진단합니다
argument-hint: "[openclaw-project-dir]"
allowed-tools:
  - Bash
  - Read
---

# OpenClaw Diagnose

OpenClaw Docker 환경을 체계적으로 진단하고, 문제를 발견하면 해결 방법을 제안합니다.

openclaw-ops 스킬을 참고하세요.

## 프로젝트 경로 결정

인자가 주어지면 해당 경로를, 아니면 `~/Documents/openclaw`를 `$PROJECT_DIR`로 사용합니다.

## 진단 순서

### 0단계: 환경 사전 검증

`check-env.sh` 스크립트로 환경을 먼저 검증합니다:

```bash
bash $PLUGIN_DIR/scripts/check-env.sh $PROJECT_DIR
```

FAIL 항목이 있으면 해당 문제를 먼저 해결하도록 안내합니다.

### 1단계: 컨테이너 상태

```bash
$PROJECT_DIR/run.sh status
```

- Running이면 → 다음 단계로
- Stopped이면 → "게이트웨이가 중지되어 있습니다. `./run.sh start`로 시작하세요."

### 2단계: 최근 로그 확인

`docker-compose.override.yml` 존재 여부를 먼저 확인하고, 존재하면 `-f` 옵션에 포함합니다:

```bash
COMPOSE_CMD="docker compose -f $PROJECT_DIR/docker-compose.yml"
[ -f "$PROJECT_DIR/docker-compose.override.yml" ] && \
  COMPOSE_CMD="$COMPOSE_CMD -f $PROJECT_DIR/docker-compose.override.yml"

$COMPOSE_CMD logs --tail 30 --no-color openclaw-gateway
```

다음 키워드를 찾습니다:
- `Error`, `error`, `FATAL`, `panic` → 오류 발생
- `No API key found` → 인증 문제
- `Unknown model` → 모델 설정 오류
- `ECONNREFUSED`, `ETIMEDOUT` → 네트워크 문제
- `port already in use` → 포트 충돌

### 3단계: 인증 상태

`check-auth.sh` 스크립트로 토큰 만료 여부를 확인합니다:

```bash
bash $PLUGIN_DIR/scripts/check-auth.sh $PROJECT_DIR
```

**보안 주의**: 토큰 값은 절대 표시하지 않습니다.

- `STATUS=EXPIRED` → "`codex login` 후 `./run.sh sync-auth` → `./run.sh restart`"
- 프로필 없음 → "`./run.sh sync-auth`로 Codex 인증을 동기화하세요"

### 4단계: 헬스체크

```bash
$PROJECT_DIR/run.sh health
```

- 성공 → "헬스체크 통과"
- 실패 → "헬스체크 실패. 게이트웨이가 정상 기동되지 않았을 수 있습니다."

### 5단계: 진단 요약

모든 결과를 종합하여 다음 형식으로 출력합니다:

```
# 진단 결과

| 항목 | 상태 | 비고 |
|------|------|------|
| 환경 검증 | ✅/❌ | ... |
| 컨테이너 | ✅/❌ | ... |
| 로그 | ✅/⚠️ | ... |
| 인증 | ✅/❌ | ... |
| 헬스체크 | ✅/❌ | ... |

## 발견된 문제

1. [문제 설명]
   → 해결: [run.sh 명령 + 설명]

## 권장 조치

[문제가 없으면 "모든 항목이 정상입니다." 표시]
[문제가 있으면 해결을 위한 run.sh 명령을 제안]
```
