# 기존 코드 주석을 주석 규칙에 맞게 정리한다

> **Plan — 이 시점의 결정 기록.** 현재 정책의 신뢰 소스가 아니다.
> 날짜: 2026-10-05 · 브랜치: refactor/legacy-comment-cleanup · 이슈: #901 · 승인 출처: council pass (협의체 3라운드 검증 pass — 2라운드 한도를 사용자 승인으로 1회 추가)

남은 이슈 5건을 작업 4개(A 규칙 근거 위임 · B 주석 정리 · C 플러그인 거버넌스 · D Stop hook 메시지 전달)로 나눠 각각 독립 PR 로 진행했어요. 이 plan 은 그중 한 작업의 기록이에요.

## 작업 간 파일 분할

| 파일 | 소유 작업 | 근거 |
|---|---|---|
| `plugins/atelier/cli/src/session/mod.rs` | D 전부 (주석 정리 포함) | D 가 `emit()`(L105-110)·모듈 문서(L20-24)를 고쳐요. B 대상 주석 L57·64·71·105·181 도 같은 파일이에요 |
| `plugins/atelier/cli/src/session/commands/simplify.rs` | D 전부 (주석 정리 포함) | D 가 렌더 함수를 더해요. B 대상 주석 L140-141 이 `#725` 를 인용해요. 패턴 밖 이력 주석 L120-121 도 D 가 정리해요 |
| `plugins/atelier/cli/tests/session_cli_simplify.rs` (신규) | D | `emit` 배선 테스트 (D 결정 7) |
| `common/**` | 어느 작업도 편집 안 함 | B 의 `call-*.sh:23` 은 "남긴다" 판정 |
| `plugins/atelier/hooks/suggest-simplify.sh` | B | D 는 shim 을 건드리지 않아요 (shim 은 `exec atelier session simplify-check` 만 해요, `plugins/atelier/hooks/suggest-simplify.sh:13`) |
| `plugins/atelier/skills/orchestrator/**` | A | 다른 작업은 건드리지 않아요 |
| `.claude/rules/plugin-governance.md` (신규) | C | 다른 작업은 건드리지 않아요 |
| `.claude/rules/comments.md` | B (변경 없음으로 결정, §B 결정 4) | — |

- 버린 대안: "B 는 `mod.rs:105-106` 만 빼고 나머지 줄은 정리" — 같은 파일 다른 hunk 라 git 이 자동 병합할 가능성은 높지만, D 의 모듈 문서·`emit` 수정 범위가 구현 중 넓어지면 인접 hunk 충돌이 생겨요. 파일 단위로 나누면 판정이 결정적이에요.



## 요구사항

- 이슈·PR 번호, plan·spec 절·스킬 문서 포인터를 담은 기존 주석을 `.claude/rules/comments.md` §1 기준으로 정리해요. 번호만 기계적으로 지우지 않고, code-local 함정이 있으면 그 사실만 남게 다시 써요 (#901 요구사항 1).
- 로직·출력 문자열·테스트 단언은 바꾸지 않아요 (#901 요구사항 2).
- 정리 후 `comments.md` "기존 코드" 절 유지 여부를 재검토해요 (#901 요구사항 3).

## 사이드이펙트 조사

- **이슈의 grep 명령 중 두 토큰이 macOS `git grep -E`(BSD regex)에서 동작하지 않아요.**
  - `\s`: `^\s*` 가 줄 맨 앞 주석만 잡아요.
    - 신호 1: `git grep -nE '^\s*(//[/!]?|#).*#[0-9]{2,}' -- '*.rs' '*.go' '*.sh' | wc -l` → 18, `[[:space:]]` 로 바꾸면 32
    - 신호 2: 들여쓴 `plugins/atelier/cli/src/session/mod.rs:181` 이 `[[:space:]]` 판에만 나와요
  - `\b`: 셋째 명령의 `[A-Za-z0-9_-]+\.md\b` 쪽이 0줄이에요 (검증 r1 F1 재현). `([^A-Za-z0-9_]|$)` 로 바꾸면 매치돼요.
  - → 대상 명령은 `[[:space:]]` 와 `([^A-Za-z0-9_]|$)` 를 써요 (결정 1).
- 현재 대상 (origin/main `6996341`, D 소유 두 파일 제외)
  - 번호 인용(첫 명령) **26줄 / 14파일**. 파일별 개수: `git grep -cE '^[[:space:]]*(//[/!]?|#).*#[0-9]{2,}' -- '*.rs' '*.go' '*.sh' ':!plugins/atelier/cli/src/session/mod.rs' ':!plugins/atelier/cli/src/session/commands/simplify.rs'`
    - 이 중 다른 플러그인 파일 1개: `plugins/suggest-workflow/cli/tests/blackbox_role_filter.rs:1` (`(#202)`)
  - 문서 포인터(둘째 명령) 32줄, `.md`·규칙 경로(셋째 명령) 43줄. 두 결과는 겹쳐요. 합집합은 **65줄**이에요 (결정 3 의 `U` 함수로 실행해 확인했어요).
  - 이슈 본문 목록과 다른 이유: 이후 PR(#903·#908 등)이 줄을 옮기거나 지웠어요. 이슈도 "줄 번호는 정리 시점에 다시 뽑으세요" 라고 해요.
- `check-decision-graph.sh` 는 문서 구조를 검사하는 스크립트예요. 그래서 검사 대상 문서·절 이름(`§예산과 가드레일 값`, SKILL.md `## 정지 조건`)은 도메인 입력이에요 (#901 B 절 단서). `.claude/rules/decision-graph.md:9` 가 이 규칙 문서와 스크립트를 짝지어요.
- `common/scripts/call-{codex,gemini}.sh:23` 은 셋째 명령에 잡혀요. 하지만 `"- file.md" 패턴` 은 스크립트가 파싱하는 입력 형식이라 도메인이에요 → 남겨요. 그래서 B 는 `common/` 을 건드리지 않아요.
- 버전 범프: `release.yml` 이 변경된 플러그인을 전부 범프해요 (검증 r1 F7). B 는 atelier 와 suggest-workflow 두 플러그인을 고치므로 **둘 다 patch 범프**돼요. `scripts/`·`tools/` 는 범프 대상이 아니에요.
- 검증 도구: atelier·suggest-workflow 는 cargo, `tools/` 는 Go, 공통은 `make validate-ci`(`Makefile:73-75`).

## 결정과 근거

1. **대상 산출 명령 (결정적)** — 작업 시작 시 다시 실행해 목록을 뽑아요. D 소유 두 파일은 pathspec 으로 뺐어요.
   ```
   git grep -nE '^[[:space:]]*(//[/!]?|#).*#[0-9]{2,}' -- '*.rs' '*.go' '*.sh' ':!plugins/atelier/cli/src/session/mod.rs' ':!plugins/atelier/cli/src/session/commands/simplify.rs'
   git grep -nE '^[[:space:]]*(//[/!]?|#).*(plans/|spec §|§|SKILL\.md|불변식)' -- '*.rs' '*.go' '*.sh' ':!plugins/atelier/cli/src/session/mod.rs' ':!plugins/atelier/cli/src/session/commands/simplify.rs'
   git grep -nE '^[[:space:]]*(//[/!]?|#).*(\.claude/rules|[A-Za-z0-9_-]+\.md([^A-Za-z0-9_]|$))' -- '*.rs' '*.go' '*.sh' ':!plugins/atelier/cli/src/session/mod.rs' ':!plugins/atelier/cli/src/session/commands/simplify.rs'
   ```
2. **판정 기준** — 줄마다 셋 중 하나로 정해요. 근거는 `comments.md` §1 판별 체크리스트예요. 애매하면 지워요 (§1 "애매하면 지운다").

   | 판정 | 조건 |
   |---|---|
   | 지운다 | 번호·경로·절을 지우면 남는 내용이 없음 / 다른 문서가 바뀌면 틀려지는 복제 / 코드가 이미 말하는 what |
   | 의도로 다시 쓴다 | 번호·포인터를 지워도 code-local 함정·외부 동작이 남음 → 포인터 부분만 빼요 |
   | 남긴다 | 문서·경로 이름이 그 코드의 **입력·출력 도메인**(읽는 파일, 쓰는 디렉토리, 파싱하는 형식, 출력 토큰의 소비자) |

   **번호 인용 26줄**은 전부 "지운다" 또는 "의도로 다시 쓴다" 예요. 남길 줄은 없어요. 예:
   - `git/core/guard/repo_layout.rs:7` "mis-judges relative `file_path`s (#780)" → 번호만 빼요 (함정 문장 유지)
   - `tests/git_commands_guard.rs:121` `// ---- #778: PreToolUse payload parsing ... ----` → 번호만 빼고 구분 제목은 남겨요

   **포인터·`.md` 합집합 65줄** 중 바꿀 24줄은 아래와 같아요.

   | 파일 | 원문 조각 | 판정 |
   |---|---|---|
   | `plugins/atelier/cli/src/git/core/git.rs` | `non-standard defaults — see commands/setup.md.)` | 다시 쓴다 — `— see commands/setup.md` 만 빼요 |
   | `plugins/atelier/hooks/{push-check,session-baseline,suggest-simplify}.sh` | `(\`.claude/rules/tool-layer-boundary.md\`)` | 다시 쓴다 — 괄호만 빼고 "판정·…은 전부 CLI 에 있습니다" 유지 |
   | `scripts/check-decision-graph.sh` | `# 설계: plans/atelier/11-orchestrator-lean.md` | 지운다 |
   | 〃 | `§2 목표 구조 / 파일별·총합 줄수 상한 → 검사 (5)` 등 `#   §N … → 검사 (M)` 5줄 | 지운다 (plan 절 대응표) |
   | 〃 | `구조 heading 화이트리스트 (설계 §2 목표 구조 · 규약 §1).` | 다시 쓴다 — 괄호만 빼요 |
   | 〃 | `폐지돼(규약 §8) \`## D##.\` 같은 예외 통로는 없다` | 다시 쓴다 — `폐지돼(규약 §8)` 를 빼요 (이력+포인터) |
   | 〃 | `크기 상한 (설계 §2 · 규약 §5).` | 다시 쓴다 — 괄호만 빼요 |
   | 〃 | `유일한 소유 절 (규약 §6).` | 다시 쓴다 — 괄호만 빼요 |
   | 〃 | `외부 계약 표면 (설계 §9-a·§9-b · 규약 §7).` | 다시 쓴다 — 괄호만 빼요 |
   | 〃 | `조건 셀이다 (grill SKILL.md 가 "정지 조건의 그 행"으로 지목한다).` | 다시 쓴다 — "조건 셀이다 — 외부 스킬이 그 행을 이름으로 지목한다." (다른 스킬 문서 내용의 복제 제거) |
   | 〃 | 검사 헤더 7줄 `# (2) …`, `# (5) …`, `# (6) …`, `# (7) …`, `# (11) …`, `# (12) …`, `# (13) …` 끝의 `(설계 §… · 규약 §…)` / `(규약 §1 판단 대본 금지)` | 다시 쓴다 — 괄호만 빼요 |
   | 〃 | `(규약 §6 "값은 예산 값 표에만, 이름은 어디서나")` (단독 줄) | 지운다 |
   | 〃 | `# 규약: .claude/rules/decision-graph.md (검사 번호 = 어긴 규칙 번호의 대응표)` | 다시 쓴다 — 괄호만 빼요. 이 스크립트가 집행하는 규칙 문서라 경로는 도메인이에요. 괄호 속 "대응표" 는 사실이 아니에요 (`:524` 검사 (12)↔규약 §2, `:372` 검사 (6)↔규약 §1) |

   **남기는 41줄**은 "남긴다" 행이에요. 아래 keep 조각 목록(결정 3)이 그대로 판정표예요. 대표 사유는 이래요.
   - drift (`drift/**`, `tests/drift_*.rs`): drift 가 읽고 쓰는 `CLAUDE.md`·`~/.claude/rules/atelier` 는 도메인이에요. `commands/update.md` 언급(`drift/core/types.rs:35,153`, `drift/mod.rs:23`)은 stdout 토큰을 파싱하는 소비자를 지목해요. "이 문자열을 바꾸면 소비자가 깨진다" 는 함정이라 code-local 이에요
   - `common/scripts/call-*.sh:23`: 파싱하는 입력 형식이에요
   - `suggest-workflow/cli/tests/blackbox_correctness.rs:132,326,385`: 테스트 픽스처 파일 이름이에요
   - `tools/**` 9줄: 읽는 파일(`SKILL.md`)·경로 패턴 예시·검증 단계 이름이에요
   - `check-decision-graph.sh` 의 `SKILL.md §기본값 표 소속`·`<log_dir>/HANDOFF.md`·`` `§예산과 가드레일 값` `` 2줄·`## 정지 조건` 2줄·`` 외부 스킬의 `§절 이름` 참조 ``: 이 스크립트가 검사하는 문서·절 이름이에요 (검증 r1 F2 의 `:28,31,86,405,479` 중 `:28` 은 위 표에서 다시 쓰고, 나머지는 남겨요)
   - 정리하다 문자열 패턴에 안 잡히는 위반(정책 근거 복제 등)을 같은 파일에서 보면 함께 고쳐도 돼요. 단 grep 대상 파일 밖으로 범위를 넓히지 않아요.
3. **keep 조각 목록 (결정적 판정표)** — 줄 번호가 아니라 원문 조각으로 고정해요. 구현 agent 가 repo 밖 경로에 이 내용 그대로 `b-keep.txt` 를 만들어 검증에 써요. 원본 사본: `/private/tmp/claude-501/-Users-kys0213-workspace-kys-claude-plugin/d2bbc1bd-9227-421a-b3fe-cc765265d7b7/scratchpad/orchestrator/remaining-issues/b-keep.txt` (40개 조각, 41줄 대응 — `"- file.md" 패턴만` 은 2줄, `` `commands/update.md` branches on `` 은 `types.rs:35`·`:153` 2줄에 걸려요).
   ```
   "- file.md" 패턴만
   judges whether the installed copies (CLAUDE.md block
   Judges every artifact (CLAUDE.md block
   is the user's CLAUDE.md, whose inode
   Block markers in the user CLAUDE.md
   (default `~/.claude/rules/atelier`)
   (`<project>/.claude/rules/atelier`)
   `commands/update.md` branches on
   the strings `commands/update.md` branches on
   the consumer spec (`commands/update.md`)
   marker range inside the user CLAUDE.md
   The user CLAUDE.md holding the coding-style block
   CLAUDE.md coding-style block, the project rules copy
   User CLAUDE.md path (default: $HOME/.claude/CLAUDE.md)
   (default: $HOME/.claude/rules/atelier)
   plus a fake user CLAUDE.md location
   A CLAUDE.md that exists but carries neither marker
   an absent CLAUDE.md is refused
   A CLAUDE.md without the block is "not installed"
   CLAUDE.md or a temp directory
   README.md: Edit×1 = 1
   edits config.toml, .env, main.rs, ipynb, README.md
   config.toml(3), .env(1), main.rs(1), ipynb(1), README.md(1)
   SKILL.md §기본값 표 소속
   경로 템플릿(`<log_dir>/HANDOFF.md`)
   `spec 확정 게이트` 만 heading 이 아니라 SKILL.md `## 정지 조건` 표의
   `§예산과 가드레일 값` 표의 행 이름
   외부 스킬의 `§절 이름` 참조가 조용히 죽는다
   절 밖에서 예산 이름 옆의 숫자 리터럴
   행 이름 1 — SKILL.md `## 정지 조건` 표의 조건 셀로 정확히 1회
   loads a skill's registered identity from its SKILL.md
   "plugins/atelier/skills/git/SKILL.md" → "git"
   "plugins/hud/skills/SKILL.md" → "hud"
   Pattern: skills/{name}/SKILL.md
   Pattern: skills/SKILL.md — use parent plugin name
   relative path like "plugins/atelier/commands/setup.md"
   Should have 1 valid (./README.md)
   Remove anchor from path (e.g., ./file.md#section)
   3. SKILL.md validation
   # 규약: .claude/rules/decision-graph.md
   ```
4. **`comments.md` "기존 코드" 절은 그대로 둬요.** 이 절의 문장("손대지 않는 줄을 규칙 때문에 일괄 수정하지 않는다")은 도입 시점 단서가 아니에요. **다른 작업의 diff 에 무관한 주석 수정이 섞이지 않게 하는 범위 규칙**이에요. 일괄 정리가 끝나도 그 필요는 남아요. 재검토 결론은 PR 본문에 한 줄로 남겨요 (전역 규칙 "Refactoring Findings" 의 skip 사유 기록).

## 버린 대안

| 대안 | 이유 |
|---|---|
| 이슈 본문 grep 명령을 그대로 사용 | macOS 에서 `\s` 는 들여쓴 주석 14줄을, `\b` 는 `.md` 언급 전부를 놓쳐요 |
| 번호·경로만 정규식으로 일괄 삭제 | #901 요구사항 1 이 금지해요. "(#780)" 처럼 함정 문장과 섞인 줄은 문장이 깨져요 |
| keep 목록을 줄 번호로 고정 | 같은 파일 위쪽 주석을 지우면 번호가 밀려 판정이 깨져요 (검증 r1 F2) |
| `common/scripts/call-*.sh:23` 정리 | 도메인 입력 형식이라 남겨요. 고치면 `common/` 변경이 생겨 범프 대상이 늘어요 |
| `comments.md` "기존 코드" 절 삭제 | 다음 작업 agent 가 무관한 파일 주석까지 고쳐 diff 가 커져요 |
| grep 명령을 `comments.md` 에 등재해 상시 검사화 | 이슈 요구가 아니에요 (YAGNI). 도메인 언급과 위반을 grep 만으로 못 가려 CI 검사로는 오탐이 나요 |

## PR 타이틀 / 브랜치

- `refactor(plugins): align legacy comments with comment rules`
- `refactor/legacy-comment-cleanup`
- type 근거: `plugins/` 변경 포함 → 버전 범프 type, 동작 불변이라 `refactor` (#901 권장).
- scope 근거: atelier 와 suggest-workflow 두 플러그인을 고쳐요. 선례는 `refactor(plugins): remove plugins absorbed into atelier (#839)` 예요 (`git log --format=%s -- .claude/rules`).
- PR 본문에 남길 것
  - `\s`·`\b` 를 바꾼 이유
  - atelier·suggest-workflow 둘 다 patch 범프된다는 사실
  - "기존 코드" 절 유지 사유 한 줄
  - D 소유 두 파일 제외 사실
  - keep 조각 판정표 (결정 3)



## 가정

- **B1** — `\s`·`\b` 미동작은 macOS 기본 git(BSD regex) 환경 기준이에요. Linux CI 에서는 `\s` 도 동작할 수 있지만 `[[:space:]]`·`([^A-Za-z0-9_]|$)` 는 양쪽에서 동작하므로 결론은 같아요.
