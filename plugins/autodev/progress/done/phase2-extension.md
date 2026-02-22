# Phase 2: 확장 - 완료

## 항목

- [x] **9. pipeline/pr.rs + pipeline/merge.rs**
  - `pipeline/pr.rs` — PR 리뷰 파이프라인 (pending → reviewing → review_done)
  - `pipeline/merge.rs` — 머지 파이프라인 (pending → merging → done/conflict)
  - Reviewer/Merger 컴포넌트에 실행 로직 위임, 파이프라인은 오케스트레이션만 담당

- [x] **10. daemon/recovery.rs (orphan wip 정리)**
  - GitHub에서 autodev:wip 라벨이 있지만 ActiveItems에 없는 고아 항목 감지
  - 고아 항목의 wip 라벨 자동 제거
  - 데몬 루프 최상단에서 매 tick 실행 (recovery → scan → pipeline 순서)

- [x] **11. components/ (reviewer, merger)**
  - `components/reviewer.rs` — ReviewOutput (review, stdout, stderr, exit_code)
  - `components/merger.rs` — MergeOutcome enum (Success/Conflict/Failed/Error)
  - `contains_conflict()` 헬퍼로 대소문자 무관 충돌 감지 (git은 "CONFLICT" 대문자 출력)

- [x] **12. 슬래시 커맨드 (auto-setup, auto, auto-config, auto-dashboard)**
  - `commands/auto.md` — 메인 대시보드 커맨드
  - `commands/auto-setup.md` — 레포 등록/해제
  - `commands/auto-config.md` — 설정 관리
  - `commands/auto-dashboard.md` — TUI 대시보드 실행
  - plugin.json에 등록 완료

- [x] **13. 에이전트 파일**
  - `agents/issue-analyzer.md` — 이슈 분석 에이전트
  - `agents/pr-reviewer.md` — PR 리뷰 에이전트
  - `agents/conflict-resolver.md` — 충돌 해결 에이전트
  - plugin.json에 등록 완료

## 테스트

- component_tests.rs — 13개 (reviewer 4 + merger 9)
- daemon_recovery_tests.rs — 10개
- notifier_tests.rs — 12개 (QA 보강)
- issue_verdict_tests.rs — 6개 (QA 보강)
- queue_admin_tests.rs — 19개 (QA 보강)
