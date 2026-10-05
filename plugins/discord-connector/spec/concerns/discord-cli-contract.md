---
related_paths: []
---

# discord CLI 계약

> 플러그인이 `discord` CLI 에 기대하는 입력·명령·출력 형식이에요. CLI 의 내부 동작은 다루지 않아요.

## 역할

- CLI 는 Discord 연결과 작업 디렉토리 선택을 맡아요. 게이트웨이(실시간 연결)는 CLI 데몬 하나만 열어요.
- 플러그인은 이 문서의 계약에만 의존해요. 계약이 깨지면 플러그인이 아니라 CLI 쪽 문제로 봐요.

## 진입: `on_message` 훅

- CLI 데몬이 메시지를 감지해 필터를 통과시키면 `on_message` 에 설정된 실행 파일을 **메시지당 한 번** 실행하고, 메시지 JSON 을 stdin 으로 넘겨요.
- CLI 는 `on_message` 를 **적용된 작업 디렉토리를 cwd 로 해서** 실행해요.
- 훅 실행 환경에는 `DISCORD_BOT_TOKEN` 환경변수가 넘어오지 않아요. 훅 stdout 은 버려지고, stderr 는 백그라운드 데몬이라 보이지 않아요.
  - 영향 1: 플러그인이 `discord` 명령을 부르려면 토큰이 CLI config 파일에 있어야 해요 (`discord init`). 사전 조건이에요.
  - 영향 2: 플러그인은 자체 로그를 남겨요 ([failure-policy](failure-policy.md)).
- `on_message` 는 argv 배열 하나예요. 플러그인은 설정 명령(`/discord-connector:setup`)으로 고정된 실행 경로를 설치하고 설정 줄을 안내해요.
  - 왜: 플러그인 버전이 담긴 경로를 CLI 설정에 박으면 업데이트 때마다 깨져요.

### stdin JSON

| 필드 | 값 |
|------|----|
| `trigger` | `"mention"` 또는 `"issue_channel"` |
| `guild_id` | 서버 ID |
| `channel_id` | 메시지가 있는 채널 ID (스레드 안이면 스레드 ID) |
| `parent_channel_id` | 스레드 안이면 부모 채널 ID, 아니면 `null` |
| `is_thread` | 스레드 안 메시지인지 |
| `message_id` | 메시지 ID |
| `content` | 본문 |
| `author` | `{id, username, bot}` |
| `timestamp` | 작성 시각 |
| `bot_user_id` | 봇 사용자 ID |
| `message_reference` | 답글이면 참조 정보, 아니면 `null` |
| `cwd` | CLI 가 정한 작업 디렉토리. 심볼릭 링크를 푼 정규화 절대경로 (마지막 필드) |

### 필터는 CLI 가 맡아요

| 메시지 | 훅 호출 |
|--------|---------|
| 봇이 쓴 메시지 (`trigger_bots` 에 없는 봇) | 안 해요 |
| `trigger_bots` 목록의 봇(자기 자신 포함)이 쓴 메시지 | 사람과 같은 필터를 거쳐요 |
| 스레드 안 + 봇 멘션 | 해요 |
| 스레드 안 + 멘션 없음 | 안 해요 |
| 이슈 채널의 최상위 새 글 | 해요 (`trigger: "issue_channel"`) |
| 그 외 채널의 봇 멘션 | 해요 (`trigger: "mention"`) |

- 이슈 채널 목록과 `trigger_bots` 는 CLI 설정에 둬요. 플러그인은 목록을 몰라요.
- 발행되는 메시지는 type 0(일반)·19(답장)뿐이에요. 시스템 메시지는 발행하지 않아요.
- 이슈 채널은 텍스트 채널만 지원해요. 포럼 채널은 지원하지 않아요.
- `trigger_bots` 때문에 `author.bot: true` 이벤트가 올 수 있어요. 플러그인은 사람 이벤트와 같게 처리해요.
  - 왜: 봇 여부로 분기하면 플러그인이 CLI 설정의 의미를 따로 알아야 해요.
- 왜 CLI 가 거르나: 플러그인이 모든 메시지마다 실행되면 안 되고, 채널 설정이 한 곳에 있어야 해요.

## 의존하는 명령

모든 명령은 `--json` 으로 호출해요.

| 명령 | 쓰임 | 결과 `data` |
|------|------|-------------|
| `thread create` | 최상위 메시지에서 스레드 만들기 | `{thread_id, name}` |
| `send --split` | 결과·안내·알림 게시 | `{messages: [{message_id, channel_id, timestamp, attachments}]}`. 2000자 초과 본문은 CLI 가 나눠 보내고, 조각이 하나여도 배열이에요 |
| `ask create --allowed-user <숫자 ID>` | 스레드에 질문 올리기 | `{ask_id, status:"pending", channel_id}` |
| `ask wait` | 질문의 응답 기다리기 | 아래 표 |

- `--allowed-user` 는 반복할 수 있어요. 숫자가 아니면 Usage 오류예요.
- 허용 목록 밖 사용자가 클릭하면 그 사람에게만 "이 질문에 답할 수 있는 사용자가 아닙니다." 를 안내하고, 질문은 pending 으로 남아요.
- 스레드 ID 를 대상으로 `ask create` 가 동작해요 (실환경 확인됨).
- 메시지에서 만든 스레드의 `thread_id` 는 원 메시지 ID 와 같았어요 (실측). 플러그인은 이 동일성에 의존하지 않아요.

### `ask create` 질문 형식

| 질문 | 옵션 | 선택지 | 라벨 |
|------|------|--------|------|
| 단일 선택 | (기본) | 1~4개 | 80자 이하 |
| multiSelect | `--multi-select` (선택 메뉴) | 1~25개 | 100자 이하 |

- `--allow-text` 와 함께 쓸 수 있고, 먼저 채택된 답이 이겨요.

### `ask wait` 결과

| `status` | 의미 | 추가 필드 |
|----------|------|-----------|
| `answered` | 응답 받음 | `kind: "choice"\|"multi_choice"\|"text"`, `value`, `answered_by`, `answered_at` |
| `timed_out` | 마감 지남 | 없음 |
| `pending` | 아직 응답 없음 | 없음 |

- `kind: "multi_choice"` 의 `value` 는 선택한 라벨의 배열이에요. 나머지 `kind` 의 `value` 는 문자열이에요.

### `--json` 엔벨로프

```json
{"ok": true, "command": "..", "data": {}}
{"ok": false, "command": "..", "error": ".."}
```

- `ok:false` 는 실패예요. 플러그인은 이를 성공으로 재해석하지 않고 [failure-policy](failure-policy.md) 에 따라 드러내요.

## 작업 디렉토리는 CLI 가 정해요

- 채널→디렉토리 매핑과 기본 디렉토리는 CLI 설정 `~/.areum/discord/config.json` 에 있어요. 플러그인은 설정 파일을 갖지 않아요.
  - `workdirs`: `{ "<채널 ID 또는 channels 별칭>": "<디렉토리>" }`
  - `default_workdir`: 매핑이 없을 때 쓰는 기본 디렉토리
  - 경로는 절대경로이거나 `~/` 로 시작해야 해요.
- 스레드 안 메시지는 `parent_channel_id`, 최상위 메시지는 `channel_id` 로 매핑을 찾고, 없으면 기본값을 써요.
  - 왜: 매핑은 사람이 채널 단위로 정해요. 스레드는 매번 새로 생겨요.
- 매핑된 디렉토리가 실제로 없으면 기본값으로 대신하지 않고 훅을 실행하지 않아요. 데몬 stderr 로그만 남고 스레드 알림은 없어요.
  - 왜: 엉뚱한 디렉토리에서 Claude 가 일하면 안 돼요.
- `on_message` 가 있는데 `workdirs` 매핑도 `default_workdir` 도 없으면 데몬 시작 오류예요.
- 플러그인이 이 값을 어떻게 쓰는지는 [conversation-session](conversation-session.md) 이에요.

## 에러 처리

- CLI 호출 실패(`ok:false`, 비정상 종료, 형식 불일치)는 계약 위반으로 보고 스레드에 드러내요.
- 훅 입력이 위 필드를 만족하지 않으면 실행하지 않고 오류로 끝내요.

## 제약 조건

- 게이트웨이를 여는 주체는 CLI 데몬 하나예요. 플러그인은 게이트웨이를 열지 않아요.
- 이 계약은 CLI 쪽 구현(메시지 감지, `on_message` 훅, `send --split`, `ask --allowed-user`, `cwd` 전달)이 릴리스돼야 실환경에서 성립해요. 구현 전에는 계약 문서로만 존재해요.

## 미결

없음

## 검증 항목

- multi-select 에서 2개 이상 선택한 응답은 실환경에서 아직 관측하지 못했어요. 설계 미결이 아니라 확인할 항목이에요.

## 관련 문서

| 문서 | 관계 |
|------|------|
| [DESIGN.md](../DESIGN.md) | 책임 경계 |
| [claude-code-dependencies.md](claude-code-dependencies.md) | 질문 마감 시간과 연결 |
| [../flows/02-question-during-run.md](../flows/02-question-during-run.md) | `ask` 명령이 쓰이는 흐름 |
