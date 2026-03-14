# Flow 6: 진행 상태 확인 (칸반 보드)

### 시나리오

사용자가 다수 레포 × 다수 스펙의 전체 진행 상황을 한눈에 확인한다.
**전체 보기**와 **레포별 보기** 두 가지 뷰를 전환할 수 있다.

### 보드 아키텍처 (OCP)

보드 렌더링은 **BoardRenderer trait**으로 추상화. 현재는 TUI만 구현하되, 향후 확장 가능.

```rust
/// 보드 렌더링 추상화. 새 렌더러 추가 = 구현체 추가.
pub trait BoardRenderer: Send {
    /// 현재 상태를 렌더링한다.
    async fn render(&self, state: &BoardState) -> Result<()>;
}

// 구현체
// ├── TuiBoardRenderer       (터미널 칸반, 현재 구현)
// ├── (GitHubProjectSync)    (향후: GitHub Projects 연동)
// └── (WebBoardRenderer)     (향후: HTML 리포트)
```

### BoardState (렌더러 입력)

```rust
pub struct BoardState {
    pub repos: Vec<RepoBoardState>,
    pub hitl_items: Vec<HitlItem>,       // cross-repo HITL 대기
    pub claw_status: ClawStatus,         // 마지막 판단 시각, 다음 판단까지
}

pub struct RepoBoardState {
    pub repo_name: String,
    pub specs: Vec<SpecBoardState>,
    pub orphan_issues: Vec<IssueBoardItem>,  // 스펙에 속하지 않는 이슈
    pub hitl_items: Vec<HitlItem>,           // 이 레포의 HITL 대기
}

pub struct SpecBoardState {
    pub spec_title: String,
    pub status: SpecStatus,
    pub progress: Progress,              // done/total
    pub columns: Vec<BoardColumn>,       // 칸반 컬럼
    pub acceptance_criteria: Vec<CriteriaItem>,
}

pub struct BoardColumn {
    pub phase: String,                   // Pending, Analyzing, Implementing, ...
    pub items: Vec<BoardItem>,
}
```

### CLI 명령어

```bash
# 전체 보기 (모든 레포)
autodev dashboard

# 레포별 보기
autodev dashboard --repo org/repo-a
```

---

### 뷰 1: 전체 보기 (`autodev dashboard`)

모든 레포의 스펙과 HITL을 한 화면에서 확인.

```
┌─ autodev dashboard ──────────────────────────────────────────────────┐
│                                                                      │
│ [Tab: 전체 보기 | 레포별 보기]                                        │
│                                                                      │
│ 🔔 HITL 대기 (2건)                                                   │
│ ├─ [HIGH] org/repo-a PR #42 리뷰 3회 반복                            │
│ └─ [MED]  org/repo-b 스펙 충돌 감지                                   │
│                                                                      │
│ ═══ org/repo-a ══════════════════════════════════════════════════════ │
│                                                                      │
│ 📋 Auth Module v2 [Active] 3/5 (60%)                                 │
│ ┌─────────┬───────────┬──────────────┬───────────┬────────┐         │
│ │ Pending │ Analyzing │ Implementing │ Reviewing │ Done   │         │
│ ├─────────┼───────────┼──────────────┼───────────┼────────┤         │
│ │ #45     │ #46 (gap) │ #44          │           │ #42    │         │
│ │         │           │              │           │ #43    │         │
│ └─────────┴───────────┴──────────────┴───────────┴────────┘         │
│                                                                      │
│ ═══ org/repo-b ══════════════════════════════════════════════════════ │
│                                                                      │
│ 📋 Payment Gateway [Active] 1/4 (25%)                                │
│ ┌─────────┬───────────┬──────────────┬───────────┬────────┐         │
│ │ Pending │ Analyzing │ Implementing │ Reviewing │ Done   │         │
│ ├─────────┼───────────┼──────────────┼───────────┼────────┤         │
│ │ #12     │ #13       │              │ #11       │ #10    │         │
│ │ #14     │           │              │           │        │         │
│ └─────────┴───────────┴──────────────┴───────────┴────────┘         │
│                                                                      │
│ Claw: 마지막 판단 10초 전 │ 다음 판단 50초 후                          │
└──────────────────────────────────────────────────────────────────────┘
```

---

### 뷰 2: 레포별 보기 (`autodev dashboard --repo org/repo-a`)

단일 레포에 집중. 스펙 상세 + HITL + acceptance criteria를 넓게 표시.

```
┌─ autodev dashboard: org/repo-a ──────────────────────────────────────┐
│                                                                      │
│ [Tab: 전체 보기 | 레포별 보기]                                        │
│                                                                      │
│ 🔔 HITL 대기 (1건)                                                   │
│ └─ [HIGH] PR #42 리뷰 3회 반복                                       │
│    "동일한 피드백(에러 핸들링 누락)이 반복되고 있습니다."               │
│    [1: 직접 리뷰] [2: skip] [3: 스펙 수정]                            │
│                                                                      │
│ ═══ Auth Module v2 [Active] 3/5 (60%) ═══════════════════════════════│
│                                                                      │
│ ┌─────────┬───────────┬──────────────┬───────────┬────────┐         │
│ │ Pending │ Analyzing │ Implementing │ Reviewing │ Done   │         │
│ ├─────────┼───────────┼──────────────┼───────────┼────────┤         │
│ │ #45     │ #46 (gap) │ #44          │           │ #42    │         │
│ │ Error   │ Missing   │ Session      │           │ JWT    │         │
│ │ handling│ tests     │ adapter      │           │ middle │         │
│ │         │           │              │           │ #43    │         │
│ │         │           │              │           │ Token  │         │
│ │         │           │              │           │ API    │         │
│ └─────────┴───────────┴──────────────┴───────────┴────────┘         │
│                                                                      │
│ Acceptance Criteria:                                                 │
│ ✅ POST /auth/login → JWT (200)                                      │
│ ✅ 만료 토큰 → 401                                                   │
│ ❌ POST /auth/refresh → 새 토큰                                      │
│ ❌ e2e 테스트 전체 통과                                               │
│                                                                      │
│ 최근 Claw 판단:                                                      │
│ 10초 전  Advance  #44 Pending→Implementing  (0.95)                   │
│ 10초 전  Skip     #45 "depends on #44"       (0.90)                  │
│ 10초 전  DetectGap #46 "Missing tests"       (0.80)                  │
│                                                                      │
│ Claw: 마지막 판단 10초 전 │ 다음 판단 50초 후                          │
└──────────────────────────────────────────────────────────────────────┘
```

---

### 뷰 전환

TUI 내에서 `Tab` 키로 전체/레포별 뷰를 전환. 레포별 뷰에서는 `←→`로 레포 간 이동.

### TUI 인터랙션

dashboard는 **읽기 전용 모니터링**. 제어는 CLI(`autodev <command>`) 또는 Claw(`autodev agent`)로 수행.

| 키 | 전체 보기 | 레포별 보기 |
|---|----------|-----------|
| `Tab` | → 레포별 보기 (현재 선택된 레포) | → 전체 보기 |
| `↑↓` | 레포/스펙 간 이동 | 스펙/이슈 간 이동 |
| `←→` | 칸반 컬럼 간 이동 | 레포 간 전환 |
| `Enter` | 이슈/PR 상세 보기 | 이슈/PR 상세 보기 |
| `h` | HITL 대기 항목 보기 | 이 레포의 HITL 항목 |
| `s` | 스펙 상세 보기 | 스펙 상세 + acceptance criteria |
| `d` | Claw 판단 이력 (전체) | Claw 판단 이력 (이 레포) |
| `q` | 종료 | 전체 보기로 돌아가기 |

### 제어는 CLI로

dashboard에서 상태를 확인한 후, 별도 터미널에서 CLI로 제어한다.

```bash
# dashboard에서 HITL 대기 확인 → CLI로 응답
autodev hitl respond hitl-01 --choice 2

# dashboard에서 스펙 진행도 확인 → CLI로 상세 조회
autodev spec status auth-v2

# 판단이 필요하면 Claw에게 위임
autodev agent
```

전체 CLI 명령어는 [Flow 12: CLI 레퍼런스](./12-cli-reference.md) 참조.
