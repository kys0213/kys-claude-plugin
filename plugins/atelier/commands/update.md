---
description: atelier CLI 바이너리를 플러그인 버전에 맞게 갱신합니다 (설치 모듈·hook·설정 파일에는 손대지 않음)
argument-hint: ""
allowed-tools: ["Bash"]
---

# atelier update

설치된 atelier CLI 를 활성 플러그인 버전으로 갱신합니다. **버전 갱신만 수행합니다** — 모듈 설치·hook 등록·`CLAUDE.md` 병합·설정 파일 편집은 하지 않습니다. 그런 작업이 필요하면 `/atelier:setup` 을 사용하세요.

사용자 선택이 필요 없는 경로이므로 질문 없이 바로 실행합니다 (스크립트·자동화에서도 그대로 동작).

## 실행

```bash
bash "${CLAUDE_PLUGIN_ROOT}/scripts/ensure-binary.sh"
```

스크립트가 plugin.json 버전과 설치된 `atelier --version` 을 SemVer 비교해 스스로 판단합니다:

- **이미 최신/상위 버전** → 아무것도 하지 않고 현재 버전만 출력 (멱등)
- **미설치 또는 하위 버전** → `cargo build --release` 후 `~/.local/bin/atelier` 에 설치

## 결과 보고

스크립트 출력을 근거로 다음 중 하나를 한 줄로 보고합니다:

- 최신 상태: `atelier CLI is up to date (vX.Y.Z).` → 갱신할 것이 없음을 알립니다
- 갱신 완료: `atelier CLI updated successfully: vA.B.C → vX.Y.Z.` → 이전/새 버전을 알립니다
- 신규 설치: `atelier CLI installed successfully (vX.Y.Z).`

## 에러 처리

- **Rust 툴체인 부재** (`ERROR: Rust toolchain not found.`) → https://rustup.rs/ 에서 설치 후 재실행하도록 안내하고 종료합니다. 다른 작업으로 대체하지 않습니다.
- **빌드 실패** → cargo 에러 원문을 보여주고 종료합니다. 설정 파일을 고치려 들지 않습니다.
