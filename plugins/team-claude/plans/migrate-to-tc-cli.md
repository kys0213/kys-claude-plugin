# Migration Plan: Bash Scripts to tc CLI

> Team Claude를 bash 스크립트 의존성에서 tc CLI 중심 아키텍처로 마이그레이션

## Overview

### 목표
- Windows/macOS/Linux 크로스 플랫폼 지원
- tc CLI가 단일 진입점
- setup.md에서 agent가 환경 감지 후 유연하게 대응
- bash 스크립트 완전 제거

### 현재 상태

```
setup.md
    ↓
tc-config.sh (bash)  ─┐
tc-server.sh (bash)  ─┼─→ bash 의존성
tc-state.sh (bash)   ─┘
```

### 목표 상태

```
setup.md
    ↓
Agent: 환경 감지 (OS, bun)
    ↓
tc CLI 빌드 (bun run build)
    ↓
tc CLI 설치 (AskUserQuestion으로 위치 확인)
    ↓
tc init / tc server install / tc server start
```

---

## Phase 1: tc CLI Bootstrap (setup.md)

### 1.1 환경 감지

Agent가 `/team-claude:setup` 실행 시 환경을 감지합니다:

```markdown
## Phase 0: 환경 감지

1. OS 확인
   - macOS: `uname -s` = "Darwin"
   - Linux: `uname -s` = "Linux"
   - Windows: PowerShell 환경 또는 WSL

2. bun 설치 확인
   - `command -v bun` 또는 `where bun`

3. 미설치 시 안내:
   - macOS/Linux: `curl -fsSL https://bun.sh/install | bash`
   - Windows: `powershell -c "irm bun.sh/install.ps1|iex"`
```

### 1.2 tc CLI 빌드

```markdown
## Phase 1: tc CLI 빌드

플러그인 캐시 경로:
~/.claude/plugins/cache/kys-claude-plugin/team-claude/{version}/

빌드 명령:
cd cli && bun install && bun run build

결과:
cli/dist/tc (바이너리)
```

### 1.3 tc CLI 설치

```markdown
## Phase 2: tc CLI 설치

AskUserQuestion으로 설치 위치 확인:

| OS | 기본 경로 | 대안 |
|----|----------|------|
| macOS | ~/.local/bin/tc | /usr/local/bin/tc |
| Linux | ~/.local/bin/tc | /usr/local/bin/tc |
| Windows | %USERPROFILE%\.local\bin\tc.exe | 사용자 지정 |

바이너리 복사 후 PATH 설정 안내 (필요시)
```

### 1.4 tc 명령어로 초기화

```markdown
## Phase 3: 초기화

tc setup init      # 설정 + 디렉토리 초기화
tc server install  # 서버 빌드
tc server start    # 서버 시작
tc doctor          # 최종 검증
```

---

## Phase 2: 새 CLI 명령어 추가

### 2.1 tc server

**파일:** `cli/src/commands/server.ts`

| 명령어 | 설명 | 대체 |
|--------|------|------|
| `tc server status` | 서버 상태 확인 | tc-server.sh status |
| `tc server start` | 서버 시작 | tc-server.sh start |
| `tc server stop` | 서버 중지 | tc-server.sh stop |
| `tc server restart` | 서버 재시작 | tc-server.sh restart |
| `tc server ensure` | 실행 확인 후 시작 | tc-server.sh ensure |
| `tc server install` | 서버 빌드/설치 | tc-server.sh install |
| `tc server build` | 서버 빌드만 | tc-server.sh build |
| `tc server logs [-f]` | 로그 확인 | tc-server.sh logs |

**핵심 구현:**
```typescript
// 크로스 플랫폼 프로세스 관리
import { spawn } from "child_process";

function startServerBackground(port: number): number {
  const proc = spawn(TC_SERVER_BINARY, [], {
    detached: true,
    stdio: ["ignore", "pipe", "pipe"],
    env: { ...process.env, TEAM_CLAUDE_PORT: String(port) },
  });
  proc.unref();
  return proc.pid!;
}
```

### 2.2 tc state

**파일:** `cli/src/commands/state.ts`

| 명령어 | 설명 | 대체 |
|--------|------|------|
| `tc state init` | 상태 파일 초기화 | tc-state.sh init |
| `tc state check` | 현재 상태 표시 | tc-state.sh check |
| `tc state get <key>` | 값 조회 | tc-state.sh get |
| `tc state require <phase>` | phase 검증 | tc-state.sh require |
| `tc state transition <to>` | phase 전이 | tc-state.sh transition |
| `tc state set-session <id>` | 세션 설정 | tc-state.sh set-session |
| `tc state reset` | 상태 초기화 | tc-state.sh reset |

**State 구조:**
```typescript
interface WorkflowState {
  phase: "idle" | "setup" | "designing" | "checkpoints_approved" | "delegating" | "merging" | "completed";
  serverRunning: boolean;
  currentSessionId: string | null;
  prerequisites: {
    setup: boolean;
    architect: boolean;
    checkpointsApproved: boolean;
    serverHealthy: boolean;
  };
  createdAt: string;
  updatedAt: string;
}
```

### 2.3 tc session

**파일:** `cli/src/commands/session.ts`

| 명령어 | 설명 | 대체 |
|--------|------|------|
| `tc session create <title>` | 세션 생성 | tc-session.sh create |
| `tc session list` | 세션 목록 | tc-session.sh list |
| `tc session show <id>` | 세션 상세 | tc-session.sh show |
| `tc session delete <id>` | 세션 삭제 | tc-session.sh delete |
| `tc session update <id> <k> <v>` | 세션 업데이트 | tc-session.sh update |

---

## Phase 3: 기존 명령어 강화

### 3.1 tc setup init 강화

tc-config.sh init의 모든 기능을 포함:

1. 디렉토리 생성
   - `~/.team-claude/{hash}/sessions`
   - `~/.team-claude/{hash}/state`
   - `~/.team-claude/{hash}/worktrees`
   - `.claude/agents`

2. 설정 파일 생성
   - `~/.team-claude/{hash}/team-claude.yaml`

3. Hooks 설정
   - `.claude/settings.local.json`에 hooks 추가

4. 상태 파일 초기화
   - `workflow.json`
   - `psm-index.json`

### 3.2 tc doctor 강화

서버 설치 여부 확인 및 자동 수정 추가.

---

## Phase 4: setup.md 워크플로우 업데이트

### Before → After

| Before (bash) | After (tc CLI) |
|---------------|----------------|
| `${SCRIPTS}/tc-config.sh init` | `tc setup init` |
| `${SCRIPTS}/tc-state.sh init` | `tc state init` |
| `${SCRIPTS}/tc-server.sh install` | `tc server install` |
| `${SCRIPTS}/tc-server.sh start` | `tc server start` |
| `${SCRIPTS}/tc-server.sh status` | `tc server status` |
| `${SCRIPTS}/tc-config.sh verify` | `tc doctor` |

---

## Phase 5: Bash 스크립트 Deprecation

각 스크립트에 deprecation 경고 추가:

```bash
#!/bin/bash
# ============================================================
# DEPRECATED: This script is deprecated.
# Use tc CLI instead:
#   tc-server.sh status  →  tc server status
#   tc-server.sh start   →  tc server start
# This script will be removed in v1.0.0
# ============================================================
echo "[DEPRECATED] Use 'tc server' instead" >&2
```

---

## Implementation Order

| # | Task | 파일 | 우선순위 |
|---|------|------|----------|
| 1 | tc server 명령어 | `cli/src/commands/server.ts` | HIGH |
| 2 | tc state 명령어 | `cli/src/commands/state.ts` | HIGH |
| 3 | tc session 명령어 | `cli/src/commands/session.ts` | MEDIUM |
| 4 | tc setup init 강화 | `cli/src/commands/setup.ts` | HIGH |
| 5 | index.ts 등록 | `cli/src/index.ts` | HIGH |
| 6 | setup.md 업데이트 | `commands/setup.md` | HIGH |
| 7 | bash 스크립트 deprecation | `scripts/*.sh` | LOW |

---

## Verification Checklist

- [ ] `bun run build` 성공
- [ ] `tc server status/install/start/stop` 작동
- [ ] `tc state init/check/transition` 작동
- [ ] `tc session create/list/show` 작동
- [ ] `tc setup init` 전체 초기화 완료
- [ ] `tc doctor` 모든 검증 통과
- [ ] `/team-claude:setup` bash 없이 작동
- [ ] macOS에서 전체 워크플로우 테스트

---

## Commit Strategy

```
1. feat(team-claude): add tc server command for cross-platform server management
2. feat(team-claude): add tc state command for workflow state management
3. feat(team-claude): add tc session command for session management
4. feat(team-claude): enhance tc setup init to replace tc-config.sh
5. refactor(team-claude): update setup.md for tc CLI-centric workflow
6. chore(team-claude): deprecate bash scripts with migration notices
```

---

## Notes

- Windows 테스트는 Phase 2 이후 진행
- 기존 사용자를 위한 마이그레이션 가이드 필요
- bash 스크립트는 v1.0.0까지 유지 후 제거
