# discord-connector 스레드마다 worktree 로 격리한다

> **Plan — 이 시점의 결정 기록.** 현재 정책의 신뢰 소스가 아니다.
> 날짜: 2026-10-07 · 브랜치: kys0213/discord-connector-thread-worktree · 이슈: 없음 · 승인 출처: 사용자 승인 (설계안 제시 후 "진행해줘")

## 요구사항

1. 같은 저장소에 매핑된 채널의 서로 다른 스레드가 동시에 돌아도 작업 트리를 서로 덮어쓰지 않는다.
2. 같은 스레드의 이어가기는 그 스레드의 작업 공간을 계속 쓴다.
3. git 저장소가 아닌 디렉토리에 매핑된 채널도 지금처럼 동작한다.

## 현재 상태 (사이드이펙트 조사)

- 런처는 CLI 가 넘긴 `cwd` 를 스레드 첫 실행 때 기록하고 이어가기에 계속 쓴다. 스레드별 잠금은 같은 스레드 안의 동시 실행만 막는다. 그래서 product-idea·product-issues·product-dev 세 채널을 `~/workspace/areum-lab` 하나에 매핑하면 서로 다른 스레드가 같은 메인 checkout 에서 동시에 파일·브랜치를 바꿀 수 있다.
- `claude` CLI 에 `-w, --worktree [name]` 이 있다. 실험(Claude Code 2.1.290, 임시 저장소) 결과:
  - `<저장소 최상위>/.claude/worktrees/<name>` 에 생기고 브랜치는 `worktree-<name>` 이다. Claude 의 cwd 도 그 경로다.
  - 결과 JSON 에 worktree 경로 필드는 없다.
  - 같은 이름으로 다시 실행하면 기존 worktree 를 재사용한다. 오류나 새 이름은 없다.
  - `--resume <sid>` 는 어디서 실행해도 그 세션의 worktree 로 돌아가 이전 대화를 기억한다. `--worktree <같은 이름>` 을 함께 붙여도 같다. worktree 가 디스크에 있을 때만 확인했다.
  - 같은 저장소에서 서로 다른 이름 두 개를 동시에 실행해도 둘 다 성공한다. stdin 이 없으면 "no stdin data received" 경고가 난다.
  - git 저장소가 아니면 exit 1 과 `Can only use --worktree in a git repository` 로 실패한다.
  - `-p` 종료 후에도 worktree·브랜치·변경 파일이 남는다. lock 은 죽은 pid 기준이라 지우려면 `git worktree unlock` 후 `remove --force`, `branch -D` 가 필요하다. `claude rm` 은 백그라운드 세션용이라 쓸 수 없다.
- `areum-lab` 은 `.gitignore` 의 `.claude/*` 로 `.claude/worktrees/` 를 이미 무시한다.

## 결정

1. **CLI 가 넘긴 `cwd` 가 git 저장소면** 스레드 첫 실행에 `--worktree discord-<스레드ID>` 를 붙인다. 저장소가 아니면 지금처럼 그 디렉토리에서 실행한다.
2. **worktree 경로는 규칙으로 계산해 기록한다** — `<git rev-parse --show-toplevel>/.claude/worktrees/discord-<스레드ID>`. 이어가기는 기록한 경로에서 `--resume <sid> --worktree discord-<스레드ID>` 로 실행한다.
3. **기록한 작업 공간이 사라졌으면** 기본값으로 대신하지 않고 스레드에 오류를 알린다 (기존 실패 정책과 같다).
4. **`claude` 의 stdin 은 `/dev/null`** 로 연결한다.
5. **정리는 자동으로 하지 않는다.** README 에 수동 정리 명령을 둔다. 대상 저장소가 `.claude/worktrees/` 를 무시하지 않으면 그 저장소의 `.gitignore` 에 추가하라고 안내한다.

## 버린 선택지

- **런처가 `git worktree add` 를 직접 실행** — 내장 옵션으로 위치·이름·재사용이 이미 해결된다.
- **프롬프트로 Claude 에게 worktree 사용을 맡김** — 모델이 어기면 메인 checkout 을 건드린다.
- **일정 기간 뒤 자동 정리** — 며칠 뒤 이어가는 스레드나 커밋 안 된 변경이 사라질 수 있다.
- **플러그인이 대상 저장소의 `.git/info/exclude` 를 고침** — 다른 저장소 설정을 대신 바꾸는 일이고, `areum-lab` 은 이미 `.gitignore` 로 처리돼 있다.

## 구현 계획

1. 런처: git 저장소 판별, `--worktree` 인자, worktree 경로 기록, 이어가기 인자, stdin `/dev/null`.
2. 테스트: git 저장소 / 비저장소 / 이어가기 / 작업 공간 소실. 가짜 `claude` 로 인자·cwd 를 검증하고 실제 `claude` 로 임시 저장소에서 한 번 확인.
3. spec·README 반영.

## 확인 방법

- `node --test 'plugins/discord-connector/tests/**/*.test.mjs'`
- `CLAUDE_CODE_ENABLE_FUNCTION_HOOKS=1 claude plugin test plugins/discord-connector`
- `claude plugin validate plugins/discord-connector`
- 실제 Discord: 같은 저장소에 매핑된 채널에서 스레드 두 개를 동시에 돌려 서로 다른 worktree 에서 실행되는지, 재멘션이 같은 worktree 로 이어지는지 확인.
