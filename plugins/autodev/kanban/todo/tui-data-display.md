# TUI 대시보드 데이터 표시 (M-06)

> **Priority**: High — 대시보드 핵심 기능 미작동
> **Gap Report**: DESIGN-GAP-REPORT.md v2 §3 M-06
> **난이도**: 중간

## 항목

- [ ] **24. daemon status file 생성**
  - `daemon/mod.rs` — 매 tick마다 `~/.autodev/daemon.status.json`에 TaskQueues 스냅샷 저장
  - 포함 정보: active items (work_id, type, phase), label counts, daemon uptime

- [ ] **25. `query_active_items()` 구현**
  - `tui/views.rs:121-125` — `Vec::new()` 대신 status file에서 active items 읽기
  - daemon이 실행중이 아니면 빈 배열 반환 (graceful fallback)

- [ ] **26. `query_label_counts()` 구현**
  - `tui/views.rs:127-130` — `LabelCounts::default()` 대신 status file에서 label counts 읽기
  - 또는 GitHub API 캐시 방식으로 구현

## 현재 문제

```rust
pub fn query_active_items(_db: &Database) -> Vec<ActiveItem> {
    Vec::new()  // ← 항상 비어있음
}

pub fn query_label_counts(_db: &Database) -> LabelCounts {
    LabelCounts::default()  // ← 항상 0
}
```

Active Items, Labels Summary 패널이 항상 비어있어 대시보드의 실질적 가치가 없음.

## 관련 파일

| 파일 | 변경 내용 |
|------|----------|
| `daemon/mod.rs` | status file 생성 로직 추가 |
| `tui/views.rs` | `query_active_items()`, `query_label_counts()` 실제 구현 |
| `queue/task_queues.rs` | TaskQueues → JSON 직렬화 (serde) |

## 완료 조건

- [ ] daemon 실행 중 `~/.autodev/daemon.status.json` 갱신됨
- [ ] TUI Active Items 패널에 현재 처리중인 아이템 표시됨
- [ ] TUI Labels 패널에 wip/done/skip 카운트 표시됨
- [ ] daemon 미실행 시 graceful하게 빈 화면 표시
