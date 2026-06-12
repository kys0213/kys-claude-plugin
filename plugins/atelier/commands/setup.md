---
name: setup
description: atelier 통합 환경을 초기화합니다 (git / autopilot / style / workflow 모듈 선택 + 기존 hook 마이그레이션 + guard hook 관리)
allowed-tools: ["Bash", "Read", "Write", "Edit", "AskUserQuestion"]
---

# atelier setup

흡수된 6개 plugin(git-utils, github-autopilot, coding-style, ...)의 설정을 단일 진입점으로 통합합니다.
모듈을 선택해 설치하고, 기존 frozen plugin 경로로 등록된 hook 을 atelier 로 마이그레이션합니다.

> ⚠️ 모든 hook 은 user scope(`~/.claude/settings.json`)에 등록됩니다.
> 등록은 LLM 이 settings.json 을 직접 편집하지 않고 **`atelier git hook register` CLI** 로 수행합니다
> (`.claude/rules/tool-layer-boundary.md`). `--project-dir "$HOME"` 을 주면 `~/.claude/settings.json` 에 기록됩니다.

등록되는 command 는 두 형태뿐입니다:

- **CLI 직접 호출** (결정적 로직이 CLI 에 있는 hook): `atelier git guard write ...` — 바이너리가 PATH 에서 해석되므로 버전 비의존
- **`${CLAUDE_PLUGIN_ROOT}` 리터럴 shim** (#776 에서 CLI 이전 예정인 `.sh` hook): 절대경로로 expand 하지 않고 리터럴 그대로 기록

## Step 0 — atelier CLI 보장 (공통 선행)

모든 모듈이 hook 등록에 CLI 를 사용하므로 가장 먼저 실행합니다:

```bash
bash "${CLAUDE_PLUGIN_ROOT}/scripts/ensure-binary.sh"
```

- plugin.json 버전과 설치된 `atelier --version` 을 SemVer 비교해 필요 시 빌드/설치합니다 (`~/.local/bin/atelier`)
- 실패하면(cargo 부재 등) 이후 Step 을 진행하지 말고 에러를 안내합니다

## Step 1 — 설치 모듈 선택

`AskUserQuestion` 으로 설치할 모듈을 선택합니다 (multiSelect).

| 선택 | 수행 동작 | 출처 |
|---|---|---|
| `git` | GitHub 인증 확인 + `~/.git-workflow-env` 생성 + Default Branch Guard hook | git-utils setup |
| `autopilot` | `github-autopilot.local.md` 생성 + autopilot hook 3개 등록 | github-autopilot setup |
| `style` | `~/.claude/CLAUDE.md` 코딩 원칙 + Stop hook 등록 | coding-style setup |
| `workflow` | `.claude/rules/agent-design-principles.md` 룰 설치 | workflow-guide install |
| `all` | 위 네 가지 전부 | 신규 |

선택된 모듈만 아래 해당 Step 을 수행합니다.

> 이미 설치된 환경에서 guard hook 만 비활성화/재설정하려면 Step 5 (hook 관리 모드)로 바로 진행합니다.

## Step 2a — git 모듈

1. GitHub CLI 인증 확인:
   ```bash
   gh auth status || gh auth login
   ```
2. 환경 설정 파일 생성 (기존 git-utils 와 동일 스키마, 경로 `~/.git-workflow-env`).
3. **기본 브랜치 감지** — guard 는 읽기전용이라 비표준 기본 브랜치(예: `trunk`)를 런타임에 감지하지 못할 수 있으므로,
   1회성인 setup 시점에 full detection(set-head 포함)으로 감지해 주입합니다 (#785):
   ```bash
   DEFAULT_BRANCH=$(bash "${CLAUDE_PLUGIN_ROOT}/scripts/detect-default-branch.sh")
   ```
   감지 실패 시(remote 없음 등) `--default-branch` 를 생략하고 guard 의 런타임 감지에 맡깁니다.
4. Default Branch Guard hook 2종 등록 — `.sh` 경로가 아니라 **CLI 커맨드를 직접** 기록합니다:
   ```bash
   atelier git hook register PreToolUse "Write|Edit" \
     'atelier git guard write --project-dir "${CLAUDE_PROJECT_DIR:-.}" --default-branch '"${DEFAULT_BRANCH}" \
     --project-dir "$HOME"

   atelier git hook register PreToolUse "Bash" \
     'atelier git guard commit --project-dir "${CLAUDE_PROJECT_DIR:-.}" --default-branch '"${DEFAULT_BRANCH}" \
     --project-dir "$HOME"
   ```
   > `${CLAUDE_PROJECT_DIR:-.}` 는 **리터럴로 보존**해야 합니다 (hook 실행 시점에 셸이 expand).
   > 반면 `${DEFAULT_BRANCH}` 는 setup 시점 감지값으로 expand 해서 기록합니다.

   결과적으로 settings.json 에는 다음과 같이 기록됩니다:
   ```json
   { "type": "command", "command": "atelier git guard commit --project-dir \"${CLAUDE_PROJECT_DIR:-.}\" --default-branch main" }
   ```

## Step 2b — autopilot 모듈

1. 프로젝트 설정 파일 `github-autopilot.local.md` 생성 (기존 스키마/경로 동일 — 호환).
2. autopilot hook 3종 등록 — 로직이 아직 `.sh` 에 있으므로(#776 에서 CLI 이전 예정) `${CLAUDE_PLUGIN_ROOT}` 리터럴 shim 으로 기록:
   ```bash
   atelier git hook register SessionStart "*" \
     '${CLAUDE_PLUGIN_ROOT}/hooks/check-cli-version.sh' --project-dir "$HOME"

   atelier git hook register PreToolUse "Bash" \
     '${CLAUDE_PLUGIN_ROOT}/hooks/guard-pr-base.sh' --project-dir "$HOME"

   atelier git hook register PreToolUse "Bash" \
     '${CLAUDE_PLUGIN_ROOT}/hooks/protect-stagnation.sh' --project-dir "$HOME"
   ```
   > `PreToolUse`/`Bash` matcher 는 git 모듈의 commit guard 와 공유됩니다 — `hook register` 는 같은 matcher
   > 그룹에 command 를 append 하므로 서로 덮어쓰지 않습니다.

> autopilot SQLite store(ledger/task DB)는 기존 스키마/경로를 계승하므로 마이그레이션 불필요.

## Step 2c — workflow 모듈

`workflow` skill 의 §"설계 원칙 룰 설치" 절차를 수행합니다:

1. `.claude/rules/` 디렉토리가 없으면 생성, 대상 파일이 존재하면 덮어쓸지 AskUserQuestion 으로 확인
2. `${CLAUDE_PLUGIN_ROOT}/rules/agent-design-principles.md` 를 **내용 수정 없이 그대로** `.claude/rules/agent-design-principles.md` 에 복사

## Step 2d — style 모듈

1. `~/.claude/CLAUDE.md` 에 코딩 원칙 템플릿 병합 (워터마크 기반 중복 확인 — 기존 coding-style 로직 동일):
   - 템플릿 원본: `${CLAUDE_PLUGIN_ROOT}/templates/claude-md/CLAUDE.md`
2. Stop hook 등록:
   ```bash
   atelier git hook register Stop "*" \
     '${CLAUDE_PLUGIN_ROOT}/hooks/suggest-simplify.sh' --project-dir "$HOME"
   ```

## Step 3 — 기존 hook 마이그레이션 (frozen → atelier)

기존 6개 plugin 사용자는 `~/.claude/settings.json` 에 **frozen plugin 경로**의 hook 이 박혀 있습니다.
이를 atelier 로 재작성합니다. (상세: `plans/atelier/03-migration.md §A.3`)

```
1. atelier git hook list --project-dir "$HOME" 으로 현재 등록 현황 조회 (없으면 skip)
2. 출력의 모든 command 문자열 순회, 다음 정규식에 매칭되는 entry 수집:
     .*/plugins/(github-autopilot|coding-style)/hooks/(<file>)\.sh
     .*/plugins/git-utils/scripts/(default-branch-guard.*)\.sh
     .*/plugins/atelier/scripts/(default-branch-guard.*)\.sh   # 구버전 atelier setup 잔재
3. 변경 전 ~/.claude/settings.json 을 settings.json.bak-<timestamp> 로 백업 (cp)
4. 사용자에게 치환 목록을 보여주고 AskUserQuestion 으로 확인
5. 매칭 entry 마다: atelier git hook unregister <type> <old-command> --project-dir "$HOME"
   → 아래 표의 대응 command 로 atelier git hook register (hook register 는 command 기준
   중복 제거를 하므로 frozen + atelier 양쪽에 있던 hook 도 한 개만 남음)
```

> **멱등성**: 이미 atelier 로 재작성된 settings.json 에 재실행하면 변경 0건이어야 합니다.

치환 대상:

| frozen 경로 | atelier 등록 command |
|---|---|
| `github-autopilot/hooks/check-cli-version.sh` | `${CLAUDE_PLUGIN_ROOT}/hooks/check-cli-version.sh` (리터럴) |
| `github-autopilot/hooks/guard-pr-base.sh` | `${CLAUDE_PLUGIN_ROOT}/hooks/guard-pr-base.sh` (리터럴) |
| `github-autopilot/hooks/protect-stagnation.sh` | `${CLAUDE_PLUGIN_ROOT}/hooks/protect-stagnation.sh` (리터럴) |
| `coding-style/hooks/suggest-simplify.sh` | `${CLAUDE_PLUGIN_ROOT}/hooks/suggest-simplify.sh` (리터럴) |
| `git-utils/scripts/default-branch-guard-hook.sh` (또는 구버전 atelier 동명 스크립트) | `atelier git guard write ...` (§"git 모듈" 등록 형식) |
| `git-utils/scripts/default-branch-guard-commit-hook.sh` (또는 구버전 atelier 동명 스크립트) | `atelier git guard commit ...` (§"git 모듈" 등록 형식) |

## Step 4 — CLI alias (선택)

기존 `autopilot` / `git-utils` 호출 호환을 위해 셸 rc 에 alias 추가를 **AskUserQuestion 으로 동의받은 경우에만** 제안합니다:

```bash
alias autopilot='atelier autopilot'
alias git-utils='atelier git'
```

거부 시 안내 문구만 출력합니다. 기존 바이너리는 setup 이 삭제하지 않습니다 (외부 도구를 함부로 지우지 않음).

## Step 5 — Hook 관리 모드 (비활성화 / 재설정)

설치가 아니라 "hook 꺼줘 / 다시 등록해줘" 요청이면 이 모드만 수행합니다.

1. **현황 조회** — 프로젝트/사용자 양쪽 범위를 탐색합니다:
   ```bash
   atelier git hook list PreToolUse
   atelier git hook list PreToolUse --project-dir "$HOME"
   ```
   - 양쪽 모두 없으면: "hook이 설정되지 않았습니다. 먼저 모듈 설치(Step 1)를 진행하세요." 안내 후 종료
   - 양쪽 모두 있으면: AskUserQuestion 으로 관리할 범위 선택 (프로젝트 `.claude/` vs 사용자 `~/.claude/`)
2. **설정 파싱** — list 출력(JSON)의 command 문자열로 활성 hook 을 판별합니다:
   - `atelier git guard write` 포함 → Write/Edit Guard
   - `atelier git guard commit` 포함 → Commit Guard
   - `atelier git guard pr` 또는 `atelier git pr-guard`(legacy alias) 포함 → PR Guard
3. **대상 선택** — AskUserQuestion: [Write/Edit Guard] [Commit Guard] [PR Guard] [모두] [취소]
4. **액션 선택** — AskUserQuestion: [비활성화] [재설정] [취소]
   - **비활성화**: 대상 hook 마다 `atelier git hook unregister PreToolUse "<Step 2에서 찾은 command 문자열 그대로>" [--project-dir "$HOME"]`
   - **재설정**: 비활성화와 동일하게 unregister 후, §"git 모듈"의 등록 형식(guard 2종) / PR Guard 는 `atelier git hook register PreToolUse "Bash" 'atelier git guard pr' --timeout=10 [--project-dir "$HOME"]` 형식으로 재등록
5. **결과 출력**: 제거/갱신된 settings 경로와 항목을 안내하고, 재활성화는 모듈 설치(Step 1)로 가능함을 알립니다.

> unregister 의 command 인자는 **list 에서 발견된 문자열 그대로** 사용합니다 (legacy `pr-guard` 설치분 포함 — 추측으로 새 형식을 만들지 않음).

## 에러 처리

**ensure-binary 실패 (cargo 미설치 등):**
- 이후 Step 을 중단하고 Rust toolchain 설치를 안내합니다 (`rustup`)

**기본 브랜치 감지 실패 (remote 미설정):**
- `--default-branch` 없이 guard 를 등록하고, 비표준 기본 브랜치 repo 에서는 보호가 제한될 수 있음을 안내합니다

**settings.json 이 깨진 JSON 인 경우:**
- `hook register` 가 덮어쓰기를 거부하고 에러를 반환합니다 — 사용자에게 파일 상태를 보여주고 수동 복구를 안내합니다

## Output Examples

**등록 성공:**
```json
{ "action": "created", "command": "atelier git guard commit --project-dir \"${CLAUDE_PROJECT_DIR:-.}\" --default-branch main" }
```

**재실행 (멱등):**
```json
{ "action": "updated", "command": "atelier git guard commit --project-dir \"${CLAUDE_PROJECT_DIR:-.}\" --default-branch main" }
```
