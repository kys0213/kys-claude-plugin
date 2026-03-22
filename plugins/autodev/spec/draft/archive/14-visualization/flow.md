# Flow 14: 시각화

### 시나리오

사용자가 TUI, CLI, Claw 세션 3개 레이어에서 일관된 시각 경험을 갖는다.

### `--format` 옵션 (CLI 공통)

| 값 | 용도 |
|---|------|
| `text` | 기본 텍스트 (기존 호환) |
| `json` | 구조화된 JSON (Claw 파싱용) |
| `rich` | 색상 + 박스 + 진행률 바 (터미널용) |

모든 CLI 서브커맨드(status, board, spec list, spec status, queue list 등)에 적용.

### CLI 출력 개선

#### `autodev status --format rich`

```
● autodev daemon (uptime 2h 15m)

Repos:
  org/repo-a    ● active   queue: 3P 1R 2D   specs: 2/3
  org/repo-b    ● active   queue: 0P 0R 5D   specs: 1/1 ✓

Runtime: claude/sonnet (45.2K tokens/1h)
HITL: 1 pending ⚠
Next claw-evaluate: 25s
```

#### `autodev board --format rich`

```
auth-v2  Auth Module v2                    ████████░░░░ 60% (3/5)
  ✅ #42 JWT middleware
  ✅ #43 Token API
  🔄 #44 Session adapter (running, claude/sonnet)
  ⏳ #45 Error handling (dep: #44)
  ⏳ #46 Missing tests
```

#### `autodev spec status <id> --format rich`

```
auth-v2  Auth Module v2
Status: Active | Runtime: claude/sonnet
Progress: ████████░░░░ 60% (3/5)

Issues:
  ✅ #42 JWT middleware       Done     6m   1.2K tokens
  ✅ #43 Token API            Done     8m   1.8K tokens
  🔄 #44 Session adapter      Running  3m   ...
  ⏳ #45 Error handling        Pending  dep:#44
  ⏳ #46 Missing tests         Pending

Acceptance Criteria:
  ✅ POST /auth/login → JWT 반환 (200)
  ✅ 만료 토큰 → 401 반환
  ⬜ POST /auth/refresh → 새 토큰 반환
  ⬜ cargo test -p auth 전체 통과

Dependencies:
  #45 depends on #44 (shared: src/auth/session.rs)
```

### TUI Dashboard 추가 패널

v4 대비 추가:

```
┌─ Runtime ──────────────────┐  ┌─ DataSource ────────────────┐
│ claude/sonnet  12 runs  OK │  │ github  ● connected         │
│ claude/opus     2 runs  OK │  │   last scan: 30s ago        │
│ Tokens: 45.2K in / 12.1K  │  │   compensation queue: 0     │
│ Avg duration: 4m 32s      │  │                             │
└────────────────────────────┘  └─────────────────────────────┘

┌─ Hooks (1h) ──────────────────────┐
│ on_phase_enter   24 ok  1 ⚠       │
│ before_task       8 ok            │
│ after_task        8 ok            │
│ ⚠ LabelSyncer: rate limit (1 pending) │
└───────────────────────────────────┘
```

### 전이 타임라인

ItemDetail 오버레이에서 아이템의 상태 전이 이력 표시:

```
┌─ #42 JWT middleware ─────────────────────────┐
│ Timeline:                                     │
│  14:00 ○ Pending  ← github.collect()         │
│         ├ DependencyAnalyzer: no deps         │
│         └ SpecLinker: linked to auth-v2       │
│  14:05 ○ Ready    ← Claw advance             │
│  14:06 ○ Running  ← AnalyzeTask              │
│         └ claude/sonnet (1.2K tokens, 6m)    │
│  14:12 ● Done                                 │
│         └ SpecCompletionCheck: 3/5 done       │
└───────────────────────────────────────────────┘
```

### 데이터 요구사항

```sql
transition_events (
    id          TEXT PRIMARY KEY,
    work_id     TEXT NOT NULL,
    event_type  TEXT NOT NULL,    -- on_phase_enter, before_task, after_task, ...
    detail      TEXT,             -- handler name, result, error message
    created_at  TEXT NOT NULL
)
```

---

### 관련 플로우

- [Flow 6: 칸반 보드](../06-kanban-board/flow.md) — TUI/CLI 상세
- [Flow 10: Claw 워크스페이스](../10-claw-workspace/flow.md) — Claw 세션 내 시각화
