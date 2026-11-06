---
paths:
  - "**/hooks/**"
  - "**/tests/*.test.ts"
  - "**/cli/src/git/commands/hook.rs"
  - "**/cli/src/git/commands/guard.rs"
  - "**/scripts/*-hook.sh"
---

# Tool Layer Boundary — 훅 로직은 CLI, 등록은 thin shim

훅(hook)·셸 스크립트·모드 모듈과 CLI 서브커맨드 사이의 책임을 고정한다. 경계가 흐려지면
결정적 로직이 테스트 불가능한 bash·TS 에 흩어지고, 설치 시점에 절대경로가 박혀
플러그인 버전 업데이트마다 깨진다.

## 원칙

### 결정적 로직은 CLI 서브커맨드에 둔다

훅이 수행하는 판단(브랜치 보호 여부, PR 중복 차단, 세션 상태 판정 등)은 **모두 플러그인
바이너리의 서브커맨드**로 구현한다.

```
<cli> <group> <command> [args]     # 입력: args / env / stdin, 출력: stdout + exit code
```

- 동일 입력 → 동일 출력. CLI 쪽 블랙박스 테스트로 동작을 고정한다.
- 입력은 **args / env / stdin** 으로만 받는다. "지금 상황을 보고 추측"하지 않는다. 현재 시각처럼
  호출마다 달라지는 값도 인자로 받는다.
- PreToolUse 페이로드(JSON)는 stdin 으로 받고, 차단은 exit code(2) + stderr 로 신호한다.
- 사용자 확인 요청(Ask)은 exit 0 + stdout 의 `{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"ask",...}}` JSON 으로 신호한다. exit 2 와 섞지 않는다.

### 등록 진입점은 thin shim 또는 hook 등록 서브커맨드

훅을 settings.json 에 등록할 때 **`.sh` 파일 경로나 버전 절대경로를 박지 않는다.**

- shim 셸 스크립트의 책임은 **부트스트랩뿐**이다: 바이너리 존재 보장, 버전 확인 후 CLI 로 위임.
  로직(파싱·분기·판단)을 shim 에 두지 않는다.
- 가능하면 settings.json 에는 shim 대신 **CLI 커맨드를 직접** 기록한다:

  ```jsonc
  // ✅ 버전 비의존 — 바이너리가 PATH/등록 경로에서 해석됨
  { "command": "<cli> <group> <command>" }

  // ✅ shim 경유가 필요하면 리터럴 ${CLAUDE_PLUGIN_ROOT} 보존 (expand 금지)
  //    경로에 공백이 있어도 한 단어로 넘어가도록 큰따옴표로 감싼다 (plugin validate 경고)
  { "command": "\"${CLAUDE_PLUGIN_ROOT}/hooks/<shim>.sh\"" }

  // ❌ 실행 시점 절대경로로 expand — 버전 업데이트 시 깨짐
  { "command": "bash /Users/me/.claude/plugins/cache/<plugin>/<version>/hooks/<shim>.sh" }
  ```

- 등록 자체도 결정적 변환이므로 hook 등록 서브커맨드(`<type> <matcher> <command>` 를 받아
  settings.json 을 고치는 것)로 수행한다. LLM 이 `Write` 로 settings.json 을 직접 편집하지 않는다.
- **플러그인 번들 `.sh` hook 은 플러그인의 `hooks.json` 으로 선언한다 (setup-register 금지).**
  슬래시 커맨드는 로드 시점에 본문의 `${CLAUDE_PLUGIN_ROOT}` 를 *그때의* 버전 절대경로로
  expand 한다 — 단일 따옴표도 못 막는다. 그래서 setup 이 `.sh` shim 을 등록하면 settings.json 에
  버전 절대경로가 박혀 **업데이트마다 frozen** 된다. 반면 `hooks.json` 의 `${CLAUDE_PLUGIN_ROOT}` 는
  Claude Code 가 hook **실행 시점**에 활성 버전으로 해석하므로 frozen 이 없다. 모든 세션에
  적용되므로 안전이 전제다 — 차단(exit 2)형 hook 은 **self-gate**(config 파일 / 명령 패턴)로 비대상
  세션에서 no-op 하고, advisory(비차단)형 hook 은 출력만 하므로 게이트 없이 둔다. 사용자 설정 파일을
  **쓰는** hook 은 비차단이어도 plugin `userConfig` 동의 값(`CLAUDE_PLUGIN_OPTION_<KEY>`)으로
  self-gate 하고, CLI 는 없는 키만 추가한다(기존 값 덮어쓰기 금지 — 사용자가 끌 수 있어야 한다).
- **예외 — setup 시점 값 주입이 필요한 hook**: 기본 브랜치처럼 프로젝트별 값을 setup 이 1회 감지해
  박아야 하는 hook 은 전용 setup 서브커맨드로 등록한다 — warm-up → 값 감지 → 기존 항목 접두 매칭
  정리 → 단일 쓰기를 원자적으로 수행하며, hook 등록 서브커맨드는 그 안에서 쓰이는 하위 수단이다.
  이때도 settings.json 에 기록되는 것은 `.sh` 경로가 아니라 **CLI 커맨드 직접** 형태라 PATH
  해석으로 버전 비의존이다.

### 모드(function hooks) 모듈은 TS 형태의 shim

`hooks.json` 의 `modules` 로 등록하는 모드 모듈도 위 shim 원칙의 적용 대상이다 — 언어만
TypeScript 일 뿐 책임은 같다.

- 모듈은 이벤트 사실(payload)을 CLI 입력 JSON 으로 옮기고, CLI 출력 JSON 을 이벤트 결과(경고
  문구·`context`·`instructions`·그릴 내용 등)로 옮기는 **변환만** 한다. 정규식·문자열 해석·비교·임계값
  판정 같은 결정 로직은 두지 않는다 — 전부 CLI 서브커맨드로 위임한다.
- CLI 호출이 실패하면(바이너리 없음·exit ≠ 0·stdout 파싱 실패·timeout) **원래 이벤트 결과를 그대로
  돌려준다** (fail-open). hook 이 세션을 깨뜨리거나 이벤트를 막는 부작용을 내면 안 된다.
- 모듈 테스트(`claude plugin test`)는 어댑터 계약만 고정한다 — 보내는 argv/stdin 모양, 받은 CLI
  출력을 이벤트 결과로 바꾸는 변환, CLI 실패 시 fail-open 경로. 판정 규칙 자체(경계값·분기)의
  테스트는 CLI 쪽 언어의 블랙박스 테스트가 소유한다 — 모듈 테스트에서 규칙을 재검증하지 않는다.
- 모드는 Claude Code 에서 기본으로 켜지지만, 모드가 꺼진 세션(`--safe-mode`·`disableAllHooks`·
  모드 이전 버전)에서는 모듈과 함께 그 기능이 사라진다. 그래서 기존 classic `hooks`(command hook)를
  모듈로 옮기는 것은 별도 작업으로 다룬다. 새 기능은 모듈에 두고, 기존 classic hook 은 공존시킨다.
- `modules` 에는 경로를 하나만 쓴다. 기능별 코드는 기능마다 파일을 나누고, 진입 모듈의
  `register(on)` 이 각 기능의 등록 함수를 부른다.
- `claude plugin validate` 가 강제하는 형태를 따른다: `on()` 의 이벤트 이름은 문자열 리터럴이고,
  `$` 는 파일 최상위에 선언한(또는 import 한) 함수에만 넘긴다. 모듈 상태는 `register` 안에서 만든
  객체로 그 함수들에 넘긴다.

## 판단 기준

| 대상 | 위치 | 이유 |
|---|---|---|
| 가드/카운트/파싱/포맷/상태 판정 | CLI 서브커맨드 | 동일 입력 → 동일 출력, 테스트 가능 |
| 바이너리 보장·버전 확인 | shim (`.sh`) | 부트스트랩은 CLI 가 없을 때 동작해야 함 |
| 플러그인 번들 `.sh` hook 등록 | 플러그인 `hooks.json` (plugin-declared) | 실행 시점 `${CLAUDE_PLUGIN_ROOT}` 해석 → frozen 없음 |
| setup 시점 값 주입 hook 등록 | 전용 setup 서브커맨드 (내부에서 hook 등록 서브커맨드) | 프로젝트별 값 주입 필요, PATH 해석으로 버전 비의존 |
| 모드 이벤트 어댑터 | `hooks.json` 의 `modules` 진입 모듈 | 이벤트 ↔ CLI JSON 변환만, 판정은 CLI |

헷갈리면 "두 번 호출해서 결과가 항상 같아야 하나?"를 묻는다. 같아야 하면 CLI,
부트스트랩(바이너리가 아직 없을 수 있음)이면 shim.

## 안티패턴

- ❌ stateless PreToolUse 훅으로 PR base 를 검증해 차단 — 정당한 promotion·hotfix 까지
  false-block 한다. 결정적 부분(base 계산)만 CLI 단일 출처로 올리고, PR 을 만드는 쪽이 처음부터
  올바른 base 를 쓰게 한다. 가드 자체는 두지 않는다.
- ❌ setup 이 `bash <version-path>/hooks/<shim>.sh` 를 settings.json 에 기록 — hook 등록
  서브커맨드로 CLI 커맨드 또는 리터럴 `${CLAUDE_PLUGIN_ROOT}` 를 기록한다.
- ✅ shim 은 바이너리 보장 후 `exec <cli> ...`, 로직은 CLI, 등록은 CLI 커맨드로 (값 주입이 필요하면
  전용 setup 서브커맨드, 그 외는 hook 등록 서브커맨드).
