# Spec v5 Draft

> **Date**: 2026-03-21
> **구조**: 사용자 플로우 기반 + 관심사별 상세 스펙

## 설계 문서

- **[DESIGN-v5.md](./DESIGN-v5.md)** — 전체 아키텍처 (DataSource + AgentRuntime)

## 사용자 플로우 (flows/)

"사용자가 X를 하면 어떻게 되지?" — 시나리오 기반, 기획자/사용자 대상

| # | Flow | 설명 |
|---|------|------|
| 01 | [온보딩](./flows/01-setup.md) | 레포 등록 → 컨벤션 부트스트랩 → Claw 초기화 |
| 02 | [스펙 생명주기](./flows/02-spec-lifecycle.md) | 스펙 등록 → 우선순위 → 이슈 분해 → 완료 판정 |
| 03 | [이슈 파이프라인](./flows/03-issue-pipeline.md) | 이슈 감지 → 분석 → 실행 → 피드백 루프 |
| 04 | [실패 복구와 HITL](./flows/04-failure-and-hitl.md) | 장애 대응 → escalation → 사람 개입 → 복구 |
| 05 | [모니터링](./flows/05-monitoring.md) | 칸반 보드 + CLI 시각화 + TUI Dashboard |

## 관심사별 상세 스펙 (concerns/)

"이 시스템은 내부적으로 어떻게 동작하지?" — 구현자 대상

| 문서 | 설명 |
|------|------|
| [DataSource](./concerns/datasource.md) | 외부 시스템 추상화 trait + GitHub/Slack/Jira 구현 |
| [AgentRuntime](./concerns/agent-runtime.md) | LLM 실행 추상화 trait + Registry + 확장 시나리오 |
| [Claw 워크스페이스](./concerns/claw-workspace.md) | 판단 레이어 규칙/스킬 구조 + /claw 세션 |
| [Cron 엔진](./concerns/cron-engine.md) | 주기 실행 + force trigger + 환경변수 주입 |
| [CLI 레퍼런스](./concerns/cli-reference.md) | 3-layer SSOT + 전체 커맨드 트리 |
