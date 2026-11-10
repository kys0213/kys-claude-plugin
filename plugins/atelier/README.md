# atelier

> Epic 1 (consolidation, [#765](https://github.com/kys0213/kys-claude-plugin/issues/765)) + Epic 2 (skill extraction, [#766](https://github.com/kys0213/kys-claude-plugin/issues/766)) 완료.
> 설계 히스토리(plan — 시점 기록): [`plans/atelier/`](../../plans/atelier/) · 상위 epic: [#738](https://github.com/kys0213/kys-claude-plugin/issues/738)

**atelier**(공방)는 개발 워크플로우를 처음부터 끝까지 책임지는 단일 큐레이션 plugin입니다.
설계 합의 → spec 작성 → 구현 → 리뷰 → PR 머지까지의 전체 흐름을 하나의 책임 경계 안에서 제공합니다.

흩어져 있던 6개 plugin을 흡수해, 묵시적 의존과 중복 책임을 명시적 단일 namespace로 정리합니다.

## 흡수 매핑 (6 → 1)

| 기존 plugin | 제거 시점 버전 | atelier 내 위치 |
|---|---|---|
| `git-utils` | 2.4.2 | `skills/git/`(+references), `cli/` (Rust 포팅) |
| `github-autopilot` | 0.30.1 | **제거됨** — 에이전트 스웜이 클로드만으로 동작하게 되어 GitHub 이슈 구동 autopilot 루프를 걷어내고, 자율 개발은 `skills/orchestrator/`(기본 자율 주행)가 담당 |
| `spec-kit` | 0.7.1 | `skills/spec-write/`, `templates/spec/` |
| `workflow-guide` | 0.6.0 | `agents/workflow/*`, `skills/{workflow,agent-design-principles}/`, `rules/` |
| `coding-style` | 0.3.0 | `templates/claude-md/` — `/simplify` 제안 hook 은 제거하고, 단순화 검토는 orchestrator 검토 게이트의 단순화 관점이 맡음 |
| `orchestrator` | 0.2.0 | `skills/orchestrator/`(+references) |

흡수된 6개 plugin은 저장소에서 **제거되었습니다** — git history만 참조 가능하며, 후속 개발은 atelier에서만 진행합니다. `autodev`, `develop-workflow`도 함께 제거되었습니다. 마이그레이션 이력은 [`plans/atelier/03-migration.md`](../../plans/atelier/03-migration.md)를 참조하세요.

## 슬래시 표면 (관심사 단위)

Epic 2 ([#766](https://github.com/kys0213/kys-claude-plugin/issues/766))에서 capability 슬래시(35개)를 **관심사 단위**로 수렴했습니다. skill 이 `user-invocable` 이라 슬래시 호출과 모델 자동 호출을 모두 지원하며, 세부 동작은 skill 의 `references/` 로 progressive disclosure 합니다.

### 관심사 skill (슬래시 + 모델 자동 호출)

```
/atelier:spec-write   # 합의된 설계를 스펙 문서 계층(DESIGN→concerns→flows)으로 작성
/atelier:communicate  # 맥락 전달 작문 기준 (독자 수준·맥락 이전·채널 적응) — 공유/보고/문서 작성 시 사용, 보고서는 HTML 레포트
/atelier:git          # git 워크플로우 (커밋·push·PR·충돌 해결·리뷰 정리·이슈 우선순위)
/atelier:workflow     # 컨벤션 scaffold·.claude/rules 설계·설계 원칙 룰 설치·워크플로우 리뷰
/atelier:orchestrator # 위임/병렬 분해·worktree 격리·머지 조정 (기본 자율 주행, HITL opt-out)
/atelier:grill        # 설계를 대화로 생성(발산→수렴)하거나 이미 있는 계획을 심문 (빈틈·가정 드러내기)
```

### 유지 command (deliberate 진입점)

```
/atelier:setup       # 통합 setup (git / style / workflow 모듈 + hook 관리)
```

자율 개발 루프는 별도 진입점 없이 `/atelier:orchestrator` 가 기본 자율 주행으로 수행합니다.

### 출력 스타일 (플러그인 레벨 자동 적용)

`output-styles/communicate.md` 가 `force-for-plugin` 으로 플러그인 활성화 시 자동 적용되어,
매 턴의 채팅 응답을 communicate 스킬의 기준선(결론 먼저 · 해요체 단문 · 개조식 · 군더더기 제거)으로 정제합니다.
대화 밖 독자용 산출물(슬랙 공유·PR 본문·문서)은 여전히 `/atelier:communicate` skill 이 담당합니다.
보고서(분석·조사·진행 보고)는 Mermaid 다이어그램을 SVG 로 구워 넣은 단일 파일 HTML 레포트로 만듭니다 (`scripts/report/render-report.mjs` 로 렌더링 검증).

capability 슬래시(commit-and-pr, prioritize-issues, hook-config, scaffold-conventions 등)는
모두 위 관심사 진입점으로 흡수되었습니다 — 슬래시 없이 자연어로 요청해도 해당 skill 이 자동 트리거됩니다.

### 기계적 호출만 CLI

`atelier git <reviews|guard|hook>` 등 hook·구조화 read 처럼 **기계적 호출이 꼭 필요한** 연산은 슬래시도 skill 도 아닌 Rust CLI 가 담당합니다 (CLAUDE.md 책임 경계). 커밋·브랜치·PR 은 git/gh 가 이미 결정적이라 CLI 로 감싸지 않고, skill 이 컨벤션을 적용해 plain git/gh 로 실행합니다.

## CLI

atelier는 단일 Rust crate(`cli/`)로 빌드되며, 바이너리 `atelier` 하나가 subcommand로 라우팅합니다.

```
atelier drift <check|sync>                # setup 이 복사한 산출물의 드리프트 판정/갱신 (shell 스크립트 → Rust 포팅)
atelier git <reviews|guard|hook>          # git-utils 의 기계적 호출 표면 (TypeScript → Rust 포팅)
atelier session <push-check|ensure-env>   # 세션 경계 hook (SessionStart / Stop)
atelier orchestrator <spawn-check|compact-note>      # function hooks 모듈이 위임하는 결정적 판정 (아래 §Function hooks)
atelier code-style <install|update|check|restore|uninstall> # 프로젝트 코드 스타일 룰(base·언어·앱 성격) 설치·갱신·점검·제어 (/atelier:code-style)
atelier kanban <board|answer|send|stop|inbox|spawn>          # 세션 칸반 판정과 세션 사이 요청 전달 (아래 §세션 칸반)
```

`drift` 는 `/atelier:update`·`/atelier:setup` 명세가 호출하는 결정적 도구입니다.
`check` 는 CLAUDE.md `[coding-style]` 블록과 `~/.claude/rules/atelier/` 룰 복사본을 플러그인 원본과
비교해 `<check>=<STATUS>` 라인으로 보고하고 (exit 0 무드리프트 / 1 드리프트 / 2 오류),
`sync --target <claude-md|rules|user-rules>` 는 백업(`<file>.bak-<timestamp>`) 후 해당 산출물만
원본으로 갱신합니다 — 신규 설치는 setup 담당이고, sync 는 이미 설치한 정책 룰에 새 버전이 추가한 파일만 채웁니다. drift 가 다루는 룰은 user scope 에만 설치합니다.

`code-style` 은 `/atelier:code-style`·`/atelier:update` 명세가 호출합니다. 룰 원본은 세 층 — `rules/code-style/base/`(언어 무관),
`lang/<언어>/`(`--lang`), `app/<성격>/`(`--app`, 예: cli·library) — 이고, 프로젝트 `.claude/rules/atelier/code-style/` 아래에
나란히(`base/`, `<언어>/`, `<성격>/`) 그대로 복사합니다. 같은 `<name>=<STATUS>` 형식과 exit 규칙으로 점검합니다
(원본에서 없어진 룰은 `ORPHANED`, 프로젝트가 지운 룰은 `EXCLUDED`). 설치된 언어·앱 성격과 건넨 룰은 `installed.json` 에 기록하고,
`restore --rule` 로 뺀 룰을, `uninstall --lang/--app` 으로 언어·앱 성격을 제어합니다. 프로젝트마다 다르고 팀과 공유돼야 해서 프로젝트 scope 에 둡니다.

`push-check` 는 Stop 시점에 **열린 PR 이 있는 브랜치가 upstream 보다 ahead** 이면
`{"decision":"block","reason":...}` 를 stdout 에 내보내 세션 종료를 막습니다 (항상 exit 0 —
판정 조건과 push 정책은 `skills/git/SKILL.md` §열린 PR 최신화 원칙 이 단일 출처입니다).

## Agent team 자동 활성화

orchestrator 의 아키텍트 협의체·자문단은 agent team(Claude Code 실험 기능)이 있어야 돕니다.
SessionStart hook `hooks/ensure-agent-teams.sh` 가 `~/.claude/settings.json`(`CLAUDE_CONFIG_DIR` 가 있으면
그 아래)의 `env` 에 `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS` 가 **없을 때만** `"1"` 로 추가합니다.

- **동의**: plugin `userConfig` 의 `agent_teams`(기본 켜짐)입니다. plugin 을 켤 때 묻고,
  `/plugin configure atelier` 로 바꿀 수 있어요.
- **적용 시점**: Claude Code 는 env 를 시작할 때 읽으므로 **추가한 다음 세션부터** 켜져요.
- **끄기**: settings 의 값을 `"0"` 으로 두세요. 키가 있으면 값과 무관하게 다시 쓰지 않아요.
- **안전장치**: 쓰기 전 `<file>.bak-<timestamp>` 백업, temp 파일 → rename 으로 교체, 기존 키 순서·파일 권한 유지.
  settings 가 심링크(dotfiles)면 링크를 그대로 두고 대상 파일에 써요.
  JSON 이 깨졌거나 `env` 가 객체가 아니면 파일을 건드리지 않고 이유만 한 줄 알려요.
- **적용 범위**: user scope 라 atelier 밖의 세션에도 켜져요. `-p`·Agent SDK 같은 비대화형 실행에서는
  켜져 있어도 teammate 가 뜨지 않아요.

판정·쓰기는 `atelier session ensure-env --settings <file> --key <K> --value <V>` 가 하고, shim 은
동의·이미 켜짐·CLI 미설치를 보고 건너뛰기만 해요.

기존 `git-utils` 호출 호환을 위한 alias는 `/atelier:setup`이 안내합니다.

## 모드 (function hooks)

`hooks/register.ts`(`hooks.json` 의 `modules`)는 Claude Code 모드로 로드됩니다. 모드는 Claude Code
v2.1.287(Desktop v2.1.286)부터 플래그 없이 기본으로 켜지고, 플러그인을 설치하면 함께 동작합니다.

### 세션 칸반

`/kanban` 을 입력하면 이 머신에서 atelier 가 켜진 Claude Code 세션 전체를 칸반 pane 으로 보여줍니다.
다시 입력하면 닫힙니다. 컬럼은 **입력 대기 → 작업 중 → 쉬는 중 → 응답 없음** 이고, 카드에는 저장소
이름·그 컬럼에 머문 시간·세션 제목(없으면 첫 프롬프트)과 진행 상황(할 일 진척, 작업 중이면 지금 실행 중인
도구), 쉬는 중이면 직전 응답 첫 줄이 나옵니다.

- **입력 대기** 는 권한 확인·질문(AskUserQuestion)·턴 끝의 결정 요청(데스크톱의 결정 카드, `WrapUp`)·MCP
  입력 요청을 기다리는 세션입니다. 질문은 보드 아래에 펼쳐져, 선택지를 누르거나 직접 입력한 뒤 **보내기** 를
  누르면 그 세션의 질문 화면이 닫히고 사람이 답한 것처럼 이어갑니다.
- 카드 제목을 누르면 그 컬럼에 맞는 상세가 열립니다.
  - 입력 대기: 요청·기다리기 직전 응답·기다리는 것(권한 요청의 명령 등), 결정 요청은 답 버튼, 그리고 **중지**.
  - 작업 중: 요청·할 일 목록·최근 도구, 그리고 **중지**(한 번 더 눌러 확인).
  - 쉬는 중: 직전 요청·직전 응답, 그리고 **입력해 보내기** — 그 세션의 새 턴으로 실행됩니다.
  - 좁은 pane 에서는 상세가 보드 자리를 대신하고 **뒤로** 로 돌아갑니다.
- pane 은 `ctrl+x tab` 이나 클릭으로 포커스하고, Tab 으로 버튼 사이를 옮깁니다. 데스크톱에서도 같습니다.
- 카드에서 그 세션 화면으로 이동하는 기능은 없고, 권한 확인에 허용·거부로 답할 수도 없습니다(그 세션에서
  답하거나 칸반에서 중지).
- 세션마다 모드가 이벤트 사실을 모드 저장소(`$.store`, 같은 머신의 세션이 공유)에 기록하고, 30초마다
  생존 신호를 남깁니다. 생존 신호가 2분 끊긴 세션은 응답 없음, 24시간 끊기거나 정상 종료한 세션은 사라집니다.
- 보내기·중지는 받는 세션의 inbox(`<CLAUDE_CONFIG_DIR 또는 ~/.claude>/atelier/kanban/inbox/<세션>/`)에 남는
  요청이고, 받는 세션이 2초마다 한 번씩만 가져갑니다. 작업 중일 때 온 프롬프트·60초 지난 요청·끝난 턴의 중지는
  실행하지 않고 버립니다.
- **오케스트레이터**: 읽기 전용 모드 툴 `kanban_board` 로 레포별 세션 목록을 읽고, 쉬는 세션에는
  `atelier kanban send --session <id> --from agent --from-label <이 세션> --text <일>` 로 맡기고, 레포에
  세션이 없으면 `atelier kanban spawn --cwd <메인 작업 트리 루트> [--worktree <이름>] --text <일>` 로
  `claude --bg` 세션을 띄웁니다. Bash 로 부르므로 오케스트레이터 세션의 권한 규칙을 그대로 따릅니다.
  어느 세션에 맡길지·언제 띄울지의 절차는 `orchestrator` skill 이 담습니다.
- 판정·요청 전달은 `atelier kanban` CLI 가 하고, 모듈은 기록과 그리기만 합니다. CLI 가 없으면 pane 에 안내
  한 줄만 나옵니다.
- 상세 규격: [`spec/concerns/session-kanban.md`](spec/concerns/session-kanban.md)

### orchestrator 규약 집행

orchestrator 규약 일부를 집행합니다. model 미지정만 **차단**하고, 나머지는 경고입니다:

- Agent 도구로 sub-agent 를 띄울 때 `model` 을 지정하지 않으면(fork 제외) **띄우기 전에 차단**하고,
  model 과 effort 를 지정해 다시 호출하라는 사유를 돌려줍니다. 모델은 그 자리에서 고쳐 다시 호출합니다.
  차단은 실행 전에만 할 수 있어 실제로 어떤 모델이 정해질지 모르는 시점에 판정합니다. 그래서 에이전트
  정의가 모델을 정해 둔 타입(예: frontmatter 에 `model` 을 지정한 에이전트)도 `model` 없이 띄우면 차단됩니다.
- 메인 model 의 집행 위임 상한(Fable→Opus, Opus→Opus, Sonnet→Sonnet, Haiku→Haiku)을 넘는 `model`
  을 지정하면 경고합니다. 자문 소집은 상한을 정당하게 넘으므로 차단하지 않고, 경고 문구가 무시해도 됨을
  안내합니다.
- 메인 대화가 compaction 될 때 orchestrator 런 상태(epic 이름·log_dir·task 상태 등)를 요약에
  보존하라는 지시를 덧붙입니다.

판정은 전부 CLI(`atelier orchestrator spawn-check` / `atelier orchestrator compact-note`)가
하고, 모듈은 이벤트 ↔ CLI JSON 어댑터입니다(`.claude/rules/tool-layer-boundary.md`). CLI 바이너리가
없거나 호출이 실패하면 모듈은 차단도 경고도 하지 않고 그대로 통과시킵니다(fail-open).

- **모드가 꺼진 세션**(`--safe-mode`, `disableAllHooks`, v2.1.287 미만)에서는 모듈이 로드되지 않고
  기존 classic hook(`hooks.json` 의 `hooks`)만 그대로 동작합니다 — 이 기능이 없어도 세션은 정상입니다.
- 모드 API 는 버전마다 바뀔 수 있어 CI 는 Claude Code 버전을 고정해 검증합니다(현재 `2.1.318`).
- 상한 경고는 호출 **후**에 붙으므로 그 호출은 막지 못합니다. 이번 호출을 재실행할 필요는 없고, 다음
  dispatch 부터 바로잡으면 됩니다.
- 한계: tier 를 판별할 수 없는 모델 id(Bedrock 추론 프로필 ARN, 게이트웨이 id 등)에서는 상한 검사가
  꺼집니다.
- 한계: `atelier` CLI 가 이 서브커맨드 이전 버전이면 차단·경고가 조용히 빠지므로 `/atelier:update` 로
  바이너리를 갱신해야 합니다. 차단 이전 버전의 CLI 는 model 미지정을 경고로만 돌려줍니다.
- 개발자: 에디터 타입이 필요하면 plugin 디렉토리에서 `/plugin-types` 를 실행합니다(생성물은 커밋하지
  않습니다).

## 상태

| Phase | 내용 | 상태 |
|---|---|---|
| Phase 0 | 사전 검증 | ✅ |
| Phase 1 | 골격 (plugin.json · README · marketplace WIP entry) | ✅ |
| Phase 2 | CLI 통합 (Rust 단일 바이너리 — autopilot 흡수 + git-utils 포팅) | ✅ |
| Phase 3 | commands / agents / skills / hooks 이동 + namespace 치환 | ✅ |
| Phase 4 | CI 인프라 (validate · rust-binary) | ✅ |
| Phase 5 | 흡수 6개 제거 (`autodev`·`develop-workflow` 포함) | ✅ |

> **현재 상태**: Epic 1 (consolidation) + Epic 2 (skill extraction) 완료.
> 에이전트 스웜이 클로드만으로 동작하게 되어 GitHub 이슈 구동 autopilot 서브시스템(skill·agents·commands·CLI 모듈)을 제거하고,
> 자율 개발은 `orchestrator` skill 의 **기본 자율 주행**(HITL opt-out)으로 통합했습니다.
> 단일 `atelier` 바이너리는 `atelier git <...>` 를 제공하며,
> 슬래시 표면은 capability 35개 → 관심사 단위로 수렴되었습니다.
>
> ⚠️ `gh` CLI 의존 git 명령(reviews, guard pr)은 mock 단위 테스트만 완료 —
> 실제 `gh`/네트워크 라이브 검증은 정식 릴리스 전 별도 수행이 필요합니다.
