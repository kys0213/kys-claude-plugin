---
related_paths: []
---

# Claude Code 의존 동작

> 플러그인이 기대는 Claude Code 의 동작이에요. **출처: Claude Code 2.1.289 실험 기준**이고, 버전이 바뀌면 다시 확인해야 해요.

## 역할

- 어떤 Claude Code 동작에 기대는지, 그 때문에 어떤 제약이 생기는지를 한곳에 모아요.

## 의존 동작과 영향

| 확인된 동작 | 플러그인에 미치는 영향 |
|-------------|------------------------|
| `AskUserQuestion` 은 `--permission-prompt-tool stdio` 를 붙여야 headless(`-p`) 도구 목록에 들어와요 | 플러그인은 headless 실행을 `claude -p --permission-mode auto --permission-prompt-tool stdio --output-format json --model sonnet` 으로 해요. 이어가기 때는 `--resume <session_id>` 를 더해요. `--permission-prompt-tool stdio` 는 실행 때마다 필요해요. 빠지면 Claude 가 질문을 못 해요 |
| 도구 호출을 가로채는 hook 이 `{result:{questions, answers}}` 를 돌려주면 Claude 가 답을 받아 이어가요 | 질문 중계는 이 형식으로 답해야 해요. 다른 형식은 `Stream closed` 로 실패했어요 |
| hook 이 외부 프로세스를 기다리는 상한은 10분이에요 (그보다 짧은 대기는 hook 시간 예산 10초에서 끊겨요) | 버튼 질문 마감은 10분 이하여야 해요. 플러그인은 9분으로 해요 |
| `--permission-mode auto` 에서는 일반 도구에 승인 지점(`PermissionRequest`)이 없어요. 자동 판단 모델이 허용·거부를 정하고, 거부되면 `PermissionDenied` 가 발생해요 | 승인 버튼은 만들 수 없고 거부 알림만 가능해요 |
| `--model haiku` 에서는 auto 가 적용되지 않고 default 모드가 됐어요 (sonnet 은 auto) | 모델은 `--model sonnet` 으로 고정해요. 설정 항목이 아니에요. haiku 에서는 쓰기마다 승인이 필요해져요 |
| `--resume <session_id>` 로 이전 대화가 이어져요 | 스레드별 대화 이어가기의 근거예요 ([conversation-session](conversation-session.md)) |

## 정책

- 플러그인은 Discord 에서 시작한 실행임을 식별하는 표식(스레드·요청자 정보)이 있을 때만 개입해요. 없으면(터미널 세션 포함) 그대로 통과해요.
  - 왜: Discord 에서 시작하지 않은 세션에 영향을 주지 않기 위해서예요.
- 질문 마감은 9분이고, 안내에 마감 시각을 표시해요.
- multiSelect 질문은 `ask create --multi-select` 로 올려요. 답은 라벨들을 하나의 답 문자열로 합쳐 돌려줘요.
- 질문 응답이 없으면(마감 지남·`ask` 실패) 실행을 멈추지 않고 "Discord에서 시간 안에 답이 없었어요" 를 답으로 돌려줘요. 이후 판단은 Claude 에 맡겨요.
- 권한 거부가 일어나면 거부된 도구와 사유를 스레드에 알려요. 사용자는 다시 멘션해서 재지시해요.
- Discord 실행에서는 백그라운드 실행을 막고 포그라운드로 다시 하게 해요. Discord 에는 알리지 않아요.
  - 왜: 턴이 끝나면 프로세스가 종료돼 백그라운드 결과가 사용자에게 돌아오지 않아요 (실환경 E2E 관측).

## 에러 처리

- Claude 가 비정상 종료하거나 결과를 해석할 수 없으면 [failure-policy](failure-policy.md) 를 따라요.

## 제약 조건

- 위 표는 실험 결과예요. 공식 보장이 아니므로 Claude Code 업그레이드 후 같은 항목을 재확인해야 해요.

## 관련 문서

| 문서 | 관계 |
|------|------|
| [DESIGN.md](../DESIGN.md) | 버린 선택지(권한 승인 버튼) |
| [discord-cli-contract.md](discord-cli-contract.md) | `ask` 마감과 연결 |
| [../flows/02-question-during-run.md](../flows/02-question-during-run.md) | 질문 중계 흐름 |
