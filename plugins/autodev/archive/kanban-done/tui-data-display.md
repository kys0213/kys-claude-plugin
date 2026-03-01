# TUI 대시보드 데이터 표시 (M-06)

> **Priority**: High — 대시보드 핵심 기능 미작동
> **Gap Report**: DESIGN-GAP-REPORT.md v2 §3 M-06
> **난이도**: 중간
> **완료일**: 2026-02-23

## 항목

- [x] **24. daemon status file 생성**
  - `daemon/status.rs` — `DaemonStatus` 구조체 + `build_status()`, `write_status()`, `read_status()`
  - `daemon/mod.rs` — 매 tick마다 `~/.autodev/daemon.status.json`에 TaskQueues 스냅샷 저장
  - 포함 정보: active items (work_id, type, phase), counters (wip/done/skip/failed), daemon uptime
  - 종료 시 `remove_status()`로 cleanup

- [x] **25. `query_active_items()` 구현**
  - `tui/views.rs` — status file에서 active items 읽기
  - daemon 미실행 시 빈 배열 반환 (graceful fallback)

- [x] **26. `query_label_counts()` 구현**
  - `tui/views.rs` — status file에서 counters 읽기
  - daemon 미실행 시 기본값 반환 (graceful fallback)

## 구현 요약

| 파일 | 변경 내용 |
|------|----------|
| `daemon/status.rs` (신규) | `DaemonStatus`, `StatusItem`, `StatusCounters` + build/write/read/remove + 6개 테스트 |
| `daemon/mod.rs` | 매 tick status file 쓰기 + 종료 시 cleanup |
| `tui/views.rs` | `query_active_items(path)`, `query_label_counts(path)` status file 기반 구현 |
| `tui/mod.rs` | status_path 전달 |
| `tests/tui_tests.rs` | status file 기반 테스트 5개 (읽기, 빈 파일, 패널 네비게이션) |

## 완료 조건

- [x] daemon 실행 중 `~/.autodev/daemon.status.json` 갱신됨
- [x] TUI Active Items 패널에 현재 처리중인 아이템 표시됨
- [x] TUI Labels 패널에 wip/done/skip 카운트 표시됨
- [x] daemon 미실행 시 graceful하게 빈 화면 표시
- [x] 전체 테스트 311개 모두 통과 (2026-02-23 기준)
