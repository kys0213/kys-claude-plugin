# 세션 칸반 카드 행동 spike 결과

> **Plan — 이 시점의 결정 기록.** 현재 정책의 신뢰 소스가 아니다.
> 날짜: 2026-11-10 · 브랜치: claude/zen-feynman-otrusm · 이슈: 없음
> 선행 기록: `plans/2026-11-10-atelier-kanban-card-actions.md`

## 확인한 사실 (Claude Code 2.1.322, tmux 대화형 세션, `--plugin-dir` spike 모드)

| # | 위험 | 결과 |
|---|---|---|
| 1 | 타이머에서 `$.prompt.submit` | **된다.** 쉬는 세션에서 바로 턴이 시작되고 "Prompt from the <plugin> plugin" 으로 보인다. 입력 칸에 쓰던 draft 는 그대로 남는다. 부른 모드 자신의 `prompt.submit` 훅은 이 프롬프트를 보지 못한다 |
| 2 | 타이머에서 `$.turn.abort` | **된다.** 생성 중인 턴이 끊기고 `turn.complete` 가 `isAborted: true` 로 온다 |
| 3 | 권한 창을 바깥 답으로 닫기 | **안 된다.** `classic.PermissionRequest` 의 `next(e)` 는 1ms 만에 `{}` 로 끝나고 권한 창은 그 뒤에 뜬다 → 경주시킬 대상이 없다. `tool.check` 는 창이 뜨기 전에 결정해야 하고 훅 예산이 걸린다. → 권한 승인은 빼고 요청 내용만 보인다 (사용자 결정 "spike 후 되면 포함" 의 대체 경로) |
| 4 | 모드에서 `claude --bg` | **된다.** 약 5초 만에 `backgrounded · <짧은 id>` 를 내고 끝난다. bg 세션은 모드를 로드하고 타이머·턴 이벤트가 돈다 → 칸반에 오른다. `claude agents --json` 이 대화형·bg 세션을 `busy`/`idle` 과 함께 나열한다 |
| 5 | 모드 툴과 권한 | 툴 호출은 **된다.** 그러나 같은 모드의 `tool.call` 훅이 답하는 툴은 manual 모드에서도 **권한 확인 없이** 실행됐다 — 권한 단계가 `tool.call` 체인 아래(core)에 있어서 그 툴의 `tool.check` 훅도 불리지 않는다. 훅 안에서 `$.tool.check({ tool: 'Bash', input: { command: 'claude --bg …' } })` 는 manual·auto 모두 `ask` 를 준다(분류기는 그 뒤 단계). bypass 는 확인하지 못했다 (bypass 세션을 띄우는 것을 auto 분류기가 막음) |

## 덤으로 확인한 사실

- `prompt.submit` 은 `origin.kind: 'task-notification'` 으로도 온다(`<task-notification>…` 텍스트). 지금 칸반은 이것도 첫 프롬프트로 기록할 수 있다 → 사람 프롬프트(`composer`)만 기록해야 한다.
- 사용자 스크린샷의 `<system-reminder> You are operating in a git worktree…` 부제목은 데스크톱이 프롬프트 앞에 붙이는 안내문이다.

## 사용자 보고: 입력 대기가 제대로 반영되지 않는다

- 데스크톱에서 턴 끝에 "Claude Code 알림" 카드(요약 + "…시작할까요?" + `초안대로 진행`·`고칠 행 있음` 버튼)가 떠 있는데 칸반은 쉬는 중으로 보인다.
- 그 카드는 `WrapUp` 툴이다 (types: `recap`, `ask`, `blocked`, `actions[{label} | {action}]`). 버튼 `label` 을 누르면 그 글이 사용자 메시지로 들어간다. AskUserQuestion 이 아니라서 질문 기록도 권한 이벤트도 남지 않고, 곧이어 `turn.complete` 가 쉬는 중으로 만든다.
- 조정: 메인 루프 `tool.call{WrapUp}` 에 `ask` 가 있으면 그 결정 요청(`ask`·`recap`·버튼 label)을 기록하고 다음 턴 시작에 지운다. CLI 는 턴이 끝난 상태에 결정 요청이 남아 있으면 **입력 대기(`결정 요청`)** 로 본다. 칸반 상세는 요약·질문·버튼을 그리고, 버튼은 그 label 을 프롬프트로 보낸다 (`kanban:prompt` 경로 재사용). 직접 입력도 받는다.
- 데스크톱에서 AskUserQuestion 창(`입력이 필요합니다`)이 떠 있는데도 입력 대기에 없다는 보고도 있었다.
- SDK host 모드(`-p --input-format stream-json --permission-prompt-tool stdio`, host 가 6초 뒤 허용)로 확인한 순서: `tool.call` 시작 → `tool.check`(approver `host`, `ask`) → `classic.PermissionRequest` → (host 응답) → `classic.Notification(permission_prompt)` → `classic.PostToolUse` → `tool.call` 끝. **Spinner 는 한 번도 그려지지 않았다.** `permission_prompt` 알림은 요청 약 6초 뒤에 오는 "아직 기다림" 알림이다(터미널 spike 와 같은 간격).
- 그러니 SDK host 에서도 이벤트만으로는 입력 대기로 가야 한다. 남은 원인 후보는 데스크톱이 창을 띄운 동안 Spinner 를 그려서 "입력 대기 뒤 Spinner = 답함" 규칙이 곧바로 작업 중으로 바꾸는 것이다 — 데스크톱은 여기서 확인할 수 없다.
- 조정: 질문은 이벤트 순서가 아니라 **열린 질문 기록(`asks`)이 있는 동안 입력 대기**로 판정한다. 기록은 모드가 `tool.call{AskUserQuestion}` 의 시작·끝에서 직접 열고 닫으므로 화면의 Spinner 규칙과 무관하다. 권한 요청은 같은 방법이 없어(승인 순간 이벤트 없음) 지금 규칙을 유지하고, 데스크톱 동작은 사용자 확인으로 남긴다.

## 추가 spike (Fable 자문 이후)

- 모드 툴 훅이 `next(e)` 를 부르면 core 권한 단계가 돈다 — manual 에서 툴 입력이 보이는 권한 창이 떴다. 그러나 허용하면 "답한 훅이 없음", 거부하면 "사용자가 원하지 않음" 오류로 끝나고, 둘 다 `isError` 라 **오류 글로만 구분된다.**
- 권한 창이 열린 채로 `$.turn.abort` → 창이 닫히고 턴이 `aborted` 로 끝난다. → 입력 대기(권한) 카드에도 중지를 줄 수 있다.
- `$.session.repo()` 는 `{ root, remote, internal, name }` 이고 `root` 는 worktree 라도 메인 작업 트리다. `name` 은 내부 레포가 아니면 null 이라 지금 카드 이름이 대부분 cwd 이름이었다.
- `prompt.submit` 의 `origin.kind`: `composer`·`bridge`·`sdk`·`task-notification`·`scheduled-trigger`·`plugin` 등.
- `$.fs` 에는 삭제가 없다. classic 훅 입력에는 `permission_mode` 가 있다.
- `claude --bg` 의 short id 는 session id 의 접두다 (`claude agents --json`).

## 설계 조정 (자문 반영)

| 선행 plan | 조정 | 이유 |
|---|---|---|
| 모드 툴 `kanban_send`·`kanban_spawn` | **CLI 명령 `atelier kanban send`·`spawn` 을 오케스트레이터 스킬이 Bash 로 부른다.** 모드 툴은 읽기 전용 `kanban_board` 하나만 | 모드 툴은 권한 단계를 건너뛰거나(답할 때) 결과를 글로만 구분한다(next 할 때). Bash 는 manual·auto 분류기·bypass·allow 규칙을 그대로 따른다 — 사용자 결정 "권한 모드를 따름" |
| 요청을 store 키로 전달 | CLI 가 `<config>/atelier/kanban/inbox/<sid>/<id>.json` 에 temp→rename 으로 쓴다. 받는 모드는 `$.fs.list` 로 보고, 파일이 있을 때만 `atelier kanban inbox take` 를 부른다 | CLI 는 store 를 쓸 수 없다. rename 은 원자적이라 반쯤 쓴 파일을 읽지 않는다. 만료·작업 중 거절·삭제를 CLI 가 한다 |
| 만료·중복 없음 | 요청에 `at`·`id`. `take` 는 파일을 먼저 지우고(최대 1회) TTL(60초) 지난 것·작업 중일 때 온 프롬프트를 거절한다. 결과(`delivered`·`busy`·`expired`)를 기록에 남겨 카드에 보인다 | 죽었다 살아난 세션의 늦은 실행, 동시 보내기의 중복 실행 |
| 중지 키 `kanban:stop:<sid>:<turn_id>` | 같은 inbox 의 `stop` 요청(`turn_id` 포함). 입력 대기 카드에도 중지 | 위와 같은 전달 경로. 권한 창이 열려도 중지가 된다 |
| 권한 승인 `[허용]`·`[거부]` | 뺀다 | spike 3 |
| 레포 = `repo.name` | `repo_root`(`repo.root`)·`remote` 를 기록하고 이름·묶음·spawn 위치를 이것으로 | `name` 은 거의 늘 null |
| spawn 허용 = 보드의 레포 | `spawn --cwd` 는 그 경로가 git 메인 작업 트리 루트일 때만, `--worktree` 이름은 `[a-z0-9-]+` 만. 프롬프트 앞에 늘 머리말을 붙여 `-` 로 시작하지 않는다. short id 를 돌려준다 | 경로·옵션 주입 방지. 세션 수 상한·같은 체크아웃 회피는 스킬 절차 |
| `last_answer` 만 | 질문·권한·결정 요청 시점에 마지막 응답 글을 `pending_context` 로 | 턴 중간 대기의 맥락은 이전 턴 응답이 아니다 |
| 부제목 = 태그 걷기 | 사람 프롬프트(`composer`·`bridge`·`sdk`)만 기록, CLI 가 `<system-reminder>` 를 걷고 `first_prompt` 가 비면 `last_prompt` 로 | task-notification 오염, 500자 잘림 뒤 걷기 문제 |
| 상세 전체 | CLI 가 줄 수를 정해 자른다(요청 3줄·응답 6줄·할 일은 진행 항목 주변). 세로 배치에서는 상세가 보드 자리를 대신하고 `[뒤로]` | dock pane 폭 |
| 권한 모드 비교로 보내기 제한 | 하지 않는다. 대상 세션의 `permission_mode` 를 보드에 보이고, 보내기 자체가 보내는 쪽 권한 확인을 거친다 | 보내는 글은 사람 또는 분류기가 승인한다. 비교 규칙은 auto 작업자를 막는다 |
| 작업자 완료 알림 | v1 은 보드를 다시 읽는다 | 범위 |

## 권한 승인 재검토 (사용자 "권한 요청 허용/거부는 못해?" 이후)

- 앞선 결론(불가)은 `classic.PermissionRequest` 의 `next(e)` 와 경주시키려던 시도였다. 훅이 `next` 없이 기다리면 **엔진은 훅을 기다리는 동안에도 자기 권한 창을 띄워 두고**, 훅이 `decision` 을 돌려주는 순간 창을 닫고 그 결정을 쓴다("Allowed by PermissionRequest hook"). 창에서 사람이 먼저 답하면 엔진은 훅을 기다리지 않고 진행한다.
- 훅 예산은 `$` 호출이 걸려 있는 동안 멈춘다. `$.process.run` 으로 2초씩 기다리는 호출을 이어 20초 넘게 기다려도 끊기지 않았다.
- 첫 시도였던 `$.ui.ask` 대기 수단은 버렸다 — 사람이 창에서 먼저 답하면 대기하던 질문 창이 뒤늦게 뜨고, 데스크톱처럼 질문을 앱이 그리는 곳에서는 별도 카드가 생길 수 있다. UI 가 없는 CLI 대기(`inbox wait-permit`)로 바꿨다.
- SDK host 모드: 앱은 `can_use_tool` 요청을 그대로 받고, 훅이 먼저 답하면 엔진이 앱에 `control_cancel_request` 를 보낸다. 데스크톱 화면에서의 확인은 사용자 몫으로 남긴다.
- 결정: 권한을 묻는 세션은 권한 id 를 기록하고 `atelier kanban inbox wait-permit` 을 2초씩 이어 부른다. pane 의 허용·거부는 `atelier kanban permit` 이 남긴다. 도구 결과·턴 끝이 오면 기다림을 멈춘다. 상한은 약 30분.
