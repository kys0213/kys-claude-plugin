# Discord 에서 Claude 에게 일을 시키는 connector 플러그인

> **Plan — 이 시점의 결정 기록.** 현재 정책의 신뢰 소스가 아니다.
> 날짜: 2026-10-04 · 브랜치: kys0213/디스코드-connector-구현 · 이슈: 없음 · 승인 출처: 사용자 승인 (설계안 제시 후 "초안 구현 후 테스트 계획도 같이 고려해줘")

## 요구사항

1. 트리거는 둘이다 — 아무 채널에서 봇을 멘션한 메시지, 설정한 "이슈 채널" 의 최상위 새 글. 스레드 안에서는 봇을 멘션한 답글에만 반응한다.
2. 최초 메시지와 스레드 대화 이력을 Claude 에게 넘기고, 결과는 스레드에 답한다.
3. 트리거마다 headless `claude -p` 를 새로 띄우고, 턴이 끝나면 종료한다. 다음 멘션은 `--resume <session_id>` 로 이어간다.
4. 작업 디렉토리는 채널별 고정 매핑, 매핑이 없으면 기본 디렉토리.
5. 실행 중 `AskUserQuestion` 은 스레드에 버튼으로 올리고, 답을 받아 같은 실행 안에서 이어간다.
6. Discord 에서 시작한 작업만 다룬다. 터미널 세션에는 영향이 없어야 한다.

## 현재 상태 (사이드이펙트 조사)

- Discord 연결은 `discord` CLI(areum-lab-tools/tools/discord, Rust)가 맡는다. 데몬이 게이트웨이를 소유하고 CLI 와 SQLite 를 공유한다. 같은 봇 토큰으로 게이트웨이를 두 군데서 열면 interaction 응답이 경쟁한다.
- CLI 쪽 추가 작업(메시지 감지, `on_message` exec 훅, `send --split`, `ask --allowed-user`, multiSelect)은 areum-lab-tools 세션이 맡는다. 이 plan 의 범위가 아니다.
- 이 저장소에 데몬을 가진 플러그인은 없다. function hook 선례는 atelier 의 `hooks/register.ts` (`plugins/atelier/hooks/hooks.json:3`) 이고, 테스트는 `claude plugin test` (`.github/workflows/validate.yml`) 로 돈다.
- 새 플러그인은 `.claude-plugin/marketplace.json` 등록과 `validate.yml` 단계 추가가 필요하다. release.yml 은 바뀐 `plugins/*` 를 자동 감지한다.
- 실험 결과 (Claude Code 2.1.289, `--plugin-dir`):
  - `AskUserQuestion` 은 `--permission-prompt-tool stdio` 를 붙여야 `-p` 의 도구 목록에 들어온다.
  - `tool.call` hook 이 `next` 없이 `{ result: { questions, answers } }` 를 돌려주면 Claude 가 답을 받아 이어간다. SDK 식 `next({...e, answers})` 는 `Stream closed` 로 실패한다.
  - hook 안에서 `$.process.run` 으로 기다리면 180초도 끊기지 않는다(`timeoutMs` 최대 10분). `$.clock.sleep` 은 hook 예산 10초에서 끊긴다.
  - auto 모드에서는 일반 도구에 `PermissionRequest` 가 발화하지 않는다. 자동 판단 모델이 허용하거나 거부하고, 거부 시 `PermissionDenied` 가 발화한다.
  - `--model haiku` 에서는 `--permission-mode auto` 가 default 로 적용됐다. sonnet 은 auto.
  - `--resume <session_id>` 로 이전 대화가 이어진다.

## 결정

1. **새 플러그인 `plugins/discord-connector`**, Rust 바이너리 없이 Node 22 로 만든다.
2. **진입점은 exec 훅** — `discord` 데몬이 메시지를 감지하면 `on_message = ["~/.local/bin/discord-connector-launch"]` 를 실행하고 메시지 JSON 을 stdin 으로 넘긴다. 런처는 메시지당 한 번 실행된다.
3. **판단 로직은 런처 `bin/launch.mjs` 에 모은다** — 매핑·잠금·이력 조립·실행·결과 게시. hook 은 `discord` CLI 만 부르는 얇은 층으로 둔다 (`.claude/rules/tool-layer-boundary.md`).
4. **런처 흐름**
   1. stdin JSON 을 받아 자신을 분리 실행하고 즉시 종료한다 — 데몬 재시작이 실행 중 작업을 끊지 않게.
   2. 스레드가 아니면 `discord thread create --from-message` 로 만든다.
   3. 스레드별 잠금을 잡는다. 이미 실행 중이면 "이전 작업이 끝난 뒤 다시 멘션해 주세요" 를 올리고 끝낸다.
   4. `discord read <thread>` 로 이력을 읽는다 — 첫 실행은 전체, 이어가기는 마지막 처리 메시지 이후만.
   5. `claude -p --permission-mode auto --permission-prompt-tool stdio --output-format json --model <설정, 기본 sonnet> [--resume <sid>]` 를 매핑된 디렉토리에서 실행한다. env 로 `DISCORD_CONNECTOR_THREAD_ID`, `DISCORD_CONNECTOR_REQUESTER_ID` 를 넘긴다.
   6. 종료 후 `session_id`·마지막 처리 메시지 ID 를 저장하고 `discord send --split` 으로 결과를 올린 뒤 잠금을 푼다. 실패하면 오류 요약을 올린다.
5. **hook 은 env 가 있을 때만 동작한다** — `DISCORD_CONNECTOR_THREAD_ID` 가 없으면 그대로 통과한다.
   - `AskUserQuestion`: 질문마다 `discord ask create <thread> --allowed-user <요청자>` → `$.process.run` 으로 `ask wait --timeout 540` → `{ result: { questions, answers } }`. multiSelect 는 CLI 결과 형식 확정 전까지 직접 입력으로 받는다. 응답이 없으면 "응답 없음" 을 답으로 돌려준다.
   - `PermissionDenied`: 거부된 도구와 사유를 `discord send` 로 스레드에 알린다.
6. **설정·상태는 `~/.areum/discord-connector/`** — `config.json`(채널→디렉토리, 기본 디렉토리, 모델), 스레드별 `session_id`·커서, 잠금 파일. 이슈 채널 목록은 CLI 설정에 둔다.
7. **`/discord-connector:setup`** 이 `~/.local/bin/discord-connector-launch` shim 을 설치하고 `on_message` 설정 줄을 안내한다 — 플러그인 버전 경로를 CLI 설정에 박지 않기 위해서다.

## 버린 선택지

- **connector 가 `discord listen` 을 상주 구독(`serve`)** — 상주 프로세스가 하나 더 생긴다.
- **CLI 데몬이 `claude -p` 를 직접 실행** — CLI 가 Claude 에 묶인다.
- **상주 Claude 세션의 mod 가 트리거까지 처리** — 세션에 프롬프트를 넣는 기능이 미확인이고, 모든 스레드가 한 세션에 섞이며 단일 장애점이 된다.
- **`Stop` hook 으로 프로세스를 붙잡아 턴 사이 대기** — 다음 멘션까지 몇 시간이 걸릴 수 있고, 데몬 재시작에 끊기며, 연속 block 한도가 있다.
- **런처를 Rust CLI 로** — CLI 작업은 discord CLI 쪽 세션이 맡기로 했다.
- **권한 승인 버튼(default 모드 또는 거부 후 재승인)** — auto 모드에서는 승인 지점이 생기지 않는다는 게 실험으로 확인됐고, default 모드는 쓰기마다 버튼이 뜬다. 거부 알림 + 다음 멘션으로 재지시하는 방식을 택했다.

## 구현 계획 · 작업 순서

1. 플러그인 골격·런처·hook·setup 명령·런처 테스트·hook 테스트를 한 작업 단위로 구현한다 (파일 집합이 서로 얽혀 있어 나누지 않는다).
2. 저장소 연동 — `marketplace.json` 등록, `validate.yml` 에 런처 테스트와 `claude plugin validate`·`claude plugin test` 단계 추가.
3. CLI 쪽 기능이 릴리스되면 계약 확인 후 통합·실환경 테스트를 진행한다 (아래 테스트 계획 3·4단계).

## 범위 밖

- `discord` CLI 변경 전부.
- 권한 승인 버튼.
- 터미널에서 띄운 세션의 Discord 알림.
- multiSelect select menu 대응 — CLI 결과 형식 확정 후.

## 테스트 계획

1. **런처 블랙박스 테스트 (`node --test`, CLI 릴리스와 무관)** — 가짜 `discord`·`claude` 실행 파일을 임시 `PATH` 앞에 두고 호출 인자와 stdin 을 기록한다. 상태 디렉토리는 임시 HOME 으로 격리한다.
   - 최상위 메시지 → 스레드 생성 후 그 스레드로 실행·게시
   - 스레드 안 메시지 → 스레드 생성 없이 실행
   - 채널 매핑 있음/없음 → cwd 가 매핑 디렉토리/기본 디렉토리
   - 첫 실행 → `--resume` 없음, `session_id` 저장 / 두 번째 실행 → `--resume <저장값>`, 커서 이후 이력만 프롬프트에 포함
   - 같은 스레드 잠금 보유 중 → 실행하지 않고 대기 안내 게시
   - `claude` 비정상 종료·JSON 파싱 실패 → 오류 요약 게시, 잠금 해제
   - 결과 게시는 `send --split` 으로 한 번
   - env `DISCORD_CONNECTOR_THREAD_ID`·`DISCORD_CONNECTOR_REQUESTER_ID` 전달, 필수 플래그(`--permission-prompt-tool stdio`, `--permission-mode auto`) 포함
2. **hook 테스트 (`claude plugin test`)** — atelier `plugins/atelier/tests/register.test.ts` 와 같은 하네스.
   - env 없음 → `next` 그대로 통과
   - `AskUserQuestion` → 질문 수만큼 `ask create`(`--allowed-user` 포함)·`ask wait` 호출, `{ result: { questions, answers } }` 형식
   - `ask wait` 시간 초과·실패 → "응답 없음" 답
   - `PermissionDenied` → `discord send` 호출 후 원래 흐름 유지
3. **CLI 통합 테스트 (CLI 릴리스 후, 수동 스크립트)** — 실제 `discord` CLI 로 계약을 확인한다. 데몬 없이 가능한 범위는 `--json` 출력 스키마(`thread create`, `read`, `send --split`, `ask create/wait`)를 런처 파서에 넣어 보는 것까지.
4. **실환경 E2E (테스트 서버, 수동 체크리스트)** — 노트북 전용 봇 토큰, 테스트 길드에서.
   - 멘션 → 스레드 생성·답장 / 이슈 채널 새 글 → 답장 / 스레드 안 멘션 없는 답글 → 무반응
   - 스레드 안 재멘션 → 이전 대화 기억(`--resume`)
   - `AskUserQuestion` 유도 프롬프트 → 버튼 표시, 요청자 외 클릭 거부, 답 반영
   - 연속 멘션 → 잠금 안내
   - 데몬 재시작 중 실행 → 작업 완료·결과 게시
   - auto 거부 유도(외부 업로드성 명령) → 거부 알림 게시
   - 터미널 세션에서 `AskUserQuestion` → hook 무개입

## 확인 방법

- `node --test plugins/discord-connector/tests/`
- `claude plugin validate plugins/discord-connector` · `claude plugin test plugins/discord-connector`
- `make validate-ci`
