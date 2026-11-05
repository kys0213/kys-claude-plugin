# atelier 모드로 로컬 Claude 세션 전체를 칸반 pane 에 그린다

> **Plan — 이 시점의 결정 기록.** 현재 정책의 신뢰 소스가 아니다.
> 날짜: 2026-11-05 · 브랜치: claude/stoic-archimedes-uysvw9 · 이슈: 없음 · 승인 출처: 사용자 승인 (2026-11-05 "좋아" — 전제: 개인 사용, 모드는 무조건 켠다, panel 지원 필수)

## 요구사항

1. 이 머신에서 활성화된 Claude Code 세션 전체를 칸반으로 본다. 세션 하나가 카드 하나이고, 컬럼은 세션의 현재 상태다.
2. 칸반은 Claude Code 안의 pane(panel)으로 그린다. 터미널·Desktop Code 탭·VS Code 확장에서 같은 모드가 그린다.
3. 모드(Claude Mods, 구 function hooks)는 필수다. 모드가 없는 세션·구버전은 대상이 아니다. 사용자 한 명이 쓰는 기능이라 opt-in 게이트를 두지 않는다.
4. 결정적 판정(이벤트 → 컬럼, stale·만료, 정렬)은 atelier CLI 에 둔다. 모듈은 이벤트 ↔ CLI JSON 어댑터와 렌더만 한다 (`.claude/rules/tool-layer-boundary.md`).

## 현재 상태 (사이드이펙트 조사)

- **모드 기본값** (mods overview 문서, 2026-11-05 확인): 모드는 기본으로 켜진다 — 터미널 v2.1.287+, Desktop v2.1.286+. 설치 시 모드 활성화를 묻는 단계는 없고, 플러그인·마켓플레이스 신뢰만 확인한다. `CLAUDE_CODE_ENABLE_FUNCTION_HOOKS` 는 v2.1.287 부터 무시된다.
- **세션 간 공유 수단** (mods reference·interface 문서):
  - `$.store` 는 그 모드를 돌리는 이 머신의 모든 세션이 하나를 공유하는 KV 다 (plugin 별 JSON 파일, 총 4 MiB). `get`→`set` 은 atomic 이 아니다 — 문서 권장은 "항목마다 키를 따로", "쓰기 직전에 다시 읽기".
  - `$.fs.write` 는 atomic 이 아니어서 여러 세션이 바꾸는 데이터에는 쓰지 말라고 문서가 권한다. → 파일 디렉토리 방식은 버린다.
  - 다른 세션이 store 를 바꿔도 이벤트가 오지 않는다. 화면 갱신은 `$.clock.every(ms, () => $.ui.invalidate('ui.render'))` 로 polling 해야 한다. redraw 는 초당 10회(터미널 visible pane 30회)로 throttle 된다.
- **렌더** : `Pane` render site 는 Terminal·Desktop·VS Code 에서 그려진다. `Box` 가 flex(`flexDirection: 'row'`)·`borderStyle`·`width`·`gap` 을 받아 컬럼을 나란히 둘 수 있다. dock pane 은 터미널 폭의 45% 까지만 넓어지고, 폭은 `e.props.bodyColumns` 로 받는다. 좁은 화면에서는 pane 이 프롬프트 위 inline 으로 간다.
- **쓸 수 있는 이벤트·API**: `session.start`·`session.end`(`e.reason`), `prompt.submit`, `turn.start`·`turn.complete`, `tool.call`(matcher `{ tool }`), `classic.<Event>`(settings hook 이벤트를 stdin JSON 그대로 — `classic.Notification`·`classic.PermissionRequest`·`classic.PostToolUse`), `$.session.id()`·`cwd()`·`repo()`, `$.clock.now()`, `$.command.register`, `$.ui.open`.
- **atelier 현황**:
  - `hooks/hooks.json` 의 `modules` 는 경로 하나만 받는다 (reference: "an array with one path"). 이미 `./register.ts` 가 orchestrator dispatch guard 를 등록하고 있다 → 칸반 hook 은 같은 `register(on)` 에서 등록해야 한다.
  - CLI 최상위 그룹: `drift`·`code-style`·`git`·`session`·`orchestrator` (`cli/src/cli.rs`). hook 용 서브커맨드 계약은 "stdin JSON, stdout JSON, 항상 exit 0" (`cli/src/orchestrator/mod.rs` 모듈 문서).
  - 모듈 테스트는 `plugins/atelier/tests/register.test.ts` 하나다.
- **stale 문서·CI** (이번 작업의 사이드이펙트):
  - `.claude/rules/tool-layer-boundary.md` 마지막 bullet 이 "function hooks 가 early access 인 동안 classic hook 을 이관하지 않는다" 고 적고 있다 — 지금 사실과 다르다.
  - `plugins/atelier/README.md` "Function hooks (early access)" 절이 플래그로만 로드된다고 적고 있다.
  - `.github/workflows/validate.yml` 이 `@anthropic-ai/claude-code@2.1.283` 고정 + `CLAUDE_CODE_ENABLE_FUNCTION_HOOKS=1` 로 `plugin validate/test` 를 돈다. `$.store`·`$.clock.every`·`Pane` 이 그 버전에 있는지 모른다 → 고정 버전을 올려야 할 가능성이 높다.

## 검토한 대안

| 대안 | 판단 |
|---|---|
| 세션별 JSON 파일을 `~/.claude/atelier/kanban/` 에 쓰고 pane 이 디렉토리를 읽음 | 버림 — `$.fs.write` 가 atomic 이 아니라 읽는 쪽이 반쯤 쓰인 파일을 본다. `$.store` 가 같은 일을 공식 지원한다 |
| classic command hook 이 쓰기(`atelier kanban record`), 모드는 렌더만 | 버림 — 모드 필수 전제라 classic 경로의 이점(모드 없는 세션 커버)이 없다. 이벤트마다 프로세스를 띄우는 비용만 남는다 |
| 판정(컬럼 매핑·stale)을 TS 모듈에 구현 | 버림 — 경계 규칙 위반. 같은 스냅샷이면 같은 보드여야 하므로 CLI 의 블랙박스 테스트가 소유한다 |
| CLI 가 `$.store` 의 JSON 파일을 직접 읽음 | 버림 — store 파일 경로·형식은 Claude Code 내부 계약이다. 모듈이 스냅샷을 stdin 으로 넘긴다. 그러면 CLI 는 파일 I/O 없는 순수 변환이 된다 |
| 별도 plugin 으로 분리 (`hud` 등) | 버림 — Rust 바이너리 배포 경로(`ensure-binary.sh`·`check-cli-version.sh`)가 atelier 에만 있고, plugin 수를 늘리지 않는 방침(#738)과 맞지 않는다 |
| statusline 으로 표시 | 버림 — 한 줄 텍스트라 칸반이 안 되고 panel 요구사항을 못 채운다 |
| `claude agents --json` 으로 세션 목록 | 버림 — background 세션만 나오고 대화형 세션은 빠진다 |
| `userConfig` opt-in 게이트 | 버림 — 개인 사용 전제이고, 쓰는 곳이 사용자 설정 파일이 아니라 모드 전용 store 다 |

## 결정

### 1. 데이터 흐름

```
[세션마다 · 쓰기]  이벤트 → $.store.set('kanban:session:<id>', SessionFact)
[pane 연 세션 · 읽기]  $.clock.every(2000)
                       → pane 이 열려 있을 때만: $.store.keys() 중 'kanban:session:' 접두 → get
                       → atelier kanban board (stdin = 스냅샷, args = 시각·임계값·self)
                       → 결과를 모듈 변수에 캐시 → $.ui.invalidate('ui.render')
[렌더]  ui.render { component: 'Pane' }, requestId 'kanban' → 캐시된 보드를 Box 트리로
```

### 2. store 스키마 (모듈이 쓰는 사실 — 판단 없음)

키: `kanban:session:<session_id>` — 세션마다 키가 달라 세션 간 race 가 없다.

```jsonc
{
  "v": 1,
  "session_id": "…",
  "cwd": "/…",                    // $.session.cwd()
  "repo": "owner/name" | null,    // $.session.repo()
  "started_at": 1730790000000,    // session.start 시각
  "first_prompt": "…" | null,     // 첫 prompt.submit 텍스트 (store 보호용으로 앞 500자만)
  "last_event": "turn.complete",  // 아래 이벤트 이름 중 하나, 그대로
  "last_event_at": 1730790123000,
  "ended_reason": null | "clear" | "resume" | "logout" | "prompt_input_exit" | "other"
}
```

`last_event` 를 남기는 이벤트: `session.start`, `prompt.submit`, `turn.start`, `turn.complete`, `tool.call{AskUserQuestion}`, `classic.PermissionRequest`, `classic.Notification`, `classic.PostToolUse`, `session.end`. 모든 hook 은 `next(e)` 를 먼저 부르고 결과를 바꾸지 않는다(관찰만). store 쓰기 실패는 삼킨다.

### 3. CLI — `atelier kanban board`

새 최상위 그룹 `kanban`. 계약은 `orchestrator` 와 같다: stdin JSON, stdout 한 줄 JSON, 파싱 실패 포함 항상 exit 0 (실패 시 `{"error": "…"}`).

```
atelier kanban board --now <unix-ms> --stale-after <dur> --expire-after <dur> [--self <session_id>]
  stdin : {"sessions": [SessionFact, ...]}
  stdout: {"columns": [{"id": "needs_input", "title": "Needs input", "cards": [Card, ...]}, ...],
           "expired": ["<session_id>", ...]}
  Card  : {"session_id", "label", "subtitle", "since_ms", "is_self"}
```

- 컬럼 매핑 (이벤트 이름 → 컬럼):
  - `needs_input` ← `classic.PermissionRequest`·`classic.Notification`·`tool.call{AskUserQuestion}`
  - `working` ← `prompt.submit`·`turn.start`·`classic.PostToolUse`
  - `idle` ← `turn.complete`·`session.start`
- stale: `now - last_event_at > stale-after` 이고 `working`·`needs_input` 이면 `stale` 컬럼으로 옮긴다 (비정상 종료·멈춘 세션).
- 만료: `ended_reason` 이 있으면 즉시, 또는 `now - last_event_at > expire-after` 면 카드에서 빼고 `expired` 에 담는다. 모듈은 `expired` 키를 `$.store.delete` 한다 (결정은 CLI, 실행은 어댑터).
- 컬럼 순서 고정: `needs_input` → `working` → `idle` → `stale`. 컬럼 안 정렬: `last_event_at` 오래된 것 먼저 (오래 기다린 세션이 위).
- `label`: `repo` 가 있으면 repo 이름, 없으면 `cwd` basename. `subtitle`: `first_prompt` 를 한 줄로 접어 60자 truncate.
- 알 수 없는 `v`·`last_event` 는 카드에서 빼되 `expired` 에 넣지 않는다 (다른 버전 모듈이 쓴 값을 지우지 않는다).
- 임계값 기본값은 모듈이 인자로 넘긴다: `--stale-after 10m`, `--expire-after 24h`. CLI 는 인자가 없으면 거부한다.

### 4. 모듈

- 코드는 `hooks/kanban.ts` 에 두고 `register.ts` 의 `register(on)` 끝에서 `registerKanban(on)` 을 호출한다. `modules` 는 그대로 `./register.ts` 하나.
- `session.start` 에서 `$.command.register` 로 `kanban` 명령(pane 열기/닫기 토글)과 `$.clock.every(2000, tick)` 를 건다. `tick` 은 kanban pane 이 열려 있을 때만 store 를 읽고 CLI 를 부른다.
- 렌더는 캐시된 보드 JSON 을 트리로 바꾸기만 한다:
  - `bodyColumns >= 4 * 22` 이면 컬럼 4개를 `Box row` 로 나란히, 아니면 컬럼을 세로 섹션으로 쌓는다. 폭 계산은 레이아웃이라 모듈에 둔다 (판정이 아니라 그리기).
  - 자기 세션 카드는 강조한다 (`is_self`).
  - CLI 실패(바이너리 없음·exit ≠ 0·파싱 실패·timeout)면 "보드를 불러오지 못함 — `/atelier:update`" 한 줄만 그린다 (fail-open, 세션 영향 없음).

### 5. spec

칸반 컬럼의 의미(어떤 상태를 "입력 대기"로 보는가, stale·만료 기준의 의미)는 제품 규격이라 `plugins/atelier/spec/concerns/session-kanban.md` 로 남긴다. 이벤트 이름·store 키 같은 구현 세부는 spec 에 적지 않는다.

### 6. 문서·CI 정비

- `.claude/rules/tool-layer-boundary.md`: early access bullet 을 현재 사실(모드는 v2.1.287+ 기본 on)로 고친다.
- `plugins/atelier/README.md`: "Function hooks (early access)" 절을 모드 기준으로 고치고 칸반 사용법을 추가한다.
- `.github/workflows/validate.yml`: spike 결과에 따라 고정 버전을 `$.store`·`$.clock.every`·`Pane` 이 있는 버전으로 올리고, 무시되는 `CLAUDE_CODE_ENABLE_FUNCTION_HOOKS` env 를 뺀다. 버전 고정 자체는 유지한다.

### 7. 테스트

- Rust 블랙박스 (`cli/tests/kanban_board.rs`): 이벤트별 컬럼 매핑, stale 경계값(정확히 임계값·초과), `ended_reason` 즉시 만료, expire 경계값, 정렬, `is_self`, label fallback, truncate, 알 수 없는 `v`/`last_event`, 빈 입력, 잘못된 stdin, 인자 누락.
- 모듈 (`tests/kanban.test.ts`, `claude plugin test`): 이벤트마다 store 에 쓰는 사실의 모양, pane 이 닫혀 있으면 CLI 를 안 부름, CLI argv·stdin 모양, `expired` 키 삭제, CLI 실패 시 오류 한 줄. 컬럼 규칙 자체는 다시 검증하지 않는다.

### 8. 커밋 type

plan 은 `docs(atelier)`, 구현은 `feat(atelier)`.

## 남은 위험 — spike 로 먼저 확인

1. 설치 가능한 최신 `@anthropic-ai/claude-code` 에서 `$.store` 공유가 실제로 세션 간에 보이는가 (두 세션 동시 실행).
2. `classic.PermissionRequest`·`classic.Notification`·`classic.PostToolUse` 가 모듈에 실제로 발화하는가, `e` 모양.
3. `$.command.register` 로 연 명령 이름이 `/kanban` 인지 plugin 접두가 붙는지.
4. `classic.PostToolUse` 마다 store 쓰기 비용 — 체감 지연이 있으면 `working` 갱신을 `turn.start` 로만 줄인다.
5. pane 이 닫혀 있을 때 `$.ui.panes()` 로 열림 여부를 판별할 수 있는가.

spike 에서 1·2 중 하나라도 안 되면 설계를 다시 연다.

## 작업 순서

1. 이 plan 을 `docs(atelier)` 커밋으로 남긴다.
2. spike — 위 남은 위험 1~5 를 `--plugin-dir` 실세션으로 확인하고 결과를 사용자에게 보고한다.
3. 승인 후 TDD:
   - T1 CLI `kanban` 그룹 + Rust 테스트 (`plugins/atelier/cli/**`)
   - T2 모듈 `hooks/kanban.ts`·`register.ts` 연결 + 모듈 테스트 (`plugins/atelier/hooks/**`·`plugins/atelier/tests/**`)
   - T3 spec·rules·README·CI (`plugins/atelier/spec/**`·`.claude/rules/tool-layer-boundary.md`·`plugins/atelier/README.md`·`.github/workflows/validate.yml`)
4. 통합 검증: `cargo fmt --check`·`cargo clippy -- -D warnings`·`cargo test` (cli), `claude plugin validate plugins/atelier`, `claude plugin test plugins/atelier`, 빌드한 바이너리를 PATH 에 두고 세션 두 개를 띄워 서로의 카드가 컬럼을 옮겨 다니는지 smoke.
