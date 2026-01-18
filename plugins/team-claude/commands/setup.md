---
name: setup
description: Team Claude 환경을 설정합니다 - Coordination Server 설치 및 실행
argument-hint: "[--start|--dev]"
allowed-tools: ["Bash", "Read", "Write"]
---

# Team Claude Setup 커맨드

Team Claude 환경을 초기화하고 Coordination Server를 설정합니다.

## 핵심 워크플로우

```
1. 필수 도구 확인 (bun, git)
    │
    ▼
2. 서버 의존성 설치
    │
    ▼
3. 프로젝트 디렉토리 초기화
    │
    ▼
4. 서버 시작 (선택)
```

## 필수 요구사항

### 시스템 요구사항
- **Bun**: 서버 런타임 (자동 설치 시도)
- **Git**: Worktree 관리
- **curl**: API 통신
- **jq**: JSON 파싱 (선택)

### 확인 방법
```bash
bun --version
git --version
curl --version
jq --version
```

## 설치 단계

### 1. Bun 설치 확인

```bash
# Bun이 없으면 설치
if ! command -v bun &>/dev/null; then
    curl -fsSL https://bun.sh/install | bash
fi
```

### 2. 서버 의존성 설치

```bash
cd ${CLAUDE_PLUGIN_ROOT}/server
bun install
```

### 3. 프로젝트 디렉토리 생성

```bash
# Team Claude 작업 디렉토리
mkdir -p .team-claude/notifications
mkdir -p ../worktrees
```

### 4. 서버 시작

```bash
# 일반 모드
bun run start

# 개발 모드 (hot reload)
bun run dev
```

## 사용 예시

```bash
# 설치만 (서버 시작 안함)
/team-claude:setup

# 설치 후 서버 시작
/team-claude:setup --start

# 개발 모드로 시작
/team-claude:setup --dev
```

## 설정 파일

### 환경 변수

```bash
# 서버 포트 (기본: 3847)
export PORT=3847

# 프로젝트 루트
export PROJECT_ROOT=/path/to/project

# 서버 URL (Worker용)
export TEAM_CLAUDE_SERVER_URL=http://localhost:3847
```

### 서버 설정

`server/package.json`:
```json
{
  "scripts": {
    "start": "bun run src/index.ts",
    "dev": "bun run --watch src/index.ts"
  }
}
```

## 상태 확인

### 서버 상태

```bash
curl http://localhost:3847/health
```

응답:
```json
{
  "status": "healthy",
  "uptime": 123.45,
  "timestamp": "2024-01-18T12:00:00Z"
}
```

### Git Worktree 상태

```bash
git worktree list
```

### 알림 디렉토리

```bash
ls -la .team-claude/notifications/
```

## 문제 해결

### 포트 충돌

```bash
# 사용 중인 포트 확인
lsof -i :3847

# 다른 포트로 시작
PORT=3848 bun run start
```

### Bun 설치 실패

```bash
# npm으로 대체 실행 가능 (느림)
npm install
npm start
```

### 권한 문제

```bash
# 스크립트 실행 권한
chmod +x ${CLAUDE_PLUGIN_ROOT}/scripts/*.sh
```

## 디렉토리 구조

설치 후 생성되는 구조:

```
프로젝트/
├── .team-claude/
│   └── notifications/     # Worker 알림 저장
│
├── ../worktrees/          # Git worktree 저장 (프로젝트 상위)
│   ├── feature-auth/
│   ├── feature-payment/
│   └── ...
│
└── (기존 프로젝트 파일)
```

## 관련 커맨드

- `/team-claude:spawn` - Worker 생성
- `/team-claude:status` - 상태 확인
- `/team-claude:review` - 코드 리뷰
- `/team-claude:feedback` - 피드백 전달
