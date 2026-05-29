# Atelier — 아키텍처

> **상태**: 설계 단계 (00, 01의 후속)
> **입력**: 01-inventory.md (자산/참조/충돌 매트릭스)
> **출력**: 03-migration.md, 04-rollout.md 의 구현 지침

이 문서는 atelier의 최종 디렉토리 구조, namespace 정책, hook 재배치, CLI 통합 형태,
중복 책임 통합 방안, marketplace.json 변경안을 단일 그림으로 확정한다.

---

## 0. 확정 결정 요약

01-inventory.md §7 의 6개 결정 항목을 다음과 같이 확정한다.

| # | 결정 항목 | 확정 |
|---|---|---|
| 1 | namespace 정책 | 모든 cross-plugin 접두사 제거. atelier 내부 참조는 접두사 없는 단일 namespace. |
| 2 | setup 통합 | 단일 `/atelier:setup` + AskUserQuestion 으로 하위 모듈 선택 |
| 3 | hook 재배치 | `atelier/hooks/` 단일 위치 + setup 이 `${CLAUDE_PLUGIN_ROOT}/hooks/*.sh` 로 user scope 등록 |
| 4 | CLI 통합 | Rust 단일 Cargo crate, 단일 바이너리 `atelier`, subcommand 라우팅. git-utils TypeScript는 Rust 로 포팅. |
| 5 | 중복 agent | autopilot `spec-validator` 와 spec-kit `gap-auditor` 책임 분리: validator 제거, auditor 만 유지 (§5.4). |
| 6 | deprecated 표현 | `marketplace.json` 에 `deprecated: true` + `replacedBy: "atelier"` 추가. schema 미지원 판명 시 자동 폴백 (§7.2). |

| # | 00-concept 미해결 | 확정 |
|---|---|---|
| A | 이름 `atelier` | 확정. |
| B | 첫 릴리즈 버전 | `0.1.0` 신규 시작. 흡수 plugin 버전 계승하지 않음. |
| C | deprecated 필드 지원 | 6번과 동일 — 추가 + 폴백. |

---

## 1. 디렉토리 구조

```
plugins/atelier/
├── .claude-plugin/
│   └── plugin.json
├── README.md                          # 마이그레이션 가이드 + 6→1 매핑표 포함
├── commands/                          # 슬래시 커맨드 (총 34개 — setup 통합으로 35→34)
│   ├── setup.md                       # 통합 setup (하위 선택)
│   ├── git/                           # git-utils 출신
│   │   ├── sync.md
│   │   ├── branch.md
│   │   ├── resolve.md
│   │   ├── commit-and-pr.md
│   │   ├── commit-and-push.md
│   │   ├── merge-pr.md
│   │   ├── create-issue.md
│   │   ├── prioritize-issues.md
│   │   ├── epic.md
│   │   ├── unresolved-reviews.md
│   │   ├── check-ci.md
│   │   ├── branch-status.md
│   │   └── hook-config.md
│   ├── autopilot/                     # github-autopilot 출신
│   │   ├── autopilot.md
│   │   ├── gap-watch.md
│   │   ├── build-issues.md
│   │   ├── merge-prs.md
│   │   ├── ci-watch.md
│   │   ├── ci-fix.md
│   │   ├── qa-boost.md
│   │   ├── analyze-issue.md
│   │   ├── test-watch.md
│   │   ├── work-ledger.md
│   │   └── stale-task-review.md
│   ├── spec/                          # spec-kit 출신
│   │   ├── design.md
│   │   ├── design-detail.md
│   │   ├── spec-review.md
│   │   ├── gap-detect.md
│   │   ├── annotate-spec.md
│   │   └── scaffold-rules.md          # rename: scaffold-spec-rules → scaffold-rules
│   └── workflow/                      # workflow-guide 출신
│       ├── install.md
│       └── scaffold-conventions.md    # rename: scaffold-rules → scaffold-conventions (충돌 회피)
├── agents/                            # 19개 — autopilot 11 + spec-kit 4 + workflow-guide 4
│   ├── autopilot/
│   │   ├── gap-detector.md            # spec-validator 제거 후 11개 (§5.4)
│   │   ├── gap-ledger-writer.md
│   │   ├── issue-implementer.md
│   │   ├── branch-promoter.md
│   │   ├── pr-merger.md
│   │   ├── ci-failure-analyzer.md
│   │   ├── issue-dependency-analyzer.md
│   │   ├── issue-analyzer.md
│   │   ├── ci-fixer.md
│   │   ├── test-analyzer.md
│   │   └── stale-task-reviewer.md
│   ├── spec/
│   │   ├── file-pair-observer.md
│   │   ├── gap-aggregator.md
│   │   ├── gap-auditor.md             # autopilot:spec-validator 책임 흡수
│   │   └── spec-annotator.md
│   └── workflow/
│       ├── codebase-analyzer.md
│       ├── document-analyzer.md
│       ├── rules-generator.md
│       └── workflow-reviewer.md
├── skills/                            # 10개
│   ├── git/SKILL.md                   # git-utils 출신 (name: git)
│   ├── orchestrator/                  # orchestrator 출신 (references/ 포함)
│   ├── coding-style/                  # 신규 — coding-style 의 templates/CLAUDE.md 를 skill 화
│   ├── convention-architect/          # workflow-guide 출신
│   ├── agent-design-principles/       # workflow-guide 출신
│   ├── branch-sync/                   # autopilot 출신
│   ├── draft-branch/                  # autopilot 출신
│   ├── issue-label/                   # autopilot 출신
│   ├── resilience/                    # autopilot 출신
│   ├── issue-report/                  # spec-kit 출신
│   └── spec-criteria/                 # spec-kit 출신
├── hooks/                             # 4개 — 단일 위치
│   ├── check-cli-version.sh           # SessionStart  (autopilot 출신)
│   ├── protect-stagnation.sh          # autopilot 출신
│   ├── guard-pr-base.sh               # autopilot 출신
│   └── suggest-simplify.sh            # Stop  (coding-style 출신)
├── templates/                         # spec-kit/templates + coding-style/templates 통합
│   ├── claude-md/                     # coding-style 의 CLAUDE.md 템플릿
│   └── spec/                          # spec-kit 템플릿
├── rules/                             # workflow-guide/rules 흡수
└── cli/                               # 단일 Rust crate (§4)
    ├── Cargo.toml                     # name = "atelier", bin = "atelier"
    ├── src/
    │   ├── main.rs
    │   ├── lib.rs
    │   ├── cli.rs                     # 최상위 clap 라우터: atelier git|autopilot|spec|hook ...
    │   ├── git/                       # ← plugins/git-utils/src/ 전체 Rust 포팅
    │   │   ├── mod.rs
    │   │   ├── commands/              # branch/commit/pr/guard/hook/reviews
    │   │   ├── core/                  # git/github/jira/guard/pr-guard/shell
    │   │   └── installer.rs
    │   ├── autopilot/                 # ← github-autopilot/cli/src/ 이동
    │   │   ├── mod.rs
    │   │   ├── cmd/
    │   │   ├── domain/
    │   │   ├── ports/
    │   │   ├── store/
    │   │   └── ...
    │   └── shared/                    # 두 영역 공통 (fs, gh, github, config)
    └── tests/                         # 두 CLI 의 테스트 통합
```

### 1.1 commands 의 폴더 그룹화

01-inventory 에서 \"33개 충돌 없음, setup 만 1건\"으로 봤지만,
**\"혼동 유발 유사 이름\"** (예: `merge-pr` vs `merge-prs`, `gap-detect` vs `gap-watch`)을
폴더로 분리해 의미적으로 격리한다. Claude Code 슬래시 발견은 폴더 prefix 를 함께 보여주므로
사용자가 `/atelier:git:merge-pr` vs `/atelier:autopilot:merge-prs` 식으로 식별 가능.

> **명명 규칙**: 폴더는 \"기능 도메인\" 단위. `git`, `autopilot`, `spec`, `workflow` 4개. coding-style/orchestrator 는 명령 0개라 폴더 없음. agent 도 동일 규칙.

### 1.2 rename 목록

| 출신 plugin | 원본 이름 | atelier 이름 | 사유 |
|---|---|---|---|
| git-utils | `git-sync` | `git/sync` | 폴더 prefix 가 `git` 이라 중복 제거 |
| git-utils | `git-branch` | `git/branch` | 동일 |
| git-utils | `git-resolve` | `git/resolve` | 동일 |
| spec-kit | `scaffold-spec-rules` | `spec/scaffold-rules` | 폴더 prefix 로 `spec` 명시 |
| workflow-guide | `scaffold-rules` | `workflow/scaffold-conventions` | spec/scaffold-rules 와 의미 분리 |

---

## 2. Namespace 정책

### 2.1 원칙

atelier 는 **단일 namespace**. plugin 접두사(`spec-kit:`, `git-utils:`, `github-autopilot:`) 는 **전부 제거**한다.
slash 호출 경로는 폴더 그룹 prefix 로만 표현한다 (`atelier:git/branch` 등).

### 2.2 13건 cross-plugin 참조 치환 규칙

01 §3 매트릭스의 13건을 다음 일괄 규칙으로 치환한다.

| 원본 패턴 | 치환 |
|---|---|
| `git-utils:/<cmd>` | `atelier:git/<cmd>` |
| `git-utils:<skill-or-cmd>` | `atelier:<skill-or-cmd>` (skill 이름은 변경 없음) |
| `/spec-kit:<cmd>` | `/atelier:spec/<cmd>` |
| `/github-autopilot:<cmd>` | `/atelier:autopilot/<cmd>` |
| 문서 경로 `plugins/<name>/...` | `plugins/atelier/...` 로 갱신 (예시 코드 포함) |
| `orchestrator` 단어 그대로 (skill 이름) | 변경 없음 |

치환은 `04-rollout.md` 의 검증 체크리스트에서 정규식 grep 으로 0건 확인한다.

### 2.3 skill 이름은 변경 금지

10개 skill 이름은 atelier 흡수 후에도 **그대로 유지** (`git`, `orchestrator`, `convention-architect`, ...). 이미 충돌 없음 (01 §4.3). Claude 가 skill 을 자연어 이름으로 호출하기 때문에 변경 시 외부 호환성 깨짐.

---

## 3. setup 통합 구조

### 3.1 단일 진입점

```
/atelier:setup
```

내부에서 AskUserQuestion 으로 설치 모듈을 선택. 다중 선택 가능 (multiSelect).

### 3.2 모듈 분기

| 선택 | 수행 동작 | 출처 |
|---|---|---|
| `git` | git-utils CLI 설치 + Default Branch Guard hook 등록 | git-utils setup |
| `autopilot` | `github-autopilot.local.md` 생성 + autopilot hook 3개 등록 + Rust CLI 빌드/설치 | autopilot setup |
| `style` | `~/.claude/CLAUDE.md` 코딩 원칙 + Stop hook 등록 | coding-style setup |
| `all` | 위 세 가지 전부 | 신규 |

### 3.3 hook 등록 경로 규칙

모든 hook 등록은 user scope `~/.claude/settings.json` 에 다음 형태로 기록:

```jsonc
{
  "hooks": {
    "SessionStart": [
      { "command": "${CLAUDE_PLUGIN_ROOT}/hooks/check-cli-version.sh" }
    ],
    "Stop": [
      { "command": "${CLAUDE_PLUGIN_ROOT}/hooks/suggest-simplify.sh" }
    ]
    // ...
  }
}
```

`${CLAUDE_PLUGIN_ROOT}` 가 atelier 디렉토리로 해석되도록 setup 은 atelier plugin 컨텍스트에서 실행되어야 한다.

### 3.4 마이그레이션 시 hook 갱신

기존 사용자는 frozen plugin 의 `${CLAUDE_PLUGIN_ROOT}/hooks/...` 경로가 settings.json 에 박혀 있다. atelier 의 `/atelier:setup` 은 **기존 entry 를 감지하여 atelier 경로로 재작성**한다.

감지 패턴 (정규식):
```
"command": ".*/plugins/(github-autopilot|coding-style)/hooks/.*\\.sh"
```

상세 절차는 03-migration.md §3.

---

## 4. CLI 통합 (Rust 단일 crate)

### 4.1 최종 형태

```
plugins/atelier/cli/
├── Cargo.toml          # [package] name = "atelier"
└── src/
    ├── main.rs         # 단일 바이너리 진입점
    ├── lib.rs
    ├── cli.rs          # clap 최상위 Subcommand 라우터
    ├── git/            # git-utils 포팅 결과
    ├── autopilot/      # github-autopilot CLI 이동 결과
    └── shared/         # 공통 (gh, github, fs, config)
```

### 4.2 단일 바이너리, subcommand 라우팅

```
atelier git <subcmd> [args]              # ← bun-built dist/git-utils 대체
atelier autopilot <subcmd> [args]        # ← 기존 'autopilot' 바이너리 대체
atelier hook <subcmd>                    # ← 기존 git-utils 의 hook 명령
atelier setup <module>                   # ← 슬래시 /atelier:setup 의 내부 호출 대상
```

clap derive 사용. autopilot 의 기존 Cargo 패턴(`clap = { version = "4", features = ["derive"] }`) 그대로 계승.

### 4.3 git-utils TypeScript → Rust 포팅 범위

| TS 소스 | Rust 이전 위치 | 비고 |
|---|---|---|
| `src/cli.ts` | `src/git/mod.rs` + `src/cli.rs` 통합 | clap subcommand 로 흡수 |
| `src/installer.ts` | `src/git/installer.rs` | 디스크 I/O — std::fs |
| `src/types.ts` | `src/git/types.rs` | serde derive |
| `src/core/{shell,git,github,jira,guard,pr-guard}.ts` | `src/git/core/*.rs` | shell 은 std::process::Command |
| `src/commands/{branch,commit,pr,guard,hook,reviews}.ts` | `src/git/commands/*.rs` | clap Subcommand impl |
| `tests/**/*.test.ts` (11개) | `tests/git/*.rs` | bun test → `#[test]` + `assert_cmd` |

스크립트 보조(`plugins/git-utils/scripts/*.sh`) 중 CLI 가 흡수한 기능은 제거. shell-only 인 것(예: jira ticket detect) 은 atelier/scripts/ 로 이동하거나 Rust 흡수.

### 4.4 바이너리 호환성

기존 사용자는 `autopilot` 또는 `git-utils` 바이너리에 의존. atelier 설치 후:

- `atelier` 바이너리 1개만 제공.
- 마이그레이션 가이드(03)에 다음 alias 권장:
  ```
  alias autopilot='atelier autopilot'
  alias git-utils='atelier git'
  ```
- `/atelier:setup` 의 git/autopilot 모듈은 alias 자동 생성 옵션 제공 (선택).

### 4.5 빌드/테스트 인프라

- 루트 `package.json` 의 `tsc --build` typecheck 및 eslint 대상에서 `plugins/git-utils/**` 제외.
- 루트 Makefile 에 `make build-cli` 추가: `cargo build --release --manifest-path plugins/atelier/cli/Cargo.toml`.
- CI: 기존 autopilot Rust 테스트 워크플로우를 atelier crate 로 경로 이동.

---

## 5. 중복 책임 통합

### 5.1 spec ↔ autopilot 경계

`gap-detector` (autopilot) 가 `spec-kit/file-pair-observer + gap-aggregator` 패턴을 \"단일 에이전트에서 통합 수행\" 한다고 명시되어 있음 (01 §3). atelier 안에서는 다음으로 정리:

- **gap 탐지의 운영 루프**: `agents/autopilot/gap-detector.md` (CronCreate 사이클 안에서 작동)
- **spec ↔ code 정밀 대조**: `agents/spec/file-pair-observer.md` + `gap-aggregator.md` (단발 분석 도구)
- **gap audit (스키마/품질 검증)**: `agents/spec/gap-auditor.md`
- **autopilot 의 spec 검증 책임**: `agents/autopilot/spec-validator.md` 는 **제거**, 동일 책임을 `gap-auditor` 로 위임.

근거: 01 §3 spec-kit → orchestrator 의 3건 참조가 \"orchestrator 가 파싱하는 audit 결과 스키마\" 를 다룬다. spec-validator(autopilot) 가 별도로 존재할 이유 없음.

### 5.2 orchestrator ↔ git 경계

orchestrator 가 git-utils 의 `:/epic`, `:/git-branch`, `git-resolve` 를 직접 호출. atelier 흡수 후에는 단순히 접두사만 제거 (§2.2). orchestrator skill 자체의 책임은 그대로.

### 5.3 workflow-guide ↔ coding-style 경계

workflow-guide 의 `codebase-analyzer` 가 \"coding-style.md\" 를 참조. atelier 흡수 후 coding-style 의 CLAUDE.md 템플릿은 `skills/coding-style/` 로 들어가고, codebase-analyzer 는 atelier 내부 skill 로 참조 가능.

### 5.4 중복 제거 최종 표

| 제거 | 흡수 | 사유 |
|---|---|---|
| `agents/autopilot/spec-validator.md` | `agents/spec/gap-auditor.md` | 책임 동일 |
| `commands/git/setup.md` | `commands/setup.md` (통합) | 3-way 충돌 해소 |
| `commands/autopilot/setup.md` | `commands/setup.md` | 동일 |
| `commands/style/setup.md` | `commands/setup.md` | 동일 |

agent 총수: 20 → 19. command 총수: 35 → 33 (-2 setup, +1 통합 = -2 순감, rename 으로 외관상 더 깔끔).

---

## 6. plugin.json 정의

```jsonc
{
  "$schema": "https://anthropic.com/claude-code/plugin.schema.json",
  "name": "atelier",
  "version": "0.1.0",
  "description": "통합 개발 워크플로우 — spec/git/autopilot/orchestrator/style/workflow 를 단일 책임 경계로 큐레이션",
  "author": { "name": "kys0213" },
  "commands": [
    "./commands/setup.md",
    "./commands/git/sync.md",
    "./commands/git/branch.md",
    // ... 전체 33개
  ],
  "agents": [
    "./agents/autopilot/gap-detector.md",
    // ... 전체 19개
  ],
  "skills": ["./skills"]
}
```

폴더 안의 `SKILL.md` 자동 발견 규칙은 기존 plugin 들과 동일하므로 `skills` 는 단일 디렉토리 참조면 충분.

> ⚠️ workflow-guide 의 plugin.json 은 commands/agents/skills 키를 명시하지 않았음 (01 §2.4) — atelier 에서는 모두 명시. 자동 발견 의존 제거.

---

## 7. marketplace.json 변경안

### 7.1 신규 atelier entry

```jsonc
{
  "category": "productivity",
  "description": "통합 개발 워크플로우 — spec, git, autopilot, orchestrator, coding-style, workflow-guide 를 단일 책임 경계로 큐레이션",
  "keywords": [
    "atelier", "workflow", "spec", "git", "autopilot",
    "orchestrator", "coding-style", "convention", "ddd"
  ],
  "name": "atelier",
  "source": "./plugins/atelier",
  "version": "0.1.0"
}
```

### 7.2 흡수 6개 frozen 표시

각 entry 에 `deprecated` + `replacedBy` 추가:

```jsonc
{
  "category": "automation",
  "description": "❄️ Snapshot — atelier 로 이전됨. 후속 개발 없음.",
  "name": "github-autopilot",
  "source": "./plugins/github-autopilot",
  "version": "0.30.1",
  "deprecated": true,
  "replacedBy": "atelier"
}
```

### 7.3 schema 미지원 폴백

01 §6 에서 schema fetch 가 403 으로 차단됨. 다음 절차로 검증/폴백:

1. 04-rollout 의 Phase 0 에서 `claude plugin validate marketplace` 류 명령으로 `deprecated`/`replacedBy` 검증.
2. **검증 실패 시**: 두 필드를 제거하고 description 의 ❄️ 배지 + atelier README 의 매핑표만으로 deprecation 을 표현. CI 게이트는 변경 없음.

### 7.4 4개 \"추후 검토\" plugin

`suggest-workflow`, `autodev`, `develop-workflow`, `hud` 은 변경 없음 (00 §3.3). atelier 안정화 후 별도 epic.

### 7.5 분리 active 3개

`external-llm`, `barrier-sync`, `openclaw-docker` 변경 없음.

---

## 8. 비범위

다음은 02 의 결정 대상이 아니며 03/04 의 영역이다.

- 마이그레이션 절차 (hook 재작성 알고리즘, alias 자동 생성 UX) → 03
- CI 게이트 정의 (frozen 경로 변경 차단 규칙) → 04
- PR 분할 (cli 포팅, command 이동, hook 재배치 등을 어떻게 쪼갤지) → 04
- 검증 체크리스트 (cross-plugin 참조 0건 확인, hook 마이그레이션 dry-run 등) → 04

---

## 9. 미해결 항목 (외부 의존)

- **marketplace schema `deprecated` 지원 여부**: 03 작성 전 또는 Phase 0 에서 검증. 미지원 시 7.3 폴백 자동 적용.
- **bun → cargo 빌드 환경 전환**: 사용자 머신에 Rust toolchain 가정 (rust-toolchain.toml 존재 확인됨). bun 의존 제거.
- **autopilot 바이너리 이름 → atelier 변경에 따른 외부 스크립트 영향**: 04 의 검증 체크리스트에서 codebase 내 `\\bautopilot\\b` 호출 grep 으로 확인.

---

## 10. 다음 단계

03-migration.md 에서 위 구조를 사용자가 어떻게 옮겨가는지 (CLI alias, hook 재작성, settings.json 검증) 절차로 정리.
04-rollout.md 에서 단계별 PR 분할 + CI 게이트 + 검증 체크리스트.
