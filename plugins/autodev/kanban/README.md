# Autodev Kanban

디렉토리 기반 칸반보드로 진행 상황을 추적합니다.

```
kanban/
├── todo/          ← 미착수
├── in-progress/   ← 진행 중
└── done/          ← 완료
```

## todo/

> 설계-구현 정합성 분석 (2026-02-24) 기반

| 항목 | 파일 | 우선순위 |
|------|------|---------|
| PR Review API 구현 (H-1) | [todo/pr-review-api.md](./todo/pr-review-api.md) | High |
| DESIGN.md 설계-구현 정합성 갱신 (M-1, M-2, L-1~L-3) | [todo/design-doc-sync.md](./todo/design-doc-sync.md) | Medium |
| CLI queue 서브커맨드 + IPC 설계 (M-3) | [todo/cli-queue-ipc.md](./todo/cli-queue-ipc.md) | Medium |
| 소규모 정리 (L-4, L-5, L-6) | [todo/minor-cleanup.md](./todo/minor-cleanup.md) | Low |

## in-progress/

_(없음)_

## done/

| 항목 | 파일 |
|------|------|
| 설계-구현 정합성 분석 | [done/design-implementation-analysis.md](./done/design-implementation-analysis.md) |
| Phase 상태 세분화 (M-01) | [done/phase-refinement.md](./done/phase-refinement.md) |
| 문서 정합성 갱신 (L-03+L-04) | [done/doc-consistency.md](./done/doc-consistency.md) |
| 코어 (MVP) | [done/core-mvp.md](./done/core-mvp.md) |
| PR/머지 파이프라인 | [done/pr-merge-pipeline.md](./done/pr-merge-pipeline.md) |
| TUI 대시보드 | [done/tui-dashboard.md](./done/tui-dashboard.md) |
| CI/CD 및 배포 | [done/ci-release.md](./done/ci-release.md) |
| 로그 롤링/보존 (M-04+M-05) | [done/log-rolling.md](./done/log-rolling.md) |
| TUI 데이터 표시 (M-06) | [done/tui-data-display.md](./done/tui-data-display.md) |
| suggest-workflow 통합 (M-03) | [done/suggest-workflow-integration.md](./done/suggest-workflow-integration.md) |
| Gap 개선 (H-01, H-02, H-03, M-02) | [done/gap-improvement.md](./done/gap-improvement.md) |
| SQLite → In-Memory 리팩토링 | [done/refactoring-sqlite-to-memory.md](./done/refactoring-sqlite-to-memory.md) |

## 테스트 현황

- 전체 테스트: **311개** (모두 통과)
- 마지막 검증일: 2026-02-23
