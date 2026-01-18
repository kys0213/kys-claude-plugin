---
name: config
description: Team Claude 설정 조회/수정 - get, set, list, reset 액션 지원
argument-hint: "<action> [key] [value]"
allowed-tools: ["Bash", "Read", "Write", "AskUserQuestion"]
---

# Team Claude Config Command

개별 설정을 조회하고 수정합니다.

## 사용법

```bash
/team-claude:config <action> [key] [value] [--scope]
```

## 액션

| Action | 설명 | 예시 |
|--------|------|------|
| `list` | 전체 설정 목록 | `/team-claude:config list` |
| `get` | 특정 값 조회 | `/team-claude:config get worker.maxConcurrent` |
| `set` | 값 변경 | `/team-claude:config set worker.maxConcurrent 10` |
| `reset` | 초기화 | `/team-claude:config reset worker` |

## API 연동

### list - 전체 설정 조회

```bash
curl -s http://localhost:3847/config/list | jq
```

**출력 형식:**
```json
{
  "success": true,
  "data": [
    {
      "path": "server.port",
      "value": 3847,
      "default": 3847,
      "source": "default",
      "description": "서버 포트 (1024-65535)"
    },
    {
      "path": "worker.maxConcurrent",
      "value": 10,
      "default": 5,
      "source": "project",
      "description": "동시 Worker 수 (1-20)"
    }
  ]
}
```

**표시 형식:**
```
╔══════════════════════════════════════════════════════════════╗
║                Team Claude Configuration                     ║
╠══════════════════════════════════════════════════════════════╣
║                                                              ║
║  [Server]                                                    ║
║    server.port          = 3847     (default)                 ║
║    server.host          = localhost (default)                ║
║    server.timeout       = 60000    (default)                 ║
║                                                              ║
║  [Worktree]                                                  ║
║    worktree.root        = ../worktrees (default)             ║
║    worktree.branchPrefix = feature/   (default)              ║
║                                                              ║
║  [Worker]                                                    ║
║    worker.maxConcurrent = 10       (project) ← 수정됨        ║
║    worker.defaultTemplate = standard (default)               ║
║    worker.timeout       = 1800     (default)                 ║
║                                                              ║
║  [Notification]                                              ║
║    notification.method  = file     (default)                 ║
║                                                              ║
║  [Review]                                                    ║
║    review.autoLevel     = semi-auto (default)                ║
║    review.requireApproval = true   (default)                 ║
║                                                              ║
╚══════════════════════════════════════════════════════════════╝
```

### get - 특정 값 조회

```bash
curl -s http://localhost:3847/config/worker.maxConcurrent | jq
```

**사용 예시:**
```bash
/team-claude:config get worker.maxConcurrent
# 출력: worker.maxConcurrent = 10 (project)

/team-claude:config get notification
# 출력: notification.method = file (default)
```

### set - 값 변경

```bash
curl -X POST http://localhost:3847/config/set \
  -H "Content-Type: application/json" \
  -d '{"path": "worker.maxConcurrent", "value": 10, "scope": "project"}'
```

**사용 예시:**
```bash
# 프로젝트 설정 변경 (기본)
/team-claude:config set worker.maxConcurrent 10

# 글로벌 설정 변경
/team-claude:config set worker.maxConcurrent 10 --global

# 세션 설정 (임시)
/team-claude:config set worker.maxConcurrent 10 --session
```

**유효성 검사:**
```
> /team-claude:config set worker.maxConcurrent -5

❌ 유효하지 않은 값입니다.
   worker.maxConcurrent는 1 이상이어야 합니다.
   현재 값: 10

> /team-claude:config set review.autoLevel invalid

❌ 유효하지 않은 값입니다.
   review.autoLevel은 manual, semi-auto, full-auto 중 하나여야 합니다.
   현재 값: semi-auto
```

### reset - 초기화

```bash
curl -X POST http://localhost:3847/config/reset \
  -H "Content-Type: application/json" \
  -d '{"section": "worker", "scope": "project"}'
```

**사용 예시:**
```bash
# 특정 섹션 초기화
/team-claude:config reset worker
/team-claude:config reset notification

# 전체 초기화
/team-claude:config reset --all

# 글로벌 설정 초기화
/team-claude:config reset --global --all
```

## 설정 키 목록

### server.*
| 키 | 타입 | 범위 | 설명 |
|----|------|------|------|
| server.port | number | 1024-65535 | 서버 포트 |
| server.host | string | - | 서버 호스트 |
| server.timeout | number | 1000-300000 | 요청 타임아웃 (ms) |

### worktree.*
| 키 | 타입 | 범위 | 설명 |
|----|------|------|------|
| worktree.root | string | - | Worktree 루트 경로 |
| worktree.branchPrefix | string | - | 브랜치 접두사 |
| worktree.cleanupOnComplete | boolean | - | 완료 시 자동 정리 |

### worker.*
| 키 | 타입 | 범위 | 설명 |
|----|------|------|------|
| worker.maxConcurrent | number | 1-20 | 동시 Worker 수 |
| worker.defaultTemplate | string | - | 기본 템플릿 |
| worker.timeout | number | 60-7200 | 타임아웃 (초) |
| worker.autoRetry | boolean | - | 실패 시 자동 재시도 |
| worker.retryLimit | number | 0-5 | 재시도 최대 횟수 |

### notification.*
| 키 | 타입 | 범위 | 설명 |
|----|------|------|------|
| notification.method | enum | file/notification/slack/webhook | 알림 방식 |

### review.*
| 키 | 타입 | 범위 | 설명 |
|----|------|------|------|
| review.autoLevel | enum | manual/semi-auto/full-auto | 자동화 레벨 |
| review.requireApproval | boolean | - | 승인 필수 여부 |

## 설정 계층

```
Global (~/.team-claude/config.json)
    ↓ override
Project (.team-claude/config.json)
    ↓ override
Session (런타임, 임시)
```

## 관련 커맨드

- `/team-claude:setup` - 초기 설정 위자드
- `/team-claude:template` - 템플릿 관리
- `/team-claude:rules` - 리뷰 규칙 관리
- `/team-claude:config:export` - 설정 내보내기
- `/team-claude:config:import` - 설정 가져오기
