---
description: "GitHub 이슈를 분석하여 autopilot 구현 가능 여부를 판단하고 autopilot:ready 라벨을 추가합니다"
argument-hint: "<issue-numbers: #42 #43, 42 43, ...>"
allowed-tools: ["Bash", "Read", "Agent"]
---

# Analyze Issue

GitHub 이슈를 분석하여 autopilot으로 자동 구현 가능한지 판단합니다. 가능하면 `autopilot:ready` 라벨을 추가하고, 분석 결과를 코멘트로 남깁니다.

## 사용법

```bash
/github-autopilot:analyze-issue 42          # 단일 이슈
/github-autopilot:analyze-issue 42 43 44    # 복수 이슈
/github-autopilot:analyze-issue #42 #43     # # 접두사 허용
```

## Context

- 설정 파일: !`cat github-autopilot.local.md 2>/dev/null | head -20 || echo "설정 파일 없음 - 기본값 사용"`
- 현재 브랜치: !`git branch --show-current`

## 작업 프로세스

### Step 1: 인자 파싱

`$ARGUMENTS`에서 이슈 번호를 추출합니다.
- `#` 접두사 제거
- 숫자만 추출하여 목록 생성
- 이슈 번호가 없으면 에러 메시지 출력 후 종료

### Step 2: 최신 상태 동기화

```bash
git fetch origin
```

### Step 3: 설정 로딩

`github-autopilot.local.md`에서 설정을 읽습니다.
- `label_prefix`: 라벨 접두사 (기본값: `"autopilot:"`)

### Step 4: 이슈 조회

각 이슈의 정보를 조회합니다:

```bash
gh issue view ${ISSUE_NUMBER} --json number,title,body,labels,state
```

- 존재하지 않는 이슈는 스킵하고 경고 출력
- 이미 `{label_prefix}ready` 또는 `{label_prefix}wip` 라벨이 붙은 이슈는 스킵
- closed 상태 이슈는 스킵

### Step 5: 분석 (Agent)

이슈가 3개 이하이면 순차, 4개 이상이면 병렬로 issue-analyzer 에이전트를 호출합니다.

각 이슈에 대해 전달:
- issue_number
- issue_title
- issue_body

**3개 이하**: 순차 호출 (background=false)
**4개 이상**: 병렬 호출 (background=true)

### Step 6: 결과 처리

각 이슈의 판정 결과에 따라 처리합니다:

#### ready 판정

```bash
# 분석 코멘트 추가
gh issue comment ${ISSUE_NUMBER} --body "${ANALYSIS_COMMENT}"

# autopilot:ready 라벨 추가
gh issue edit ${ISSUE_NUMBER} --add-label "{label_prefix}ready"
```

#### needs-clarification 판정

```bash
# 질문 코멘트 추가
gh issue comment ${ISSUE_NUMBER} --body "${ANALYSIS_COMMENT}"

# needs-clarification 라벨 추가
gh issue edit ${ISSUE_NUMBER} --add-label "{label_prefix}needs-clarification"
```

#### too-complex 판정

```bash
# 분할 제안 코멘트 추가
gh issue comment ${ISSUE_NUMBER} --body "${ANALYSIS_COMMENT}"

# too-complex 라벨 추가
gh issue edit ${ISSUE_NUMBER} --add-label "{label_prefix}too-complex"
```

### Step 7: 결과 보고

```
## Analyze Issue 결과

| Issue | Title | 판정 |
|-------|-------|------|
| #42 | Add user auth | ✅ Ready |
| #43 | Redesign entire system | 🔀 Too Complex |
| #44 | Fix something | ❓ Needs Clarification |

- Ready: 1건 (autopilot:ready 라벨 추가됨)
- Needs Clarification: 1건
- Too Complex: 1건
- Skipped: 0건
```

## 주의사항

- 토큰 최적화: MainAgent는 이슈 메타데이터만 조회, 코드 분석은 Agent에 위임
- 이미 autopilot 파이프라인에 있는 이슈는 중복 처리하지 않음
- 분석 코멘트는 이력으로 남아 issue-implementer가 참조할 수 있음
