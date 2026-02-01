# Team Claude Infrastructure

> **CRITICAL**: 모든 team-claude 작업 전에 이 파일을 먼저 읽으세요.
> 기존 스크립트와 도구를 **반드시** 사용하세요. 새로 만들지 마세요.

---

## 설치 구조

### 글로벌 (모든 프로젝트 공유)

```
~/.claude/
├── plugins/team-claude/        # 플러그인 정의 (이 디렉토리)
│   ├── commands/               # 슬래시 커맨드
│   ├── cli/                    # tc CLI 도구
│   ├── server/                 # 서버 소스 코드
│   └── ...
└── team-claude-server          # 빌드된 서버 바이너리
```

### 프로젝트별

```
<project>/
├── .claude/
│   └── team-claude.yaml        # 프로젝트 설정 (tc config로 관리)
└── .team-claude/
    ├── sessions/               # 설계 세션 데이터 (tc session으로 관리)
    ├── state/
    │   └── workflow.json       # 워크플로우 상태 (tc state로 관리)
    └── worktrees/              # Worker용 Git worktree (tc worktree로 관리)
```

---

## Quick Reference

### tc CLI (반드시 사용 - 새로 만들지 마세요!)

| Command | Purpose | Example |
|---------|---------|---------|
| `tc config` | YAML 설정 관리 | `tc config get project.name` |
| `tc session` | 세션 CRUD | `tc session create "title"` |
| `tc worktree` | Git worktree 관리 | `tc worktree create checkpoint-id` |
| `tc state` | 워크플로우 상태 | `tc state check` |
| `tc server` | 서버 라이프사이클 | `tc server ensure` |
| `tc flow` | 워크플로우 제어 | `tc flow start` |
| `tc hud` | HUD 표시 | `tc hud show` |
| `tc psm` | PSM 워크플로우 | `tc psm init` |
| `tc agent` | Agent 실행 | `tc agent architect` |
| `tc review` | 코드 리뷰 | `tc review start` |

---

## CLI 명령어 상세

### tc config - 설정 관리

```bash
tc config init                    # 기본 설정 파일 생성
tc config get <path>              # 값 읽기 (예: project.name)
tc config set <path> <value>      # 값 쓰기
tc config show                    # 전체 설정 출력
tc config path                    # 설정 파일 경로 출력
```

### tc session - 세션 관리

```bash
tc session create <title>         # 새 세션 생성, ID 반환
tc session list                   # 세션 목록 조회
tc session show <id>              # 세션 상세 정보
tc session update <id> <key> <val> # 메타데이터 업데이트
tc session delete <id>            # 세션 삭제
```

### tc worktree - Git Worktree 관리

```bash
tc worktree create <checkpoint-id>  # Worktree + 브랜치 생성
tc worktree list                    # Worktree 목록
tc worktree path <checkpoint-id>    # Worktree 경로 반환
tc worktree delete <checkpoint-id>  # Worktree 삭제
tc worktree cleanup                 # 모든 team-claude worktree 정리
```

### tc state - 워크플로우 상태

```bash
tc state init                     # 상태 파일 초기화
tc state check                    # 현재 상태 표시
tc state get <key>                # 특정 값 조회
tc state require <phase>          # 필요한 phase가 아니면 exit 1
tc state transition <phase>       # 상태 전이
tc state set-session <id>         # 현재 세션 설정
tc state set-server <true|false>  # 서버 상태 설정
tc state reset                    # 상태 초기화
```

### tc server - 서버 관리

```bash
tc server status                  # 서버 상태 확인
tc server start                   # 서버 시작
tc server stop                    # 서버 중지
tc server ensure                  # 미실행 시 시작 + health 검증
tc server build                   # 서버 빌드
tc server install                 # 의존성 + 빌드 + 설치
tc server logs [-f]               # 로그 확인
```

---

## 서버

- **바이너리 위치**: `~/.claude/team-claude-server`
- **기본 포트**: `7890`
- **로그**: `~/.claude/team-claude-server.log`

### Health Check

```bash
curl -s http://localhost:7890/health
# 응답: {"status":"ok","timestamp":"..."}
```

### 서버 시작/중지

```bash
# 시작 (없으면 자동 시작)
tc server ensure

# 수동 시작
tc server start

# 중지
tc server stop
```

---

## 워크플로우 상태 (Phase)

```
idle → setup → designing → checkpoints_approved → delegating → merging → completed
```

### Phase 전이 규칙

| 현재 Phase | 다음 Phase | 트리거 |
|-----------|-----------|--------|
| idle | setup | /team-claude:setup 실행 |
| setup | designing | /team-claude:architect 시작 |
| designing | checkpoints_approved | Checkpoint 승인 |
| checkpoints_approved | delegating | /team-claude:delegate 실행 |
| delegating | merging | 모든 Worker 완료 |
| merging | completed | 머지 완료 |

### 상태 확인

```bash
tc state check

# 출력:
# ━━━ Team Claude Workflow State ━━━
#   Phase: 🏗️ designing
#   Session: abc12345
#   Server: 🟢 실행 중
```

---

## 의존성 그래프

```
setup ─┬─> architect ──> checkpoint ──> delegate ──> merge
       │                                    │
       └── server (required) ───────────────┘
```

- `delegate` 실행 전: 서버가 **반드시** 실행 중이어야 함
- `delegate` 실행 전: Checkpoint가 **승인**되어야 함

---

## 전제조건 체크

### delegate 전

```bash
# 1. 워크플로우 상태 확인
tc state require checkpoints_approved

# 2. 서버 실행 보장
tc server ensure

# 둘 중 하나라도 실패하면 STOP하고 사용자에게 안내
```

### architect 전

```bash
# 설정이 존재하는지 확인
tc config show >/dev/null 2>&1 || {
  echo "'/team-claude:setup'을 먼저 실행하세요."
  exit 1
}
```

---

## 공통 패턴

### 세션 기반 작업

```bash
# 1. 세션 ID 확인
SESSION_ID="abc12345"

# 2. 세션 정보 로드
tc session show "$SESSION_ID"

# 3. 세션 상태 업데이트
tc session update "$SESSION_ID" status delegating
```

### Checkpoint 기반 작업

```bash
# 1. Worktree 생성
WORKTREE_PATH=$(tc worktree create coupon-service)

# 2. 작업 수행...

# 3. 완료 후 정리
tc worktree delete coupon-service
```

---

## 에러 해결

### "상태 파일이 없습니다"

```bash
# 해결: setup 실행
/team-claude:setup
```

### "서버가 실행 중이지 않습니다"

```bash
# 해결: 서버 시작
tc server ensure

# 또는 수동 설치
tc server install
tc server start
```

### "Checkpoint가 승인되지 않았습니다"

```bash
# 해결: architect에서 승인
/team-claude:architect --resume <session-id>
```

---

## 중요 규칙

1. **tc CLI 사용**: `tc` CLI 도구가 이미 존재합니다. 새로 만들지 마세요.
2. **상태 관리**: 워크플로우 상태는 `tc state`로 관리합니다.
3. **서버 자동 시작**: `tc server ensure`는 서버가 없으면 자동으로 시작합니다.
4. **전제조건 확인**: 각 커맨드 실행 전 전제조건을 확인하세요.
5. **결정적 동작**: CLI 명령어는 멱등성을 가집니다. 여러 번 실행해도 안전합니다.
