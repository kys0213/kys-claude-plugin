# atelier

> Epic 1 (consolidation, [#765](https://github.com/kys0213/kys-claude-plugin/issues/765)) + Epic 2 (skill extraction, [#766](https://github.com/kys0213/kys-claude-plugin/issues/766)) 완료.
> 설계: [`plans/atelier/`](../../plans/atelier/) · 상위 epic: [#738](https://github.com/kys0213/kys-claude-plugin/issues/738)

**atelier**(공방)는 개발 워크플로우를 처음부터 끝까지 책임지는 단일 큐레이션 plugin입니다.
spec 설계 → 리뷰 → 구현 → PR 머지까지의 전체 흐름을 하나의 책임 경계 안에서 제공합니다.

흩어져 있던 6개 plugin을 흡수해, 묵시적 의존과 중복 책임을 명시적 단일 namespace로 정리합니다.

## 흡수 매핑 (6 → 1)

| 기존 plugin | 동결 버전 | atelier 내 위치 |
|---|---|---|
| `git-utils` | 2.4.2 | `commands/git/*`, `skills/git/`, `cli/` (Rust 포팅) |
| `github-autopilot` | 0.30.1 | `commands/autopilot/*`, `agents/autopilot/*`, `skills/{branch-sync,draft-branch,issue-label}/`, `skills/autopilot/`(+resilience→references), `cli/` |
| `spec-kit` | 0.7.1 | `commands/spec/*`, `agents/spec/*`, `skills/{issue-report,spec-criteria}/`, `templates/spec/` |
| `workflow-guide` | 0.6.0 | `commands/workflow/*`, `agents/workflow/*`, `skills/{convention-architect,agent-design-principles}/`, `rules/` |
| `coding-style` | 0.3.0 | `skills/coding-style/`, `templates/claude-md/` |
| `orchestrator` | 0.2.0 | `skills/orchestrator/` |

흡수된 6개 plugin은 **snapshot freeze** 됩니다 — 삭제하지 않고 동결 상태로 보존하며, 후속 개발은 atelier에서만 진행합니다. 마이그레이션 절차는 [`plans/atelier/03-migration.md`](../../plans/atelier/03-migration.md)를 참조하세요.

## 슬래시 표면 (관심사 단위)

Epic 2 ([#766](https://github.com/kys0213/kys-claude-plugin/issues/766))에서 capability 슬래시(35개)를 **관심사 단위**로 수렴했습니다. skill 이 `user-invocable` 이라 슬래시 호출과 모델 자동 호출을 모두 지원하며, 세부 동작은 skill 의 `references/` 로 progressive disclosure 합니다.

### 관심사 skill (슬래시 + 모델 자동 호출)

```
/atelier:spec        # 스펙 설계/리뷰/갭분석/주석 — 자연어 의도로 디스패치
/atelier:autopilot   # 자율 개발 루프 진입점 (CLI daemon + 내부 skill references 디스패치)
/atelier:git         # git 판단 작업 (충돌 해결·우선순위·동기화 전략)
```

### 유지 command (deliberate 진입점)

```
/atelier:setup               # 통합 setup (git / autopilot / style 모듈)
/atelier:commit-and-pr       # 커밋 + PR 생성
/atelier:commit-and-push     # 커밋 + push
/atelier:unresolved-reviews  # 미해결 리뷰 조회
/atelier:prioritize-issues   # 이슈 우선순위 추천
/atelier:hook-config         # hook 설정 관리
/atelier:install /atelier:scaffold-conventions  # workflow 컨벤션
```

### 결정적 동작은 CLI

`atelier git <branch|commit|pr|...>`, `atelier autopilot <task|epic|check|...>` 등 동일 입력→동일 출력의 결정적 연산은 슬래시도 skill 도 아닌 Rust CLI 가 담당합니다 (CLAUDE.md 책임 경계).

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
| Phase 1 | 골격 (plugin.json · README · marketplace WIP entry) | ✅ |
| Phase 2 | CLI 통합 (Rust 단일 바이너리 — autopilot 흡수 + git-utils 포팅) | ✅ |
| Phase 3 | commands / agents / skills / hooks 이동 + namespace 치환 | ✅ |
| Phase 4 | CI 인프라 (validate · rust-binary · frozen 게이트 · bumpversion 제외) | ✅ |
| Phase 5 | 흡수 6개 freeze | ✅ |

> **현재 상태**: Epic 1 (consolidation) + Epic 2 (skill extraction) 완료.
> 단일 `atelier` 바이너리가 `atelier autopilot <...>` / `atelier git <...>` 를 제공하고(582 tests green),
> Fat Controller 14개가 관심사 skill(`spec`/`autopilot`/`git`) + `references/` 로 해체되었습니다.
> 슬래시 표면은 capability 35개 → 관심사 단위로 수렴, 흡수 6개 plugin 은 snapshot freeze 보존.
>
> ⚠️ `gh` CLI 의존 git 명령(pr create, reviews, pr-guard)은 mock 단위 테스트만 완료 —
> 실제 `gh`/네트워크 라이브 검증은 정식 릴리스 전 별도 수행이 필요합니다.
