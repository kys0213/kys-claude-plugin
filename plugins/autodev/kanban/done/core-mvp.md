# 코어 (MVP) - 완료

## 항목

- [x] **1. Cargo 프로젝트 초기화 + CLI 프레임워크**
  - `cli/Cargo.toml`, `cli/src/main.rs`
  - clap 기반 서브커맨드 (repo, queue, status, daemon, logs, config)

- [x] **2. infrastructure/ trait 정의 + mock 구현 (gh, git, claude)**
  - `infrastructure/gh/mod.rs` — Gh trait (field, paginate, label_add, label_remove, issue_comment)
  - `infrastructure/git/mod.rs` — Git trait (clone, worktree_add/remove, checkout, pull, push)
  - `infrastructure/claude/mod.rs` — Claude trait (run_session)
  - 각 trait에 대한 MockGh, MockGit, MockClaude 구현

- [x] **3. queue/ 모듈 (models, schema, repository)**
  - `queue/models.rs` — 전체 모델 (IssueQueueItem, PrQueueItem, MergeQueueItem 등)
  - `queue/schema.rs` — SQLite 테이블 생성 + 마이그레이션
  - `queue/repository.rs` — trait별 CRUD (RepoRepository, IssueQueueRepository, PrQueueRepository, MergeQueueRepository, QueueAdmin)
  - `queue/mod.rs` — Database 구조체 (SQLite Connection 래핑)

- [x] **4. components/ (workspace, analyzer, notifier, verdict)**
  - `components/workspace.rs` — 워크트리 생성/삭제
  - `components/verdict.rs` — 이슈 분석 결과 파싱 (AnalysisResult: implement/wontfix/needs_clarification)
  - `components/notifier.rs` — GitHub 상태 체크 + 코멘트 게시
  - `components/output.rs` — Claude 세션 출력 파싱

- [x] **5. scanner/ (issues, pulls)**
  - `scanner/issues.rs` — GitHub 이슈 스캔 (라벨 필터, 커서 기반 중복 방지)
  - `scanner/pulls.rs` — GitHub PR 스캔
  - `scanner/mod.rs` — 통합 scan 진입점

- [x] **6. pipeline/issue.rs (분석 -> 구현 흐름)**
  - pending → analyzing → verdict 분기 (implement/wontfix/needs_clarification)
  - implement → ready → processing → done 전이
  - Claude 세션 실행 + 결과 파싱

- [x] **7. daemon/ 메인 루프 + pid**
  - `daemon/mod.rs` — tick 기반 루프 (recovery → scan → pipeline)
  - PID 파일 관리

- [x] **8. infrastructure/ real 구현체 연결**
  - `infrastructure/gh/real.rs` — RealGh (gh CLI 래핑)
  - `infrastructure/git/real.rs` — RealGit (git CLI 래핑)
  - `infrastructure/claude/real.rs` — RealClaude (claude CLI 래핑)

## 테스트

- repository_tests.rs — 53개
- config_loader_tests.rs — 10개
- cli_tests.rs — 14개
- daemon_scan_tests.rs — 10개
- daemon_consumer_tests.rs — 6개
