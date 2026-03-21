# Flow 13: Cron 관리

### 시나리오

Daemon이 주기적으로 실행해야 하는 작업을 스크립트 기반 cron으로 관리한다.

### 기본 Cron Jobs

#### Global (결정적, LLM 불필요)

| Job | 주기 | Guard | 동작 |
|-----|------|-------|------|
| hitl-timeout | 5분 | 미응답 HITL 있을 때 | `autodev hitl timeout` |
| daily-report | 매일 06시 | 활동 있을 때 | 일간 리포트 |
| log-cleanup | 매일 00시 | 보관 초과 로그 있을 때 | 오래된 로그/worktree 삭제 |

#### Per-workspace (LLM, AgentRuntime 사용)

| Job | 주기 | 동작 |
|-----|------|------|
| claw-evaluate | 60초 | Claw headless (큐 평가 → advance/skip) |
| gap-detection | 1시간 | 스펙-코드 대조 |
| knowledge-extract | 1시간 | merged PR 지식 추출 |

### Force Trigger (코어)

별도 구현 불필요. 코어 이벤트에서 자동 호출:

```
코어.on_done()            → force_claw_evaluate()
코어.on_failed()          → force_claw_evaluate()
코어.on_spec_active()     → force_claw_evaluate()
코어.on_hitl_responded()  → force_claw_evaluate()
```

### 스크립트 구조

```bash
#!/bin/bash
# ~/.autodev/crons/claw-evaluate.sh

# Guard
PENDING=$(autodev queue list --workspace "$AUTODEV_WORKSPACE_NAME" --json | jq 'length')
if [ "$PENDING" = "0" ]; then
  echo "skip: 큐 비어있음"
  exit 0
fi

# 실행 (AgentRuntime 설정에 따라 적절한 LLM 사용)
autodev agent --workspace "$AUTODEV_WORKSPACE_NAME" -p "큐를 평가하고 다음 작업을 결정해줘"
```

### 환경변수 주입

Daemon이 cron 실행 시 자동 주입:

| 변수 | 예시 |
|------|------|
| `AUTODEV_WORKSPACE_NAME` | `org/repo-a` |
| `AUTODEV_WORKSPACE_ROOT` | `/Users/me/repos/repo-a` |
| `AUTODEV_WORKSPACE_URL` | `https://github.com/org/repo-a` |
| `AUTODEV_HOME` | `~/.autodev` |
| `AUTODEV_DB` | `~/.autodev/autodev.db` |
| `AUTODEV_CLAW_WORKSPACE` | `~/.autodev/claw-workspace` |

### /claw 세션에서 관리

```
"cron 목록 보여줘"       → autodev cron list --json
"gap-detection 일시정지" → autodev cron pause gap-detection
"커스텀 cron 추가"       → autodev cron add --name ... --interval ...
```

### Built-in vs Custom

| | Built-in | Custom |
|---|---|---|
| 생성 | workspace 등록 시 자동 | /cron add |
| 제거 | 불가 (pause/resume) | 자유 |
| Guard | 내장 | 사용자 자유 |

---

### 관련 플로우

- [Flow 0: AgentRuntime](../00-agent-runtime/flow.md) — claw-evaluate 실행 런타임
- [Flow 10: Claw 워크스페이스](../10-claw-workspace/flow.md) — Claw headless
