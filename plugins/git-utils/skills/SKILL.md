---
name: git
description: ALWAYS use this skill for ANY git-related task (commit, branch, PR, status, diff, log, conflict resolution). Provides automatic quality validation and enforces project conventions. Use git-utils CLI for all operations.
allowed-tools: Bash
---

# Git Workflow Skill

## Description

현재 프로젝트의 **형상관리 워크플로우**를 자동화합니다. 브랜치 생성, TODO별 커밋, PR 생성, **rebase conflict 해결**을 프로젝트 컨벤션에 맞게 처리합니다.

**이 스킬이 실행되는 경우:**

- "커밋 만들어줘", "변경사항 커밋해줘"
- "PR 만들어줘", "Pull Request 생성"
- "브랜치 만들어줘"
- "충돌 해결해줘", "conflict 해결"
- 작업 완료 후 자동 커밋/PR 필요 시

---

## GitHub 환경 설정

`gh` CLI 명령 실행 전 환경변수를 로드합니다:

```bash
[ -f ~/.git-workflow-env ] && source ~/.git-workflow-env
```

- `/setup`으로 `~/.git-workflow-env` 생성 (GH_HOST 등)
- GitHub Enterprise 사용 시 필수

---

## git-utils CLI

모든 git 워크플로우 작업은 `git-utils` CLI로 처리합니다. `/setup` 실행 시 `~/.local/bin/git-utils`에 설치됩니다.

```bash
git-utils --version    # 버전 확인
git-utils --help       # 전체 도움말
```

### 1. 브랜치 생성

```bash
git-utils branch <branch-name> [--base=<branch>]
```

**예시:**

```bash
# 기본 브랜치(main/master) 기반
git-utils branch feature/user-auth
git-utils branch feat/WAD-0212

# 특정 브랜치 기반
git-utils branch feature/user-auth --base=develop
git-utils branch fix/hotfix --base=release/1.0
```

**동작:** fetch → base checkout → pull → checkout -b (자동)

**출력 (JSON):** `{ "branchName": "...", "baseBranch": "..." }`

### 2. 커밋 생성

```bash
git-utils commit <type> <description> [--scope=<s>] [--body=<b>] [--skip-add]
```

**예시:**

```bash
# Jira 브랜치 (feat/wad-0212)에서
git-utils commit feat "implement user authentication"
# → [WAD-0212] feat: implement user authentication

# 일반 브랜치에서 scope 지정
git-utils commit feat "implement user authentication" --scope=auth
# → feat(auth): implement user authentication

# 상세 설명 포함
git-utils commit feat "add authentication" --scope=auth --body="- Add JWT tokens"
```

**지원 타입:** `feat`, `fix`, `docs`, `style`, `refactor`, `test`, `chore`, `perf`

**자동 처리:** Jira 티켓 감지, 커밋 형식 선택, 변경사항 스테이징, Claude Code 서명 추가

**출력 (JSON):** `{ "subject": "...", "jiraTicket": "WAD-0212" }`

### 3. PR 생성

```bash
git-utils pr <title> [--description=<d>]
```

**예시:**

```bash
git-utils pr "Implement user authentication"
git-utils pr "Fix memory leak" --description="Added proper cleanup in worker threads"
```

**자동 처리:** 기본 브랜치 감지, Jira 티켓 포함, push + PR 생성

**출력 (JSON):** `{ "url": "...", "title": "...", "baseBranch": "..." }`

### 4. 미해결 리뷰 조회

```bash
git-utils reviews [pr-number]
```

**출력 (JSON):** PR 제목, URL, 리뷰 쓰레드 목록

### 5. Default Branch Guard

```bash
git-utils guard <write|commit> --project-dir=<p> --create-branch-script=<s> [--default-branch=<b>]
```

- 기본 브랜치에서 차단 시 exit 2, 통과 시 exit 0

### 6. Hook 관리

```bash
git-utils hook register <hookType> <matcher> <command> [--timeout=<n>] [--project-dir=<p>]
git-utils hook unregister <hookType> <command> [--project-dir=<p>]
git-utils hook list [hookType] [--project-dir=<p>]
```

---

## 워크플로우 예시

### Jira 티켓 작업 (feat/wad-0212)

```bash
git-utils branch feat/wad-0212
git-utils commit feat "implement user authentication"
# → [WAD-0212] feat: implement user authentication
git-utils pr "Implement user authentication"
# → PR 제목: [WAD-0212] Implement user authentication
```

### 일반 Feature 작업

```bash
git-utils branch feature/user-auth
git-utils commit feat "implement user authentication" --scope=auth
# → feat(auth): implement user authentication
git-utils pr "Add user authentication"
```

---

## 브랜치 명명 규칙

### Jira 티켓 브랜치

**지원 패턴:** `WAD-0212`, `feat/WAD-0212`, `feat/wad-0212` (자동 대문자 변환)

**커밋 형식:** `[TICKET] type: description`

### 일반 브랜치

| 타입     | 패턴         | 예시                | 커밋 형식                      |
| -------- | ------------ | ------------------- | ------------------------------ |
| 기능     | `feature/*`  | `feature/user-auth` | `feat(scope): description`     |
| 수정     | `fix/*`      | `fix/memory-leak`   | `fix(scope): description`      |
| 문서     | `docs/*`     | `docs/api-guide`    | `docs(scope): description`     |
| 리팩터링 | `refactor/*` | `refactor/cleanup`  | `refactor(scope): description` |
| 성능     | `perf/*`     | `perf/optimize`     | `perf(scope): description`     |
| 테스트   | `test/*`     | `test/unit`         | `test(scope): description`     |

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
git-utils branch feature/my-work
git-utils commit feat "my changes"
git-utils pr "My feature"
# → Merge 승인 대기 → /git-sync → git branch -d feature/my-work
```

---

## Conflict 해결

Rebase 중 충돌이 발생하면 `/git-resolve` 커맨드를 사용합니다.

```bash
/git-resolve              # 충돌 상태 확인 및 대화형 해결
/git-resolve --continue   # rebase 계속
/git-resolve --abort      # rebase 취소
/git-resolve --skip       # 현재 커밋 건너뛰기
```

---

## 관련 커맨드

| 커맨드 | 설명 |
|--------|------|
| `/unresolved-reviews [PR]` | 미해결 리뷰 조회 |
| `/check-ci [PR]` | CI 실패 분석 |
| `/git-sync [branch]` | 브랜치 전환 및 동기화 |
| `/git-branch [name]` | 대화형 브랜치 생성 |
| `/commit-and-pr` | 커밋 + PR 자동 생성 |
| `/merge-pr` | PR 머지 및 정리 |
| `/git-resolve` | Rebase conflict 해결 |
| `/setup` | 플러그인 초기 설정 |
| `/hook-config` | Guard hook 관리 |

---

## 핵심 원칙

1. **작은 단위 커밋**: TODO별로 명확한 진행상황 추적
2. **PR Only**: 모든 병합은 반드시 Pull Request를 통해서만
3. **git-utils CLI 사용**: 복잡한 git 명령어 대신 `git-utils` CLI 활용
4. **Rebase 우선**: merge 대신 rebase로 깔끔한 히스토리 유지

---

## Default Branch Guard (PreToolUse Hook)

기본 브랜치에서 Write/Edit 도구 사용 또는 git commit 시도 시 **즉시 차단**하고 브랜치 생성을 제안합니다.

| Hook | Matcher | 차단 대상 |
|------|---------|----------|
| Write/Edit Guard | `Write\|Edit` | 파일 생성/수정 |
| Commit Guard | `Bash` | `git commit` 명령 |

### 동작 방식
1. PreToolUse hook → `git-utils guard write` 또는 `git-utils guard commit` 실행
2. 현재 브랜치가 기본 브랜치(main/master)인지 확인
3. 기본 브랜치이면 exit 2로 차단 → Claude가 `git-utils branch`로 새 브랜치 생성
4. 브랜치 이동 후 재시도 시 pass

### 특징
- 네트워크 호출 없이 로컬 캐시만 사용 (빠름)
- rebase/merge/detached HEAD 상태에서는 건너뜀
- 기본 브랜치 감지 실패 시 차단하지 않음 (안전)
- Bash hook은 `git commit` 명령만 차단하고 다른 명령은 통과
