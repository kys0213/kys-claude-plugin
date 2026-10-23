# doppel

Claude 세션을 이 컴퓨터의 doppel 데몬에 연결해요. 세션 상태(시작·턴·실패·종료)를 doppel 에 보고하고, 세션이 `AskUserQuestion` 을 내거나 권한 확인이 사람에게 넘어오면 같은 질문을 doppel 에도 올려요. 터미널과 doppel UI 중 **먼저 온 답 하나**로 확정하고, 진 쪽에는 어디서 답했는지 남겨요.

데몬·CLI(`doppel`)와 프로토콜 계약은 areum-lab-tools 에 있어요. 동작의 기준은 그쪽 문서예요.

- 세션 ↔ doppel 프로토콜·relay 계약·데몬 API: [areum-lab-tools `docs/doppel-protocol.md`](https://github.com/kys0213/areum-lab-tools/blob/main/docs/doppel-protocol.md)
- 세션 상태와 전체 구조: [areum-lab-tools `docs/doppel.md`](https://github.com/kys0213/areum-lab-tools/blob/main/docs/doppel.md)

## 구성

| 파일 | 역할 |
|------|------|
| `hooks/register.ts` | function hook 어댑터. 세션 이벤트 전송, `doppel attach` 유지, 질문·권한 경주 |
| `hooks/logic.ts` | 요청 본문 만들기, 데몬·CLI 응답 해석 같은 순수 변환 |
| `hooks/reactive.ts` | 이벤트 스트림용 최소 연산자(`Subject`·`map`·`filter`·`scan`·`merge`). rxjs 를 번들하지 않아요 |
| `skills/brief/SKILL.md` | 데몬이 `--brief` 로 부르는 `/doppel:brief`. 질문의 판례 번들을 읽고 추천·새 의견을 JSON 하나로 답해요 |
| `skills/triage/SKILL.md` | 데몬이 `--propose` 로 부르는 `/doppel:triage`. 들어온 카드의 번들을 읽고 프로젝트·우선순위 제안을 JSON 하나로 답해요 |

## 사전 조건

- **function hooks 플래그**: `CLAUDE_CODE_ENABLE_FUNCTION_HOOKS=1`. 플래그가 없으면 모듈이 로드되지 않고 세션은 평소처럼 돌아요.
- Claude Code 2.1.306 이상 (동작 확인 기준 버전이에요).
- `doppel` 바이너리 (areum-lab-tools): `curl -fsSL https://raw.githubusercontent.com/kys0213/areum-lab-tools/main/scripts/install.sh | sh -s -- doppel`
- doppel 데몬: `doppel daemon start`

## 동작

| 순간 | 하는 일 |
|------|---------|
| 세션 시작 | `session.start` 보고, `doppel attach --session <id>` 를 세션 내내 띄워 둠 (끊기면 1초부터 30초까지 늘려 가며 다시 띄움), 30초마다 heartbeat |
| 턴 시작·끝 (main 루프만) | `turn.start`·`turn.complete`(답 앞 300자) 보고 |
| 도구 실패 | `tool.failure` 보고 |
| `AskUserQuestion` | doppel 에 질문을 열고 터미널 대화상자와 `doppel ask wait` 를 경주. 이긴 답으로 확정하고 doppel 에 `close` |
| 권한 확인 | 엔진 판정이 `ask` 일 때만 같은 경주. doppel 에서 허용하면 프롬프트 없이 다시 실행, 거부하면 막음 |
| 세션 종료 | `session.end` 보고, heartbeat 중지 |

- **터미널이 없는 세션**(headless)은 `DOPPEL_ASK_DEFER_AFTER_MS`(기본 120000) 동안 답이 없으면 "판단 보류" 로 답하고 doppel 에 보류를 알려요. 나중에 doppel 에서 답하면 attach 로 답이 와서 새 턴으로 이어가요. 터미널 세션은 대화상자가 떠 있으니 보류하지 않아요.
- **데몬에 닿지 못하면** 아무것도 바꾸지 않아요. 질문·권한은 터미널에서만 받아요.
- **discord-connector 실행**(`DISCORD_CONNECTOR_*` env 가 있는 세션)은 이벤트만 보고하고, 질문·권한은 가로채지 않아요. 그 질문은 discord-connector 가 맡아요.

## 환경 변수

| 이름 | 뜻 |
|------|----|
| `DOPPEL_SOCKET` | 데몬 소켓 경로 (기본 `~/.areum/doppel/doppel.sock`) |
| `DOPPEL_ASK_DEFER_AFTER_MS` | headless 세션의 질문 보류까지 기다리는 시간 |
| `DOPPEL_LAUNCHED` | `1` 이면 doppel 이 띄운 세션으로 보고 (`launched_by: doppel`) |

## 주의

- auto 권한 모드에서는 엔진의 1차 판정이 `ask` 여도 분류기가 곧바로 정해요. 그 사이 doppel inbox 에 질문이 잠깐 올라왔다가 "세션에서 답변됨" 으로 닫힐 수 있어요.
- 권한 질문의 입력 요약(Bash 는 명령, 파일 도구는 경로, 그 밖은 JSON 앞 500자)은 이 모듈이 만들어요.

## 테스트

```bash
CLAUDE_CODE_ENABLE_FUNCTION_HOOKS=1 claude plugin test plugins/doppel
claude plugin validate plugins/doppel
```
