---
description: 프로젝트에 코드 스타일 룰(언어 무관 base + 언어별 + 앱 성격별)을 설치·갱신·점검하고, 룰·언어·앱 성격을 빼고 되살립니다
argument-hint: "[install|update|check|restore|uninstall]"
allowed-tools: ["Bash", "AskUserQuestion"]
---

# atelier code-style

코드 스타일 룰을 **현재 프로젝트**의 `.claude/rules/atelier/code-style/` 에 설치하고 최신으로 유지합니다. 베이스 룰(`/atelier:setup`)과 달리 user scope 가 아니라 프로젝트 scope 입니다. 언어·앱 성격은 프로젝트마다 다르고, 팀과 함께 커밋해 공유하기 때문입니다.

복사·갱신·점검은 전부 `atelier code-style` CLI 가 합니다. 이 커맨드는 언어와 앱 성격을 고르고, 덮어쓰거나 지우기 전에 확인을 받고, CLI 를 호출합니다. 파일을 직접 복사하거나 고치지 않습니다.

## 세 층

| 층 | 원본 | 설치 위치 | 언제 설치되나 |
|---|---|---|---|
| base | `${CLAUDE_PLUGIN_ROOT}/rules/code-style/base/` | `<프로젝트>/.claude/rules/atelier/code-style/base/` | 항상 (언어를 하나라도 설치하면) |
| 언어 | `${CLAUDE_PLUGIN_ROOT}/rules/code-style/lang/<언어>/` | `<프로젝트>/.claude/rules/atelier/code-style/<언어>/` | `--lang <언어>` |
| 앱 성격 | `${CLAUDE_PLUGIN_ROOT}/rules/code-style/app/<성격>/` | `<프로젝트>/.claude/rules/atelier/code-style/<성격>/` | `--app <성격>` |

- 모든 파일은 **그대로 복사**됩니다. base·앱 성격 룰은 지원하는 모든 언어의 경로를 `paths:` 에 미리 적어 두었습니다. 프로젝트에 없는 언어의 경로는 로드될 일이 없어 해가 없습니다.
- 고를 수 있는 언어는 `lang/` 아래, 앱 성격은 `app/` 아래 디렉토리 이름입니다 (예: 언어 `go`·`rust`, 앱 성격 `cli`·`library`).
- base·앱 성격 룰과 언어 룰이 충돌하면 base·앱 성격 룰이 우선합니다.
- 룰은 **파일 이름으로 나눠 로드**됩니다. 언어의 모든 파일에는 작은 핵심 룰만 로드되고, 역할별 룰은 그 역할의 파일 이름(예: `*_test.go`, `main.rs`, `config.go`, `*_client.go`)을 Read 로 열 때만 로드됩니다. 파일 이름 컨벤션은 함께 설치되는 `<언어>-file-naming` 룰이 알려 줍니다. 기존 프로젝트의 파일 이름이 다르면 역할 룰이 로드되지 않으니, 설치 뒤 사용자에게 이 점을 안내합니다.
- **설치된 것은 디렉토리로 바로 보입니다**: `base/` 옆의 디렉토리 = 설치된 언어·앱 성격, 파일 = 설치된 룰. `installed.json` 은 설치된 언어·앱 성격과 프로젝트에 건넨 룰을 기록합니다.

### 제어 방법

| 하고 싶은 것 | 방법 |
|---|---|
| 언어·앱 성격 추가 | `install --lang <언어> [--app <성격>]` (Step 2a) |
| 룰 하나 빼기 | 그 사본 파일을 지웁니다. 이후 `EXCLUDED` 로 보고되고 update 가 되살리지 않습니다 |
| 뺀 룰 되살리기 | `restore --rule <그룹>/<파일>` (Step 2d) |
| 언어·앱 성격 빼기 | `uninstall --lang <언어>` / `uninstall --app <성격>` (Step 2e). 디렉토리를 지우는 것은 그 안의 룰을 전부 뺀 것으로 봅니다 |

## Step 0 — atelier CLI 보장

```bash
bash "${CLAUDE_PLUGIN_ROOT}/scripts/ensure-binary.sh"
```

실패하면(cargo 부재 등) 이후 Step 을 진행하지 않고 에러를 안내합니다.

## Step 1 — 동작 선택

인자(`install`·`update`·`check`·`restore`·`uninstall`)가 있으면 그대로 씁니다. 사용자가 자연어로 "이 룰 빼 줘", "다시 넣어 줘", "CLI 룰도 넣어 줘", "Go 룰 지워 줘" 처럼 말하면 위 제어 방법 표로 동작을 고릅니다. 둘 다 아니면 먼저 점검합니다:

```bash
atelier code-style check --plugin-root "${CLAUDE_PLUGIN_ROOT}" --project-dir "${CLAUDE_PROJECT_DIR:-.}"
```

- exit 2 이고 stderr 가 `Error: no code-style language installed` 로 시작하면 설치된 언어가 없는 것입니다. `install` 로 진행합니다.
- `DRIFTED`·`NEW`·`ORPHANED` 가 있으면 `update` 로 진행합니다.
- 모두 `OK`·`EXCLUDED` 면 최신 상태임을 알리고 끝냅니다. `EXCLUDED` 가 있으면 그 룰 이름을 함께 보여 줍니다. 다른 언어·앱 성격을 추가하고 싶은지 AskUserQuestion 으로 묻고, 원하면 `install` 로 진행합니다.

## Step 2a — install

1. AskUserQuestion 으로 고릅니다 (둘 다 multiSelect):
   - 언어: `${CLAUDE_PLUGIN_ROOT}/rules/code-style/lang/` 아래 디렉토리 이름 (예: `[go]` `[rust]`). 하나 이상 골라야 합니다.
   - 앱 성격: `${CLAUDE_PLUGIN_ROOT}/rules/code-style/app/` 아래 디렉토리 이름 (예: `[cli]` `[library]`). 고르지 않아도 됩니다. CLI 도구면 `cli`, 다른 프로젝트가 가져다 쓰는 코드면 `library` 를 권합니다.
2. 고른 것마다 `--lang`·`--app` 을 붙여 한 번에 실행합니다:
   ```bash
   atelier code-style install --plugin-root "${CLAUDE_PLUGIN_ROOT}" --project-dir "${CLAUDE_PROJECT_DIR:-.}" --lang go --app cli
   ```
3. 설치한 파일 경로를 보고하고, 팀과 공유하려면 `.claude/rules/atelier/code-style/` 를 커밋하라고 안내합니다. 커밋은 하지 않습니다.

## Step 2b — update

1. Step 1 의 `check` 결과를 사용자에게 보여 줍니다.
2. `DRIFTED` 나 `ORPHANED` 가 있으면 AskUserQuestion 으로 확인합니다. 프로젝트에서 의도적으로 고친 룰일 수 있습니다:
   - 질문: "프로젝트의 코드 스타일 룰이 플러그인 원본과 다릅니다. 원본으로 갱신할까요? (바뀐 파일은 백업 후 덮어쓰고, 원본에서 없어진 룰은 백업 후 지웁니다)"
   - 선택지: `[갱신 (백업 후 적용)]` `[건너뛰기 (현재 상태 유지)]`
   - 질문할 수 없는 컨텍스트(자동화·headless)에서는 **묻지 않고 건너뛰기**로 처리합니다.
3. `NEW` 만 있으면 묻지 않고 진행합니다. 덮어쓸 파일이 없습니다.
4. 갱신을 고르면 실행합니다:
   ```bash
   atelier code-style update --plugin-root "${CLAUDE_PLUGIN_ROOT}" --project-dir "${CLAUDE_PROJECT_DIR:-.}"
   ```
   `--lang`·`--app` 을 둘 다 생략하면 설치된 언어·앱 성격 전부를 갱신합니다. 지정하면 그것과 base 만 갱신합니다. `EXCLUDED` 룰은 되살리지 않습니다.

이전 버전으로 설치한 프로젝트(`common/` 디렉토리가 있고 `installed.json` 이 없음)는 첫 update 에서 `common/*` 이 `ORPHANED`, `base/*` 가 `NEW` 로 나옵니다. 공통 룰이 `base/` 로 옮겨졌기 때문이며 정상입니다. 갱신하면 `common/` 사본은 백업 후 지워지고 `base/` 가 설치되며 기록 파일이 생깁니다.

## Step 2c — check

Step 1 의 점검만 하고 결과를 보고합니다. 파일을 바꾸지 않습니다.

## Step 2d — restore

뺀 룰(`EXCLUDED`)을 되살립니다. 룰 이름은 `check` 출력의 `code-style/` 뒤 부분입니다 (예: `base/feature-flags.md`, `cli/cli-output.md`).

```bash
atelier code-style restore --plugin-root "${CLAUDE_PLUGIN_ROOT}" --project-dir "${CLAUDE_PROJECT_DIR:-.}" --rule base/feature-flags.md
```

사본이 이미 있는 룰은 건드리지 않습니다. 묻지 않고 진행합니다.

## Step 2e — uninstall

언어나 앱 성격을 뺍니다. 그 사본을 백업 후 지웁니다. 마지막 언어를 빼면 base·앱 성격 룰도 백업 후 지우고 `installed.json` 을 지웁니다.

1. AskUserQuestion 으로 확인합니다: "`<언어·앱 성격>` 코드 스타일 룰을 프로젝트에서 뺄까요? (사본은 백업 후 지웁니다)" — `[빼기]` `[취소]`. 마지막 언어라면 base·앱 성격 룰도 함께 지워진다고 덧붙입니다. 질문할 수 없는 컨텍스트에서는 실행하지 않습니다.
2. 실행합니다:
   ```bash
   atelier code-style uninstall --plugin-root "${CLAUDE_PLUGIN_ROOT}" --project-dir "${CLAUDE_PROJECT_DIR:-.}" --app cli
   ```

## STATUS 와 종료 코드

| STATUS | 뜻 |
|---|---|
| `OK` | 원본과 같음 |
| `DRIFTED` | 원본과 다름 |
| `NEW` | 원본에 새로 생긴 룰이 프로젝트에 없음 |
| `ORPHANED` | 원본에서 없어진 룰의 사본이 프로젝트에 남음 |
| `EXCLUDED` | 프로젝트에 건넸던 룰을 프로젝트가 지움 — 일부러 뺀 룰 |

- check: exit `0` 갱신할 것 없음 (`OK`·`EXCLUDED` 만) / `1` `DRIFTED`·`NEW`·`ORPHANED` 발견 / `2` CLI 오류.
- install·update·restore·uninstall: exit `0` 성공 / `2` CLI 오류.
- **exit 1 은 실패가 아니라 update 의 입력입니다.**

## 에러 처리

- **ensure-binary 실패** → Rust 툴체인 설치(https://rustup.rs/)를 안내하고 끝냅니다.
- **exit 2** → stderr 원문을 보여 줍니다. 파일을 직접 고치지 않습니다.
  - `unknown language: <X> — expected one of: …` · `unknown app: <X> — expected one of: …` → 고를 수 있는 것을 다시 안내합니다.
  - `at least one --lang is required` → install 에는 언어가 하나 이상 필요합니다.
  - `no code-style language installed in <경로>` → 설치된 언어가 없습니다 (Step 1 참고).
  - `language not installed: <X> (installed: …)` · `app not installed: <X> (installed: …)` → uninstall 할 수 있는 것을 안내합니다.
  - `<룰>: <그룹> is not installed` · `unknown rule: <룰>` · `invalid rule: <룰>` → `check` 출력의 룰 이름으로 다시 고르게 안내합니다.
  - `plugin source directory not found: <경로>` · `<이름> is both a language and an app in the plugin sources` → 플러그인 설치가 깨졌습니다. 플러그인 재설치를 안내합니다.
  - `<경로>/installed.json: …` → 설치 기록이 깨졌습니다. 손으로 고쳤다면 되돌리거나, 지운 뒤 `update` 로 다시 만들도록 안내합니다 (지우면 뺀 룰 기록도 사라집니다).
  - `<파일> is not valid UTF-8` → 그 사본의 인코딩을 고치거나 지운 뒤 다시 실행하도록 안내합니다.

## Output Examples

**check — 플러그인 업데이트 뒤 바뀐 룰·새 룰·뺀 룰이 있음 (exit 1):**

```
code-style/base/cohesion.md=OK
code-style/base/comments.md=DRIFTED (/work/my-repo/.claude/rules/atelier/code-style/base/comments.md)
code-style/base/feature-flags.md=EXCLUDED
code-style/go/go-errors.md=OK
code-style/go/go-generics.md=NEW (/work/my-repo/.claude/rules/atelier/code-style/go/go-generics.md)
code-style/cli/cli-output.md=OK
→ 6 checked, 1 drifted, 1 new, 0 orphaned, 1 excluded
```

**install --lang go --app cli (exit 0):**

```
installed: /work/my-repo/.claude/rules/atelier/code-style/base/cohesion.md
installed: /work/my-repo/.claude/rules/atelier/code-style/go/go-principles.md
installed: /work/my-repo/.claude/rules/atelier/code-style/cli/cli-output.md
→ 3 installed, 0 synced, 0 removed, 0 unchanged, 0 excluded
```

**update — 이전 버전 설치본 (exit 0):**

```
installed: /work/my-repo/.claude/rules/atelier/code-style/base/cohesion.md
removed: /work/my-repo/.claude/rules/atelier/code-style/common/cohesion.md (backup: /work/my-repo/.claude/rules/atelier/code-style/common/cohesion.md.bak-20261103-101500)
→ 1 installed, 0 synced, 1 removed, 2 unchanged, 0 excluded
```

→ 보고 예: "공통 룰이 `base/` 로 옮겨져서 `common/cohesion.md` 를 백업 후 지우고 `base/cohesion.md` 를 설치했어요. 팀과 공유하려면 `.claude/rules/atelier/code-style/` 를 커밋해 주세요."

**restore --rule base/feature-flags.md (exit 0):**

```
installed: /work/my-repo/.claude/rules/atelier/code-style/base/feature-flags.md
→ 1 installed, 0 synced, 0 removed, 0 unchanged, 0 excluded
```

**uninstall --app cli (exit 0):**

```
removed: /work/my-repo/.claude/rules/atelier/code-style/cli/cli-output.md (backup: /work/my-repo/.claude/rules/atelier/code-style/cli/cli-output.md.bak-20261103-103000)
→ 0 installed, 0 synced, 1 removed, 0 unchanged, 0 excluded
```

**uninstall — 설치되지 않은 언어 (exit 2):**

```
Error: language not installed: rust (installed: go)
```
