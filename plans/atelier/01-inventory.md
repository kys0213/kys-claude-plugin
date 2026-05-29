# Atelier — 흡수 대상 6개 plugin 인벤토리

> **상태**: 설계 단계 (00-concept.md 의 후속)
> **조사 기준일**: 2026-05-29
> **대상**: git-utils, github-autopilot, spec-kit, workflow-guide, coding-style, orchestrator

이 문서는 흡수 대상 6개 plugin의 자산(commands/agents/skills/hooks)을 전수 조사하고,
plugin 간 묵시적 참조와 namespace 충돌을 매트릭스로 정리한다. 02-architecture.md 의 입력이 된다.

## 1. 버전 현황

marketplace.json 과 plugin.json 의 버전은 **6개 모두 일치** (불일치 없음).

| plugin | version | commands | agents | skills | hooks |
|---|---|---:|---:|---:|---:|
| git-utils | 2.4.2 | 14 | 0 | 1 | 0 (CLI 보유) |
| github-autopilot | 0.30.1 | 12 | 12 | 4 | 3 |
| spec-kit | 0.7.1 | 6 | 4 | 2 | 0 |
| workflow-guide | 0.6.0 | 2 | 4 | 2 | 0 |
| coding-style | 0.3.0 | 1 | 0 | 0 | 1 |
| orchestrator | 0.2.0 | 0 | 0 | 1 | 0 |
| **합계** | — | **35** | **20** | **10** | **4** |

## 2. 자산 인벤토리 (전수)

### 2.1 git-utils (v2.4.2)

- **commands (14)**: `git-sync`, `git-branch`, `git-resolve`, `commit-and-pr`, `commit-and-push`, `merge-pr`, `create-issue`, `prioritize-issues`, `epic`, `unresolved-reviews`, `check-ci`, `branch-status`, `setup`, `hook-config`
- **skills (1)**: `git` (단일 `skills/SKILL.md`, 서브디렉토리 없음)
- **agents**: 없음
- **hooks**: 없음 (단, `src/` 에 TypeScript CLI 보유 — `tests/` 포함. 결정적 도구 계층)
- **특이점**: 6개 중 유일하게 자체 빌드 CLI(`src/commands`, `src/core`)를 가짐. atelier 흡수 시 빌드/배포 경로 재조정 필요.

### 2.2 github-autopilot (v0.30.1)

- **commands (12)**: `gap-watch`, `build-issues`, `merge-prs`, `ci-watch`, `qa-boost`, `autopilot`, `setup`, `analyze-issue`, `ci-fix`, `test-watch`, `work-ledger`, `stale-task-review`
- **agents (12)**: `gap-detector`, `gap-ledger-writer`, `issue-implementer`, `branch-promoter`, `pr-merger`, `ci-failure-analyzer`, `issue-dependency-analyzer`, `issue-analyzer`, `ci-fixer`, `test-analyzer`, `spec-validator`, `stale-task-reviewer`
- **skills (4)**: `branch-sync`, `draft-branch`, `issue-label`, `resilience`
- **hooks (3)**: `check-cli-version.sh` (SessionStart), `protect-stagnation.sh`, `guard-pr-base.sh`
- **특이점**: 자체 CLI(`cli/src`, `cli/tests`) 보유. autopilot CLI 버전을 plugin.json 과 대조하는 SessionStart hook 존재.

### 2.3 spec-kit (v0.7.1)

- **commands (6)**: `gap-detect`, `spec-review`, `design`, `design-detail`, `annotate-spec`, `scaffold-spec-rules`
- **agents (4)**: `file-pair-observer`, `gap-aggregator`, `gap-auditor`, `spec-annotator`
- **skills (2)**: `issue-report`, `spec-criteria`
- **hooks**: 없음
- **특이점**: `templates/` 디렉토리 보유.

### 2.4 workflow-guide (v0.6.0)

- **commands (2)**: `install`, `scaffold-rules`
- **agents (4)**: `codebase-analyzer`, `document-analyzer`, `rules-generator`, `workflow-reviewer`
- **skills (2)**: `agent-design-principles`, `convention-architect`
- **hooks**: 없음
- **특이점**: `rules/` 디렉토리 보유. plugin.json 에 commands/skills/agents 키가 **명시되어 있지 않음** (자동 발견에 의존하는 것으로 보임 — 02에서 등록 방식 확정 필요).

### 2.5 coding-style (v0.3.0)

- **commands (1)**: `setup`
- **agents**: 없음
- **skills**: 없음
- **hooks (1)**: `suggest-simplify.sh` (Stop hook — 변경 감지 시 `/simplify` 제안)
- **특이점**: `templates/CLAUDE.md` 보유. 가장 단순한 구조. cross-plugin 참조 없음 (고립).

### 2.6 orchestrator (v0.2.0)

- **commands**: 없음
- **agents**: 없음
- **skills (1)**: `orchestrator` (`references/` 서브문서 보유 — `merge-coordinator.md` 등)
- **hooks**: 없음
- **특이점**: 순수 skill 1개 plugin. git-utils 에 가장 강하게 의존 (참조 매트릭스 참고).

## 3. Cross-plugin 참조 매트릭스

6개 plugin 사이의 묵시적 의존을 전수 조사한 결과 **총 13건**.

| Source → Target | 유형 | 위치 | 인용 |
|---|---|---|---|
| git-utils → github-autopilot | 문서 경로 언급 | `git-utils/commands/prioritize-issues.md:279` | `관련 파일: plugins/github-autopilot/internal/epic/` |
| github-autopilot → spec-kit | agent 패턴 참조 | `github-autopilot/agents/gap-detector.md:10` | `spec-kit의 file-pair-observer + gap-aggregator 흐름과 동일한 패턴` |
| github-autopilot → spec-kit | 슬래시 호출 | `github-autopilot/commands/autopilot.md:87` | `/spec-kit:design 또는 /spec-kit:spec-review 안내` |
| spec-kit → orchestrator | 파싱 계약 | `spec-kit/agents/gap-auditor.md:181` | `스키마 일관성을 깨면 orchestrator 가 파싱 실패` |
| spec-kit → orchestrator | 운영 제약 | `spec-kit/agents/gap-auditor.md:247` | `orchestrator 가 매 iteration 마다 fresh 호출` |
| spec-kit → orchestrator | 통합 요구 | `spec-kit/agents/gap-auditor.md:254` | `스키마 일관성이 orchestrator 의 파싱에 필수` |
| workflow-guide → git-utils | 예시 경로 | `workflow-guide/skills/convention-architect/SKILL.md:251,265` | `plugins/git-utils/src/core/git.ts` 등 예시 |
| workflow-guide → coding-style | 파일 참조 | `workflow-guide/agents/codebase-analyzer.md:213` | `coding-style.md ... 항상 로드됨` |
| orchestrator → git-utils | 슬래시 호출 | `orchestrator/skills/orchestrator/SKILL.md:70` | `git-utils:/epic init <name>` 또는 `git-utils:/git-branch epic/<name>` |
| orchestrator → git-utils | skill 위임 | `orchestrator/skills/orchestrator/references/merge-coordinator.md:10,79,89` | `git-utils:git-resolve 스킬에 위임` (3건) |
| orchestrator → git-utils | CLI 참조 | `orchestrator/skills/orchestrator/references/merge-coordinator.md:141` | `git-utils 또는 git worktree remove 호출` |

### 3.1 의존 방향 요약

```
orchestrator  ──강함──▶  git-utils  ──약함(문서)──▶  github-autopilot
                                                        │
workflow-guide ──약함(예시)──▶ git-utils               │ 강함(슬래시+패턴)
workflow-guide ──약함(문서)──▶ coding-style            ▼
                                                     spec-kit ──강함(파싱계약)──▶ orchestrator
```

- **순환 의존 존재**: `orchestrator → git-utils` 와 `spec-kit → orchestrator` + `github-autopilot → spec-kit` 가 사실상 큰 루프를 형성.
- **고립**: coding-style 은 outgoing 참조 0건 (incoming 1건만).
- **결론**: 6개가 단일 plugin(atelier)으로 합쳐지면 이 13건의 cross-plugin 참조는 **모두 plugin 내부 참조**가 되어 namespace 접두사(`spec-kit:`, `git-utils:`)를 제거/치환해야 한다. → 02-architecture 의 namespace 정책 핵심 입력.

## 4. Namespace 충돌 매트릭스

### 4.1 슬래시 커맨드 충돌

커맨드는 `/<plugin-name>:<command>` 로 namespace 된다 (예: `/github-autopilot:setup`, `git-utils:/epic`).
atelier 로 합치면 모든 커맨드가 `/atelier:<command>` 가 되므로 **커맨드 이름 자체가 충돌 키**다.

전수 비교 결과 충돌은 **1건 (3-way)**:

| 커맨드 | 보유 plugin | 충돌 |
|---|---|---|
| `setup` | git-utils, github-autopilot, coding-style | ⚠️ 3-way |

- 나머지 33개 커맨드는 이름 충돌 없음.
- 단, **혼동 유발 유사 이름** (충돌은 아니지만 통합 시 정리 권장):
  - `merge-pr` (git-utils) vs `merge-prs` (github-autopilot)
  - `check-ci` (git-utils) vs `ci-watch` / `ci-fix` (github-autopilot)
  - `gap-detect` (spec-kit) vs `gap-watch` (github-autopilot)
  - `scaffold-rules` (workflow-guide) vs `scaffold-spec-rules` (spec-kit)

### 4.2 setup 충돌 해소 방향 (02에서 확정)

3개 setup 은 책임이 다름:
- git-utils setup → GitHub 환경 + Default Branch Guard hook
- github-autopilot setup → `github-autopilot.local.md` 생성 + hook/CLI 설치 (user scope)
- coding-style setup → `~/.claude/CLAUDE.md` 코딩 원칙 + Stop hook 설치

후보: (a) 단일 `/atelier:setup` 으로 통합 + 하위 선택지(AskUserQuestion), (b) `setup-git` / `setup-autopilot` / `setup-style` 로 분리. → 02 에서 결정.

### 4.3 skill / agent 이름 충돌

- **skill 이름** (10개): `git`, `orchestrator`, `agent-design-principles`, `convention-architect`, `draft-branch`, `issue-label`, `resilience`, `branch-sync`, `issue-report`, `spec-criteria` → **충돌 없음**.
- **agent 이름** (20개): github-autopilot 12 + spec-kit 4 + workflow-guide 4 → **이름 충돌 없음**. (단 `spec-validator`(autopilot) 와 spec-kit agents 는 책임이 겹치므로 02 에서 중복 통합 검토.)

## 5. Hook 등록 메커니즘 (중요)

**6개 plugin 모두 hook 을 plugin.json 의 `hooks` 키나 `hooks.json` 으로 선언하지 않는다.**
hook 스크립트(`*.sh`)는 디렉토리에 존재하지만, **setup 커맨드가 사용자의 `~/.claude/settings.json` 에 직접 등록**한다.

근거:
- github-autopilot/commands/setup.md: *"모든 hook은 user scope(`~/.claude/settings.json`)에 설치됩니다"*
- coding-style/commands/setup.md: *"Stop hook을 등록합니다"*
- 설치된 hook 명령은 `${CLAUDE_PLUGIN_ROOT}/hooks/*.sh` 경로를 가리킴.

### 5.1 마이그레이션 리스크 🔴

`${CLAUDE_PLUGIN_ROOT}` 는 plugin 디렉토리에 따라 달라진다. 6개 plugin 이 frozen 되고 atelier 로 옮겨지면:

- 기존 사용자의 `~/.claude/settings.json` 에 이미 설치된 hook 은 **여전히 frozen plugin 경로**(`plugins/github-autopilot/hooks/...`)를 가리킨다.
- frozen 디렉토리는 유지되므로 hook 은 *깨지지 않지만*, atelier 의 갱신된 hook 이 아니라 동결된 버전이 계속 실행된다.
- → 마이그레이션 가이드(03)는 **atelier setup 재실행으로 hook 경로를 atelier 로 갱신**하는 절차를 반드시 포함해야 한다.

### 5.2 hook 목록

| hook | plugin | 트리거 | 역할 |
|---|---|---|---|
| `check-cli-version.sh` | github-autopilot | SessionStart | autopilot CLI 버전 ↔ plugin.json 대조 안내 |
| `protect-stagnation.sh` | github-autopilot | (확인 필요) | 정체 ledger 보호 |
| `guard-pr-base.sh` | github-autopilot | (확인 필요) | PR base 브랜치 가드 |
| `suggest-simplify.sh` | coding-style | Stop | 변경 감지 시 `/simplify` 제안 |

## 6. marketplace.json 현황 (deprecated 정책 입력)

- 최상위 키: `$schema`, `description`, `name`, `owner`, `plugins`
- plugin entry 키: `category`, `description`, `keywords`, `name`, `source`, `version`
- **`deprecated` 필드 현재 미사용** (6개 어디에도 없음).
- `$schema`: `https://anthropic.com/claude-code/marketplace.schema.json`
- ⚠️ **schema fetch 403 차단** — `deprecated` 필드 지원 여부 **미확인**. 02/04 진행 전 다음 중 하나로 검증 필요:
  - schema 원본을 인증 경로/로컬 캐시로 확보
  - 또는 strict 검증 없이 `deprecated`/`hidden` 추가 후 `claude plugin validate` 류로 확인
  - 미지원이면 대안: README 배지 + atelier README 매핑표로만 deprecation 표현 (필드 없이)

## 7. 02-architecture 로 넘길 결정 항목

1. **namespace 정책**: 13건 cross-plugin 참조의 접두사 치환 규칙 (`spec-kit:design` → `atelier:design` 등) 일괄 정의.
2. **setup 통합**: 3-way 충돌 해소 — 단일 `/atelier:setup` + 하위 선택 vs 분리 명명.
3. **hook 재배치**: 4개 hook 의 atelier 내 경로 + setup 재설치 절차 (5.1 리스크 대응).
4. **CLI 통합**: git-utils `src/` + github-autopilot `cli/` 두 빌드 산출물의 atelier 내 배치/빌드 파이프라인.
5. **중복 agent 통합**: `spec-validator`(autopilot) ↔ spec-kit gap agents 책임 정리.
6. **deprecated 표현 방식**: schema 검증 결과에 따라 확정 (6절).
