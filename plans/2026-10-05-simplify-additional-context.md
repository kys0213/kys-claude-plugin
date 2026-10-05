# simplify 제안을 Stop additionalContext 로 전달한다

> **Plan — 이 시점의 결정 기록.** 현재 정책의 신뢰 소스가 아니다.
> 날짜: 2026-10-05 · 브랜치: fix/simplify-additional-context · 이슈: #725 · 승인 출처: council pass (협의체 3라운드 검증 pass — 2라운드 한도를 사용자 승인으로 1회 추가)

남은 이슈 5건을 작업 4개(A 규칙 근거 위임 · B 주석 정리 · C 플러그인 거버넌스 · D Stop hook 메시지 전달)로 나눠 각각 독립 PR 로 진행했어요. 이 plan 은 그중 한 작업의 기록이에요.

## 작업 간 파일 분할

| 파일 | 소유 작업 | 근거 |
|---|---|---|
| `plugins/atelier/cli/src/session/mod.rs` | D 전부 (주석 정리 포함) | D 가 `emit()`(L105-110)·모듈 문서(L20-24)를 고쳐요. B 대상 주석 L57·64·71·105·181 도 같은 파일이에요 |
| `plugins/atelier/cli/src/session/commands/simplify.rs` | D 전부 (주석 정리 포함) | D 가 렌더 함수를 더해요. B 대상 주석 L140-141 이 `#725` 를 인용해요. 패턴 밖 이력 주석 L120-121 도 D 가 정리해요 |
| `plugins/atelier/cli/tests/session_cli_simplify.rs` (신규) | D | `emit` 배선 테스트 (D 결정 7) |
| `common/**` | 어느 작업도 편집 안 함 | B 의 `call-*.sh:23` 은 "남긴다" 판정 |
| `plugins/atelier/hooks/suggest-simplify.sh` | B | D 는 shim 을 건드리지 않아요 (shim 은 `exec atelier session simplify-check` 만 해요, `plugins/atelier/hooks/suggest-simplify.sh:13`) |
| `plugins/atelier/skills/orchestrator/**` | A | 다른 작업은 건드리지 않아요 |
| `.claude/rules/plugin-governance.md` (신규) | C | 다른 작업은 건드리지 않아요 |
| `.claude/rules/comments.md` | B (변경 없음으로 결정, §B 결정 4) | — |

- 버린 대안: "B 는 `mod.rs:105-106` 만 빼고 나머지 줄은 정리" — 같은 파일 다른 hunk 라 git 이 자동 병합할 가능성은 높지만, D 의 모듈 문서·`emit` 수정 범위가 구현 중 넓어지면 인접 hunk 충돌이 생겨요. 파일 단위로 나누면 판정이 결정적이에요.



## 요구사항

- simplify 제안이 모델 컨텍스트에 닿도록 Stop hook 출력을 `hookSpecificOutput.additionalContext` JSON 으로 바꿔요 (#725 A안).
- 이슈 대상 파일(`coding-style/hooks/suggest-simplify.sh`)은 이제 없고, 판정·출력은 atelier CLI 로 옮겨졌어요 (`plugins/atelier/hooks/suggest-simplify.sh:5,13`, `plugins/atelier/README.md:19`).

## 사이드이펙트 조사

- 현재 출력 지점은 하나예요: `emit()` 이 `render_banner` 결과를 `print!` 해요 (`plugins/atelier/cli/src/session/mod.rs:105-111`). 호출은 `mod.rs:177-179` 한 곳.
- 외부 사실: Stop 의 plain stdout 은 debug log 로만 가고 모델·transcript 어디에도 안 닿아요. Stop 은 `{"hookSpecificOutput":{"hookEventName":"Stop","additionalContext":"..."}}` 를 지원하고, 다음 턴 system reminder 로 주입돼요 (facts-external.md "Stop hook").
- 선례: `push_check::render_block_json` 이 `serde_json::json!` 으로 Stop JSON 을 만들고 `Option<String>` 을 돌려줘요 (`plugins/atelier/cli/src/session/commands/push_check.rs:129-145`, doc 주석 `:127-128`). `emit_push_check` 가 그걸 `println!` 해요 (`mod.rs:113-119`).
- 같은 Stop 에 push-check 도 등록돼 있어요 (`plugins/atelier/hooks/hooks.json` Stop 배열 두 항목). 별도 프로세스라 각자 JSON 한 개씩 내보내요 — 서로 섞이지 않아요.
- push-check 가 block 하면 Stop 이 다시 와요. simplify 는 세션당 1회 `notified` 플래그로 이미 막혀 있어 중복 주입은 없어요 (`simplify.rs:155-157`).
- 모듈 문서가 "`simplify-check` prints at most an advisory banner" 라고 출력 계약을 적고 있어 같이 고쳐야 해요 (`mod.rs:20-24`).
- 출력 계약 "always exits 0" 은 그대로예요 (`mod.rs:11-18`).
- 테스트 하네스: `plugins/atelier/cli/tests/session_commands_simplify.rs` (결정 테스트, `fn notified_files` 헬퍼 `:13`), push 쪽 렌더 테스트 패턴은 `tests/session_commands_push_check.rs:114-116` (`render_block_json(&decide(s)).expect(...)` 후 파싱). `render_banner` 문자열을 단언하는 테스트는 없어요 (`grep -rln render_banner plugins/atelier/cli/tests` → 없음).
- README 가 "`/simplify` 를 제안합니다. 세션당 1회, 비차단(항상 exit 0)" 이라고만 적어요 (`plugins/atelier/README.md:79-81`) — 전달 채널을 말하지 않아 고칠 필요는 구현자가 판단해요 (아래 범위).
- 파일 분할로 D 가 두 파일의 주석 정리를 B 와 같은 기준으로 맡아요.
  - 번호 인용: `mod.rs:57,64,71,105,181`, `simplify.rs:141` (`git grep -nE '^[[:space:]]*(//[/!]?|#).*#[0-9]{2,}' -- 'plugins/atelier/cli/src/session/*'`)
  - 패턴 밖 이력 narration: `simplify.rs:120-121` `render_banner` doc 의 "Divider and title are unchanged from the shell hook; the count sentence now states what is actually counted" (검증 r1 F5)
- `emit` 배선을 블랙박스로 볼 수 있어요. `assert_cmd`·`tempfile` 이 이미 dev-dependency 예요(`plugins/atelier/cli/Cargo.toml:22,24`). CLI 테스트 선례는 `tests/session_cli.rs:10-11` 이에요. 베이스라인 저장 위치는 `std::env::temp_dir()`(= `$TMPDIR`)/`atelier-sessions` 예요(`mod.rs:92-93`). payload 는 `session_id`·`cwd`·`stop_hook_active` 를 읽어요(`session/commands/payload.rs:31-33`). 그래서 `TMPDIR` 와 임시 git repo 를 주입하면 baseline → 파일 변경 → simplify-check 를 바이너리로 돌릴 수 있어요.

## 결정과 근거

1. **`simplify.rs` 에 `pub fn render_context_json(decision: &SimplifyDecision) -> Option<String>` 추가** — `Notify` 면 `{"hookSpecificOutput":{"hookEventName":"Stop","additionalContext": render_banner(files,total)}}` 를 `serde_json::json!` 으로 만들고, `Silent` 면 `None`. `render_block_json` 과 같은 모양이라 두 Stop 렌더러가 대칭이에요. serde 로 만들어 파일명에 따옴표·줄바꿈이 있어도 JSON 이 깨지지 않아요.
2. **`emit()` 은 `render_context_json` 결과를 `println!`** — 결정은 그대로, 출력 형식만 바꿔요.
3. **본문 문구(`render_banner`)는 바꾸지 않아요.** 구분선이 사람용 장식이긴 하지만, 문구 변경은 이슈 요구가 아니에요.
4. **비차단 유지** — `decision:"block"` 을 쓰지 않아요.
5. **모듈 문서(`mod.rs:20-24`) 갱신**: simplify-check 가 Stop `hookSpecificOutput.additionalContext` 문서를 내보낸다고 고쳐요.
6. **두 파일의 주석을 B 와 같은 기준(`comments.md` §1, 패턴 밖 위반 포함)으로 정리** — 번호 인용 6줄과 `simplify.rs:120-121` 이력 narration 이 대상이에요. `mod.rs:105-106`·`simplify.rs:140-141` 의 "#725 has a single call site" 는 이 PR 로 의미가 사라지니 지우거나 의도로 다시 써요. `mod.rs:57,64,71` 의 "hook cwd may differ" 처럼 함정 문장은 번호만 빼요.
7. **`emit` 배선 블랙박스 테스트 1개 추가** (검증 r1 F6, 메인 채택) — 새 파일 `tests/session_cli_simplify.rs` 에 둬요. `session_cli.rs` 는 clap 표면 테스트라 분리해요. 절차는 이래요.
   1. 임시 git repo 와 임시 `TMPDIR` 을 만들어요.
   2. `atelier session baseline --project-dir <repo>` 에 stdin `{"session_id":"wiring-test-01","cwd":"<repo>"}` 를 넣어요. session id 는 8자 이상이어야 해요 (`plugins/atelier/cli/src/session/core/baseline.rs:20` `MIN_SESSION_ID_LEN = 8`). 짧으면 `NoSessionId` 로 조용히 빠져 테스트가 엉뚱한 이유로 실패해요.
   3. repo 에 코드 파일(`a.rs`)을 만들고 `git add a.rs` 로 stage 해요. untracked 로 두면 사용자 git 설정(`status.showUntrackedFiles=no`)에 따라 결과가 달라질 수 있어요 (`src/session/core/repo.rs:96`).
   4. `atelier session simplify-check --project-dir <repo>` 를 같은 stdin 으로 실행해요.
   5. stdout 을 `serde_json` 으로 파싱해 `hookSpecificOutput.hookEventName == "Stop"` 이고 `additionalContext` 에 `a.rs` 가 들어 있는지, exit 0 인지 단언해요.
   - 외부 의존이 git 바이너리뿐이에요. 기존 테스트도 git 을 쓰는지는 구현 agent 가 `grep -rln 'Command::new("git")' plugins/atelier/cli/tests` 로 확인하고, 없으면 테스트 안에서 `git init` 해요.

## 버린 대안

| 대안 | 이유 |
|---|---|
| #725 B안: `type: "prompt"` Stop hook 으로 block | 매번 추가 턴 비용이 들고 제안을 강제로 바꿔요. 현재 설계는 세션당 1회 비차단 권고예요 (`README.md:80-81`) |
| `decision:"block"` + reason | 같은 이유로 강제 실행이 돼요 |
| 셸 shim 에서 `jq` 로 JSON 조립 | 출력 판정은 CLI 소유예요 (`.claude/rules/tool-layer-boundary.md` "결정적 로직은 CLI 서브커맨드에 둔다"). `jq` 의존도 새로 생겨요 |
| 사용자 화면용 `systemMessage` 동시 출력 | Stop 에서의 동작을 확인하지 않았어요(가정 D1). 지금도 사용자에게 안 보이므로 회귀가 아니에요 (YAGNI) |
| 모델용 평문으로 문구 재작성 | 이슈 요구 밖이에요 |
| `CLAUDE.md` 에 "안내 오면 /simplify 호출" 유도 문구 추가 (#725 결합 권장) | 사용자 전역 설정 영역이고 이슈의 필수 요구가 아니에요 |

## PR 타이틀 / 브랜치

- `fix(atelier): send simplify hint as stop additionalContext`
- `fix/simplify-additional-context`
- 머지 시 #725 close.



## 가정

- **D1** — Stop 의 `systemMessage` 동작은 확인하지 않았어요. 사용하지 않으므로 결과에 영향 없어요.
- **D2** — 같은 Stop 이벤트의 두 command hook 이 각자 JSON 을 내보내도 Claude Code 가 각각 해석한다. 근거: hook 은 별도 프로세스로 실행되고 출력이 hook 단위로 처리돼요(facts-external.md 의 Stop 출력 규격이 hook 단위). 직접 재현은 안 했어요 — D-t1 수동 E2E 에서 push-check 와 동시 발동 상황을 함께 보면 확인돼요.
