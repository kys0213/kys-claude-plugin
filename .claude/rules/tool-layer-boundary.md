---
paths:
  - "**/hooks/**"
  - "**/tests/*.test.ts"
  - "**/cli/src/git/commands/hook.rs"
  - "**/cli/src/git/commands/guard.rs"
  - "**/scripts/*-hook.sh"
---

# Tool Layer Boundary — 훅 로직은 CLI, 등록은 thin shim

> CLAUDE.md "책임 경계 (CLI vs Skill/Agent)" 절의 hook/CLI 적용 규칙.
> 설계 배경(plan — 시점 기록): `plans/atelier/02-architecture.md` §3(setup), §4(CLI 통합). 현재 경계의 신뢰 소스는 이 문서와 코드다.

훅(hook)·셸 스크립트와 CLI 서브커맨드 사이의 책임을 고정한다. 경계가 흐려지면
결정적 로직이 테스트 불가능한 bash에 흩어지고, 설치 시점에 절대경로가 박혀
플러그인 버전 업데이트마다 깨진다.

## 원칙

### 결정적 로직은 CLI 서브커맨드에 둔다

훅이 수행하는 판단(브랜치 보호 여부, PR 중복 차단, stagnation 카운트 해석 등)은
**모두 `atelier` 바이너리의 서브커맨드**로 구현한다.

```
atelier git guard <write|commit|pr>              # PreToolUse 가드 판단 (차단·확인 요청)
atelier git hook <register|unregister|list>      # settings.json 편집
atelier git setup guard --scope <user|project>   # guard hook 2종 등록 (감지→정리→쓰기)
atelier autopilot check stagnation               # stdin payload 해석
```

- 동일 입력 → 동일 출력. 단위 테스트로 동작을 고정한다 (`tests/git_core_guard.rs` 등).
- 입력은 **args / env / stdin** 으로만 받는다. "지금 상황을 보고 추측"하지 않는다.
- PreToolUse 페이로드(JSON)는 stdin 으로 받고, 차단은 exit code(2) + stderr 로 신호한다.
- 사용자 확인 요청(Ask)은 exit 0 + stdout 의 `{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"ask",...}}` JSON 으로 신호한다. exit 2 와 섞지 않는다.

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
- **플러그인 번들 `.sh` hook 은 `hooks/hooks.json` 으로 선언한다 (setup-register 금지).**
  슬래시 커맨드(`/atelier:setup`)는 로드 시점에 본문의 `${CLAUDE_PLUGIN_ROOT}` 를 *그때의*
  버전 절대경로로 expand 한다 — 단일 따옴표도 못 막는다. 그래서 setup 이 `.sh` shim 을
  `hook register` 로 등록하면 settings.json 에 버전 절대경로가 박혀 **업데이트마다 frozen**
  된다(0.4.x 잔재의 원인). 반면 `hooks/hooks.json` 의 `${CLAUDE_PLUGIN_ROOT}` 는 Claude Code
  가 hook **실행 시점**에 활성 버전으로 해석하므로 frozen 이 없다. 모든 세션에 적용되므로 안전이
  전제다 — 차단(exit 2)형 hook 은 **self-gate**(config 파일 / 명령 패턴)로 비대상 세션에서 no-op 하고,
  advisory(비차단)형 hook 은 출력만 하므로 게이트 없이 둔다. 사용자 설정 파일을 **쓰는** hook 은
  비차단이어도 plugin `userConfig` 동의 값(`CLAUDE_PLUGIN_OPTION_<KEY>`)으로 self-gate 하고, CLI 는
  없는 키만 추가한다(기존 값 덮어쓰기 금지 — 사용자가 끌 수 있어야 한다).
- **예외 — setup 시점 값 주입이 필요한 hook**: git guard 처럼 `--default-branch <감지값>` 등
  프로젝트별 값을 setup 이 1회 감지해 박아야 하는 hook 은 전용 진입점
  `atelier git setup guard --scope <user|project>` 로 등록한다 — warm-up → 기본 브랜치 감지 →
  기존 항목 접두 매칭 정리 → 단일 쓰기를 원자적으로 수행하며, `hook register` 는 그 안에서 쓰이는
  하위 수단이다. 단 이때도 settings.json 에 기록되는 것은 `.sh` 경로가 아니라
  **CLI 커맨드 직접**(`atelier git guard ...`) 형태라 PATH 해석으로 버전 비의존이다.

### function hooks 모듈은 TS 형태의 shim

`hooks.json` 의 `modules`(예: `hooks/*.ts`)로 등록하는 function hooks 모듈도 위 shim 원칙의
적용 대상이다 — 언어만 TypeScript 일 뿐 책임은 같다.

- 모듈은 이벤트 사실(payload)을 CLI 입력 JSON 으로 옮기고, CLI 출력 JSON 을 이벤트 결과(경고
  문구·`context`·`instructions` 등)로 옮기는 **변환만** 한다. 정규식·문자열 해석·비교·임계값
  판정 같은 결정 로직은 두지 않는다 — 전부 CLI 서브커맨드로 위임한다.
- CLI 호출이 실패하면(바이너리 없음·exit ≠ 0·stdout 파싱 실패·timeout) **원래 이벤트 결과를 그대로
  돌려준다** (fail-open). hook 이 세션을 깨뜨리거나 이벤트를 막는 부작용을 내면 안 된다.
- 모듈 테스트(`claude plugin test`)는 어댑터 계약만 고정한다 — 보내는 argv/stdin 모양, 받은 CLI
  출력을 이벤트 결과로 바꾸는 변환, CLI 실패 시 fail-open 경로. 판정 규칙 자체(경계값·분기)의
  테스트는 CLI 쪽 언어의 블랙박스 테스트가 소유한다 — 모듈 테스트에서 규칙을 재검증하지 않는다.
- function hooks 가 early access 인 동안은 기존 classic `hooks`(command hook)를 모듈로 이관해
  없애지 않는다 — 플래그 없는 세션이 그 hook 을 완전히 잃는다. 새로 결정적으로 판정 가능해진
  기능만 모듈로 추가하고, classic hook 은 공존시킨다.

## 판단 기준

| 대상 | 위치 | 이유 |
|---|---|---|
| 가드/카운트/파싱/포맷 | CLI 서브커맨드 | 동일 입력 → 동일 출력, 테스트 가능 |
| 바이너리 보장·버전 확인 | shim (`.sh`) | 부트스트랩은 CLI 가 없을 때 동작해야 함 |
| 플러그인 번들 `.sh` hook 등록 | `hooks/hooks.json` (plugin-declared) | 실행 시점 `${CLAUDE_PLUGIN_ROOT}` 해석 → frozen 없음 |
| setup 시점 값 주입 hook 등록 | `git setup guard` (내부에서 `hook register`) | 프로젝트별 값 주입 필요, PATH 해석으로 버전 비의존 |
| function hooks 이벤트 어댑터 | `hooks/register.ts` (`hooks.json` 의 `modules`) | 이벤트 ↔ CLI JSON 변환만, 판정은 CLI |

헷갈리면 "두 번 호출해서 결과가 항상 같아야 하나?"를 묻는다. 같아야 하면 CLI,
부트스트랩(바이너리가 아직 없을 수 있음)이면 shim.

## 안티패턴

- ❌ stateless PreToolUse 훅으로 PR base 를 검증해 차단 — 정당한 promotion·hotfix 까지
  false-block 한다. 결정적 부분(base 계산)만 CLI 단일 출처(`atelier autopilot base-branch`)로
  올리고, 생성 측(branch-promoter)이 처음부터 올바른 base 를 쓰게 한다. 가드 자체는 두지 않는다
  (#776 에서 `guard-pr-base.sh` 제거).
- ❌ setup 이 `bash <version-path>/hooks/x.sh` 를 settings.json 에 기록 — `hook register`
  로 `atelier ...` 또는 리터럴 `${CLAUDE_PLUGIN_ROOT}` 를 기록한다 (#762, #772).
- ✅ shim 은 바이너리 보장 후 `exec atelier ...`, 로직은 CLI, 등록은 CLI 커맨드로 (guard 는 `git setup guard`, 그 외는 `hook register`).
