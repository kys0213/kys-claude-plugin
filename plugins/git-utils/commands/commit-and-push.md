---
description: "변경사항을 커밋하고 현재 브랜치에 push합니다"
argument-hint: "[commit message]"
allowed-tools:
  - Bash
  - Read
  - AskUserQuestion
---

# Commit and Push

현재 변경사항을 커밋한 후 현재 브랜치에 push합니다. PR 생성 없이 빠르게 저장하고 싶을 때 사용합니다.

## 실행 흐름

### Step 1: 변경사항 확인

```bash
git status
git diff --stat
```

변경사항이 없으면 안내 후 종료.

### Step 2: 안전 검사

현재 브랜치가 main/master인 경우 경고를 출력하고 사용자에게 확인을 요청합니다.

```bash
git branch --show-current
```

main 또는 master 브랜치이면:
- 경고 메시지를 출력
- AskUserQuestion으로 "정말 main에 직접 push하시겠습니까?" 확인
- 거부 시 중단

### Step 3: 커밋 생성

변경사항을 분석하여 커밋 메시지를 작성하고 커밋합니다.

- `git add`로 관련 파일 스테이징 (민감 파일 `.env`, `credentials*` 등 제외)
- 인자로 메시지가 주어지면 해당 메시지 사용
- 메시지가 없으면 변경 내용 기반 Conventional Commits 형식으로 자동 작성
- `git commit` 실행

### Step 4: Push

```bash
git push -u origin {현재 브랜치}
```

upstream이 설정되어 있지 않으면 `-u` 플래그를 자동으로 추가합니다.

### Step 5: 결과 안내

push 완료 메시지를 출력합니다.


ARGUMENTS: $ARGUMENTS
