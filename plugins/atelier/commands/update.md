---
description: atelier CLI 바이너리를 플러그인 버전에 맞게 갱신하고 복사형 산출물 드리프트를 점검합니다 (점검은 보고만 — 설치 모듈·hook·설정 파일에는 손대지 않음)
argument-hint: ""
allowed-tools: ["Bash"]
---

# atelier update

설치된 atelier CLI 를 활성 플러그인 버전으로 갱신하고, setup 이 복사한 산출물의 드리프트를 점검합니다. **파일을 고치는 것은 CLI 바이너리뿐입니다** — 모듈 설치·hook 등록·`CLAUDE.md` 병합·설정 파일 편집은 하지 않고, 복사형 산출물은 diff 판정 결과를 보고만 합니다. 갱신이 필요하면 `/atelier:setup` 을 사용하세요.

사용자 선택이 필요 없는 경로이므로 질문 없이 바로 실행합니다 (스크립트·자동화에서도 그대로 동작).

## 실행

### Step 1 — CLI 바이너리 갱신

```bash
bash "${CLAUDE_PLUGIN_ROOT}/scripts/ensure-binary.sh"
```

스크립트가 plugin.json 버전과 설치된 `atelier --version` 을 SemVer 비교해 스스로 판단합니다:

- **이미 최신/상위 버전** → 아무것도 하지 않고 현재 버전만 출력 (멱등)
- **미설치 또는 하위 버전** → `cargo build --release` 후 `~/.local/bin/atelier` 에 설치

### Step 2 — 복사형 산출물 드리프트 점검 (read-only)

```bash
bash "${CLAUDE_PLUGIN_ROOT}/scripts/check-drift.sh" --project-dir "${CLAUDE_PROJECT_DIR:-.}"
```

setup 이 스냅샷으로 복사한 두 산출물이 현재 플러그인 원본과 일치하는지 diff 로 판정합니다:

| 점검 대상 | 원본 |
|---|---|
| `~/.claude/CLAUDE.md` 의 `[coding-style:begin]~[end]` 블록 | `templates/claude-md/CLAUDE.md` |
| `<project>/.claude/rules/agent-design-principles.md` | `rules/agent-design-principles.md` |

- **보고만 합니다** — 어떤 파일도 수정하지 않습니다. 갱신은 `/atelier:setup` 담당입니다
- 출력은 `<check>=<STATUS>` 라인(STATUS: `OK` | `DRIFTED` | `NOT_INSTALLED`)과 요약 한 줄입니다
- exit code: `0` 드리프트 없음 / `1` 드리프트 발견 / `2` 스크립트 오류. **exit 1 은 update 실패가 아니라 보고 대상입니다** — Step 1 결과와 함께 그대로 보고합니다

## 결과 보고

Step 1 출력을 근거로 다음 중 하나를 보고합니다:

- 최신 상태: `atelier CLI is up to date (vX.Y.Z).` → 갱신할 것이 없음을 알립니다
- 갱신 완료: `atelier CLI updated successfully: vA.B.C → vX.Y.Z.` → 이전/새 버전을 알립니다
- 신규 설치: `atelier CLI installed successfully (vX.Y.Z).`

Step 2 출력을 근거로 이어서 보고합니다:

- 모두 `OK` → 드리프트 없음을 한 줄로 알립니다
- `DRIFTED` 발견 → 어떤 산출물이 어긋났는지 알리고, `/atelier:setup` (해당 모듈 재선택)으로 갱신할 수 있음을 안내합니다. **직접 파일을 고치지 않습니다**. 스크립트는 "다르다"는 사실만 판정하므로, 프로젝트가 의도적으로 다르게 유지하는 파일(커스터마이즈된 rules 등)이라면 갱신하지 않아도 된다는 점을 함께 안내합니다
- `NOT_INSTALLED` → 해당 모듈이 설치되지 않았음을 알립니다 (문제가 아니며, 필요 시 `/atelier:setup` 안내)

## 에러 처리

- **Rust 툴체인 부재** (`ERROR: Rust toolchain not found.`) → https://rustup.rs/ 에서 설치 후 재실행하도록 안내하고 종료합니다. 다른 작업으로 대체하지 않습니다.
- **빌드 실패** → cargo 에러 원문을 보여주고 종료합니다. 설정 파일을 고치려 들지 않습니다.
- **check-drift.sh exit 2** (`ERROR: ...`) → 플러그인 원본 누락 또는 인자 오류입니다. Step 1 결과는 그대로 보고하고, 드리프트 점검은 실패했음을 stderr 원문과 함께 알립니다.

## Output Examples

**드리프트 없음 (exit 0):**

```
claude-md-coding-style-block=OK
rules/agent-design-principles.md=OK
→ 2 checked, 0 drifted, 0 missing
```

**드리프트 + 미설치 혼재 (exit 1):**

```
claude-md-coding-style-block=DRIFTED (/Users/me/.claude/CLAUDE.md)
rules/agent-design-principles.md=NOT_INSTALLED (./.claude/rules/agent-design-principles.md)
→ 2 checked, 1 drifted, 1 missing
```

→ 보고 예: "CLAUDE.md 코딩 원칙 블록이 플러그인 템플릿과 어긋났습니다. `/atelier:setup` 에서 style 모듈을 다시 선택하면 갱신됩니다."
