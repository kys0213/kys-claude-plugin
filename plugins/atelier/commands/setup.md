---
description: atelier 통합 환경을 초기화합니다 (git / style / workflow 모듈 선택 + 기존 hook 마이그레이션 + guard hook 관리)
argument-hint: ""
allowed-tools: ["Bash", "Read", "Write", "Edit", "AskUserQuestion"]
---

# atelier setup

흡수된 plugin(git-utils, coding-style, workflow-guide 등)의 설정을 단일 진입점으로 통합합니다.
모듈을 선택해 설치하고, 기존 frozen plugin 경로로 등록된 hook 을 atelier 로 마이그레이션합니다.

> ⚠️ 모든 hook 은 user scope(`~/.claude/settings.json`)에 등록됩니다.
> 등록은 LLM 이 settings.json 을 직접 편집하지 않고 **`atelier git hook register` CLI** 로 수행합니다
> (`.claude/rules/tool-layer-boundary.md`). `--project-dir "$HOME"` 을 주면 `~/.claude/settings.json` 에 기록됩니다.

setup 이 settings.json 에 등록하는 hook 은 **CLI 직접 호출 형태뿐**입니다 (`atelier git guard write ...` — 바이너리가 PATH 에서 해석되므로 버전 비의존). 이는 setup 시점에 프로젝트별 값(예: `--default-branch <감지값>`)을 주입해야 하기 때문입니다.

> 플러그인에 번들된 `.sh` hook(`check-cli-version`·`suggest-simplify`)은 플러그인이 `hooks/hooks.json` 으로 직접 선언합니다. 둘 다 비차단 advisory 라 모든 세션에 적용돼도 안전합니다. `${CLAUDE_PLUGIN_ROOT}` 가 hook 실행 시점에 활성 버전으로 해석돼 frozen 이 없습니다 (`.claude/rules/tool-layer-boundary.md`).

## Step 0 — atelier CLI 보장 (공통 선행)

모든 모듈이 hook 등록에 CLI 를 사용하므로 가장 먼저 실행합니다:

```bash
bash "${CLAUDE_PLUGIN_ROOT}/scripts/ensure-binary.sh"
```

- plugin.json 버전과 설치된 `atelier --version` 을 SemVer 비교해 필요 시 빌드/설치합니다 (`~/.local/bin/atelier`)
- 실패하면(cargo 부재 등) 이후 Step 을 진행하지 말고 에러를 안내합니다

> Step 0 은 *setup 시점* 바이너리를 보장하고, plugin-declared SessionStart hook(`check-cli-version`)이
> *이후 버전 드리프트*를 알립니다 — 둘이 한 쌍입니다.

## Step 1 — 설치 모듈 선택

`AskUserQuestion` 으로 설치할 모듈을 선택합니다 (multiSelect).

| 선택 | 수행 동작 | 출처 |
|---|---|---|
| `git` | GitHub 인증 확인 + `~/.git-workflow-env` 생성 + Default Branch Guard hook | git-utils setup |
| `style` | `~/.claude/CLAUDE.md` 코딩 원칙 병합 | coding-style setup |
| `workflow` | `.claude/rules/agent-design-principles.md` 룰 설치 | workflow-guide install |
| `notify` | 대기 알림 채널 설정 (`~/.claude/atelier-notify.json`) | 신규 |
| `all` | 위 네 가지 전부 | 신규 |

선택된 모듈만 아래 해당 Step 을 수행합니다.

> 이미 설치된 환경에서 guard hook 만 비활성화/재설정하려면 Step 5 (hook 관리 모드)로 바로 진행합니다.

## Step 2a — git 모듈

1. GitHub CLI 인증 확인:
   ```bash
   gh auth status || gh auth login
   ```
2. 환경 설정 파일 생성 (기존 git-utils 와 동일 스키마, 경로 `~/.git-workflow-env`).
3. **기본 브랜치 감지 + warm-up** — guard 는 읽기전용이라 비표준 기본 브랜치(예: `trunk`)를 런타임에 감지하지 못할 수 있으므로,
   1회성인 setup 시점에 두 가지를 합니다 (#785, #779). **둘 다 보호 대상 프로젝트 repo 를 기준으로 실행**해야 합니다 —
   가드 런타임이 `--project-dir "${CLAUDE_PROJECT_DIR:-.}"` 로 그 repo 의 `origin/HEAD` 를 읽기 때문입니다(#780). setup 의
   cwd 가 다른 repo($HOME·multi-repo workspace)이면 엉뚱한 repo 를 warm 하거나 그 repo 의 기본 브랜치를 박게 됩니다:
   ```bash
   PROJECT_DIR="${CLAUDE_PROJECT_DIR:-.}"   # 가드 런타임과 동일한 앵커
   # (a) #779 warm-up — 프로젝트 repo 의 origin/HEAD 를 채워, bake 가 비어도 guard 런타임 readonly 감지가
   #     비표준 기본 브랜치(trunk 등)·비-GitHub remote 에서도 동작하게 한다.
   #     gh/인증은 불필요하지만 --auto 는 origin 에 1회 질의하므로 remote 가 닿아야 한다(오프라인이면 no-op).
   git -C "$PROJECT_DIR" remote set-head origin --auto 2>/dev/null || true
   # (b) 명시적 pin 용 값 — GitHub repo 면 gh 로 조회해 아래에서 --default-branch 로 박는다. gh 는 cwd 의
   #     remote 로 repo 를 추론하므로 프로젝트 repo 안에서 실행한다.
   #     실패(인증 안 됨·비-GitHub·gh 없음)는 빈 값으로 두고 setup 은 계속 진행 — 빈 값이면 플래그 생략.
   DEFAULT_BRANCH=$( (cd "$PROJECT_DIR" && gh repo view --json defaultBranchRef -q .defaultBranchRef.name) 2>/dev/null || true )
   ```
   gh 조회가 실패해 `--default-branch` 를 **생략**해도, (a) 의 warm-up 덕분에 guard 의 런타임 readonly 감지(`origin/HEAD` → main/develop/master)가 기본 브랜치를 해결한다.
4. Default Branch Guard hook 2종 등록 — `.sh` 경로가 아니라 **CLI 커맨드를 직접** 기록합니다.
   감지값이 비어있으면 `--default-branch` 플래그 자체를 빼야 합니다 (빈 플래그를 박으면 hook 실행 시 clap 이
   값 누락으로 exit 2 → 모든 편집 차단되거나, 빈 브랜치로 guard 가 무력화됨):
   ```bash
   # 비어있으면 플래그 생략, 값이 있으면 ' --default-branch <값>' 만 덧붙임
   DB_FLAG=""
   [ -n "$DEFAULT_BRANCH" ] && DB_FLAG=" --default-branch $DEFAULT_BRANCH"

   atelier git hook register PreToolUse "Write|Edit" \
     'atelier git guard write --project-dir "${CLAUDE_PROJECT_DIR:-.}"'"$DB_FLAG" \
     --project-dir "$HOME"

   atelier git hook register PreToolUse "Bash" \
     'atelier git guard commit --project-dir "${CLAUDE_PROJECT_DIR:-.}"'"$DB_FLAG" \
     --project-dir "$HOME"
   ```
   > `${CLAUDE_PROJECT_DIR:-.}` 는 **리터럴로 보존**해야 합니다 (hook 실행 시점에 셸이 expand).
   > 반면 `$DB_FLAG`(감지값) 는 setup 시점에 expand 해서 기록합니다 — 빈 값이면 플래그가 통째로 빠집니다.

   결과적으로 settings.json 에는 다음과 같이 기록됩니다:
   ```json
   { "type": "command", "command": "atelier git guard commit --project-dir \"${CLAUDE_PROJECT_DIR:-.}\" --default-branch main" }
   ```

## Step 2b — workflow 모듈

`workflow` skill 의 §"설계 원칙 룰 설치" 절차를 수행합니다:

1. `.claude/rules/` 디렉토리가 없으면 생성, 대상 파일이 존재하면 덮어쓸지 AskUserQuestion 으로 확인
2. `${CLAUDE_PLUGIN_ROOT}/rules/agent-design-principles.md` 를 **내용 수정 없이 그대로** `.claude/rules/agent-design-principles.md` 에 복사

## Step 2c — style 모듈

`~/.claude/CLAUDE.md` 에 코딩 원칙 템플릿을 병합합니다 (워터마크 기반 중복 확인 — 기존 coding-style 로직 동일).

- 템플릿 원본: `${CLAUDE_PLUGIN_ROOT}/templates/claude-md/CLAUDE.md`

## Step 2d — notify 모듈

자율주행/백그라운드 세션이 입력(질문·도구 권한)을 기다리기 시작하는 순간을 밖에서 캐치할
채널을 설정합니다. 훅 자체는 플러그인이 `hooks/hooks.json` 으로 이미 선언하므로(advisory,
채널 미설정 시 무음 no-op) **채널 설정 파일만 만들면 활성화**됩니다.

1. `AskUserQuestion` 으로 채널을 선택합니다 (multiSelect):

   | 선택 | 용도 | 필요 입력 |
   |---|---|---|
   | `desktop` | 같은 머신에서 딴 작업 중일 때 OS 알림 배너 (macOS osascript / Linux notify-send) | 없음 |
   | `slack` | 자리에 없을 때 폰/Slack 으로 push | Incoming Webhook URL (Other 로 입력받음) |
   | `file` | 워처 세션이 Monitor(`tail -F`)로 반응하는 로컬 JSONL 싱크 | 없음 (기본 경로 사용) |

2. 선택 결과로 **글로벌 설정** `~/.claude/atelier-notify.json` 을 작성합니다
   (모든 프로젝트의 세션에 적용 — 프로젝트별로 다르게 하려면 `<project>/.claude/atelier-notify.json` 이 우선):

   ```json
   {
     "channels": [
       { "type": "desktop" },
       { "type": "slack", "webhookUrl": "<입력받은 URL>" },
       { "type": "file", "path": "~/.claude/atelier-notify/events.jsonl" }
     ]
   }
   ```

   선택하지 않은 채널 항목은 넣지 않습니다. 기존 파일이 있으면 덮어쓸지 AskUserQuestion 으로 확인합니다.

3. 검증 — 테스트 페이로드를 흘려 각 채널 리포트가 `"ok": true` 인지 확인합니다:

   ```bash
   echo '{"cwd":"'$(pwd)'","tool_input":{"questions":[{"question":"notify setup test"}]}}' \
     | atelier notify ask-question
   ```

**에러 처리:**

- Slack webhook URL 이 비어 있거나 `https://` 로 시작하지 않으면 재입력을 요청합니다.
- 검증 리포트에 `"ok": false` 가 있으면 해당 채널의 `error` 메시지를 보여주고
  (예: Linux 에서 `notify-send` 미설치 → `libnotify` 설치 안내), 설정 파일은 그대로 둡니다 —
  hook 은 advisory 라 실패해도 세션에 영향이 없습니다.
- webhook URL 은 시크릿입니다. 프로젝트 설정 파일로 만들 경우 `.gitignore` 에 포함됐는지 확인합니다.

## Step 3 — 기존 hook 마이그레이션 (frozen → atelier)

기존 6개 plugin 사용자는 `~/.claude/settings.json` 에 **frozen plugin 경로**의 hook 이 박혀 있습니다.
이를 atelier 로 재작성합니다. (상세: `plans/atelier/03-migration.md §A.3`)

```
1. atelier git hook list --project-dir "$HOME" 으로 현재 등록 현황 조회 (없으면 skip)
2. 출력의 모든 command 문자열 순회, 다음 정규식에 매칭되는 entry 수집:
     .*/plugins/(github-autopilot|coding-style)/hooks/(<file>)\.sh
     .*/plugins/git-utils/scripts/(default-branch-guard.*)\.sh
     .*/plugins/atelier/scripts/(default-branch-guard.*)\.sh   # 구버전 atelier setup 잔재
     .*/atelier/[^/]*/hooks/(check-cli-version|guard-pr-base|protect-stagnation|suggest-simplify)\.sh   # 구버전 atelier setup 이 frozen 버전경로로 박은 .sh shim (이제 plugin-declared 또는 #776 에서 삭제됨 → "제거만")
3. 변경 전 ~/.claude/settings.json 을 settings.json.bak-<timestamp> 로 백업 (cp)
4. 사용자에게 치환 목록을 보여주고 AskUserQuestion 으로 확인
5. 매칭 entry 마다: atelier git hook unregister <type> <old-command> --project-dir "$HOME"
   → 아래 표의 대응 command 로 atelier git hook register (hook register 는 command 기준
   중복 제거를 하므로 frozen + atelier 양쪽에 있던 hook 도 한 개만 남음).
   단, 표에서 "제거만" 으로 표시된 hook 은 plugin-declared 로 대체됐으므로 unregister 만
   하고 재등록하지 않는다 (재등록 시 hooks.json 선언과 SessionStart 이중 실행)
```

> **멱등성**: 이미 atelier 로 재작성된 settings.json 에 재실행하면 변경 0건이어야 합니다.

치환 대상:

| frozen 경로 | atelier 등록 command |
|---|---|
| `github-autopilot/hooks/check-cli-version.sh` | **제거만** (재등록 안 함) — 플러그인이 `hooks/hooks.json` 으로 직접 선언 |
| `github-autopilot/hooks/guard-pr-base.sh` | **제거만** (재등록 안 함) — 스크립트 삭제됨 (#776) |
| `github-autopilot/hooks/protect-stagnation.sh` | **제거만** (재등록 안 함) — 스크립트 삭제됨 (#776) |
| `coding-style/hooks/suggest-simplify.sh` | **제거만** (재등록 안 함) — 플러그인이 `hooks/hooks.json` 으로 직접 선언 |
| `git-utils/scripts/default-branch-guard-hook.sh` (또는 구버전 atelier 동명 스크립트) | `atelier git guard write ...` (§"git 모듈" 등록 형식) |
| `git-utils/scripts/default-branch-guard-commit-hook.sh` (또는 구버전 atelier 동명 스크립트) | `atelier git guard commit ...` (§"git 모듈" 등록 형식) |

## Step 4 — CLI alias (선택)

기존 `git-utils` 호출 호환을 위해 셸 rc 에 alias 추가를 **AskUserQuestion 으로 동의받은 경우에만** 제안합니다:

```bash
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
