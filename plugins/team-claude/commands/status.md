---
name: status
description: 전체 Worker 상태를 조회합니다 - 실행 중인 모든 Worker의 진행 상황을 확인합니다
argument-hint: "[feature-name]"
allowed-tools: ["Bash", "Read"]
---

# Worker Status 커맨드

모든 Worker Claude의 현재 상태를 조회하고 요약합니다.

## 기능

```
1. Coordination Server에서 상태 조회
    │
    ├── 전체 조회: GET /status
    └── 특정 Worker: GET /status/<worktree>
    │
    ▼
2. 상태 정보 포맷팅
    │
    ▼
3. 사용자에게 표시
```

## 실행 방법

### 전체 Worker 상태 조회

```bash
curl -s http://localhost:3847/status | jq
```

### 특정 Worker 상태 조회

```bash
curl -s http://localhost:3847/status/<worktree-name> | jq
```

### 상태 요약 조회

```bash
curl -s http://localhost:3847/status/report/summary | jq -r '.data.summary'
```

## 상태 정보

### Worker 상태 종류

| 상태 | 설명 |
|------|------|
| `running` | Worker가 작업 중 |
| `pending_review` | 작업 완료, 리뷰 대기 중 |
| `blocked` | 문제로 인해 중단됨 |
| `completed` | 완전히 완료됨 |

### 응답 형식

```json
{
  "success": true,
  "data": {
    "stats": {
      "total": 3,
      "running": 1,
      "completed": 1,
      "blocked": 0,
      "pendingReview": 1
    },
    "workers": [
      {
        "worktree": "feature-auth",
        "feature": "auth",
        "branch": "feature/auth",
        "status": "running",
        "startedAt": "2024-01-18T10:00:00Z",
        "lastReport": null
      }
    ]
  },
  "timestamp": "2024-01-18T12:00:00Z"
}
```

## 표시 형식

```
╔══════════════════════════════════════════════════════════════╗
║                  Team Claude Status                          ║
╠══════════════════════════════════════════════════════════════╣
║  Total Workers: 3                                            ║
║  ├─ Running: 1                                               ║
║  ├─ Pending Review: 1                                        ║
║  ├─ Blocked: 0                                               ║
║  └─ Completed: 1                                             ║
╠══════════════════════════════════════════════════════════════╣
║                                                              ║
║  [RUNNING] feature-auth                                      ║
║    Branch: feature/auth                                      ║
║    Started: 2h ago                                           ║
║                                                              ║
║  [REVIEW] feature-payment                                    ║
║    Branch: feature/payment                                   ║
║    Files: 12 changed (+450/-120)                             ║
║    Last Update: 30m ago                                      ║
║                                                              ║
║  [DONE] feature-config                                       ║
║    Branch: feature/config                                    ║
║    Merged: 1h ago                                            ║
║                                                              ║
╚══════════════════════════════════════════════════════════════╝
```

## 사용 예시

```bash
# 전체 상태 조회
/team-claude:status

# 특정 Worker 상태
/team-claude:status auth-feature

# 블로킹 된 Worker만 확인
/team-claude:status --blocked
```

## 알림 확인

Worker 완료 시 생성되는 알림을 확인하려면:

```bash
# 최신 알림 확인
cat .team-claude/notifications/latest.md

# 모든 알림 목록
ls -la .team-claude/notifications/
```

## Git Worktree 상태

등록된 Worker 외에 실제 git worktree도 확인:

```bash
git worktree list
```

## 주의사항

- Coordination Server가 실행 중이어야 상태 조회 가능
- Worker가 비정상 종료되면 상태가 업데이트되지 않을 수 있음
- 수동으로 worktree를 삭제하면 서버의 Worker 레코드와 불일치 발생
