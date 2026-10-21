---
description: atelier CLI 와 설치한 모듈의 복사형 산출물을 활성 플러그인 버전으로 동기화합니다 (모듈 신규 설치·hook 등록은 /atelier:setup 담당)
argument-hint: ""
allowed-tools: ["Bash", "AskUserQuestion"]
---

# atelier update

설치된 atelier CLI 와 setup 이 복사한 산출물을 활성 플러그인 버전으로 동기화합니다. 이미 설치한 모듈을 새 버전과 같게 맞춥니다. 기존 파일을 갱신하고, **정책 룰을 설치했다면 새 버전이 추가한 정책 룰도 설치합니다**. setup 에서 고르지 않은 모듈은 설치하지 않으니, 모듈 설치·hook 등록이 필요하면 `/atelier:setup` 을 사용하세요.

rules 복사본 덮어쓰기 확인(Step 3)을 제외하면 사용자 선택이 필요 없으므로 질문 없이 바로 실행합니다. 질문이 불가능한 자동화·headless 경로에서는 rules 덮어쓰기를 **건너뛰기(기본값)** 로 처리하고 보고만 남깁니다 — 나머지 단계는 그대로 동작합니다.

## 실행

### Step 1 — CLI 바이너리 갱신

```bash
bash "${CLAUDE_PLUGIN_ROOT}/scripts/ensure-binary.sh"
```

스크립트가 plugin.json 버전과 설치된 `atelier --version` 을 SemVer 비교해 스스로 판단합니다:

- **이미 최신/상위 버전** → 아무것도 하지 않고 현재 버전만 출력 (멱등)
- **미설치 또는 하위 버전** → `cargo build --release` 후 `~/.local/bin/atelier` 에 설치

### Step 2 — 복사형 산출물 드리프트 점검 (read-only)

Step 1 (ensure-binary) 이 바이너리를 플러그인 버전으로 보장하므로, 이 시점에는 `drift` 서브커맨드가 반드시 존재합니다:

```bash
atelier drift check --plugin-root "${CLAUDE_PLUGIN_ROOT}"
```

setup 이 스냅샷으로 복사한 산출물이 현재 플러그인 원본과 일치하는지 diff 로 판정합니다:

| 점검 대상 | 원본 |
|---|---|
| `~/.claude/CLAUDE.md` 의 `[coding-style:begin]~[end]` 블록 | `templates/claude-md/CLAUDE.md` |
| `~/.claude/rules/atelier/agent-design-principles.md` | `rules/agent-design-principles.md` |
| `~/.claude/rules/atelier/<정책 룰>.md` | `rules/policies/*.md` (목록은 atelier CLI 가 소유) |

- 출력은 `<check>=<STATUS>` 라인과 요약 한 줄입니다. STATUS:
  - `OK` — 원본과 일치
  - `DRIFTED` — 원본과 다름
  - `NEW` — 정책 룰을 설치했는데(정책 룰이 하나라도 있음) 새 버전이 추가한 파일만 없음
  - `NOT_INSTALLED` — 사용자가 설치하지 않은 산출물
- exit code: `0` 갱신할 것 없음 / `1` `DRIFTED` 또는 `NEW` 발견 / `2` CLI 오류. **exit 1 은 실패가 아니라 Step 3 의 입력입니다**

### Step 3 — 드리프트 갱신 적용

Step 2 결과의 산출물별 STATUS 에 따라 처리합니다. 갱신의 실제 쓰기는 전부 `atelier drift sync` 에 위임합니다 (덮어쓰기 전 `<file>.bak-<timestamp>` 백업, 설치하지 않은 산출물·마커 손상 시 exit 2 로 거부).

**`claude-md-coding-style-block=DRIFTED`** → 질문 없이 바로 갱신합니다. 마커 구간은 `DO NOT REMOVE` 로 플러그인 소유가 선언된 영역이고, 마커 밖 사용자 내용은 보존됩니다:

```bash
atelier drift sync --target claude-md --plugin-root "${CLAUDE_PLUGIN_ROOT}"
```

**`rules/agent-design-principles.md=DRIFTED`** → 스크립트는 "원본과 다르다"는 사실만 판정하며, 사용자가 의도적으로 다르게 유지하는 변형본일 수 있습니다. **반드시 AskUserQuestion 으로 확인한 뒤에만 덮어씁니다:**

- 질문: "rules 복사본이 플러그인 원본과 다릅니다. 플러그인 원본으로 덮어쓸까요?"
- 선택지: `[덮어쓰기 (백업 후 갱신)]` `[건너뛰기 (커스터마이즈 유지)]`
- **덮어쓰기** 선택 시:
  ```bash
  atelier drift sync --target rules --plugin-root "${CLAUDE_PLUGIN_ROOT}"
  ```
- **건너뛰기** 선택 시 파일을 건드리지 않고, 유지했다는 사실만 보고합니다
- 질문할 수 없는 컨텍스트(자동화·headless)에서는 **묻지 않고 건너뛰기**로 처리합니다 — 덮어쓰기는 사용자 확인 없이는 수행하지 않습니다

**`user-rules/<파일>=DRIFTED`** → rules 복사본과 같은 방식으로 처리합니다 (사용자가 편집했을 수 있으므로 AskUserQuestion 확인 후에만 덮어쓰기, headless 는 건너뛰기). check 가 파일 단위로 판정하므로 동기화도 같은 단위로 — **해당 파일만** `--name` 으로 덮어씁니다:

```bash
atelier drift sync --target user-rules --name <파일명> --plugin-root "${CLAUDE_PLUGIN_ROOT}"
```

여러 파일이 드리프트면 파일마다 확인 후 각각 실행합니다. `--name` 없이 실행하면 매니페스트의 **모든** 파일을 한 번에 덮어쓰므로(없는 파일은 새로 설치), 사용자가 전체 갱신에 동의한 경우에만 씁니다. 어느 쪽이든 덮어쓴 파일마다 `synced:` 라인이 출력됩니다.

**`user-rules/<파일>=NEW`** → 질문 없이 바로 설치합니다. 사용자가 이미 설치한 정책 룰에 새 버전의 파일을 채우는 것이고, 덮어쓸 기존 파일이 없어 백업도 만들지 않습니다. headless 에서도 그대로 설치합니다. 파일마다 `--name` 으로 실행합니다:

```bash
atelier drift sync --target user-rules --name <파일명> --plugin-root "${CLAUDE_PLUGIN_ROOT}"
```

출력은 파일마다 `installed: <경로>` 한 줄입니다.

**`NOT_INSTALLED`** → 건너뜁니다. 사용자가 setup 에서 고르지 않은 모듈이므로 설치하지 않고, 필요 시 `/atelier:setup` 을 안내만 합니다.

## 결과 보고

Step 1 출력을 근거로 다음 중 하나를 보고합니다:

- 최신 상태: `atelier CLI is up to date (vX.Y.Z).` → 갱신할 것이 없음을 알립니다
- 갱신 완료: `atelier CLI updated successfully: vA.B.C → vX.Y.Z.` → 이전/새 버전을 알립니다
- 신규 설치: `atelier CLI installed successfully (vX.Y.Z).`

Step 2·3 결과를 산출물별로 이어서 보고합니다:

- `OK` → 최신 상태임을 알립니다
- 갱신됨 → `atelier drift sync` 출력의 백업 경로를 함께 알립니다
- 새로 설치됨 (`NEW`) → 새 버전에서 추가된 정책 룰을 설치했음을 경로와 함께 알립니다
- 건너뜀 (사용자 선택) → 커스터마이즈를 유지했음을 알립니다
- `NOT_INSTALLED` → 해당 모듈이 설치되지 않았음을 알립니다 (문제가 아니며, 필요 시 `/atelier:setup` 안내)

## 에러 처리

- **Rust 툴체인 부재** (`ERROR: Rust toolchain not found.`) → https://rustup.rs/ 에서 설치 후 재실행하도록 안내하고 종료합니다. 다른 작업으로 대체하지 않습니다.
- **빌드 실패** → cargo 에러 원문을 보여주고 종료합니다. 설정 파일을 고치려 들지 않습니다.
- **`atelier drift check` exit 2** (`Error: ...`) → 플러그인 원본 누락 또는 인자 오류입니다. Step 1 결과는 그대로 보고하고, 드리프트 점검은 실패했음을 stderr 원문과 함께 알립니다.
- **`atelier drift sync` exit 2** (`Error: ...`) → 설치하지 않은 대상, 마커 손상, 또는 인코딩 거부(CRLF·비 UTF-8 파일은 byte 보존이 불가능해 쓰지 않음)입니다. 직접 파일을 고치려 들지 말고 stderr 원문과 함께 `/atelier:setup` 재실행을 안내합니다.

## Output Examples

**드리프트 없음 (`drift check` exit 0, Step 3 없음):**

```
claude-md-coding-style-block=OK
rules/agent-design-principles.md=OK
user-rules/spec-writing.md=OK
user-rules/rules-writing.md=OK
user-rules/plan-writing.md=OK
user-rules/doc-dependency.md=OK
→ 6 checked, 0 drifted, 0 new, 0 missing
```

**CLAUDE.md 블록 드리프트 → 자동 갱신:**

```
claude-md-coding-style-block=DRIFTED (/Users/me/.claude/CLAUDE.md)
rules/agent-design-principles.md=NOT_INSTALLED (/Users/me/.claude/rules/atelier/agent-design-principles.md)
user-rules/spec-writing.md=OK
user-rules/rules-writing.md=OK
user-rules/plan-writing.md=OK
user-rules/doc-dependency.md=OK
→ 6 checked, 1 drifted, 0 new, 1 missing
```

```
synced: coding-style block in /Users/me/.claude/CLAUDE.md (backup: /Users/me/.claude/CLAUDE.md.bak-20260821-093012)
```

→ 보고 예: "CLAUDE.md 코딩 원칙 블록을 최신 템플릿으로 갱신했습니다 (백업: `~/.claude/CLAUDE.md.bak-20260821-093012`). workflow 모듈(설계 원칙 룰)은 설치되어 있지 않습니다."

**새 정책 룰 추가 → 자동 설치:**

```
claude-md-coding-style-block=OK
rules/agent-design-principles.md=OK
user-rules/spec-writing.md=OK
user-rules/rules-writing.md=OK
user-rules/plan-writing.md=OK
user-rules/doc-dependency.md=NEW (/Users/me/.claude/rules/atelier/doc-dependency.md)
→ 6 checked, 0 drifted, 1 new, 0 missing
```

```
installed: /Users/me/.claude/rules/atelier/doc-dependency.md
```

→ 보고 예: "새 버전에서 추가된 정책 룰 `doc-dependency.md` 를 `~/.claude/rules/atelier/` 에 설치했습니다."

**rules 드리프트 → 사용자가 건너뛰기 선택:**

→ 보고 예: "rules 복사본이 플러그인 원본과 다르지만, 선택에 따라 커스터마이즈를 유지했습니다. 이후에도 갱신하지 않으려면 그대로 두시면 됩니다."
