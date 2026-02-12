---
name: git
description: ALWAYS use this skill for ANY git-related task (commit, branch, PR, status, diff, log, conflict resolution). Provides automatic quality validation and enforces project conventions. NEVER use git commands directly.
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
- 스크립트는 자동으로 source하므로 커맨드에서만 수동 source 필요

---

## 스크립트 사용법

모든 작업은 `${CLAUDE_PLUGIN_ROOT}/scripts/` 폴더의 스크립트로 처리됩니다.

### 1. 브랜치 생성

```bash
${CLAUDE_PLUGIN_ROOT}/scripts/create-branch.sh <branch-name> [base-branch]
```

**예시:**

```bash
# 기본 브랜치(main/master) 기반
${CLAUDE_PLUGIN_ROOT}/scripts/create-branch.sh feature/user-auth
${CLAUDE_PLUGIN_ROOT}/scripts/create-branch.sh feat/WAD-0212
${CLAUDE_PLUGIN_ROOT}/scripts/create-branch.sh WAD-0212

# 특정 브랜치 기반
${CLAUDE_PLUGIN_ROOT}/scripts/create-branch.sh feature/user-auth develop
${CLAUDE_PLUGIN_ROOT}/scripts/create-branch.sh fix/hotfix release/1.0
```

**동작:**

- 기본 브랜치(main/master) 또는 지정한 base 브랜치 자동 감지 및 최신화
- uncommitted changes가 있으면 에러로 중단 (안전 모드)
- 새 브랜치 생성 및 체크아웃

### 2. 커밋 생성

```bash
${CLAUDE_PLUGIN_ROOT}/scripts/commit.sh <type> <description> [scope] [body]
```

**예시:**

```bash
# Jira 브랜치 (feat/wad-0212)에서
${CLAUDE_PLUGIN_ROOT}/scripts/commit.sh feat "implement user authentication"
# → [WAD-0212] feat: implement user authentication

# 일반 브랜치 (feature/auth)에서
${CLAUDE_PLUGIN_ROOT}/scripts/commit.sh feat "implement user authentication" "auth"
# → feat(auth): implement user authentication

# 상세 설명 포함
${CLAUDE_PLUGIN_ROOT}/scripts/commit.sh feat "add authentication" "auth" "- Add JWT tokens\n- Add bcrypt"
```

**지원 타입:** `feat`, `fix`, `docs`, `refactor`, `test`, `chore`, `perf`

**자동 처리:**

- ✅ Jira 티켓 자동 감지 (대/소문자 모두 지원)
- ✅ 적절한 커밋 형식 자동 선택
- ✅ 모든 변경사항 자동 스테이징
- ✅ Claude Code 서명 추가

### 3. PR 생성

```bash
${CLAUDE_PLUGIN_ROOT}/scripts/create-pr.sh <title> [description]
```

**예시:**

```bash
${CLAUDE_PLUGIN_ROOT}/scripts/create-pr.sh "Implement user authentication"
${CLAUDE_PLUGIN_ROOT}/scripts/create-pr.sh "Fix memory leak" "Added proper cleanup in worker threads"
```

**자동 처리:**

- ✅ 기본 브랜치 자동 감지
- ✅ Jira 티켓 자동 감지하여 PR 제목에 포함
- ✅ 원격 브랜치 자동 push 후 PR 생성

---

## 워크플로우 예시

### Jira 티켓 작업 (feat/wad-0212)

```bash
# 1. 브랜치 생성
${CLAUDE_PLUGIN_ROOT}/scripts/create-branch.sh feat/wad-0212

# 2. 작업 후 커밋
${CLAUDE_PLUGIN_ROOT}/scripts/commit.sh feat "implement user authentication"
# → [WAD-0212] feat: implement user authentication

# 3. PR 생성 (품질 검증 자동)
${CLAUDE_PLUGIN_ROOT}/scripts/create-pr.sh "Implement user authentication"
# → PR 제목: [WAD-0212] Implement user authentication
```

### 일반 Feature 작업

```bash
# 1. 브랜치 생성
${CLAUDE_PLUGIN_ROOT}/scripts/create-branch.sh feature/user-auth

# 2. 작업 후 커밋
${CLAUDE_PLUGIN_ROOT}/scripts/commit.sh feat "implement user authentication" "auth"
# → feat(auth): implement user authentication

# 3. PR 생성
${CLAUDE_PLUGIN_ROOT}/scripts/create-pr.sh "Add user authentication"
```

---

## 브랜치 명명 규칙

### Jira 티켓 브랜치

**지원 패턴:**

- `WAD-0212` - 티켓 직접 사용
- `feat/WAD-0212` - prefix + 대문자
- `feat/wad-0212` - prefix + 소문자 (자동 대문자 변환)
- `fix/wad-2223`, `docs/proj-123` - 모든 타입 지원

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

### ❌ 기본 브랜치 직접 수정 금지

```bash
# 기본 브랜치에서 직접 커밋/push 절대 금지
git checkout main
git commit -m "..."        # ❌
git push origin main       # ❌
```

### ✅ 올바른 프로세스

```bash
# 1. Feature 브랜치에서 작업
${CLAUDE_PLUGIN_ROOT}/scripts/create-branch.sh feature/my-work
${CLAUDE_PLUGIN_ROOT}/scripts/commit.sh feat "my changes"

# 2. PR 생성
${CLAUDE_PLUGIN_ROOT}/scripts/create-pr.sh "My feature"

# 3. GitHub에서 Merge 승인 대기

# 4. Merge 후 로컬 정리
git checkout $(${CLAUDE_PLUGIN_ROOT}/scripts/detect-default-branch.sh)
git pull
git branch -d feature/my-work
```

**핵심 원칙:**

- 모든 변경사항은 PR을 통해서만 기본 브랜치에 병합
- PR 승인 전 브랜치 삭제 금지

---

## 유틸리티 스크립트

```bash
# 기본 브랜치 확인
${CLAUDE_PLUGIN_ROOT}/scripts/detect-default-branch.sh
# → master 또는 main

# Jira 티켓 추출
${CLAUDE_PLUGIN_ROOT}/scripts/detect-jira-ticket.sh
# → WAD-0212 (있는 경우) 또는 exit 1
```

---

## Conflict 해결 워크플로우

Rebase 중 충돌이 발생하면 `/git-resolve` 커맨드를 사용하여 파일별로 분할정복 방식으로 해결합니다.

### 기본 사용법

```bash
# 충돌 상태 확인 및 대화형 해결 시작
/git-resolve

# 현재 충돌 해결 완료 후 rebase 계속
/git-resolve --continue

# rebase 전체 취소 (원래 상태로 복원)
/git-resolve --abort

# 현재 커밋 건너뛰고 다음으로
/git-resolve --skip
```

### Rebase 워크플로우 예시

```bash
# 1. 기본 브랜치 최신화
git fetch origin
/git-sync main

# 2. feature 브랜치에서 rebase
git checkout feature/my-work
git rebase origin/main

# 3. 충돌 발생 시 - 파일별 대화형 해결
/git-resolve
# → 각 파일에 대해 ours/theirs/manual 선택

# 4. 모든 충돌 해결 후
/git-resolve --continue

# 5. 추가 충돌이 있으면 반복
```

### 해결 전략

| 전략 | 설명 | 사용 시점 |
|------|------|----------|
| **Ours** | Upstream(base) 변경사항 유지 | base 변경이 올바른 경우 |
| **Theirs** | 내 커밋 변경사항으로 대체 | 내 변경이 더 적절한 경우 |
| **Manual** | 양쪽 변경을 수동 병합 | 두 변경 모두 필요한 경우 |

> ⚠️ **주의**: Rebase에서 ours/theirs는 merge와 **반대**입니다!

### Conflict 마커 이해하기

```typescript
<<<<<<< HEAD (ours - upstream/base)
// Upstream 브랜치 코드 (예: origin/main)
const config = { timeout: 3000, retries: 3 };
=======
// 내 커밋 코드 (feature 브랜치)
const config = { timeout: 5000 };
>>>>>>> my-commit (theirs - 내 커밋)
```

**해결 예시 (양쪽 병합):**
```typescript
const config = { timeout: 5000, retries: 3 };
```

### 주의사항

1. **이미 push한 브랜치 rebase 시**
   ```bash
   # force push 필요 (주의해서 사용)
   git push --force-with-lease
   ```

2. **충돌이 너무 복잡하면**
   - `/git-resolve --abort`로 취소
   - merge 방식 고려

---

## PR 리뷰 & CI 관련 커맨드

### 미해결 리뷰 조회

다음 패턴이 감지되면 `/unresolved-reviews` 커맨드를 안내합니다:

- "리뷰 코멘트", "미해결 리뷰", "unresolved review"
- "리뷰 확인", "코드 리뷰 상태"
- "PR 피드백", "리뷰어 코멘트"

```bash
# 현재 브랜치의 PR
/unresolved-reviews

# 특정 PR 번호 지정
/unresolved-reviews 123
```

### CI 실패 분석

다음 패턴이 감지되면 `/check-ci` 커맨드를 안내합니다:

- "CI 실패", "빌드 에러", "check failed"
- "CI 확인", "파이프라인 실패", "GitHub Actions 에러"
- "테스트 실패", "빌드 깨짐"

```bash
# 현재 브랜치의 PR
/check-ci

# 특정 PR 번호 지정
/check-ci 123
```

---

## 핵심 원칙

1. **작은 단위 커밋**: TODO별로 명확한 진행상황 추적
2. **PR Only**: 모든 병합은 반드시 Pull Request를 통해서만
3. **스크립트 사용**: 복잡한 명령어 대신 검증된 스크립트 활용
4. **Rebase 우선**: merge 대신 rebase로 깔끔한 히스토리 유지

---

## Default Branch Guard (PreToolUse Hook)

기본 브랜치에서 Write/Edit 도구 사용 또는 git commit 시도 시 **즉시 차단**하고 브랜치 생성을 제안합니다.

### 보호 범위

| Hook | Matcher | 차단 대상 |
|------|---------|----------|
| Write/Edit Guard | `Write\|Edit` | 파일 생성/수정 |
| Commit Guard | `Bash` | `git commit` 명령 |

### 동작 방식
1. Write, Edit 또는 Bash(git commit) 도구 호출 시 PreToolUse hook 실행
2. 현재 브랜치가 기본 브랜치(main/master)인지 확인
3. 기본 브랜치이면 exit 2로 도구 실행 차단
4. Claude가 stderr 메시지를 읽고 `create-branch.sh`로 새 브랜치 생성
5. 브랜치 이동 후 재시도 시 hook pass

### 특징
- 네트워크 호출 없이 로컬 캐시만 사용 (빠름)
- rebase/merge/detached HEAD 상태에서는 건너뜀
- 기본 브랜치 감지 실패 시 차단하지 않음 (안전)
- Bash hook은 `git commit` 명령만 차단하고 다른 명령은 통과
