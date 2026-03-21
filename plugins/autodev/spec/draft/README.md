# Spec v5 Draft

> **Date**: 2026-03-21

## 설계 문서

- **[DESIGN-v5.md](./DESIGN-v5.md)** — 전체 아키텍처 설계

## Flow 문서

| # | Flow | 설명 |
|---|------|------|
| 00 | [DataSource](./00-datasource/flow.md) | 외부 시스템 OCP (GitHub, Slack, Jira) |
| 00 | [AgentRuntime](./00-agent-runtime/flow.md) | LLM 실행 OCP (Claude, Gemini, Codex) |
| 01 | [레포 등록](./01-repo-registration/flow.md) | 레포 등록/변경/제거 + DataSource 바인딩 |
| 02 | [이슈 등록](./02-issue-registration/flow.md) | 이슈 모드 + 의존성 분석 + 스펙 링크 |
| 03 | [스펙 등록](./03-spec-registration/flow.md) | 스펙 모드 + lifecycle (6상태) |
| 04 | [스펙 우선순위](./04-spec-priority/flow.md) | 다중 스펙 + DependencyGuard |
| 05 | [HITL 알림](./05-hitl-notification/flow.md) | HITL 생성/응답/라우팅 |
| 06 | [칸반 보드](./06-kanban-board/flow.md) | TUI + CLI + Claw 시각화 |
| 07 | [피드백 루프](./07-feedback-loop/flow.md) | PR 리뷰 / spec update / replan |
| 08 | [스펙 완료](./08-spec-completion/flow.md) | 완료 감지 + 테스트 + gap detection |
| 09 | [실패 복구](./09-failure-recovery/flow.md) | DataSource별 escalation + shutdown |
| 10 | [Claw 워크스페이스](./10-claw-workspace/flow.md) | 세션 경험 + slash command 통합 |
| 11 | [컨벤션](./11-convention-bootstrap/flow.md) | 부트스트랩 + 자율 개선 |
| 12 | [CLI 레퍼런스](./12-cli-reference/flow.md) | 3-layer + 전체 커맨드 참조 |
| 13 | [Cron](./13-cron/flow.md) | force trigger + 스크립트 관리 |
| 14 | [시각화](./14-visualization/flow.md) | --format rich + TUI 패널 + 타임라인 |
