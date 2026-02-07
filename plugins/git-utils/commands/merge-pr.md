---
name: merge-pr
description: "현재 브랜치의 PR을 머지하고 기본 브랜치로 동기화합니다"
argument-hint: "[PR_NUMBER]"
allowed-tools:
  - Bash
  - AskUserQuestion
---

# Merge PR

현재 브랜치의 PR을 머지한 후 기본 브랜치로 전환하여 최신 상태로 동기화합니다.

## 실행 흐름

### Step 1: PR 확인

```bash
gh pr view --json state,title,number,mergeable
```

PR이 없거나 이미 머지되었으면 안내 후 종료.

### Step 2: PR 머지

```bash
gh pr merge --squash --delete-branch
```

- squash merge로 깔끔한 히스토리 유지
- 머지 후 원격 브랜치 자동 삭제

### Step 3: 기본 브랜치로 전환 및 동기화

```bash
git checkout {기본 브랜치}
git pull origin {기본 브랜치}
```

### Step 4: 로컬 브랜치 정리

머지 완료된 로컬 브랜치를 삭제합니다.

```bash
git branch -d {이전 브랜치}
```

### Step 5: 결과 안내

머지된 PR 정보와 현재 브랜치 상태를 안내합니다.
