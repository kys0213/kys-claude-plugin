# atelier

> 🚧 **WIP** — Epic 1 (atelier consolidation, [#765](https://github.com/kys0213/kys-claude-plugin/issues/765)) 진행 중입니다.
> 설계: [`plans/atelier/`](../../plans/atelier/) · 상위 epic: [#738](https://github.com/kys0213/kys-claude-plugin/issues/738)

**atelier**(공방)는 개발 워크플로우를 처음부터 끝까지 책임지는 단일 큐레이션 plugin입니다.
spec 설계 → 리뷰 → 구현 → PR 머지까지의 전체 흐름을 하나의 책임 경계 안에서 제공합니다.

흩어져 있던 6개 plugin을 흡수해, 묵시적 의존과 중복 책임을 명시적 단일 namespace로 정리합니다.

## 흡수 매핑 (6 → 1)

| 기존 plugin | 동결 버전 | atelier 내 위치 |
|---|---|---|
| `git-utils` | 2.4.2 | `commands/git/*`, `skills/git/`, `cli/` (Rust 포팅) |
| `github-autopilot` | 0.30.1 | `commands/autopilot/*`, `agents/autopilot/*`, `skills/{branch-sync,draft-branch,issue-label,resilience}/`, `cli/` |
| `spec-kit` | 0.7.1 | `commands/spec/*`, `agents/spec/*`, `skills/{issue-report,spec-criteria}/`, `templates/spec/` |
| `workflow-guide` | 0.6.0 | `commands/workflow/*`, `agents/workflow/*`, `skills/{convention-architect,agent-design-principles}/`, `rules/` |
| `coding-style` | 0.3.0 | `skills/coding-style/`, `templates/claude-md/` |
| `orchestrator` | 0.2.0 | `skills/orchestrator/` |

흡수된 6개 plugin은 **snapshot freeze** 됩니다 — 삭제하지 않고 동결 상태로 보존하며, 후속 개발은 atelier에서만 진행합니다. 마이그레이션 절차는 [`plans/atelier/03-migration.md`](../../plans/atelier/03-migration.md)를 참조하세요.

## 슬래시 표면 (예정)

> Epic 1 Phase 3에서 채워집니다. 현재는 placeholder입니다.

```
/atelier:setup                  # 통합 setup (git / autopilot / style / all 모듈 선택)
/atelier:git/*                  # sync, branch, resolve, commit-and-pr, merge-pr, epic, ...
/atelier:autopilot/*            # autopilot, gap-watch, build-issues, merge-prs, ci-watch, ...
/atelier:spec/*                 # design, design-detail, spec-review, gap-detect, annotate-spec, scaffold-rules
/atelier:workflow/*             # install, scaffold-conventions
```

> Epic 2 ([#766](https://github.com/kys0213/kys-claude-plugin/issues/766))에서 위 capability 슬래시를 관심사 단위(~5)로 재구성하고, 도메인 지식은 skill의 `references/`로 progressive disclosure 합니다.

## CLI

atelier는 단일 Rust crate(`cli/`)로 빌드되며, 바이너리 `atelier` 하나가 subcommand로 라우팅합니다.

```
atelier git <subcmd>         # git-utils 대체 (TypeScript → Rust 포팅)
atelier autopilot <subcmd>   # 기존 autopilot 바이너리 대체
atelier hook <subcmd>
atelier setup <module>
```

기존 `autopilot` / `git-utils` 호출 호환을 위한 alias는 `/atelier:setup`이 안내합니다.

## 상태

| Phase | 내용 | 상태 |
|---|---|---|
| Phase 0 | 사전 검증 | ✅ |
| Phase 1 | 골격 (plugin.json · README · marketplace WIP entry) | 🚧 |
| Phase 2 | CLI 통합 (Rust 단일 바이너리) | ⬜ |
| Phase 3 | commands / agents / skills / hooks 이동 + namespace 치환 | ⬜ |
| Phase 4 | CI 인프라 (validate · rust-binary · frozen 게이트 · bumpversion 제외) | ⬜ |
| Phase 5 | 흡수 6개 freeze | ⬜ |
