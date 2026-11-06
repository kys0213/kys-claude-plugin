# 세션 칸반 spike 결과와 설계 조정

> **Plan — 이 시점의 결정 기록.** 현재 정책의 신뢰 소스가 아니다.
> 날짜: 2026-11-05 · 브랜치: claude/stoic-archimedes-uysvw9 · 이슈: 없음 · 승인 출처: 사용자 승인 (2026-11-05 "실제 검증해보면서 자율주행해줘. 의도한대로 구현되는지가 가장 중요")
> 선행 기록: `plans/2026-11-05-atelier-session-kanban.md`

## spike 로 확인한 사실 (Claude Code 2.1.318, 클라우드 컨테이너)

- `claude -p` 두 개를 동시에 돌리고 `--plugin-dir` 로 같은 모드를 로드 → 한 세션의 `$.store.set` 이 다른 세션의 다음 `$.store.get`/`keys` 에 바로 보인다.
- tmux 대화형 세션에서 `$.command.register({ name: 'kanban' })` 는 접두 없이 `/kanban` 이 된다. `$.ui.open` 은 `{ isPlaced: true }`, `$.ui.panes()` 는 `isShown` 을 준다. `Box row` 4컬럼이 dock pane 에 나란히 그려진다.
- `classic.PermissionRequest`(`tool_name`) → 약 6초 뒤 `classic.Notification`(`notification_type: permission_prompt`) 순으로 온다. 턴이 끝나고 약 60초 뒤 `classic.Notification`(`idle_prompt`) 이 온다.
- **권한 확인에 답한 순간을 알리는 이벤트는 없다.** 모든 이벤트(`!fs.*`)를 기록해 봤지만 승인과 `classic.PostToolUse` 사이에 아무것도 오지 않았다. 대신 dialog 가 열려 있는 동안 `ui.render{Spinner}` 가 그려지지 않고, 승인 후 약 30ms 안에 다시 그려진다. 턴이 끝난 뒤에도 한 번 그려지는 경우가 있다.
- AskUserQuestion 의 질문 화면도 `classic.PermissionRequest(tool_name: AskUserQuestion)` 으로 온다. `tool.call{AskUserQuestion}` 직후 dialog 가 뜨기 전에 Spinner 가 한 번 그려진다.
- `turn.start`/`turn.complete` 는 subagent 루프에서도 `agentId` 와 함께 온다.
- 모듈을 다시 로드하면 `session.start` 가 다시 돈다. `session.end` 는 `sessionId` 를 준다.
- `claude plugin validate` 는 `on()` 이벤트 이름이 리터럴이 아니거나 `$` 를 최상위가 아닌 함수에 넘기면 거부한다.

## 설계 조정

| 선행 plan | 조정 | 이유 |
|---|---|---|
| store 에 `last_event` 하나 | 최근 이벤트 8개(`events`) | 상태를 바꾸지 않는 이벤트(`auth_success` 알림 등)가 직전 상태를 덮지 않게, 고르는 판단을 CLI 가 한다 |
| stale = 이벤트 없이 10분 + 작업 중/입력 대기 | 30초 heartbeat, 2분 끊기면 응답 없음 | 오래 걸리는 도구·오래 기다리는 입력을 죽은 세션과 구분한다. 프로세스 생존이 기준이다 |
| expire = 이벤트 없이 24시간 | heartbeat 24시간 또는 `ended_reason` | 같은 이유 |
| Notification → 입력 대기 | `notification_type` 별로: `permission_prompt`·`elicitation_dialog` → 입력 대기, `idle_prompt` → 쉬는 중, 나머지 → 상태 없음 | `idle_prompt` 가 입력 대기로 잘못 간다 |
| `tool.call{AskUserQuestion}` → 질문 | `classic.PermissionRequest(AskUserQuestion)` → 질문, `tool.call` 기록 제거 | 질문 화면도 권한 요청이고, `tool.call` 직후 Spinner 가 잠깐 작업 중으로 튀게 한다 |
| (없음) | `ui.spinner` 사실: 기록된 이벤트 하나마다 Spinner 첫 그리기를 한 번 기록. CLI 는 입력 대기 바로 다음일 때만 작업 중으로 본다 | 승인 후 긴 도구가 끝날 때까지 입력 대기로 남는 문제. 턴 뒤 Spinner 는 무시해야 한다 |
| 나이 = 마지막 상태 이벤트부터 | 같은 상태가 이어지면 처음 시각 유지 | `idle_prompt`·`PostToolUse` 반복이 머문 시간을 리셋한다 |
| subagent 구분 없음 | `turn.*` 는 `agentId` 없을 때만 기록 | subagent 턴 종료가 메인을 쉬는 중으로 바꾼다 |
| `session.start` 마다 기록 | 기록이 없을 때만 `session.start` 를 더함 | 리로드가 작업 중 세션을 쉬는 중으로 되돌린다 |
| 좁은 pane: 세로로 쌓기 | 세로일 때는 테두리 없이 제목 + 들여쓴 카드 | 테두리가 줄을 먹어 마지막 컬럼이 밀려난다 |
| CI 고정 2.1.283 + 플래그 | 2.1.318 고정, 플래그 제거 | 2.1.287 부터 플래그가 무시된다. discord-connector 도 2.1.318 에서 validate·test 통과 확인 |

## e2e 확인 (tmux 대화형 세션 3개, 빌드한 `atelier` 를 PATH 에)

- 세 세션이 쉬는 중으로 올라온다.
- 권한 확인 대기 → 입력 대기(`권한 요청: Bash`), 승인 → 작업 중, 끝 → 쉬는 중. 1초 안에 빠르게 승인해도 늦은 알림이 되돌리지 않는다.
- AskUserQuestion → 입력 대기(`질문`), 답 → 작업 중.
- `kill -9` 한 세션 → 약 2분 뒤 응답 없음.
- `/exit` 한 세션 → 카드가 사라진다.
- 90칸 터미널 → pane 이 프롬프트 위로 가고 컬럼이 세로로 쌓인다. `/kanban` 두 번 → 닫힘·다시 열림.
