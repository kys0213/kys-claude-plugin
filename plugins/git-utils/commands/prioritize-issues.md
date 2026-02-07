---
name: prioritize-issues
description: "열린 issue를 분석하여 우선순위가 높은 작업을 추천합니다"
argument-hint: "[--limit N]"
allowed-tools:
  - Bash
  - Read
  - Glob
  - Grep
  - AskUserQuestion
---

# Prioritize Issues

열린 GitHub issue들을 분석하여 우선순위를 매기고 다음에 작업할 항목을 추천합니다.

## 실행 흐름

### Step 1: 열린 Issue 수집

```bash
gh issue list --state open --json number,title,labels,createdAt,comments,body --limit 50
```

모든 열린 issue를 가져옵니다.

### Step 2: 우선순위 분석

각 issue를 다음 기준으로 평가합니다:

| 기준 | 가중치 | 설명 |
|------|--------|------|
| **긴급도** | 높음 | bug > enhancement > documentation |
| **영향 범위** | 높음 | 핵심 기능 영향 여부 |
| **의존성** | 중간 | 다른 issue가 이 작업에 의존하는지 |
| **난이도** | 중간 | 구현 복잡도 (낮을수록 우선) |
| **오래된 정도** | 낮음 | 오래 방치된 issue 가점 |
| **댓글 수** | 낮음 | 관심도 지표 |

### Step 3: 코드베이스 연관성 분석

issue 내용과 현재 코드베이스를 대조하여:

- 관련 파일/모듈 식별
- 현재 작업 중인 영역과의 연관성 확인
- 구현 난이도 추정

### Step 4: 우선순위 결과 출력

상위 5개 issue를 다음 형식으로 출력합니다:

```
## 추천 작업 순위

### 1위: #{번호} {제목}
- 긴급도: ★★★
- 영향 범위: ★★☆
- 난이도: ★☆☆
- 관련 파일: src/...
- 추천 이유: ...

### 2위: ...
```

### Step 5: 다음 액션 제안

사용자에게 선택지를 제공합니다:

- 특정 issue 작업 시작 (브랜치 생성 포함)
- issue에 코멘트 추가
- 라벨 업데이트
