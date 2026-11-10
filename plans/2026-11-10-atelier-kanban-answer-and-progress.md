# 세션 칸반에서 질문에 답하고 진행 상황을 보여준다

> **Plan — 이 시점의 결정 기록.** 현재 정책의 신뢰 소스가 아니다.
> 날짜: 2026-11-10 · 브랜치: claude/sweet-bell-0qzbhm · 이슈: 없음 · 승인 출처: 사용자 승인 (2026-11-10 "좋아 진행해줘")
> 선행 기록: `plans/2026-11-05-atelier-session-kanban.md`, `plans/2026-11-05-atelier-session-kanban-spike.md`

## 요구사항

1. 칸반이 보이기만 하고 세션으로 갈 방법이 없다 → 카드에서 바로 무언가 할 수 있어야 한다.
2. 데스크톱에서도 돼야 한다.
3. 세션이 AskUserQuestion 으로 질문하면 칸반에서 질문·선택지를 보면서 답할 수 있다.
4. 카드에 진행 상황(지금 하는 일, 할 일 진척)을 보여준다.

## 검토한 대안과 버린 이유

| 대안 | 버린 이유 |
|---|---|
| 카드를 눌러 그 세션 화면으로 이동 | 모드 API 에 다른 세션으로 화면을 바꾸는 수단이 없다. `$.ui.view` 는 같은 세션의 agent 사이만 바꾼다. 데스크톱 원격 surface 의 `Link` 는 `https:` 만 그린다 |
| tmux·WezTerm·kitty 로 pane 포커스 이동 | 터미널에서만 된다 — 데스크톱 요구(2)를 못 채운다 |
| `claude --resume` 명령 복사 | `$.ui.copy` 는 원격 surface 에 경로가 없다(데스크톱 불가). 살아 있는 세션을 다른 프로세스가 재개하면 transcript 를 두 프로세스가 쓴다 |
| 권한 요청(Bash 허용 등)도 칸반에서 승인 | `tool.check` 의 allow 는 사람이 필요한 도구의 dialog 를 닫지 못하고, `tool.call` 이 대신 결과를 돌려주려면 도구를 직접 실행해야 한다. 이번 범위에서 뺀다 |

## spike 로 확인한 사실 (Claude Code 2.1.322, tmux 대화형 세션)

- `tool.call{AskUserQuestion}` 훅이 `next(e)`(engine dialog)를 띄운 채, dialog 보다 먼저 `{ result: { questions, answers } }` 를 돌려주면 dialog 가 닫히고 모델은 그 답을 사람의 답으로 받는다 ("User answered Claude's questions: … → Blue").
- dialog 가 열린 지 23초 뒤에 바깥 답을 넣어도 훅이 끊기지 않았다 — `next(e)` 가 진행 중인 동안 hook 예산(10초) 시계가 멈춘다.
- dialog 에서 사람이 먼저 답하면 `next(e)` 결과가 그대로 쓰인다.
- 복수 선택은 `"Cat, Fish"` 처럼 `, ` 로 이은 문자열을 답으로 넣으면 그대로 전달된다.
- 바깥 답이 이기면 `classic.PostToolUse` 가 오지 않는다(엔진의 도구 실행 경로를 건너뜀). 그 뒤 Spinner 재그리기도 기록되지 않았다 → 작업 재개를 알리는 사실을 모드가 직접 남겨야 한다.

## 결정

### 질문 답하기

- 질문한 세션(B)의 모드가 `tool.call{AskUserQuestion}` 에서 질문을 자기 기록의 `asks[tool_use_id]` 에 남기고, `next(e)` 와 "칸반 답" 을 경주시킨다.
- 칸반을 연 세션(A)의 pane 이 입력 대기 카드의 질문을 펼쳐 그리고(선택지 `Button`, 직접 입력 `Input`, 보내기 `Button`), 보내기 때 `atelier kanban answer` 로 선택을 답 배열로 바꿔 `kanban:answer:<B>:<tool_use_id>` 에 쓴다.
- B 는 2초 주기에서 자기가 기다리는 키만 읽는다. 키 이름에 tool_use_id 가 들어 있어 끝난 질문의 답은 읽히지 않는다(비교 로직 없음).
- 답은 질문 순서의 배열로 주고받고, B 가 자기 `e.questions` 로 질문 문장 키를 붙인다 — 저장 크기 때문에 질문을 잘라 저장해도 답의 키가 어긋나지 않는다.
- 바깥 답이 이기면 B 가 `kanban.answer` 사실을 기록하고, CLI 는 이를 작업 중으로 판정한다.
- 남은 답 키(경합으로 늦게 쓴 답)는 그 세션이 만료될 때 세션 기록과 함께 지운다.

### 진행 상황

- 모드는 메인 루프 도구 호출마다 `activity = { tool, input, at }` 를 남긴다. `input` 은 문자열 최상위 필드만, 각 200자로 자른다(저장 크기 상한일 뿐 판단이 아니다). 턴이 끝나면 지운다.
- 할 일: `TodoWrite` 는 `todos` 를 통째로 바꾸고, `TaskCreate`·`TaskUpdate` 는 `tasks[id]` 를 병합한다.
- 무엇을 어떻게 한 줄로 보일지(어느 필드를 고를지, 경로는 파일 이름만, 완료 수/전체 수, 진행 중 할 일)는 `atelier kanban board` 가 `progress` 문자열로 정한다.

### 기록 형식

- 새 필드(`asks`·`activity`·`todos`·`tasks`)는 모두 선택 필드다. `v` 는 1 그대로 — 예전 CLI 는 모르는 필드를 무시하고, 새 CLI 는 없는 필드를 비어 있음으로 읽는다.

## 작업 순서

1. spec 갱신 (`plugins/atelier/spec/concerns/session-kanban.md`)
2. CLI: `board` 카드에 `progress`·`asks`, `answer` 서브커맨드 — 블랙박스 테스트 먼저
3. 모드: 기록·답 전달·pane 그리기 — `claude plugin test` 먼저
4. README, tmux e2e 로 확인
