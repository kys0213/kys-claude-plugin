---
name: create-issue
description: "GitHub issue를 생성합니다"
argument-hint: "[title]"
allowed-tools:
  - Bash
  - AskUserQuestion
---

# Create Issue

대화 내용이나 사용자 입력을 기반으로 GitHub issue를 생성합니다.

## 실행 흐름

### Step 1: Issue 내용 수집

사용자에게 다음 정보를 확인합니다:

- **제목**: issue 제목 (필수)
- **설명**: 상세 내용 (선택, 자동 작성 가능)
- **라벨**: bug, enhancement, documentation 등 (선택)

사용자가 자연어로 설명하면 구조화된 issue로 변환합니다.

### Step 2: 리포지토리 확인

```bash
gh repo view --json name,owner
```

현재 리포지토리 정보를 확인합니다.

### Step 3: 기존 Issue 중복 확인

```bash
gh issue list --state open --limit 20
```

유사한 제목의 열린 issue가 있는지 확인하고, 중복이 의심되면 사용자에게 알립니다.

### Step 4: Issue 생성

```bash
gh issue create --title "{제목}" --body "{본문}" --label "{라벨}"
```

- 본문에는 배경, 기대 동작, 재현 방법 등을 구조화하여 작성
- bug 라벨인 경우 버그 리포트 템플릿 적용
- enhancement 라벨인 경우 기능 제안 템플릿 적용

### Step 5: 결과 안내

생성된 issue URL과 번호를 사용자에게 안내합니다.
