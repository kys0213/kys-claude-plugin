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
