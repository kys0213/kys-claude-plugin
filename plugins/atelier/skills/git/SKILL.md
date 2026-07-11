---
name: git
description: ALWAYS use this skill for ANY git-related task (commit, push, branch, PR, status, diff, log, conflict resolution, unresolved review followup, issue prioritization). Provides automatic quality validation and enforces project conventions. Mechanical calls (guard/hook/reviews) go through the atelier git CLI; commit/branch/PR run as plain git/gh applying the conventions below.
version: 1.0.0
allowed-tools: Bash
---

# Git Workflow Skill

## Description

현재 프로젝트의 **형상관리 워크플로우**를 자동화합니다. 브랜치 생성, TODO별 커밋, push, PR 생성, **rebase conflict 해결**, 미해결 리뷰 정리, 이슈 우선순위 추천을 프로젝트 컨벤션에 맞게 처리합니다.

**이 스킬이 실행되는 경우:**

- "커밋 만들어줘", "변경사항 커밋해줘", "커밋하고 push 해줘"
- "PR 만들어줘", "Pull Request 생성", "커밋하고 PR 까지"
- "브랜치 만들어줘"
- "충돌 해결해줘", "conflict 해결"
- "미해결 리뷰 정리해줘", "리뷰 코멘트 봐줘"
- "뭐부터 작업할까?", "이슈 우선순위 추천해줘"
- 작업 완료 후 자동 커밋/PR 필요 시

---

## GitHub 환경 설정

`gh` CLI 명령 실행 전 환경변수를 로드합니다:

```bash
[ -f ~/.git-workflow-env ] && source ~/.git-workflow-env
```

- 통합 setup 의 git 모듈이 `~/.git-workflow-env` 를 생성 (GH_HOST 등)
- GitHub Enterprise 사용 시 필수

---

## 도구 경계 (CLI vs plain git/gh)

git 연산마다 **정답 도구가 하나**입니다. setup 실행 시 단일 `atelier` 바이너리가 `~/.local/bin/atelier`에 설치됩니다.

| 분류 | 도구 | 연산 |
|---|---|---|
| 조회·plumbing | **plain `git`** | `status`, `diff`, `log`, `push` |
| 컨벤션 쓰기 (정책 적용) | **plain `git` / `gh`** | 커밋·브랜치·PR 생성 |
| 바이너리 필수 (hook·구조화 read) | **`atelier git`** | `reviews`, `guard`, `pr-guard`, `hook` |

| `atelier git` 서브커맨드 | 역할 |
|---|---|
| `atelier git reviews [pr-number]` | 미해결 리뷰 쓰레드 조회 (gh GraphQL → 구조화 JSON) |
| `atelier git guard <write\|commit\|pr>` | 기본 브랜치 보호 / PR 중복 차단 (hook 용) |
| `atelier git hook <register\|unregister\|list>` | settings.json hook 관리 |

> 커밋·브랜치·PR 은 `atelier git` 으로 감싸지 않습니다 — git/gh 가 이미 결정적이라 래핑이 더할 게 없습니다.
> 대신 Jira/Conventional 형식·브랜치 명명·base 분기 같은 **컨벤션을 에이전트가 적용**해 plain git/gh 로 실행합니다.

CLI 인자 형식·출력 계약·git 정책(커밋 형식·브랜치 명명·PR 본문 스타일)·워크플로우 예시는 `references/cli-reference.md` 를 로드합니다.

---

## 커밋·push·PR 워크플로우 (판단 절차)

"커밋해줘 / push 해줘 / PR 만들어줘" 요청 시:

1. **변경사항 확인**: `git status` + `git diff --stat`. 변경이 없으면 안내 후 종료.
2. **안전 검사**: 현재 브랜치가 기본 브랜치(main/master)이면 경고하고 AskUserQuestion 으로 확인 — 거부 시 `git switch -c` 로 새 브랜치 생성을 제안. (PreToolUse guard 가 차단하기도 함.)
3. **커밋 메시지 판단**: 사용자가 메시지를 주면 그대로, 없으면 변경 내용을 분석해 type/scope/description 을 결정. 브랜치명에서 Jira 티켓을 감지해 형식을 선택. 민감 파일(`.env`, `credentials*` 등)은 스테이징에서 제외.
4. **PR pre-check (PR 요청 시)**: diff 스코프를 확정하기 **전에** 브랜치·머지 상태를 검증합니다. 아래 3가지 확인 없이 PR 스코프를 확정하지 않습니다.
   1. **base 와의 관계 확인**: `git fetch origin --prune` 후 `git log --oneline origin/<base>..HEAD` 로 base 에 없는 커밋만 스코프에 담기는지 확인
   2. **이미 머지되었는지 확인**: `gh pr list --state merged --search` + merge-base 비교로 동일 변경이 이미 base(develop/main)에 머지되지 않았는지 확인 — 확인 없이 diff 만 보고 스코핑하면 중복 PR 이 생깁니다
   3. **열린 동일 목적 PR 확인**: `gh pr list --state open` 으로 같은 목적의 PR 이 이미 열려 있는지 확인
   back-merge/hotfix 세션처럼 레포 상태 파악이 흔들렸다면 pre-check 를 처음부터 다시 수행합니다. 상세 명령 예시는 `references/cli-reference.md` §B.3 참조.
5. **실행 (plain git/gh)**: `git add -u && git commit -m "<형식 적용한 subject>"` → (push 요청 시) `git push -u origin {브랜치}` → (PR 요청 시) `gh pr create --title ... --body ...`.
6. **결과 안내**: 커밋 subject / PR URL 을 사용자에게 출력.

> 형식·명명·PR 본문 규칙은 `references/cli-reference.md` §B 참조.

---

## 금지 사항

### 기본 브랜치 직접 수정 금지

```bash
git checkout main
git commit -m "..."        # 금지
git push origin main       # 금지
```

### 올바른 프로세스

```bash
git fetch origin --prune && git switch -c feature/my-work origin/main
git add -u && git commit -m "feat(scope): my changes"
git push -u origin feature/my-work
gh pr create --title "My feature" --body "..."
# → Merge 승인 대기 → git switch <base> && git pull → git branch -d feature/my-work
```

### force-push 금지

hook 차단 여부와 **별개로 에이전트 스스로 지키는 정책**입니다.

```bash
git push --force origin feature/my-work              # 금지
git push --force-with-lease origin feature/my-work   # 금지
```

- force-push 를 시도하지 않습니다. history 재작성이 필요하면 대안을 먼저 제시합니다: `git revert`, 새 커밋으로 수정, 새 브랜치에서 재작업.

### push 권한 거부 시 사용자 위임

- push/force-push 가 permission denied 되면 **같은 명령을 재시도하지 않습니다** — 반복 재시도는 레포 상태 추적만 잃게 합니다.
- 대신 실행할 정확한 명령(브랜치·remote 포함 전체 커맨드)을 정리해 사용자에게 실행을 위임합니다.

### hotfix / back-merge 상태 재확인

- hotfix 는 **target 브랜치(master/main) 기반 워크플로우**를 따릅니다 — develop 기반 워크플로우와 혼동하지 않습니다.
- back-merge 시 `git status`, `git log --graph --oneline` 으로 현재 레포 상태를 먼저 재확인한 뒤 진행합니다.

---

## Default Branch Guard (PreToolUse Hook)

기본 브랜치에서 Write/Edit 도구 사용 또는 git commit 시도 시 **즉시 차단**하고 브랜치 생성을 제안합니다.

| Hook | Matcher | 차단 대상 |
|------|---------|----------|
| Write/Edit Guard | `Write\|Edit` | 파일 생성/수정 |
| Commit Guard | `Bash` | `git commit` 명령 |

1. PreToolUse hook → `atelier git guard write` 또는 `atelier git guard commit` 실행
2. 기본 브랜치이면 exit 2로 차단 → Claude가 `git switch -c`로 새 브랜치 생성 → 재시도 시 pass
3. 네트워크 호출 없이 로컬 캐시만 사용. rebase/merge/detached HEAD 상태와 기본 브랜치 감지 실패 시에는 차단하지 않음 (안전)

hook 의 등록·비활성화·재설정은 통합 setup 의 hook 관리 모드가 담당합니다.

---

## references 로드 가이드

판단·프로토콜이 필요한 git 워크플로우는 아래 references 를 progressive disclosure 로 로드합니다.

| reference | 언제 로드 | 관련 흐름 |
|---|---|---|
| `references/cli-reference.md` | CLI 인자 형식·출력 계약, 또는 커밋 형식·브랜치 명명·PR 본문 정책이 필요할 때 | 커밋/브랜치/PR 실행 |
| `references/conflict-resolution.md` | rebase 충돌의 ours/theirs 반전 gotcha·파일별 분할정복 정책 (mechanical git 은 모델이 직접) | "충돌 해결해줘" / rebase 중 |

> hook·guard·구조화 read(reviews) 만 `atelier git` CLI 로 위임하고, 커밋·브랜치·PR 은 정책을 적용해 plain git/gh 로 실행합니다. references 는 **판단·정책**이 필요한 부분을 담습니다.
> 여러 변경의 머지 조정(순서·worktree 통합)이 필요하면 `orchestrator` skill 의 `references/merge-coordinator.md` 가 canonical 입니다.

---

## 핵심 원칙

1. **작은 단위 커밋**: TODO별로 명확한 진행상황 추적
2. **PR Only**: 모든 병합은 반드시 Pull Request를 통해서만
3. **도구 경계 준수**: hook·guard·reviews 는 `atelier git` CLI, 커밋·브랜치·PR 은 컨벤션을 적용한 plain git/gh
4. **Rebase 우선**: merge 대신 rebase로 깔끔한 히스토리 유지
