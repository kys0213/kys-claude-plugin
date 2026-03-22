# Flow 5: 모니터링 — 칸반 보드 + 시각화

> 사용자가 다수 workspace × 다수 스펙의 전체 진행 상황을 TUI, CLI, /claw 세션에서 일관되게 확인한다.

---

## 1. 인터페이스 3개 레이어

| 레이어 | 진입 | 특징 |
|--------|------|------|
| TUI Dashboard | `autodev dashboard` | ratatui, 실시간 갱신, 키보드 네비게이션 |
| CLI 출력 | `autodev board --format rich` | 정적 스냅샷, 진행률 바 |
| Claw 세션 | `/claw` → "보드 보여줘" | 자연어, autodev board --json 파싱 |

---

## 2. TUI Dashboard

### AllRepos 뷰 (기본)

```
┌─ Repos ──────┐┌─ Active Items ─────┐┌─ Runtime ─────────────┐
│ ● org/repo-a ││ 🔄 #42 Analyze     ││ claude/sonnet  12 OK  │
│ ○ org/repo-b ││ 🔄 #44 Implement   ││ Tokens: 45.2K / 1h   │
│              ││                    ││ Avg: 4m 32s          │
├──────────────┤├────────────────────┤├───────────────────────┤
│ Logs         ││ DataSource         ││ Hooks (1h)            │
│ 14:30 adv #42││ github ● connected ││ on_phase_enter 24 ok  │
│ 14:25 dec .. ││   scan: 30s ago    ││ before_task     8 ok  │
│ 14:20 skip   ││   compensation: 0  ││ ⚠ LabelSyncer: 1     │
└──────────────┘└────────────────────┘└───────────────────────┘
```

### PerRepo 뷰 (Tab 전환)

```
┌─ Board ──────────────────────────────┐┌─ Active ──────────┐
│ auth-v2  ████████░░ 60% (3/5)        ││ 🔄 #44 Implement  │
│   ✅ #42 JWT middleware               ││   claude/sonnet    │
│   ✅ #43 Token API                    ││   3m elapsed       │
│   🔄 #44 Session adapter (running)   ││                    │
│   ⏳ #45 Error handling (dep: #44)   ││                    │
│   ⏳ #46 Missing tests               ││                    │
├──────────────────────────────────────┤├────────────────────┤
│ Orphan: 0 | HITL: 1 pending         ││ Logs               │
│ Kanban: 0P | 0R | 1R | 2D | 0S     ││ ...                │
└──────────────────────────────────────┘└────────────────────┘
```

### 전이 타임라인 (ItemDetail 오버레이, Enter)

```
┌─ #42 JWT middleware ─────────────────────────┐
│ Phase: Done | Runtime: claude/sonnet         │
│                                               │
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

### 키보드

| 키 | 동작 |
|----|------|
| j/k, ↑/↓ | 아이템 이동 |
| ←/→ | 레포 전환 |
| Tab | AllRepos ↔ PerRepo |
| Enter | 상세 / 전이 타임라인 |
| h | HITL 오버레이 |
| s | Spec 상세 |
| d | Claw 판단 이력 |
| R | 새로고침 |

---

## 3. CLI 출력

### `--format` 옵션 (CLI 공통)

| 값 | 용도 |
|---|------|
| `text` | 기본 텍스트 (기존 호환) |
| `json` | 구조화된 JSON (Claw 파싱용) |
| `rich` | 색상 + 박스 + 진행률 바 (터미널용) |

모든 CLI 서브커맨드(status, board, spec list, spec status, queue list 등)에 적용.

### `autodev status --format rich`

```
● autodev daemon (uptime 2h 15m)

Repos:
  org/repo-a    ● active   queue: 3P 1R 2D   specs: 2/3
  org/repo-b    ● active   queue: 0P 0R 5D   specs: 1/1 ✓

Runtime: claude/sonnet (45.2K tokens/1h)
HITL: 1 pending ⚠
Next claw-evaluate: 25s
```

### `autodev board --format rich`

```
auth-v2  Auth Module v2                    ████████░░░░ 60% (3/5)
  ✅ #42 JWT middleware
  ✅ #43 Token API
  🔄 #44 Session adapter (running, claude/sonnet)
  ⏳ #45 Error handling (dep: #44)
  ⏳ #46 Missing tests
```

### `autodev spec status <id> --format rich`

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

---

## 4. TUI 추가 패널 (v4 대비)

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

---

## 5. 데이터 요구사항

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

### 관련 문서

- [스펙 생명주기](./02-spec-lifecycle.md) — 스펙 진행률
- [실패 복구와 HITL](./04-failure-and-hitl.md) — HITL 오버레이
- [CLI 레퍼런스](../concerns/cli-reference.md) — 전체 커맨드 트리
