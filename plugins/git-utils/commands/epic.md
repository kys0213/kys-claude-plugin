---
description: "Epic 계획서 기반 서브태스크 분해 및 병렬 작업 관리"
argument-hint: "[init <name>|plan|next|status]"
allowed-tools:
  - Bash
  - Read
  - Glob
  - Grep
  - AskUserQuestion
---

# Epic - 서브태스크 관리

Epic 계획서를 기반으로 서브태스크를 분해하고 체계적으로 관리합니다.

## 인자

`$ARGUMENTS`를 파싱하여 모드를 결정합니다:

| 인자 | 모드 | 설명 |
|------|------|------|
| (없음) 또는 `status` | status | 전체 Epic 진행 상황 표시 |
| `init <epic-name>` | init | 새로운 Epic 초기화 |
| `plan` | plan | 계획서 분석 → 서브태스크 자동 생성 |
| `next` | next | 다음 서브태스크 선택 → 브랜치 생성 |

## 실행 흐름

### Mode: init

1. `$ARGUMENTS`에서 epic 이름을 추출합니다 (예: `init payment-refactor`)
2. AskUserQuestion으로 Epic의 목표와 범위를 질문합니다:
   - "이 Epic의 목표는 무엇인가요?"
   - "대략적인 범위(관련 모듈, 파일)가 있나요?"
3. `.plans/epic-{name}.md`에 Epic 계획서를 생성합니다

```markdown
# Epic: {name}

## 목표
{사용자 입력}

## 범위
{사용자 입력}

## 서브태스크
(plan 모드에서 생성)

## 상태
- 생성일: {날짜}
- 진행률: 0%
```

### Mode: plan

1. `.plans/epic-*.md` 파일을 검색하여 현재 Epic을 찾습니다
2. Epic이 여러 개면 AskUserQuestion으로 선택
3. Epic 계획서와 관련 코드를 분석하여 서브태스크를 분해합니다:
   - 의존성 순서 파악
   - 병렬 가능한 작업 그룹 식별
   - 각 서브태스크의 범위와 완료 조건 정의
4. TaskCreate로 서브태스크를 생성하고 의존성(blockedBy/blocks)을 설정합니다
5. AskUserQuestion으로 서브태스크 목록을 확인받습니다

### Mode: next

1. TaskList로 현재 서브태스크 목록을 확인합니다
2. blockedBy가 없는 pending 태스크를 찾습니다
3. 후보가 여러 개면 AskUserQuestion으로 선택지 제시
4. 선택된 태스크에 대해:
   - `git-branch` 커맨드로 서브태스크용 브랜치를 생성 (예: `feat/epic-payment-1-model`)
   - 태스크를 in_progress로 업데이트
   - 작업 범위와 완료 조건을 안내

### Mode: status (기본값)

1. `.plans/epic-*.md` 파일을 읽습니다
2. TaskList로 전체 태스크 상태를 조회합니다
3. 진행률을 시각적으로 표시합니다:

```
## Epic: 결제 시스템 리팩토링

진행률: [====------] 4/10 (40%)

  completed: #1 DB 스키마 설계, #2 모델 생성, #3 API 설계, #4 결제 모듈
  in_progress: #5 테스트 작성
  pending: #6 에러 핸들링 (blocked by #5)
  pending: #7 문서화, #8 배포 설정, #9 모니터링, #10 QA
```

## 사용 예시

```
/epic init payment-refactor
/epic plan
/epic next
/epic status
/epic
```
