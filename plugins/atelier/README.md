# atelier

> **상태: WIP — Epic 1 (atelier 통합) 진행 중.** 현재는 골격만 존재하며 capability(commands/agents/skills/hooks)는 아직 이전되지 않았습니다. 설치해도 동작하는 기능은 없습니다.

"개발 워크플로우를 처음부터 끝까지 책임지는 단일 큐레이션 plugin." 도구·재료·습관·작업 절차가 한 작업실(atelier)에 정돈된다는 메타포입니다.

흩어진 6개 plugin을 하나로 흡수해, 묵시적으로 서로를 호출하던 의존을 **단일 plugin 안의 명시적 그래프**로 정리합니다.

## 흡수 매핑 (6 → 1)

| 출신 plugin | atelier 안 위치 |
|---|---|
| `git-utils` | `commands/git/*`, `skills/git-utils/` |
| `github-autopilot` | `commands/autopilot/*`, `agents/autopilot-*`, `skills/autopilot-*` |
| `spec-kit` | `commands/spec/*`, `agents/spec-*`, `skills/spec-*` |
| `workflow-guide` | `skills/convention-architect`, `docs/workflow-principles.md` |
| `coding-style` | `skills/coding-style/` |
| `orchestrator` | `skills/orchestrator/`, `docs/orchestrator-pattern.md` |

흡수하지 않는 plugin은 두 부류다 (concept §3.2 / §3.3):

- **영구 분리** (성격이 다른 인프라): `external-llm`, `barrier-sync`, `openclaw-docker`
- **일단 유지, atelier 안정화 후 제거 검토**: `suggest-workflow`, `autodev`, `develop-workflow`, `hud`

## 슬래시 표면 (placeholder)

이전 완료 후 모든 커맨드는 단일 namespace `/atelier:<group>/<command>` 로 노출됩니다 (`git` · `autopilot` · `spec` · `workflow`). 상세는 `plans/atelier/06-invocation-surface.md`.

## 통합 CLI

단일 Rust 바이너리 `atelier` 가 `atelier git|autopilot|spec|hook|setup <subcmd>` 라우팅을 담당합니다 (기존 per-plugin `autopilot`·`git-utils` 바이너리 대체). 상세는 `plans/atelier/02-architecture.md` §4.

## 설계 문서

진행 상황과 전체 설계는 [`plans/atelier/`](../../plans/atelier/) 를 참조하세요 (concept · inventory · architecture · migration · rollout · skill-architecture · invocation-surface).
