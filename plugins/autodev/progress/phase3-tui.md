# Phase 3: TUI 대시보드 - 미착수

## 항목

- [ ] **14. ratatui 기본 레이아웃 (active items, labels, logs)**
  - `tui/` 모듈 구현
  - Active items 실시간 표시 (이슈/PR/머지 큐 상태)
  - 라벨 상태 시각화
  - 최근 로그 패널

- [ ] **15. daemon.log tail 표시**
  - 데몬 로그 파일 실시간 tail
  - TUI 하단 패널에 스트리밍 표시
  - 로그 레벨별 색상 구분

- [ ] **16. 키바인딩**
  - 큐 항목 선택/필터링
  - 수동 retry/skip 액션
  - 패널 간 포커스 전환
  - 종료/새로고침

## 의존성

- ratatui (Cargo.toml에 추가 필요)
- crossterm (터미널 백엔드)

## 참고

- DESIGN.md Section 14 (TUI 레이아웃 설계) 참조
- `auto-dashboard` 슬래시 커맨드가 TUI 진입점으로 이미 등록됨
