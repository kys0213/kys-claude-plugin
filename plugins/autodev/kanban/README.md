# Autodev Kanban

디렉토리 기반 칸반보드로 진행 상황을 추적합니다.

```
kanban/
├── todo/          ← 미착수
├── in-progress/   ← 진행 중
└── done/          ← 완료
```

## todo/

> DESIGN-GAP-REPORT.md v2 (2026-02-23) 기반 잔존 gap

| 항목 | 파일 | 우선순위 |
|------|------|---------|
| 로그 롤링/보존 (M-04+M-05) | [todo/log-rolling.md](./todo/log-rolling.md) | Critical |
| TUI 데이터 표시 (M-06) | [todo/tui-data-display.md](./todo/tui-data-display.md) | High |
| Phase 상태 세분화 (M-01) | [todo/phase-refinement.md](./todo/phase-refinement.md) | Medium |
| suggest-workflow 통합 (M-03) | [todo/suggest-workflow-integration.md](./todo/suggest-workflow-integration.md) | Medium |
| 문서 정합성 갱신 (L-03+L-04) | [todo/doc-consistency.md](./todo/doc-consistency.md) | Low |

## in-progress/

_(없음)_

## done/

| 항목 | 파일 |
|------|------|
| 코어 (MVP) | [done/core-mvp.md](./done/core-mvp.md) |
| PR/머지 파이프라인 | [done/pr-merge-pipeline.md](./done/pr-merge-pipeline.md) |
| TUI 대시보드 | [done/tui-dashboard.md](./done/tui-dashboard.md) |
| CI/CD 및 배포 | [done/ci-release.md](./done/ci-release.md) |

## 테스트 현황

- 전체 테스트: **185개** (모두 통과)
- 마지막 검증일: 2026-02-22
