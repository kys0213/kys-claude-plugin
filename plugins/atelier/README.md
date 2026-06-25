# atelier

> Epic 1 (consolidation, [#765](https://github.com/kys0213/kys-claude-plugin/issues/765)) + Epic 2 (skill extraction, [#766](https://github.com/kys0213/kys-claude-plugin/issues/766)) 완료.
> 설계: [`plans/atelier/`](../../plans/atelier/) · 상위 epic: [#738](https://github.com/kys0213/kys-claude-plugin/issues/738)

**atelier**(공방)는 개발 워크플로우를 처음부터 끝까지 책임지는 단일 큐레이션 plugin입니다.
spec 설계 → 리뷰 → 구현 → PR 머지까지의 전체 흐름을 하나의 책임 경계 안에서 제공합니다.

흩어져 있던 6개 plugin을 흡수해, 묵시적 의존과 중복 책임을 명시적 단일 namespace로 정리합니다.

## 흡수 매핑 (6 → 1)

| 기존 plugin | 동결 버전 | atelier 내 위치 |
|---|---|---|
| `git-utils` | 2.4.2 | `skills/git/`(+references), `cli/` (Rust 포팅) |
| `github-autopilot` | 0.30.1 | **제거됨** — 에이전트 스웜이 클로드만으로 동작하게 되어 GitHub 이슈 구동 autopilot 루프를 걷어내고, 자율 개발은 `skills/orchestrator/`(기본 자율 주행)가 담당 |
| `spec-kit` | 0.7.1 | `agents/spec/*`, `skills/spec-write/`·`skills/spec-review/`(+issue-report·spec-criteria→references), `templates/spec/` |
| `workflow-guide` | 0.6.0 | `agents/workflow/*`, `skills/{workflow,agent-design-principles}/`, `rules/` |
| `coding-style` | 0.3.0 | `templates/claude-md/`, `hooks/suggest-simplify.sh` |
| `orchestrator` | 0.2.0 | `skills/orchestrator/` |

흡수된 6개 plugin은 **snapshot freeze** 됩니다 — 삭제하지 않고 동결 상태로 보존하며, 후속 개발은 atelier에서만 진행합니다. 마이그레이션 절차는 [`plans/atelier/03-migration.md`](../../plans/atelier/03-migration.md)를 참조하세요.

## 슬래시 표면 (관심사 단위)

Epic 2 ([#766](https://github.com/kys0213/kys-claude-plugin/issues/766))에서 capability 슬래시(35개)를 **관심사 단위**로 수렴했습니다. skill 이 `user-invocable` 이라 슬래시 호출과 모델 자동 호출을 모두 지원하며, 세부 동작은 skill 의 `references/` 로 progressive disclosure 합니다.

### 관심사 skill (슬래시 + 모델 자동 호출)

```
/atelier:spec        # 스펙 설계/리뷰/갭분석/주석/품질평가 — 자연어 의도로 디스패치
/atelier:git         # git 워크플로우 (커밋·push·PR·충돌 해결·리뷰 정리·이슈 우선순위)
/atelier:workflow    # 컨벤션 scaffold·.claude/rules 설계·설계 원칙 룰 설치·워크플로우 리뷰
/atelier:orchestrator # 위임/병렬 분해·worktree 격리·머지 조정 (기본 자율 주행, HITL opt-out)
/atelier:grill       # 이미 있는 계획·설계를 대화로 심문 (빈틈·가정 드러내기)
/atelier:brainstorm  # 무에서 설계를 대화로 생성 (발산→수렴)
```

### 유지 command (deliberate 진입점)

```
/atelier:setup       # 통합 setup (git / style / workflow 모듈 + hook 관리)
```

자율 개발 루프는 별도 진입점 없이 `/atelier:orchestrator` 가 기본 자율 주행으로 수행합니다.

capability 슬래시(commit-and-pr, prioritize-issues, hook-config, scaffold-conventions 등)는
모두 위 관심사 진입점으로 흡수되었습니다 — 슬래시 없이 자연어로 요청해도 해당 skill 이 자동 트리거됩니다.

### 기계적 호출만 CLI

`atelier git <reviews|guard|hook>` 등 hook·구조화 read 처럼 **기계적 호출이 꼭 필요한** 연산은 슬래시도 skill 도 아닌 Rust CLI 가 담당합니다 (CLAUDE.md 책임 경계). 커밋·브랜치·PR 은 git/gh 가 이미 결정적이라 CLI 로 감싸지 않고, skill 이 컨벤션을 적용해 plain git/gh 로 실행합니다.

## CLI

atelier는 단일 Rust crate(`cli/`)로 빌드되며, 바이너리 `atelier` 하나가 subcommand로 라우팅합니다.

```
atelier git <reviews|guard|hook>   # git-utils 의 기계적 호출 표면 (TypeScript → Rust 포팅)
```

기존 `git-utils` 호출 호환을 위한 alias는 `/atelier:setup`이 안내합니다.

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
> 에이전트 스웜이 클로드만으로 동작하게 되어 GitHub 이슈 구동 autopilot 서브시스템(skill·agents·commands·CLI 모듈)을 제거하고,
> 자율 개발은 `orchestrator` skill 의 **기본 자율 주행**(HITL opt-out)으로 통합했습니다.
> 단일 `atelier` 바이너리는 `atelier git <...>` 를 제공하며,
> 슬래시 표면은 capability 35개 → 관심사 단위로 수렴되었습니다.
>
> ⚠️ `gh` CLI 의존 git 명령(reviews, guard pr)은 mock 단위 테스트만 완료 —
> 실제 `gh`/네트워크 라이브 검증은 정식 릴리스 전 별도 수행이 필요합니다.
