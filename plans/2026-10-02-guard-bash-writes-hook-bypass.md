# guard 가 Bash 파일 쓰기와 hook 우회를 잡게 한다

> **Plan — 이 시점의 결정 기록.** 현재 정책의 신뢰 소스가 아니다.
> 날짜: 2026-10-02 · 브랜치: claude/project-thread-izyu65 · 이슈: #791, #899, #865 · 승인 출처: 사용자 승인 (2026-10-02 설계안 제시 후 "진행해줘"; 설계는 협의체 2라운드 검증 pass 를 거침)

## 요구사항

1. **#791** — hook 우회는 브랜치와 무관하게 막는다: git commit/push/merge/am/rebase 의 `--no-verify`(위치 무관, 축약 포함), commit 의 `-n`(묶음 `-an` 포함), `core.hooksPath` 변경(`git -c`, `--config-env`, `git config` 쓰기, `GIT_CONFIG_*` env), `HUSKY=0`·`SKIP=` 접두. `git log -n`, `git push -n` 은 통과한다.
2. **#899** — 보호 브랜치에서 Bash 로 저장소 파일을 바꾸는 명령을 잡는다. 인라인 분석이 가능한 쓰기는 차단, 내부를 볼 수 없는 스크립트 실행은 확인 요청(ask). 저장소 밖 대상은 통과. 목표는 가드레일 수준이다.
3. **#865** — 차단 메시지에 현재 브랜치, 대상 저장소(worktree) 경로, 발동 규칙(기본 브랜치 pin/감지 · develop · 추가 보호), "가드를 의심하기 전에 브랜치부터 확인", 해소 명령을 싣는다.

## 현재 상태 (사이드이펙트 조사)

- 등록은 바꿀 필요가 없다 — 모든 Bash 호출이 이미 `guard commit` 을 거친다 (`git/commands/guard_setup.rs` 의 `GUARD_TARGETS`, 이 repo 의 `.claude/settings.json`). 그 결과 이 repo 의 main/develop 에서도 Bash `rm`·`sed -i` 가 막히기 시작한다 — 의도한 변화다.
- `git/core/guard.rs` 의 Commit 사전 필터는 커밋이 아닌 명령을 git 호출 없이 통과시킨다. 이 "무관한 명령은 subprocess 0회" 특성을 유지한다.
- `strip_quoted` 는 따옴표 구간을 공백으로 지워 경로를 잃는다 — 경로를 보존하는 lexer 가 필요하다. #754 회귀 테스트는 그대로 통과해야 한다.
- git 은 project_dir 에 고정돼 있어, 다른 worktree 로의 쓰기가 메인 checkout 브랜치로 판정된다.
- 출력은 `GuardOutput.allowed: bool` 과 exit 0/2 뿐이다. ask 를 내려면 3상태와 stdout JSON 이 필요하다.
- 공식 hooks 문서 확인 사항(협의체 검증자): PreToolUse payload 에 top-level `cwd` 가 있다 · `permissionDecision: "ask"` 는 exit 0 + stdout JSON 으로 동작한다 · ask 의 `permissionDecisionReason` 은 사용자에게만 보이고 Claude 에는 안 보인다 · exit 2 는 deny 와 같다. 무인 모드(auto · bypassPermissions · `-p` · subagent)에서 ask 의 처리는 문서에 없다.

## 결정

1. **순수 분류기를 분리한다** — `git/core/bash_classifier.rs`. 입력(command · cwd · HOME)만으로 결과가 정해지는 변환이다. lexer 는 따옴표·`$'…'`·`#` 주석·heredoc(줄 단위로 본문을 잘라낸 뒤 토큰화)·분리자·redirect·서브셸을 다루고, `cd` 는 같은 셸의 다음 segment 에만 적용한다. 래퍼(`env` `sudo` `nohup` `timeout` `time` `command` `xargs` `find -exec` `bash -c`)는 깊이 2 까지 안쪽 명령을 재귀 분류한다. 명령별 규칙은 정적 표로 둔다(새 명령은 한 줄 추가).
2. **판정 순서를 고정한다** — ① hook 우회는 무조건 Block(git 호출 없음) ② 커밋·쓰기·스크립트 hit 가 저장소 안에 하나도 없으면 Allow(git 호출 없음) ③ 대상의 저장소 root 별로 기존 게이트(work-tree · default 감지 · rebase/merge · detached · 보호 판정) ④ 보호 브랜치에서 커밋·쓰기는 Block, 스크립트·미해결 대상은 `--opaque-exec <block|ask|allow>`(기본 ask) ⑤ root 별 결과 중 가장 강한 것.
3. **스크립트(O) 정책** — 범용 인터프리터가 스크립트 파일이나 인라인 코드(`-c`/`-e`)를 실행하면 O 다. 빌드·테스트·패키지 도구(`cargo`, `npm/pnpm/yarn run|test|install|ci`, `npx`, `pnpm dlx`, `make`, `python -m pytest|unittest`, `node --test`)는 파일을 쓸 수 있어도 신뢰해 통과시키고 알려진 한계로 문서화한다. O 의 anchor 는 스크립트 경로가 아니라 그 segment 의 실효 cwd 다 — 스크립트가 플러그인 캐시에 있어도 판정은 작업 저장소 기준이다(#899 사고 재현 방지).
4. **대상 저장소 기준 판정** — 대상 경로의 repo root 가 `find_repo_root(project_dir)` 와 같으면 주입된 git 과 `--default-branch` pin 을 쓰고, 다르면 그 root 에 고정한 git(팩토리 주입)으로 판정한다. 같은 저장소의 다른 worktree(`.git` 파일의 gitdir 이 같은 common dir)면 pin 을 적용하고, 다른 저장소면 감지값만 쓴다. Write 도구도 같은 규칙을 따른다.
5. **출력** — `GuardVerdict{Allow, Ask, Block}` 과 `ProtectionRule` enum 을 둔다. Block 은 지금처럼 exit 2 + stderr, Ask 는 exit 0 + stdout `hookSpecificOutput`(`permissionDecision: "ask"`, 같은 안내를 `permissionDecisionReason` 과 `additionalContext` 양쪽에). guard 는 `permission_mode` 를 읽지 않는다 — 정책은 인자로만 받는다(CLI 결정성).
6. **lexer 실패 시 보수적으로** — 닫히지 않은 따옴표 등으로 해석에 실패하면 기존 정규식 커밋 검사와 함께 cwd 기준 O hit 를 더해, 보호 브랜치에서는 최소 ask 가 된다.
7. **메시지는 render 함수 하나** — Write/Bash 공통. hook 우회 차단은 별도 `[Hook Guard]` 메시지(브랜치 무관 명시).

## 버린 선택지

- **스크립트 실행 기본 block** — node/python 테스트 실행까지 막혀 오탐이 크다.
- **스크립트 실행 기본 allow** — #899 사고(스크립트 경유 수정)를 그대로 재현한다.
- **guard 가 `permission_mode` 를 보고 무인 세션에서 동작을 바꿈** — 같은 입력에 다른 출력을 내는 추론이라 CLI 원칙에 어긋난다. `--opaque-exec` 인자로 대체.
- **`git pull/merge/reset/restore/checkout --` 를 쓰기로 분류** — 보호 브랜치 동기화의 정상 흐름이다. `git stash pop/apply` 만 쓰기로 본다.
- **hook 우회 opt-out 플래그** — #791 이 "검토" 수준이고 쓰는 곳이 없다(YAGNI).
- **`GuardOutput.allowed` 를 유지하고 `allowed()` 도우미 추가** — Ask 가 allowed 인지 모호하고 기존 `assert!(allowed)` 가 Ask 를 조용히 덮는다.
- **CLI 타깃 `bash` 별칭 추가** — setup 마이그레이션 범위가 커진다.
- **다른 저장소에도 project pin 적용** — #810 처럼 다른 저장소의 기본 브랜치를 잘못 보호하게 된다.

## 구현 계획 · 작업 순서

1. 분류기(순수 모듈 + 블랙박스 테스트) 와 출력 3상태 배선(동작 불변) 을 병렬로.
2. 분류기를 guard 에 통합 — 단일 저장소 판정, payload `cwd`, `--opaque-exec`.
3. 대상 저장소 기준 판정(팩토리 주입) 과 #865 메시지.
4. 계약 문서 갱신 — `skills/git/SKILL.md`, `skills/git/references/cli-reference.md`, `.claude/rules/tool-layer-boundary.md`.

## 범위 밖

- #830(커밋 type 경고) — 이번 분류 결과(Commit)에 나중에 붙일 수 있다.
- #810 잔여(pin 과 감지의 합집합 보호).
- 신뢰 도구 · alias · 셸 함수 · 스크립트 내부를 통한 우회(알려진 한계).

## 확인 방법

- `cd plugins/atelier/cli && cargo fmt --check && cargo clippy --locked --all-targets -- -D warnings && cargo test --locked`
- `make validate-ci`
- 블랙박스 테스트가 보호 브랜치/feature 브랜치/저장소 밖/다른 worktree/rebase 상태별로 Block·Ask·Allow 와 git 호출 0회를 고정한다.
