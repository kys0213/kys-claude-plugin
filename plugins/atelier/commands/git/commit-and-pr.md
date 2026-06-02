---
description: "변경사항을 커밋하고 PR을 생성합니다"
argument-hint: "[commit message]"
allowed-tools:
  - Bash
  - Read
  - AskUserQuestion
---

# Commit and PR

현재 변경사항을 커밋한 후 PR을 자동으로 생성합니다.

## 실행 흐름

### Step 1: 변경사항 확인

```bash
git status
git diff --stat
```

변경사항이 없으면 안내 후 종료.

### Step 2: 커밋 생성

변경사항을 분석하여 커밋 메시지를 작성하고 커밋합니다.

- `git add`로 관련 파일 스테이징
- 변경 내용 기반 커밋 메시지 자동 작성
- `git commit` 실행

### Step 3: Push

```bash
git push -u origin {현재 브랜치}
```

### Step 4: PR 생성

`gh pr create`로 PR을 생성합니다.

- 커밋 히스토리 기반 PR 제목/본문 자동 작성
- base 브랜치는 기본 브랜치 사용

### Step 5: 결과 안내

생성된 PR URL을 사용자에게 안내합니다.
