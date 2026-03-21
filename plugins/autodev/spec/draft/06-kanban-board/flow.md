# Flow 6: 진행 상태 확인 (칸반 보드)

### 시나리오

사용자가 다수 workspace × 다수 스펙의 전체 진행 상황을 한눈에 확인한다.

### 인터페이스 3개 레이어

| 레이어 | 진입 | 특징 |
|--------|------|------|
| TUI Dashboard | `autodev dashboard` | ratatui, 실시간 갱신, 키보드 네비게이션 |
| CLI 출력 | `autodev board --format rich` | 정적 스냅샷, 진행률 바 |
| Claw 세션 | `/claw` → "보드 보여줘" | 자연어, autodev board --json 파싱 |

### TUI Dashboard

#### AllRepos 뷰 (기본)

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

#### PerRepo 뷰 (Tab 전환)

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

#### 전이 타임라인 (ItemDetail 오버레이, Enter)

```
┌─ #42 JWT middleware ─────────────────────────┐
│ Phase: Done | Runtime: claude/sonnet         │
│                                               │
│ Timeline:                                     │
│  14:00 ○ Pending  ← github.collect()         │
│         └ SpecLinker: linked to auth-v2       │
│  14:05 ○ Ready    ← Claw advance             │
│  14:06 ○ Running  ← AnalyzeTask              │
│         └ claude/sonnet (1.2K tokens, 6m)    │
│  14:12 ● Done                                 │
│         └ SpecCompletionCheck: 3/5            │
└───────────────────────────────────────────────┘
```

#### 키보드

| 키 | 동작 |
|----|------|
| j/k, ↑/↓ | 아이템 이동 |
| ←/→ | workspace 전환 |
| Tab | AllRepos ↔ PerRepo |
| Enter | 상세 / 전이 타임라인 |
| h | HITL 오버레이 |
| s | Spec 상세 |
| d | Claw 판단 이력 |
| R | 새로고침 |

### CLI 출력 (`--format rich`)

```
autodev board --format rich

auth-v2  Auth Module v2                    ████████░░░░ 60% (3/5)
  ✅ #42 JWT middleware
  ✅ #43 Token API
  🔄 #44 Session adapter (running, claude/sonnet)
  ⏳ #45 Error handling (dep: #44)
  ⏳ #46 Missing tests
```

### `--format` 옵션 (CLI 공통)

| 값 | 용도 |
|---|------|
| `text` | 기본 텍스트 (기존 호환) |
| `json` | 구조화된 JSON (Claw 파싱용) |
| `rich` | 색상 + 박스 + 진행률 바 |

### 추가 테이블 (시각화용)

```sql
transition_events (
    id          TEXT PRIMARY KEY,
    work_id     TEXT NOT NULL,
    event_type  TEXT NOT NULL,
    detail      TEXT,
    created_at  TEXT NOT NULL
)
```

---

### 관련 플로우

- [Flow 14: 시각화](../14-visualization/flow.md) — 전체 시각화 설계
