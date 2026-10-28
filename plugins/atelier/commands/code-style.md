---
description: 프로젝트에 언어별 코드 스타일 룰(Go·Rust, Google 스타일 가이드 기반 + 언어 공통 취향 룰)을 설치하거나 갱신합니다
argument-hint: "[install|update|check]"
allowed-tools: ["Bash", "AskUserQuestion"]
---

# atelier code-style

언어별 코드 스타일 룰을 **현재 프로젝트**의 `.claude/rules/atelier/code-style/` 에 설치하고 최신으로 유지합니다. 베이스 룰(`/atelier:setup`)과 달리 user scope 가 아니라 프로젝트 scope 입니다. 언어 룰은 프로젝트 성격에 따르고, 팀과 함께 커밋해 공유하기 때문입니다.

복사·갱신·점검은 전부 `atelier code-style` CLI 가 합니다. 이 커맨드는 언어를 고르고, 덮어쓰기 전에 확인을 받고, CLI 를 호출합니다. 파일을 직접 복사하거나 고치지 않습니다.

| 원본 | 설치 위치 |
|---|---|
| `${CLAUDE_PLUGIN_ROOT}/rules/code-style/<언어>/*.md` | `<프로젝트>/.claude/rules/atelier/code-style/<언어>/*.md` |
| `${CLAUDE_PLUGIN_ROOT}/rules/code-style/common/*.md` | `<프로젝트>/.claude/rules/atelier/code-style/common/*.md` |

- 언어 하나라도 설치하면 `common/`(언어 공통 취향 룰)도 함께 설치됩니다. 언어 룰과 함께 로드돼 같이 적용됩니다.
- common 룰과 언어 룰(Google 가이드)이 충돌하면 common 룰이 우선합니다.
- 룰은 **파일 이름으로 나눠 로드**됩니다. 언어의 모든 파일에는 작은 core 룰만 로드되고, 역할별 룰은 그 역할의 파일 이름(예: `*_test.go`, `main.go`, `errors.go`, `config.go`, `*_client.go`, `*_worker.go`, `doc.go`)을 Read 로 열 때만 로드됩니다. 파일 이름 컨벤션은 함께 설치되는 core 룰이 알려 줍니다. 기존 프로젝트의 파일 이름이 다르면 역할 룰이 로드되지 않으니, 설치 뒤 사용자에게 이 점을 안내합니다.

## Step 0 — atelier CLI 보장

```bash
bash "${CLAUDE_PLUGIN_ROOT}/scripts/ensure-binary.sh"
```

실패하면(cargo 부재 등) 이후 Step 을 진행하지 않고 에러를 안내합니다.

## Step 1 — 동작 선택

인자(`install`·`update`·`check`)가 있으면 그대로 씁니다. 없으면 먼저 점검합니다:

```bash
atelier code-style check --plugin-root "${CLAUDE_PLUGIN_ROOT}" --project-dir "${CLAUDE_PROJECT_DIR:-.}"
```

- exit 2 이고 stderr 가 `Error: no code-style language installed` 로 시작하면 설치된 언어가 없는 것입니다. `install` 로 진행합니다.
- `DRIFTED`·`NEW`·`ORPHANED` 가 있으면 `update` 로 진행합니다.
- 모두 `OK` 면 최신 상태임을 알리고 끝냅니다. 다른 언어를 추가하고 싶은지 AskUserQuestion 으로 묻고, 원하면 `install` 로 진행합니다.

## Step 2a — install

1. AskUserQuestion 으로 언어를 고릅니다 (multiSelect): `[go]` `[rust]`. 선택지는 `${CLAUDE_PLUGIN_ROOT}/rules/code-style/` 아래 `common` 을 뺀 디렉토리 이름입니다.
2. 고른 언어마다 `--lang` 을 붙여 한 번에 실행합니다:
   ```bash
   atelier code-style install --plugin-root "${CLAUDE_PLUGIN_ROOT}" --project-dir "${CLAUDE_PROJECT_DIR:-.}" --lang go --lang rust
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
   `--lang` 을 생략하면 프로젝트에 설치된 언어 전부를 갱신합니다.

## Step 2c — check

Step 1 의 점검만 하고 결과를 보고합니다. 파일을 바꾸지 않습니다.

## STATUS 와 종료 코드

| STATUS | 뜻 |
|---|---|
| `OK` | 원본과 같음 |
| `DRIFTED` | 원본과 다름 |
| `NEW` | 원본에 새로 생긴 룰이 프로젝트에 없음 |
| `ORPHANED` | 원본에서 없어진 룰의 사본이 프로젝트에 남음 |

- exit `0` 갱신할 것 없음 / `1` `DRIFTED`·`NEW`·`ORPHANED` 발견 / `2` CLI 오류 (없는 언어, 원본 누락, 인자 오류).
- **exit 1 은 실패가 아니라 update 의 입력입니다.**

## 에러 처리

- **ensure-binary 실패** → Rust 툴체인 설치(https://rustup.rs/)를 안내하고 끝냅니다.
- **exit 2** → stderr 원문을 보여 줍니다. 파일을 직접 고치지 않습니다.
  - `unknown language: <X> — expected one of: go, rust` → 고를 수 있는 언어를 다시 안내합니다.
  - `no code-style language installed in <경로>` → 설치된 언어가 없습니다 (Step 1 참고).
  - `plugin source directory not found: <경로>` → 플러그인 설치가 깨졌습니다. 플러그인 재설치를 안내합니다.
  - `<파일> is not valid UTF-8` → 그 사본의 인코딩을 고치거나 지운 뒤 다시 실행하도록 안내합니다.

## Output Examples

**check — 플러그인 업데이트 뒤 바뀐 룰과 새 룰이 있음 (exit 1):**

```
code-style/common/cohesion.md=OK
code-style/common/comments.md=DRIFTED (/work/my-repo/.claude/rules/atelier/code-style/common/comments.md)
code-style/go/go-errors.md=OK
code-style/go/go-generics.md=NEW (/work/my-repo/.claude/rules/atelier/code-style/go/go-generics.md)
→ 4 checked, 1 drifted, 1 new, 0 orphaned
```

**install --lang go (exit 0):**

```
installed: /work/my-repo/.claude/rules/atelier/code-style/common/cohesion.md
installed: /work/my-repo/.claude/rules/atelier/code-style/go/go-principles.md
→ 2 installed, 0 synced, 0 removed, 0 unchanged
```

**update — 갱신 선택 뒤 (exit 0):**

```
synced: /work/my-repo/.claude/rules/atelier/code-style/common/comments.md (backup: /work/my-repo/.claude/rules/atelier/code-style/common/comments.md.bak-20261028-141500)
installed: /work/my-repo/.claude/rules/atelier/code-style/go/go-generics.md
→ 1 installed, 1 synced, 0 removed, 2 unchanged
```

→ 보고 예: "바뀐 룰 `common/comments.md` 를 갱신했어요 (백업: `…comments.md.bak-20261028-141500`). 새 룰 `go/go-generics.md` 를 설치했어요. 팀과 공유하려면 `.claude/rules/atelier/code-style/` 를 커밋해 주세요."
