---
name: openclaw-ops
description: OpenClaw Docker 운영 가이드 - run.sh CLI 사용법, 환경 구조, 인증 관리, 트러블슈팅
---

# OpenClaw Docker 운영 가이드

이 스킬은 OpenClaw Docker 환경을 CLI(`run.sh`)로 운영할 때 필요한 모든 정보를 제공합니다.

## 변수 정의

| 변수 | 설명 | 기본값 |
|------|------|--------|
| `$PROJECT_DIR` | OpenClaw 프로젝트 루트 | `~/Documents/openclaw` |
| `$CONFIG_DIR` | `OPENCLAW_CONFIG_DIR` 값 (세션 저장소) | `~/Documents/openclaw-session-storage` |
| `$PLUGIN_DIR` | 이 플러그인의 설치 경로 | - |

---

## 1. CLI 명령어 레퍼런스 (`run.sh`)

OpenClaw 프로젝트 루트의 `run.sh`가 모든 Docker 작업의 진입점입니다.

| 명령 | 설명 | 타임아웃 고려 |
|------|------|-------------|
| `./run.sh start` | 게이트웨이 시작 (Codex 인증 자동 동기화) | 느릴 수 있음 |
| `./run.sh stop` | 게이트웨이 중지 | 빠름 |
| `./run.sh restart` | 재시작 (`--force-recreate`, 환경변수 반영) | 느릴 수 있음 |
| `./run.sh status` | 컨테이너 상태 확인 | 빠름 |
| `./run.sh health` | 헬스체크 (HTTP 토큰 기반) | 빠름 |
| `./run.sh logs` | 실시간 로그 스트리밍 (Ctrl+C로 종료) | **스트리밍** |
| `./run.sh build` | Docker 이미지 빌드 (기본 + 커스텀) | 매우 느림 |
| `./run.sh sync-auth` | Codex 인증 수동 동기화 | 빠름 |
| `./run.sh telegram` | Telegram 봇 채널 추가 | 빠름 |
| `./run.sh cli <args>` | CLI 명령 직접 실행 | 가변 |
| `./run.sh down` | 컨테이너 + 네트워크 완전 제거 | 빠름 |

### 로그 조회 팁

`run.sh logs`는 실시간 스트리밍이라 Bash에서 멈출 수 있습니다.
최근 로그를 확인할 때는 docker compose를 직접 사용합니다:

```bash
# override 존재 시 포함 (run.sh compose() 함수와 동일 패턴)
COMPOSE_CMD="docker compose -f $PROJECT_DIR/docker-compose.yml"
[ -f "$PROJECT_DIR/docker-compose.override.yml" ] && \
  COMPOSE_CMD="$COMPOSE_CMD -f $PROJECT_DIR/docker-compose.override.yml"

# 최근 50줄
$COMPOSE_CMD logs --tail 50 --no-color openclaw-gateway

# 에러만 검색
$COMPOSE_CMD logs --tail 200 --no-color openclaw-gateway 2>&1 | grep -i error
```

---

## 2. 환경 구조

### 디렉토리 레이아웃

```
~/Documents/
├── openclaw/                          # $PROJECT_DIR
│   ├── run.sh                         # CLI 진입점
│   ├── .env.local                     # 환경변수 (git 제외)
│   ├── docker-compose.yml             # 기본 서비스 정의
│   ├── docker-compose.override.yml    # 커스텀 오버라이드 (git 제외, 선택 파일)
│   └── Dockerfile / Dockerfile.override
│
├── openclaw-session-storage/          # $CONFIG_DIR (볼륨 마운트)
│   ├── openclaw.json                  # 게이트웨이 설정
│   └── agents/main/agent/
│       └── auth-profiles.json         # OAuth 인증 프로필
│
└── openclaw-workspace/                # OPENCLAW_WORKSPACE_DIR (볼륨 마운트)
```

> `docker-compose.override.yml`은 선택 파일입니다. 존재 시 run.sh가 자동으로 포함합니다.
> 주로 추가 포트 바인딩, 볼륨 마운트, 환경변수 오버라이드에 사용됩니다.

### 핵심 환경변수 (`.env.local`)

| 변수 | 역할 | 예시 |
|------|------|------|
| `OPENCLAW_CONFIG_DIR` | 세션 저장소 경로 | `~/Documents/openclaw-session-storage` |
| `OPENCLAW_WORKSPACE_DIR` | AI 작업 공간 경로 | `~/Documents/openclaw-workspace` |
| `OPENCLAW_IMAGE` | Docker 이미지명 | `openclaw:local` |
| `OPENCLAW_GATEWAY_TOKEN` | 게이트웨이 인증 토큰 | (자동 생성된 해시) |
| `OPENCLAW_GATEWAY_PORT` | 게이트웨이 포트 | `18789` |
| `OPENCLAW_BRIDGE_PORT` | 브릿지 포트 | `18790` |
| `TELEGRAM_BOT_TOKEN` | Telegram 봇 토큰 | (BotFather에서 발급) |
| `OPENAI_API_KEY` | OpenAI API 키 | `sk-...` |
| `GH_TOKEN` | GitHub PAT | `ghp_...` |

### 핵심 설정 파일

#### `openclaw.json` 주요 섹션

```json
{
  "agents": {
    "defaults": {
      "model": { "primary": "openai-codex/gpt-5.3-codex" }
    }
  },
  "gateway": {
    "port": 18789,
    "mode": "local",
    "auth": { "mode": "token", "token": "..." }
  },
  "channels": {
    "telegram": { "dmPolicy": "pairing", "groupPolicy": "allowlist" }
  },
  "plugins": {
    "entries": { "telegram": { "enabled": true } }
  }
}
```

#### `auth-profiles.json` 구조

```json
{
  "version": 1,
  "profiles": {
    "openai-codex:default": {
      "type": "oauth",
      "provider": "openai-codex",
      "access": "<JWT>",
      "refresh": "<refresh_token>",
      "expires": 1772251463654
    }
  }
}
```

> `expires`는 JWT access_token의 `exp` 클레임에서 추출됩니다 (단위: ms).
> JWT 파싱 실패 시 fallback으로 `expires` 필드를 직접 사용합니다.
> `$CONFIG_DIR/agents/main/agent/auth-profiles.json` 경로에 위치합니다.

---

## 3. 인증 관리

### Codex OAuth 인증 흐름

```
호스트: codex login
  → ~/.codex/auth.json 에 토큰 저장

run.sh start/restart:
  → sync_codex_auth() 자동 실행
  → ~/.codex/auth.json → auth-profiles.json 동기화

수동 동기화:
  → ./run.sh sync-auth
```

### 인증 상태 확인

```bash
bash $PLUGIN_DIR/scripts/check-auth.sh $PROJECT_DIR
```

출력 예시:
```
PROFILE=openai-codex:default STATUS=VALID EXPIRES=2025-02-28T10:24:23Z REMAINING=6d23h
```

### API 키 추가

1. `.env.local`에 키 추가 (예: `OPENAI_API_KEY=sk-...`)
2. `docker-compose.override.yml`에 환경변수 매핑이 있는지 확인
3. `openclaw.json`에서 모델 프로바이더 변경
4. `./run.sh restart` 로 반영

---

## 4. 채널 관리

### Telegram 추가

1. BotFather에서 봇 생성 → 토큰 발급
2. `.env.local`에 `TELEGRAM_BOT_TOKEN=<token>` 추가
3. `./run.sh restart` (환경변수 반영)
4. `./run.sh telegram` (채널 등록)
5. 봇에게 DM → 페어링 코드 수신 → 승인

### 채널 정책

| 정책 | 설명 |
|------|------|
| `dmPolicy: "pairing"` | 1:1 대화 시 페어링 코드 필요 |
| `dmPolicy: "open"` | 누구나 DM 가능 |
| `groupPolicy: "allowlist"` | 허용된 그룹만 |
| `groupPolicy: "open"` | 모든 그룹 |

---

## 5. 트러블슈팅

### 진단 순서

1. `./run.sh status` → 컨테이너가 Running인가?
2. 최근 로그 확인 → 에러 키워드 검색
3. 인증 상태 확인 → 토큰 만료 여부
4. `./run.sh health` → 헬스체크 통과 여부

### 주요 오류 패턴

| 에러 키워드 | 원인 | 해결 |
|-------------|------|------|
| `Missing config` | `openclaw.json` 설정 누락 | `./run.sh start`로 초기화 |
| `No API key found` | 인증 토큰 없거나 만료 | `codex login` → `./run.sh sync-auth` → `./run.sh restart` |
| `Unknown model` | 모델 ID 오류 | `openclaw.json`의 `agents.defaults.model.primary` 수정 |
| `port already in use` | 포트 충돌 | `lsof -i :18789` 확인 후 프로세스 종료 또는 포트 변경 |
| 채널 무응답 | 토큰/페어링/플러그인 문제 | 환경변수 → plugins.entries → 페어링 순서로 확인 |
| 환경변수 미반영 | restart vs force-recreate | 반드시 `./run.sh restart` 사용 (force-recreate 포함) |
| 이미지 빌드 실패 | 네트워크/디스크 | `docker info`, `docker system df` 확인 |

### 중요 주의사항

- **`docker compose restart` 사용 금지**: 환경변수가 반영되지 않음. 반드시 `./run.sh restart` 사용
- **토큰/API 키 표시 금지**: 사용자에게 보여줄 때 앞 8자만 노출하고 나머지는 마스킹
- **`./run.sh down` 주의**: 컨테이너 + 네트워크를 완전히 제거함. 데이터는 볼륨에 보존

---

## 플러그인 스크립트

| 스크립트 | 용도 |
|----------|------|
| `scripts/check-env.sh [project-dir]` | 환경 사전 검증 (Docker, run.sh, .env.local, 필수 변수, override) |
| `scripts/check-auth.sh [project-dir]` | 인증 상태 확인 (JWT exp 클레임 추출, 만료 여부) |
