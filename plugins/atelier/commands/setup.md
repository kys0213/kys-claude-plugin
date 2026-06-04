---
name: setup
description: atelier 통합 환경을 초기화합니다 (git / autopilot / style 모듈 선택 + 기존 hook 마이그레이션)
allowed-tools: ["Bash", "Read", "Write", "Edit", "AskUserQuestion"]
---

# atelier setup

흡수된 6개 plugin(git-utils, github-autopilot, coding-style, ...)의 설정을 단일 진입점으로 통합합니다.
모듈을 선택해 설치하고, 기존 frozen plugin 경로로 등록된 hook 을 atelier 경로로 마이그레이션합니다.

> ⚠️ 모든 hook 은 user scope(`~/.claude/settings.json`)에 등록됩니다.
> hook 명령은 `${CLAUDE_PLUGIN_ROOT}/hooks/<file>.sh` 형태로, atelier plugin 컨텍스트에서 해석됩니다.
>
> ⚠️ **`${CLAUDE_PLUGIN_ROOT}` 리터럴 보존 (필수)**: settings.json 에 기록할 때
> `${CLAUDE_PLUGIN_ROOT}` 문자열을 **리터럴 그대로** 써야 합니다. 가독성/검증을 위해 실제
> 절대경로(`/Users/.../plugins/cache/.../<version>/hooks/...`)로 **절대 풀어 쓰지 마세요.**
> 변수를 expand 해서 박으면 plugin 업데이트로 버전 디렉토리가 바뀔 때 hook 경로가 깨져 매
> 도구 호출마다 `No such file or directory` hook error 가 발생합니다. `${CLAUDE_PLUGIN_ROOT}`
> 해석은 Claude Code 런타임에 맡깁니다.

## Step 1 — 설치 모듈 선택

`AskUserQuestion` 으로 설치할 모듈을 선택합니다 (multiSelect).

| 선택 | 수행 동작 | 출처 |
|---|---|---|
| `git` | GitHub 인증 확인 + `~/.git-workflow-env` 생성 + Default Branch Guard hook | git-utils setup |
| `autopilot` | `github-autopilot.local.md` 생성 + autopilot hook 3개 등록 + atelier CLI 빌드/설치 | github-autopilot setup |
| `style` | `~/.claude/CLAUDE.md` 코딩 원칙 + Stop hook 등록 | coding-style setup |
| `all` | 위 세 가지 전부 | 신규 |

선택된 모듈만 아래 해당 Step 을 수행합니다.

## Step 2a — git 모듈

1. GitHub CLI 인증 확인:
   ```bash
   gh auth status || gh auth login
   ```
2. 환경 설정 파일 생성 (기존 git-utils 와 동일 스키마, 경로 `~/.git-workflow-env`).
3. Default Branch Guard hook 등록 — `~/.claude/settings.json` 의 `PreToolUse` 에 추가:
   ```json
   { "command": "${CLAUDE_PLUGIN_ROOT}/scripts/default-branch-guard-hook.sh" }
   ```

## Step 2b — autopilot 모듈

1. 프로젝트 설정 파일 `github-autopilot.local.md` 생성 (기존 스키마/경로 동일 — 호환).
2. atelier CLI 빌드/설치 (단일 `atelier` 바이너리):
   ```bash
   cargo build --release --manifest-path "${CLAUDE_PLUGIN_ROOT}/cli/Cargo.toml"
   # 빌드된 바이너리를 PATH 에 링크 (예: ~/.local/bin/atelier)
   ```
3. autopilot hook 3종을 `~/.claude/settings.json` 에 등록:
   ```json
   {
     "SessionStart": [{ "command": "${CLAUDE_PLUGIN_ROOT}/hooks/check-cli-version.sh" }],
     "PreToolUse":   [
       { "command": "${CLAUDE_PLUGIN_ROOT}/hooks/guard-pr-base.sh" },
       { "command": "${CLAUDE_PLUGIN_ROOT}/hooks/protect-stagnation.sh" }
     ]
   }
   ```

> autopilot SQLite store(ledger/task DB)는 기존 스키마/경로를 계승하므로 마이그레이션 불필요.

## Step 2c — style 모듈

1. `~/.claude/CLAUDE.md` 에 코딩 원칙 템플릿 병합 (워터마크 기반 중복 확인 — 기존 coding-style 로직 동일):
   - 템플릿 원본: `${CLAUDE_PLUGIN_ROOT}/templates/claude-md/CLAUDE.md`
2. Stop hook 등록:
   ```json
   { "Stop": [{ "command": "${CLAUDE_PLUGIN_ROOT}/hooks/suggest-simplify.sh" }] }
   ```

## Step 3 — 기존 hook 마이그레이션 (frozen → atelier)

기존 6개 plugin 사용자는 `~/.claude/settings.json` 에 **frozen plugin 경로**의 hook 이 박혀 있습니다.
이를 atelier 경로로 재작성합니다. (상세: `plans/atelier/03-migration.md §A.3`)

```
1. ~/.claude/settings.json 읽기 (없으면 skip)
2. hooks 의 모든 command 문자열 순회
3. 다음 정규식에 매칭되는 entry 수집:
     .*/plugins/(github-autopilot|coding-style)/hooks/(<file>)\.sh
     .*/plugins/git-utils/scripts/(default-branch-guard.*)\.sh
4. 매칭 entry 를 atelier 경로로 치환:
     hooks/<file>.sh   → ${CLAUDE_PLUGIN_ROOT}/hooks/<file>.sh
     scripts/<file>.sh → ${CLAUDE_PLUGIN_ROOT}/scripts/<file>.sh
5. 중복 제거: 같은 hook 이 frozen + atelier 양쪽에 있으면 atelier 만 남김
6. 변경 전 settings.json 을 settings.json.bak-<timestamp> 로 백업
7. 사용자에게 diff 를 보여주고 AskUserQuestion 으로 확인 후 기록
```

> **멱등성**: 이미 atelier 경로로 재작성된 settings.json 에 재실행하면 변경 0건이어야 합니다.

> **등록 검증 (필수)**: 기록 직후 `~/.claude/settings.json` 을 다시 읽어, 각 hook command 에
> **리터럴 `${CLAUDE_PLUGIN_ROOT}`** 가 포함되어 있는지 확인합니다. 절대경로(`/Users/...`,
> `/home/...`)로 박혀 있으면 `${CLAUDE_PLUGIN_ROOT}/hooks/<file>.sh` 형태로 다시 교체한 뒤
> 저장합니다. 검증 실패 시 설치 성공 메시지를 출력하지 않습니다. `atelier autopilot preflight`
> 가 hook 누락을 경고하면 settings.json 을 수동 편집하지 말고 `/atelier:setup` 을 재실행하세요.

치환 대상:

| frozen 경로 | atelier 경로 |
|---|---|
| `github-autopilot/hooks/check-cli-version.sh` | `atelier/hooks/check-cli-version.sh` |
| `github-autopilot/hooks/guard-pr-base.sh` | `atelier/hooks/guard-pr-base.sh` |
| `github-autopilot/hooks/protect-stagnation.sh` | `atelier/hooks/protect-stagnation.sh` |
| `coding-style/hooks/suggest-simplify.sh` | `atelier/hooks/suggest-simplify.sh` |
| `git-utils/scripts/default-branch-guard*.sh` | `atelier/scripts/default-branch-guard*.sh` |

## Step 4 — CLI alias (선택)

기존 `autopilot` / `git-utils` 호출 호환을 위해 셸 rc 에 alias 추가를 **AskUserQuestion 으로 동의받은 경우에만** 제안합니다:

```bash
alias autopilot='atelier autopilot'
alias git-utils='atelier git'
```

거부 시 안내 문구만 출력합니다. 기존 바이너리는 setup 이 삭제하지 않습니다 (외부 도구를 함부로 지우지 않음).
