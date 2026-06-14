---
paths:
  - "**/hooks/**"
  - "**/cli/src/git/commands/hook.rs"
  - "**/cli/src/git/commands/guard.rs"
  - "**/scripts/*-hook.sh"
---

# Tool Layer Boundary — 훅 로직은 CLI, 등록은 thin shim

> CLAUDE.md "책임 경계 (CLI vs Skill/Agent)" 절의 hook/CLI 적용 규칙.
> 설계 근거: `plans/atelier/02-architecture.md` §3(setup), §4(CLI 통합).

훅(hook)·셸 스크립트와 CLI 서브커맨드 사이의 책임을 고정한다. 경계가 흐려지면
결정적 로직이 테스트 불가능한 bash에 흩어지고, 설치 시점에 절대경로가 박혀
플러그인 버전 업데이트마다 깨진다.

## 원칙

### 결정적 로직은 CLI 서브커맨드에 둔다

훅이 수행하는 판단(브랜치 보호 여부, PR 중복 차단, stagnation 카운트 해석 등)은
**모두 `atelier` 바이너리의 서브커맨드**로 구현한다.

```
atelier git guard <write|commit|pr>             # PreToolUse 가드 판단
atelier git hook <register|unregister|list>     # settings.json 편집
atelier autopilot check stagnation              # stdin payload 해석
```

- 동일 입력 → 동일 출력. 단위 테스트로 동작을 고정한다 (`tests/git_core_guard.rs` 등).
- 입력은 **args / env / stdin** 으로만 받는다. "지금 상황을 보고 추측"하지 않는다.
- PreToolUse 페이로드(JSON)는 stdin 으로 받고, 차단은 exit code(2)로 신호한다.

### 등록 진입점은 thin shim 또는 `hook register`

훅을 settings.json 에 등록할 때 **`.sh` 파일 경로나 버전 절대경로를 박지 않는다.**

- shim 셸 스크립트의 책임은 **부트스트랩뿐**이다: 바이너리 존재 보장
  (`ensure-binary.sh`), 버전 확인(`check-cli-version.sh`) 후 CLI 로 위임.
  로직(파싱·분기·판단)을 shim 에 두지 않는다.
- 가능하면 settings.json 에는 shim 대신 **CLI 커맨드를 직접** 기록한다:

  ```jsonc
  // ✅ 버전 비의존 — 바이너리가 PATH/등록 경로에서 해석됨
  { "command": "atelier git guard commit" }

  // ✅ shim 경유가 필요하면 리터럴 ${CLAUDE_PLUGIN_ROOT} 보존 (expand 금지)
  { "command": "${CLAUDE_PLUGIN_ROOT}/hooks/check-cli-version.sh" }

  // ❌ 실행 시점 절대경로로 expand — 버전 업데이트 시 깨짐
  { "command": "bash /Users/me/.claude/plugins/cache/.../0.30.0/hooks/guard.sh" }
  ```

- 등록 자체도 결정적 변환이므로 `atelier git hook register <type> <matcher> <command>`
  로 수행한다. LLM 이 `Write` 로 settings.json 을 직접 편집하지 않는다 (#762).
- **plugin-declared hook**: setup·프로젝트 설정과 무관하게 플러그인 활성화만으로 모든
  세션에 적용돼야 하는 config-free·non-blocking hook(예: `check-cli-version`)은
  플러그인 루트의 `hooks/hooks.json` 에 리터럴 `${CLAUDE_PLUGIN_ROOT}` 로 선언한다.
  이는 settings.json 편집이 아니므로 `hook register` 가 필요 없고, 활성 플러그인 버전
  경로로 해석돼 frozen 이 없다. 반대로 프로젝트별 설정이나 차단(exit 2)이 필요한 hook
  (guard 류)은 opt-in 이라 `hook register` 로 settings.json 에 등록한다.

## 판단 기준

| 대상 | 위치 | 이유 |
|---|---|---|
| 가드/카운트/파싱/포맷 | CLI 서브커맨드 | 동일 입력 → 동일 출력, 테스트 가능 |
| 바이너리 보장·버전 확인 | shim (`.sh`) | 부트스트랩은 CLI 가 없을 때 동작해야 함 |
| settings.json 편집 | `hook register` (CLI) | 결정적 변환, 경로 하드코딩 방지 |

헷갈리면 "두 번 호출해서 결과가 항상 같아야 하나?"를 묻는다. 같아야 하면 CLI,
부트스트랩(바이너리가 아직 없을 수 있음)이면 shim.

## 안티패턴

- ❌ bash 가 config 를 파싱하고 PR base 를 검증해 차단 (`guard-pr-base.sh`) — 로직을
  `atelier ... guard` 로 올린다 (#776).
- ❌ setup 이 `bash <version-path>/hooks/x.sh` 를 settings.json 에 기록 — `hook register`
  로 `atelier ...` 또는 리터럴 `${CLAUDE_PLUGIN_ROOT}` 를 기록한다 (#762, #772).
- ✅ shim 은 바이너리 보장 후 `exec atelier ...`, 로직은 CLI, 등록은 `hook register`.
