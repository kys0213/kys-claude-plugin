# TUI 대시보드 - 완료

## 항목

- [x] **14. ratatui 기본 레이아웃 (active items, labels, logs)**
  - `tui/` 모듈 구현
  - Active items 실시간 표시 (이슈/PR/머지 큐 상태)
  - 라벨 상태 시각화 (wip/done/skip/failed 카운트 + 바 차트)
  - 최근 로그 패널

- [x] **15. daemon.log tail 표시**
  - 데몬 로그 파일 실시간 tail (`LogTailer` — 파일 오프셋 기반 incremental read)
  - TUI 하단 패널에 스트리밍 표시 (500ms 폴링)
  - 로그 레벨별 색상 구분 (ERROR=red, WARN=yellow, INFO=green, DEBUG=cyan)

- [x] **16. 키바인딩**
  - 큐 항목 선택/필터링 (j/k/↑/↓)
  - 수동 retry/skip 액션 (r: retry failed, s: skip active)
  - 패널 간 포커스 전환 (Tab: Repos → ActiveItems → Labels → Logs)
  - 종료/새로고침 (q: quit, R: refresh, ?: help toggle)

## 구현 상세

### 레이아웃 (DESIGN.md Section 10 준수)

```
┌─────────────────────────────────────────────────────────┐
│  autodev v0.1.0          ● daemon running    [?]help    │
├──────────┬──────────────────────────────────────────────┤
│          │  [I] org/repo#42  analyzing   Bug fix        │
│ Repos    │  [P] org/repo#10  reviewing   Add feature    │
│ (25%)    │  [M] org/repo#15  merging     Release v2     │
│          ├──────────────────────────────────────────────┤
│          │  autodev:wip    3  ███░░░░░░░░░░░░           │
│          │  autodev:done  28  ████████████████           │
│          │  autodev:skip   0  ░░░░░░░░░░░░░░░           │
│          │  failed         1  █░░░░░░░░░░░░░░           │
│          ├──────────────────────────────────────────────┤
│          │  14:32 INFO starting daemon                  │
│          │  14:30 WARN retrying item                    │
│          │  14:28 ERROR scan failed: timeout            │
└──────────┴──────────────────────────────────────────────┘
 Tab:panel  j/k:navigate  r:retry  s:skip  R:refresh  q:quit
```

### 파일 변경

| 파일 | 변경 내용 |
|------|----------|
| `tui/mod.rs` | LogTailer 통합, retry/skip/refresh 핸들러, 상태 메시지 |
| `tui/views.rs` | 4-패널 레이아웃, ActiveItem/LabelCounts 쿼리, 색상 코딩 |
| `tui/events.rs` | LogTailer 구현 (파일 오프셋 기반 incremental tail) |

### 테스트

- `tui/events.rs` 단위 테스트: 4개 (LogTailer, parse_log_line)
- `tests/tui_tests.rs` 통합 테스트: 8개 (active items, label counts, retry, skip)
