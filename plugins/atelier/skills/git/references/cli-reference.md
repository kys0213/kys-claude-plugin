# atelier git CLI 상세 레퍼런스

`atelier git` CLI 의 서브커맨드별 사용법·출력 계약·명명 규칙. 결정적 연산(동일 입력 → 동일 출력)은 전부 이 CLI 가 담당하고, skill/agent 는 인자 결정과 결과 해석만 한다.

```bash
atelier git --version    # 버전 확인
atelier git --help       # 전체 도움말
```

## 1. 브랜치 생성

```bash
atelier git branch <branch-name> [--base=<branch>]
```

**예시:**

```bash
# 기본 브랜치(main/master) 기반
atelier git branch feature/user-auth
atelier git branch feat/WAD-0212

# 특정 브랜치 기반
atelier git branch feature/user-auth --base=develop
atelier git branch fix/hotfix --base=release/1.0
```

**동작:** fetch → base checkout → pull → checkout -b (자동)

**출력 (JSON):** `{ "branchName": "...", "baseBranch": "..." }`

## 2. 커밋 생성

```bash
atelier git commit <type> <description> [--scope=<s>] [--body=<b>] [--skip-add]
```

**예시:**

```bash
# Jira 브랜치 (feat/wad-0212)에서
atelier git commit feat "implement user authentication"
# → [WAD-0212] feat: implement user authentication

# 일반 브랜치에서 scope 지정
atelier git commit feat "implement user authentication" --scope=auth
# → feat(auth): implement user authentication

# 상세 설명 포함
atelier git commit feat "add authentication" --scope=auth --body="- Add JWT tokens"
```

**지원 타입:** `feat`, `fix`, `docs`, `style`, `refactor`, `test`, `chore`, `perf`

**자동 처리:** Jira 티켓 감지, 커밋 형식 선택, 변경사항 스테이징, Claude Code 서명 추가

**출력 (JSON):** `{ "subject": "...", "jiraTicket": "WAD-0212" }`

## 3. PR 생성

```bash
atelier git pr <title> [--description=<d>]
```

**예시:**

```bash
atelier git pr "Implement user authentication"
atelier git pr "Fix memory leak" --description="Added proper cleanup in worker threads"
```

**자동 처리:** 기본 브랜치 감지, Jira 티켓 포함, push + PR 생성

**출력 (JSON):** `{ "url": "...", "title": "...", "baseBranch": "..." }`

**PR 본문 스타일:** 토스 PR 템플릿 4단 고정 (왜 / 무엇을 / 어떻게 / 확인 방법) + 친근한 해요체 단문 + 정보는 개조식. IT 특성화 고등학생도 이해 가능한 수준으로 약어/사내용어는 풀어쓰기. 상세 규칙과 예시는 `.claude/rules/git-workflow.md` 의 "PR 본문 작성 스타일" 참조.

## 4. 미해결 리뷰 조회

```bash
atelier git reviews [pr-number]
```

**출력 (JSON):** PR 제목, URL, 리뷰 쓰레드 목록

> PR 번호 미지정 시 현재 브랜치의 PR 을 자동 감지한다. 결과 해석·후속 액션(리뷰 정리) 제안은 git skill 이 직접 판단한다.

## 5. Tool Guard (branch 보호 · PR 중복)

```bash
atelier git guard <write|commit|pr> --project-dir=<p> --create-branch-script=<s> [--default-branch=<b>]
```

- `write`/`commit`: 기본 브랜치(보호 브랜치)에서 차단 시 exit 2, 통과 시 exit 0
- `pr`: 현재 브랜치에 열린 PR이 있으면 `gh pr create` 차단 (exit 2). branch 옵션 불필요. legacy alias: `atelier git pr-guard`

## 6. Hook 관리

```bash
atelier git hook register <hookType> <matcher> <command> [--timeout=<n>] [--project-dir=<p>]
atelier git hook unregister <hookType> <command> [--project-dir=<p>]
atelier git hook list [hookType] [--project-dir=<p>]
```

> guard hook 의 등록·비활성화·재설정 절차는 통합 setup 의 hook 관리 모드가 담당한다.

---

## 워크플로우 예시

### Jira 티켓 작업 (feat/wad-0212)

```bash
atelier git branch feat/wad-0212
atelier git commit feat "implement user authentication"
# → [WAD-0212] feat: implement user authentication
atelier git pr "Implement user authentication"
# → PR 제목: [WAD-0212] Implement user authentication
```

### 일반 Feature 작업

```bash
atelier git branch feature/user-auth
atelier git commit feat "implement user authentication" --scope=auth
# → feat(auth): implement user authentication
atelier git pr "Add user authentication"
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
