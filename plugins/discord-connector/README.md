# discord-connector

Discord 에서 봇을 멘션하면 Claude 가 일하고 결과를 같은 스레드에 답장해요. 실행 중 Claude 가 질문하면 스레드에 버튼(multiSelect 는 선택 메뉴)이 올라오고, 요청자가 누른 답으로 같은 실행이 이어져요.

제품 동작의 기준은 [`spec/DESIGN.md`](spec/DESIGN.md) 예요.

## 구성

| 파일 | 역할 |
|------|------|
| `bin/launch.mjs` | `discord` CLI 의 `on_message` 훅이 실행하는 런처. 스레드 만들기, 스레드별 잠금, `claude -p` 실행, 결과 게시 |
| `hooks/register.ts`, `hooks/logic.ts` | 실행 중 `AskUserQuestion` 을 Discord 질문으로 중계, auto 모드 권한 거부 알림 |
| `commands/setup.md` | `~/.local/bin/discord-connector-launch` shim 설치와 CLI 설정 안내 |

## 사전 조건

- Node 22 이상, `discord` CLI (릴리스 전 버전은 계약 문서 [`spec/concerns/discord-cli-contract.md`](spec/concerns/discord-cli-contract.md) 기준)
- **봇 토큰은 `discord` CLI 설정 파일(`~/.areum/discord/config.json`)에 있어야 해요** (`echo "$TOKEN" | discord init`). 훅 실행 환경에는 `DISCORD_BOT_TOKEN` 이 넘어오지 않아요.
- `discord` 데몬이 떠 있어야 버튼 질문(`ask`)이 동작해요.

## 설치

1. 마켓플레이스로 플러그인을 설치해요.
2. `/discord-connector:setup` 을 실행해 shim 을 설치해요.
3. `~/.areum/discord/config.json` 에 `on_message`, `default_workdir`, `workdirs` 를 설정하고 데몬을 재시작해요. 예시는 setup 커맨드가 보여 줘요.

## 상태와 로그

`~/.areum/discord-connector/` 아래에 저장해요.

| 경로 | 내용 |
|------|------|
| `threads/<스레드 ID>.json` | `session_id`, 작업 공간 경로(`cwd`), worktree 여부(`worktree`) |
| `locks/<스레드 ID>.lock` | 실행 중인 런처의 pid. 죽은 pid 의 잠금은 다음 실행이 회수해요 |
| `logs/launcher.log` | 런처 로그 (claude JSON 결과 포함). 훅 stdout 은 버려지고 stderr 는 안 보이니 사후 단서는 이 파일뿐이에요 |

## 헤드리스 실행 방식

런처는 아래처럼 `claude` 를 실행해요. 모델은 sonnet 고정이고, `--permission-prompt-tool stdio` 가 있어야 `AskUserQuestion` 이 도구 목록에 들어와요.

```
claude -p <프롬프트> --permission-mode auto --permission-prompt-tool stdio --output-format json --model sonnet [--resume <session_id>] [--worktree discord-<스레드 ID>] --plugin-dir <이 플러그인 루트>
```

`--worktree` 는 작업 디렉토리가 git 저장소일 때만 붙어요. 저장소가 아니면 그 디렉토리에서 바로 실행해요. `claude` 의 stdin 은 `/dev/null` 로 연결해요.

## 스레드별 worktree

작업 디렉토리가 git 저장소면 스레드마다 `<저장소>/.claude/worktrees/discord-<스레드 ID>` 에서 실행해요. 브랜치는 `worktree-discord-<스레드 ID>` 예요. 런처는 worktree 를 자동으로 지우지 않아요. 아래 절차로 직접 정리해요.

- 대상 저장소가 `.claude/worktrees/` 를 무시하지 않으면 그 저장소의 `.gitignore` 에 `.claude/worktrees/` 를 추가해요.
- 남은 worktree 확인: `git -C <저장소> worktree list`
- 정리 (스레드 하나): 아래 명령을 순서대로 실행해요. `<저장소>` 와 `<스레드 ID>` 만 바꿔요.

```bash
git -C <저장소> worktree unlock <저장소>/.claude/worktrees/discord-<스레드 ID>
git -C <저장소> worktree remove --force <저장소>/.claude/worktrees/discord-<스레드 ID>
git -C <저장소> branch -D worktree-discord-<스레드 ID>
```

`remove --force` 는 커밋하지 않은 변경도 지워요. 필요한 변경은 먼저 커밋하거나 옮겨 두세요.

## 주의

- 같은 플러그인을 마켓플레이스로도 설치했다면 `--plugin-dir` 사본이 설치본을 대신해요 (Claude Code 공식 문서의 이름 충돌 규칙: 활성화된 `--plugin-dir` 플러그인이 같은 이름의 설치 플러그인보다 우선). hook 이 두 번 로드되지 않아요. 단 조직 managed settings 가 이 플러그인을 `enabledPlugins` 로 잠가 두면 `--plugin-dir` 사본은 무시되고 설치본 hook 이 로드돼요.
- 런처와 hook 은 같은 표식 env(`DISCORD_CONNECTOR_THREAD_ID`, `DISCORD_CONNECTOR_REQUESTER_ID`, `DISCORD_CONNECTOR_BOT_ID`)가 있을 때만 hook 이 개입해요. 터미널 세션에는 영향이 없어요.
- 봇 멘션 무력화는 런처(`bin/launch.mjs`)와 hook(`hooks/logic.ts`)에 같은 규칙으로 따로 구현돼 있어요. 언어가 달라 코드를 공유하지 못해요. 규칙을 바꾸면 양쪽 테스트를 함께 고치세요.

## 테스트

```bash
node --test 'plugins/discord-connector/tests/**/*.test.mjs'
claude plugin test plugins/discord-connector
claude plugin validate plugins/discord-connector
```

런처 테스트는 가짜 `discord`·`claude` 실행 파일(`tests/fixtures/fake-bin/`)을 PATH 앞에 두고 HOME 을 임시 디렉토리로 바꿔 실행해요. 실제 `discord`·`claude` 는 실행하지 않아요. `claude plugin test` 는 function hook 플래그(`CLAUDE_CODE_ENABLE_FUNCTION_HOOKS=1`)가 필요해요.
