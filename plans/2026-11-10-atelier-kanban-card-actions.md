# 세션 칸반 카드에서 컬럼별로 보고·행동하고, 오케스트레이터가 칸반으로 작업자를 부린다

> **Plan — 이 시점의 결정 기록.** 현재 정책의 신뢰 소스가 아니다.
> 날짜: 2026-11-10 · 브랜치: claude/zen-feynman-otrusm · 이슈: 없음 · 승인 출처: 사용자 승인 (2026-11-10 결정 질문 응답 — 권한 승인 "spike 후 되면 포함", 오케스트레이터 "새 세션도 띄우기", 보내기 확인 "권한 모드를 따름")
> 선행 기록: `plans/2026-11-05-atelier-session-kanban.md`, `plans/2026-11-05-atelier-session-kanban-spike.md`, `plans/2026-11-10-atelier-kanban-answer-and-progress.md`

## 요구사항

1. 칸반이 컬럼 이름과 카드 한 줄만 보여서 가시성이 떨어진다. 컬럼마다 맞는 기능이 있어야 한다.
2. **입력 대기** — 어떤 대화였는지 보고, 칸반에서 답(입력)할 수 있어야 한다.
3. **작업 중** — 무엇을 하는지 상세를 보고, 중지를 요청할 수 있어야 한다.
4. **쉬는 중** — 칸반에서 작업을 요청할 수 있고, 직전에 한 작업이 보여야 한다.
5. 메인 오케스트레이터가 칸반을 보고 레포별 프로젝트에 맞는 작업자를 실행할 수 있어야 한다 — 쉬는 세션에 보내거나, 없으면 새 세션을 띄운다.

## 현재 상태 (사이드이펙트 조사)

- 카드 부제목이 `<system-reminder> You are operating in a git worktree…` 로 보인다 (사용자 스크린샷, 데스크톱). 데스크톱이 프롬프트 앞에 붙이는 안내문이 `first_prompt` 로 저장된다. 기록이 500자로 잘려 안내문만 남는 경우도 있다.
- 모드 API (Claude Code 2.1.322 types):
  - `$.prompt.submit({ text })` — 자기 세션에 사람이 친 것과 같은 프롬프트를 낸다. 쉬는 중이면 새 턴. 호출한 훅만 빼고 모든 훅을 거친다 → 칸반 자신의 `prompt.submit` 기록 훅은 그 프롬프트를 못 본다.
  - `$.turn.abort({ turnId })` — 메인 루프의 실행 중인 턴을 끊고 실행 중 도구도 멈춘다. `turnId` 가 지금 턴이 아니면 reject.
  - `turn.complete` 의 `answer` — 그 턴의 마지막 응답 텍스트. 직전 작업 표시에 쓴다.
  - `classic.PermissionRequest` 훅은 `decision: { behavior: 'allow' | 'deny' }` 로 답할 수 있다. 이미 열린 권한 창을 바깥 답으로 닫을 수 있는지는 모른다 (AskUserQuestion 은 `tool.call` 의 `next(e)` 와 경주시켜 됐다).
  - `$.tool.register` — 모델이 부르는 `mcp__atelier__<name>` 툴. `tool.call` 훅이 답한다.
  - `$.session.send({ to: { sessionId } })` — 다른 세션에 peer 메시지. 사람의 프롬프트가 아니라 peer 로 읽힌다.
- `claude --bg [prompt]` — 백그라운드 대화형 세션을 띄우고 바로 돌아온다. `claude attach` 로 열 수 있다. `$.process.run` 의 자식은 모듈이 내려가면 죽지만, `--bg` 는 띄운 뒤 끝나므로 영향이 없을 것으로 본다 (spike).
- 지금 있는 것: 질문 답하기(`kanban:answer:<sid>:<tool_use_id>`, 2초 polling), 진행 상황 한 줄, `atelier kanban board`·`answer` CLI.

## 검토한 대안과 버린 이유

| 대안 | 버린 이유 |
|---|---|
| 쉬는 세션에 `$.session.send` 로 작업 전달 | 받는 쪽 모델이 사람이 아니라 peer 의 메시지로 읽는다. 사람이 칸반에서 친 요청은 사람의 프롬프트여야 한다 |
| 대화 내용을 pane 이 요청할 때마다 그 세션이 응답(요청-응답 키) | 왕복 2~4초 지연과 키 수명 관리가 늘어난다. 최근 요청·직전 응답만 기록해도 요구를 채우고 크기도 세션당 수 KB 다 |
| 상세 정리(태그 걷기·자르기·상대 시간)를 모드에서 | 경계 규칙 위반. 같은 기록이면 같은 상세여야 하므로 CLI 가 소유한다 |
| 새 세션을 `claude -p` 로 | 질문·권한 창이 없어 칸반에서 답할 수 없고, 사람이 들어가 볼 수도 없다. `--bg` 는 대화형이고 attach 된다 |
| 새 세션 위치를 오케스트레이터가 아무 경로로 | 다른 세션이 이미 쓰는 위치(보드에 오른 레포의 루트)로만 받는다. 판단은 스킬, 허용 범위 확인은 결정적 |
| 칸반에서 세션 화면으로 이동 | 선행 plan 과 같은 이유로 여전히 불가 |

## 결정

### 1. 카드 상세

카드를 누르면 보드 아래에 그 카드의 상세가 펼쳐진다 (하나만, 다시 누르면 닫힘). 질문 답하기 영역은 지금처럼 늘 펼친다.

| 컬럼 | 상세 | 행동 |
|---|---|---|
| 입력 대기 | 최근 요청, 기다리기 직전 응답, 기다리는 것(권한이면 도구와 대상 전문) | 질문: 지금의 답하기. 권한: `[허용]`·`[거부]` (spike 통과 시) |
| 작업 중 | 최근 요청, 할 일 전체(✓ 완료 · ▸ 진행 · · 대기), 최근 도구 5개와 경과 | `[중지]` → `[정말 중지]`·`[취소]` |
| 쉬는 중 | 직전 요청, 직전 응답 앞부분 | 입력 칸 + `[보내기]` |
| 응답 없음 | 마지막으로 알던 요청·응답 | 없음 |

쉬는 중 카드는 부제목 아래에 `직전: <직전 응답 첫 줄>` 을 한 줄 더 보인다.

### 2. 기록 (모드 — 사실만, 모두 선택 필드, `v` 는 1 그대로)

- `last_prompt` — 마지막 사람 프롬프트 앞 2000자 (`prompt.submit`, 칸반이 보낸 것 포함).
- `last_answer` — 메인 `turn.complete` 의 `answer` 앞 2000자.
- `turn_id` — 메인 `turn.start` 의 `turnId`, 턴이 끝나면 지운다.
- `recent` — 메인 도구 호출 최근 5개 (`{tool, input, at}`), 턴 시작에 비운다.
- `permission` — `classic.PermissionRequest` 의 `{ tool, input(문자열 최상위 필드 200자), at }`, 답이 나면 지운다.

### 3. 요청 전달 (store 키 — 질문 답하기와 같은 패턴)

| 키 | 쓰는 쪽 | 받는 쪽 행동 |
|---|---|---|
| `kanban:prompt:<sid>:<nonce>` = `{ text, from, at }` | pane 보내기 / `kanban_send` | `$.prompt.submit`, `prompt.submit` 기록을 직접 남기고 키 삭제 |
| `kanban:stop:<sid>:<turn_id>` | pane 중지 | 그 `turn_id` 가 지금 턴이면 `$.turn.abort`, 아니면 그냥 삭제 |
| `kanban:permit:<sid>:<request_id>` = `allow`\|`deny` | pane 허용/거부 | `classic.PermissionRequest` 훅이 `next(e)` 와 경주 (spike) |

받는 쪽은 2초 tick 에서 `$.store.keys()` 중 자기 접두만 읽는다. 키에 `turn_id`·`request_id` 가 들어 있어 늦게 온 요청이 다음 턴·다음 권한에 붙지 않는다. 만료된 세션의 남은 키는 세션 기록과 함께 지운다.

### 4. CLI (결정적 변환)

- `atelier kanban board` 카드에 `detail`(정리된 상세)과 `actions`(그 컬럼에서 되는 행동: `send`·`stop`·`answer`·`permit`)를 더한다. pane 과 툴이 같은 규칙을 본다.
- 부제목·상세의 프롬프트는 `<system-reminder>…</system-reminder>` 블록(닫히지 않은 것 포함)을 걷은 뒤 접는다.
- `--view repos` — 레포(없으면 cwd)별로 세션을 묶고 각 세션의 컬럼·행동을 주는 보기. 오케스트레이터가 읽는다.
- `atelier kanban check-send --session <id>` / `check-spawn --cwd <path>` 같은 별도 명령은 두지 않는다 — 허용 여부는 `board` 출력의 `actions`·레포 루트로 모드가 확인한다.

### 5. 오케스트레이터 툴 (모드가 `$.tool.register`)

- `kanban_board` (읽기 전용) — `--view repos` 결과를 준다.
- `kanban_send` (쓰기) — `{ session_id, text }`. 그 카드의 `actions` 에 `send` 가 있을 때만 받는다.
- `kanban_spawn` (쓰기) — `{ cwd, text, worktree? }`. `cwd` 가 보드에 오른 레포 루트일 때만 받고 `claude --bg [--worktree <name>] <text>` 를 그 cwd 에서 실행한다. 출력의 세션 id 를 돌려준다.
- 쓰기 툴은 다른 툴과 같은 권한 규칙을 따른다 (기본 모드는 매번 확인, auto 는 분류기).
- 어느 레포의 어느 세션에 무엇을 맡길지, 새로 띄울지는 orchestrator 스킬이 판단한다 — 스킬 references 에 절차를 둔다.
- 받는 세션의 프롬프트 앞에는 `[칸반 요청 · from <레포> 세션]` 한 줄을 붙여 출처를 남긴다.

### 6. spec

`session-kanban.md` 에 카드 상세·행동·오케스트레이터 툴 정책을 더하고, "권한 확인은 그 세션에서 답해야 해요" 제약은 spike 결과에 따라 고친다.

## 남은 위험 — spike 로 먼저 확인

1. 타이머 콜백에서 부른 `$.prompt.submit` 이 쉬는 대화형 세션에서 턴을 시작하는가. 사람이 입력 칸에 쓰는 중일 때 입력이 지워지지 않는가.
2. 타이머 콜백에서 부른 `$.turn.abort` 가 실행 중인 Bash 를 멈추고 세션이 쉬는 중으로 돌아오는가.
3. `classic.PermissionRequest` 훅이 `next(e)`(권한 창)와 바깥 답을 경주시켜 창을 닫을 수 있는가. 10초 훅 예산에 끊기지 않는가. 안 되면 권한 승인은 빼고 상세만 보인다.
4. 모드의 `$.process.run(['claude', '--bg', …])` 가 세션을 띄우고 바로 돌아오는가, 그 세션이 atelier 모드를 로드해 보드에 오르는가, 모듈이 다시 로드돼도 살아 있는가.
5. 모드가 등록한 툴을 메인 모델이 부를 수 있고, 쓰기 툴에 권한 확인이 뜨는가.

## 작업 순서

1. 이 plan 을 `docs(atelier)` 커밋으로 남긴다.
2. spike — 위 위험 1~5 를 tmux 대화형 세션으로 확인하고 결과를 spike plan 으로 남긴다.
3. TDD: CLI(`detail`·`actions`·`--view repos`·태그 걷기) → 모드(기록·요청 전달·pane 상세·툴) → spec·README·orchestrator references.
4. 통합 검증: `cargo fmt --check`·`clippy -D warnings`·`cargo test`, `claude plugin validate`·`test`, tmux e2e.
