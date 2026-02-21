---
description: "리뷰 결과에서 GitHub Issue를 일괄 생성합니다"
argument-hint: "[--repo owner/repo] [--label label1,label2]"
allowed-tools:
  - Bash
  - Glob
  - Grep
  - Read
  - AskUserQuestion
---

# Review to Issues (/review-to-issues)

리뷰 결과(multi-review 출력, `.review/*.md`, `.review/*.json`)에서 개선 항목을 추출하여 GitHub Issue로 일괄 등록합니다.

## 사용법

```bash
# 기본 사용 (대화 컨텍스트 또는 .review/ 디렉토리에서 추출)
/review-to-issues

# 리포지토리 지정
/review-to-issues --repo owner/repo

# 라벨 추가 지정
/review-to-issues --label refactor,tech-debt
```

## 실행 흐름

### Step 1: 리뷰 결과 수집

리뷰 결과를 다음 우선순위로 탐색합니다:

1. **`.review/` 디렉토리**: `*.md` 또는 `*.json` 파일이 있으면 파싱
2. **대화 컨텍스트**: `.review/`가 없으면 현재 대화에서 직전 리뷰 결과를 활용

```
Glob: .review/*.md
Glob: .review/*.json
```

파일이 없고 대화 컨텍스트에도 리뷰 결과가 없으면 에러:
```
Error: 리뷰 결과를 찾을 수 없습니다.
먼저 /multi-review를 실행하거나 .review/ 디렉토리에 리뷰 리포트를 저장해주세요.
```

### Step 2: 리뷰 항목 파싱

리뷰 결과에서 개선 항목을 추출합니다.

#### 마크다운 리포트 파싱 (`.review/*.md`)

다음 섹션에서 항목을 추출합니다:

- `## 공통 약점` / `## 약점` / `## Weaknesses` 섹션
- `## 종합 권장사항` / `## 권장사항` / `## Recommendations` 섹션
- `### Critical`, `### Important`, `### 참고사항` 하위 섹션

각 항목에서 추출하는 정보:
- **제목**: 항목의 제목 또는 첫 줄
- **설명**: 상세 내용
- **심각도**: Critical / Important(Warning) / Nice-to-have(Info)
- **신뢰도**: LLM 합의 수준 (3/3, 2/3, 1/3)

#### JSON 리포트 파싱 (`.review/*.json`)

```json
{
  "recommendations": [
    {
      "priority": "critical | important | nice-to-have",
      "title": "...",
      "description": "...",
      "impact": "..."
    }
  ],
  "weaknesses": ["..."]
}
```

### Step 3: 항목 분류 및 사용자 확인

추출된 항목을 심각도별로 분류하여 사용자에게 보여줍니다:

```
## 추출된 리뷰 항목

### Critical (즉시 수정 권장 - Issue 등록 대상)
1. [제목] - 신뢰도: 3/3
2. [제목] - 신뢰도: 2/3

### Important (개선 권장 - Issue 등록 대상)
3. [제목] - 신뢰도: 3/3
4. [제목] - 신뢰도: 2/3

### Nice-to-have (선택적 개선)
5. [제목] - 신뢰도: 1/3
```

`AskUserQuestion`으로 등록할 항목을 선택받습니다:

- **multiSelect: true** 사용
- Critical과 Important는 기본 선택 상태로 안내
- Nice-to-have는 선택적으로 안내

### Step 4: 리포지토리 확인

```bash
gh repo view --json name,owner
```

`--repo` 옵션이 있으면 해당 리포지토리 사용, 없으면 현재 리포지토리 사용.

### Step 5: 기존 Issue 중복 확인

```bash
gh issue list --state open --limit 50
```

생성할 각 항목에 대해 유사한 제목의 열린 issue가 있는지 확인합니다.
중복 의심 항목은 사용자에게 안내하고 건너뛸지 확인합니다.

### Step 6: Issue 일괄 생성

확인된 항목마다 `gh issue create`를 실행합니다.

```bash
gh issue create --title "{제목}" --body "{본문}" --label "{라벨}"
```

#### Label 매핑

| 심각도 | GitHub Labels |
|--------|--------------|
| Critical | `bug`, `priority: critical` |
| Important | `enhancement`, `priority: high` |
| Nice-to-have | `enhancement`, `priority: low` |

- `--label` 옵션으로 추가 라벨을 지정할 수 있음
- 존재하지 않는 라벨은 자동 생성하지 않고 경고만 출력

#### Issue 본문 템플릿

```markdown
## 배경

이 이슈는 코드 리뷰에서 발견된 개선 사항입니다.

- **심각도**: {Critical | Important | Nice-to-have}
- **신뢰도**: {3/3 | 2/3 | 1/3} LLM 합의
- **출처**: {리뷰 파일명 또는 "multi-review 결과"}

## 설명

{리뷰에서 추출한 상세 설명}

## 제안된 개선 방안

{리뷰에서 추출한 권장사항 / 구체적 방법}

## 예상 효과

{리뷰에서 추출한 impact}
```

### Step 7: 결과 요약

생성된 이슈 목록을 요약하여 출력합니다:

```
## Issue 생성 완료

| # | 제목 | 심각도 | URL |
|---|------|--------|-----|
| 1 | ... | Critical | https://github.com/... |
| 2 | ... | Important | https://github.com/... |

총 {N}개 이슈가 생성되었습니다.
건너뛴 항목: {M}개 (중복 {X}개, 사용자 제외 {Y}개)
```

## 주의사항

- **중복 방지**: 동일한 리뷰 결과로 중복 이슈를 만들지 않도록 기존 issue를 확인합니다
- **라벨 검증**: 존재하지 않는 라벨 사용 시 경고를 출력합니다
- **Critical 항목**: Critical 심각도 항목은 이슈 등록보다 즉시 수정을 권장하는 메시지를 함께 표시합니다
- **1/3 신뢰도 항목**: 1개 LLM만 지적한 항목은 기본적으로 선택 해제 상태로 표시합니다

## 관련 커맨드

| 커맨드 | 설명 |
|--------|------|
| `/multi-review` | 3개 LLM 리뷰 실행 (리뷰 결과 생성) |
| `/create-issue` | 단일 GitHub issue 생성 |
| `/develop` | 전체 개발 워크플로우 |
