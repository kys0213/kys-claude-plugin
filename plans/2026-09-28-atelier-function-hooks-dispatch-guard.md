# atelier 에 function hooks 모듈을 도입해 dispatch 규약을 결정적으로 경고한다

> **Plan — 이 시점의 결정 기록.** 현재 정책의 신뢰 소스가 아니다.
> 날짜: 2026-09-28 · 브랜치: claude/ecstatic-hawking-e8ov7q · 이슈: 없음 (참고: anthropics/claude-code#91870) · 승인 출처: 사용자 승인 (2026-09-28 "결정적인 것 위주로만" — 기능 범위 기준)

## 요구사항

1. Claude Code 의 function hooks(제품명 Claude Mods)로 atelier orchestrator 규약 일부를 문서가 아니라 엔진이 집행하게 한다. 사용자 예시: agent 를 띄울 때 model 을 지정하지 않으면 그 사실을 모델에게 경고로 전달한다.
2. **결정적인 판정만 넣는다.** 같은 입력이면 항상 같은 결과가 나오는 조건만 대상이다. 문구 매칭·실행 상태 추정이 필요한 규칙은 넣지 않는다.
3. 판정은 Rust CLI 에 둔다 (CLAUDE.md 책임 경계, `.claude/rules/tool-layer-boundary.md`). 모듈은 이벤트를 CLI 입력으로 옮기고 결과를 이벤트 결과로 옮기는 어댑터다.
4. function hooks 는 early access 다(`CLAUDE_CODE_ENABLE_FUNCTION_HOOKS=1`, docs·changelog 미기재). 플래그 없는 세션과 구버전에서 기존 hook 동작이 그대로여야 한다.

## 현재 상태 (사이드이펙트 조사)

- **API 확인** (upstream `mods/types/claude-code.d.ts`, 2.1.277 기준 선언 + 로컬 2.1.283 실세션 spike):
  - `agent.spawn` 이 Agent 도구 실행 중 발화한다. `e.model` 은 지정했을 때만 있고(alias 그대로, 예 `sonnet`), `e.parentModel` 은 부모의 실효 model id, `e.fork`·`e.subagentType`·`e.tool_use_id` 를 준다. `next(e)` 는 `{ model }`(해석된 id) 또는 `{ deny }`.
  - `tool.call {tool:'Agent'}` 의 `next(e)` 가 끝나기 전에 `agent.spawn` 이 같은 `tool_use_id` 로 발화한다 (spike 확인).
  - `tool.call` 결과의 `context: string[]` 는 모델만 읽는 추가 텍스트다. spike 에서 모델이 `tool.call hook additional context: …` 형태로 실제 수신했다. upstream `agents-md` mod 가 같은 패턴을 쓴다.
  - `session.compact` 는 `instructions` 를 바꿔 요약 지시를 더할 수 있다. `agentId` 가 있으면 subagent 의 compaction 이다.
  - `$.process.run(argv, {stdin, timeoutMs})` 는 바이너리가 없으면 reject 한다 (`ENOENT: Executable not found in $PATH`).
- **공존 확인** (spike): `hooks.json` 에 `modules` 와 classic `hooks` 를 함께 두면 2.1.283 플래그 유무, 2.1.274(stable) 모두에서 classic SessionStart·Stop 이 그대로 실행됐다. 플래그 없는 2.1.283 은 stderr 에 `hooks module not loaded: … set CLAUDE_CODE_ENABLE_FUNCTION_HOOKS=1` 한 줄을 낸다. 대화형 화면 노출 여부는 온보딩 화면에 막혀 **미확인**.
- **테스트 도구**: `claude plugin validate` 가 모듈이 hook 하는 이벤트와 `$` 호출을 정적 스캔해 출력한다. `claude plugin test <dir>` 가 `*.test.ts` 를 실행하며, 테스트가 `on(...)` 으로 모듈 아래 세계(`process.run` 등)를 답한다. 테스트 파일은 모듈의 `register` 를 직접 부르지 않는다 — 모듈은 plugin 으로 로드되고 테스트 hook 은 그 아래에 앉는다.
- **atelier 현황**: `hooks/hooks.json` 은 classic command hook 4개(SessionStart·Stop)만 있다. plugin 에 TS·package.json 이 없다. `tools/validate` 는 `hooks.json` 을 검사하지 않는다. CI(`validate.yml`)에 Claude Code 가 설치돼 있지 않다. `@anthropic-ai/claude-code@2.1.283` 은 npm 에 있다.
- **CLI 구조**: 최상위 그룹은 `drift`·`git`·`session` (`cli/src/cli.rs`). hook 용 서브커맨드는 stdin JSON 입력, 항상 exit 0, stdout 출력 계약을 따른다 (`cli/src/session/mod.rs` 모듈 문서).

## 후보 기능과 결정성 판정

| 후보 | 이벤트 | 결정적인가 | 판단 |
|---|---|---|---|
| model 미지정 dispatch 경고 (orchestrator 불변식 21) | `agent.spawn` + `tool.call{Agent}` | 예 — `e.model` 부재·`inherit`, fork 제외 | **채택** |
| 집행 위임 tier 상한 초과 경고 (불변식 21, 표 1 사전 기준) | 같음 | 예 — model 문자열 → tier 사다리 → 부모 tier 의 상한과 비교 | **채택** (경고만. 자문 소집은 상한 예외라 차단하지 않는다) |
| compaction 에 orchestrator 런 상태 보존 지시 | `session.compact` (main 루프만) | 예 — 고정 문구 주입 | **채택** |
| 격리 dispatch 를 한 메시지에 여러 건 (불변식 6) | `tool.call{Agent}` | 아니오 — 이벤트에 메시지(step) 식별자가 없어 "같은 메시지" 를 판정할 수 없다 | 제외 |
| 메인의 tracked 파일 직접 편집 (불변식 1) | `tool.call{Write,Edit}` | 런 상태가 있어야 결정적 — 지금은 "orchestrator 런 중" 을 알 수단이 없다 | 제외 — 후속(런 상태 CLI) |
| epic 브랜치 rebase·비 ff merge (불변식 9) | `tool.call{Bash}` | 같은 이유로 epic 이름을 알 수 없다 | 제외 — 후속 |
| 배경 위임 prompt 의 보고 채널 문구·증거 계약 누락 | `agent.spawn` | 문구 매칭이라 의미상 비결정적 | 제외 (사용자 기준) |
| 승인 마커 없는 implementer dispatch (불변식 16) | `agent.spawn` | `log_dir` 이 세션 scratchpad 라 hook 이 경로를 모른다 | 제외 |

## 검토한 대안

| 대안 | 판단 |
|---|---|
| 판정을 TS 모듈에 직접 구현 | 버림 — CLI/Skill 경계 위반이고 사다리가 두 언어에 갈라진다. 모듈은 어댑터로 제한 |
| `agent.spawn` 에서 `{deny}` 로 차단 | 버림 — 사용자 예시가 "경고" 이고, tier 상한은 자문 소집 예외가 있어 차단하면 정당한 호출을 막는다. 첫 이터레이션은 전부 경고 |
| model 미지정 시 hook 이 model 을 자동 주입 | 버림 — 어떤 tier 를 쓸지는 작업 유형 판단이라 CLI 가 정할 수 없다 |
| `tool.call{Agent}` 하나로 처리 (`$.session.model()` 로 부모 model) | 버림 — subagent 가 띄우는 경우 부모 model 이 틀리고, fork 여부·해석된 model 을 모른다. `agent.spawn` 이 사실을 모으고 `tool.call` 이 경고를 싣는 짝이 정확하다 |
| classic PreToolUse(`Agent` matcher) command hook 으로 구현 | 버림 — 부모 model·fork·해석된 model 을 입력으로 받지 못한다. 플래그 없는 세션까지 덮는 장점은 있으나 판정 정확도를 잃는다 |
| 모듈을 별도 opt-in plugin 으로 분리 | 버림 — 플래그 없는 세션의 stderr 한 줄 외에는 공존 비용이 없음을 spike 로 확인. plugin 수를 늘리지 않는다(#738) |
| 기존 classic hook 4개를 이번에 모듈로 이관 | 버림 — 새 기능이 아니고, 공존 기간에 중복 실행 문제만 만든다. 후속 |

## 결정

1. **CLI** — 최상위 그룹 `atelier orchestrator` 를 추가한다. hook 용 계약은 `session` 과 같다: stdin JSON, stdout JSON, 파싱 실패 포함 항상 exit 0.
   - `atelier orchestrator spawn-check` — 입력 `{"model": string|null, "resolved_model": string|null, "parent_model": string, "fork": bool, "subagent_type": string}`, 출력 `{"warnings": [string, ...]}`.
     - 규칙 A: `fork` 가 아니고 `model` 이 null·빈 문자열·`inherit` → model 미지정 경고. 문구에 해석된 model 과 부모 model 을 싣고 불변식 21 을 지목한다.
     - 규칙 B: `fork` 가 아니고 `model` 과 `parent_model` 의 tier 를 둘 다 판별할 수 있고 `tier(model) > cap(tier(parent_model))` → 상한 초과 경고. 자문 소집이면 예외라는 문구를 싣는다.
     - tier 판별: 대소문자 무시 부분 문자열 `fable`·`mythos` → Fable, `opus` → Opus, `sonnet` → Sonnet, `haiku` → Haiku, 그 밖은 판별 불가(경고 없음). 상한: Fable → Opus, Opus → Opus, Sonnet → Sonnet, Haiku → Haiku (orchestrator `SKILL.md §기본값 표` 표 1 사전 기준과 같다. 세대가 바뀌면 둘 다 고친다).
   - `atelier orchestrator compact-note` — 입력 없음, 출력 `{"instructions": string}`. 문구는 "orchestrator 런이 진행 중이었다면 epic 브랜치 이름·log_dir 경로·task id 와 상태·담당 agent 이름·진행 중 worktree·남은 작업·정지 조건을 요약에 그대로 보존하고, 런이 없었다면 무시하라" 는 조건부 지시다.
2. **모듈** — `plugins/atelier/hooks/register.ts` 를 추가하고 `hooks.json` 에 `"modules": ["./register.ts"]` 를 더한다. classic `hooks` 4개는 그대로 둔다.
   - `agent.spawn`: `next(e)` 를 부르고, deny 가 아니면 사실(`model`·해석된 model·`parentModel`·`fork`·`subagentType`)을 `tool_use_id` 키로 모듈 메모리에 둔다. 이벤트를 바꾸지 않는다.
   - `tool.call {tool:'Agent'}`: `next(e)` 뒤 사실을 꺼내 `spawn-check` 를 부르고, 결과가 deny 가 아니고 경고가 있으면 `context` 에 덧붙인다.
   - `session.compact`: `agentId` 가 없을 때만 `compact-note` 를 불러 `instructions` 에 덧붙인다.
   - CLI 호출 실패(바이너리 없음·exit ≠ 0·JSON 파싱 실패·timeout)는 전부 원래 결과를 그대로 돌려준다. hook 이 세션을 깨지 않는다는 기존 hook 계약과 같다. 바이너리 부재는 SessionStart 의 `check-cli-version.sh` 가 이미 알린다.
3. **테스트** — Rust: `cli/tests/orchestrator_*.rs` 블랙박스(규칙 A·B·fork·판별 불가·파싱 실패·compact-note). 모듈: `plugins/atelier/tests/register.test.ts` 를 `claude plugin test` 로 — 경고 첨부, 경고 없음, 바이너리 없음, exit ≠ 0, 아래가 deny, compaction main/subagent.
4. **CI** — `validate.yml` 에 atelier hooks·tests 변경 시 `@anthropic-ai/claude-code@2.1.283` 을 설치해 `claude plugin validate plugins/atelier` 와 `CLAUDE_CODE_ENABLE_FUNCTION_HOOKS=1 claude plugin test plugins/atelier` 를 돈다. 버전은 고정한다 — early access surface 가 버전마다 바뀔 수 있어 floating 이면 무관한 PR 이 깨진다.
5. **문서** — `.claude/rules/tool-layer-boundary.md` 에 hooks module 을 "TS 형태의 shim" 으로 규정하는 원칙·판단 기준 행을 추가한다. `plugins/atelier/README.md` 에 기능·플래그·요구 버전을 적는다.
6. 커밋 type 은 `feat(atelier)`.

## 작업 순서

1. 메인이 이 plan 을 `docs` 커밋으로 남기고 승인 마커를 세운다.
2. task 3건 — 파일 집합이 disjoint 라 병렬, 각각 worktree 격리:
   - T1 CLI `orchestrator` 그룹 + Rust 테스트 (`plugins/atelier/cli/**`)
   - T2 hooks module + `claude plugin test` 테스트 (`plugins/atelier/hooks/register.ts`·`hooks/tsconfig.json`·`hooks/hooks.json`·`plugins/atelier/tests/**`·`.gitignore`)
   - T3 CI + rules + README (`.github/workflows/validate.yml`·`.claude/rules/tool-layer-boundary.md`·`plugins/atelier/README.md`)
3. 게이트 — reviewer(요구사항↔구현·경계 위반) · qa(테스트가 규칙과 실패 경로를 덮는가) 2관점, 작성 agent 와 다른 agent.
4. rebase + ff-only 머지 → epic HEAD 에서 통합 검증: `cargo fmt --check`·`clippy -D warnings`·`cargo test` (cli) · `make build && make test && make validate-ci` · `claude plugin validate` · `claude plugin test` · 빌드한 바이너리를 PATH 에 두고 플래그 켠 실세션에서 model 미지정 Agent 호출이 경고를 받는지 smoke.
