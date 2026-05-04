---
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

전체 흐름은 다음과 같습니다. 메타데이터 → 필터링 → 후보 압축 → 본문 조회 → 분석 → 출력 순으로 진행하여 tool result 크기 폭발을 방지합니다.

```
Step 1a: 메타데이터 수집 (body 제외)
   ↓
Step 1b: 머지/PR 상태로 필터링
   ↓
Step 1c: 상위 N개 후보 선정 후 body 조회
   ↓
Step 2: 우선순위 분석 + 의존성 그래프
   ↓
Step 3: 코드베이스 연관성 분석 (4단계 절차)
   ↓
Step 4: 우선순위 결과 출력
   ↓
Step 5: 다음 액션 제안
```

### Step 1a: 열린 Issue 메타데이터 수집 (body 제외)

body는 한 issue당 수 KB까지 커질 수 있어 50개 이상이면 tool result가 잘립니다. 먼저 body를 제외한 메타데이터만 가져옵니다.

```bash
gh issue list --state open --json number,title,labels,createdAt,comments --limit 50
```

이 단계의 결과는 우선순위 후보 선별을 위한 기초 데이터로만 사용합니다.

### Step 1b: 머지/PR 상태로 필터링

작업이 이미 끝났거나 진행 중인 issue를 후보에서 제외합니다.

#### (1) 최근 머지된 issue 자동 감지 (단축 필터)

(2)의 per-issue API 호출 비용을 줄이기 위해, 먼저 로컬 `git log`로 최근 머지된 issue 번호를 일괄 감지합니다.

```bash
git log --oneline -100 | grep -oE '#[0-9]+' | sort -u
```

위 결과와 Step 1a의 열린 issue 번호를 교차하여, 매칭되는 issue는 다음과 같이 처리합니다:

- 최종 출력에 `[머지됨]` 마커 표시
- 사용자에게 close 제안 (Step 5에서 일괄 처리)
- 우선순위 후보에서 제외
- **(2)의 per-issue 조회 대상에서도 제외**

#### (2) 연결된 PR 상태 체크

(1)에서 걸러지지 않은 후보 각각에 대해 연결된 PR을 조회합니다. issue 수에 비례하는 API 호출이 발생하므로 (1)을 반드시 먼저 적용합니다.

```bash
# ${NUM}은 issue 번호로 치환. 한 issue당 한 번 호출.
gh issue view ${NUM} --json closedByPullRequests,number,title

# closedByPullRequests 미지원 gh 버전 폴백
gh pr list --state all --search "${NUM} in:body"
```

| 상태 | 마커 | 후보 처리 |
|------|------|-----------|
| PR open 존재 | `[PR 진행 중 #<pr>]` | 우선순위 제외 |
| PR closed/merged 존재 | `[머지됨]` | 우선순위 제외 + close 제안 |
| 작업 브랜치만 존재 (PR 없음) | `[작업 시작됨 #<branch>]` | 후보 유지 (가중치 감점) |
| 연결 없음 | (없음) | 후보 유지 |

> **호출 비용**: N개 후보 × 1회 = N회 API 호출. 50개 이상이면 `gh api graphql`로 `closingIssuesReferences`를 일괄 조회하여 N+1을 회피한다.

"작업 브랜치만 존재"는 `gh issue develop --list ${NUM}`로 확인합니다. 단, 본 레포 컨벤션 `<type>/<short-description>`(`.claude/rules/git-workflow.md`)에는 issue 번호가 브랜치명에 포함되지 않을 수 있어 false negative가 가능합니다.

### Step 1c: 상위 N개 후보 선정 후 body 조회

Step 1a/1b를 거쳐 살아남은 후보를 Step 2 가중치 기준 **상위 10개**(또는 `--limit N` 인자)로 압축한 뒤, 그때서야 body를 조회합니다.

```bash
# CANDIDATE_NUMBERS는 공백 구분된 issue 번호 문자열 (예: "638 642 645")
for num in $CANDIDATE_NUMBERS; do
  gh issue view "$num" --json number,title,body,labels
done
```

이렇게 하면 tool result에 들어가는 body 총량을 50개분에서 10개분으로 축소합니다.

### Step 2: 우선순위 분석

각 issue를 다음 기준으로 평가합니다:

| 기준 | 가중치 | 설명 |
|------|--------|------|
| **긴급도** | 높음 | bug > enhancement > documentation |
| **영향 범위** | 높음 | 핵심 기능 영향 여부 |
| **의존성 (피의존)** | 중간 | 다른 issue가 이 issue를 참조 → 가산점 |
| **의존성 (선행)** | 중간 | 이 issue가 다른 미해결 issue에 의존 → 감점 |
| **난이도** | 중간 | 구현 복잡도 (낮을수록 우선) |
| **오래된 정도** | 낮음 | 오래 방치된 issue 가점 |
| **댓글 수** | 낮음 | 관심도 지표 |
| **작업 진행 흔적** | 낮음 | `[작업 시작됨]` 마커가 있으면 감점 |

#### Issue 의존성 그래프 구축

Step 1c에서 가져온 각 issue body에서 `#\d+` 패턴을 추출하여 참조 그래프를 만듭니다.

```bash
# 예: issue 본문에서 참조 추출
echo "$BODY" | grep -oE '#[0-9]+' | sort -u
```

```
references[issue_num] = [참조하는 다른 issue 번호들]
referenced_by[issue_num] = [이 issue를 참조하는 issue 번호들]
```

가중치 계산 시:

- `referenced_by`가 많을수록 가산점 (여러 issue가 이 작업을 기다림)
- `references` 중 열린 상태가 있으면 감점 (선행 작업 미완료)

출력 시 의존성 관계를 시각적으로 표시합니다:

```
#638
  ← #526이 이 issue에 의존
  → #634를 선행 요구
```

### Step 3: 코드베이스 연관성 분석

issue 내용과 현재 코드베이스의 연관성을 다음 4단계 절차로 검증합니다.

#### (1) body에서 파일 경로 패턴 추출

issue body에서 코드 경로처럼 보이는 토큰을 추출합니다. 디렉토리 후보는 레포의 실제 최상위 디렉토리에서 동적으로 수집해 다양한 레이아웃에 대응합니다.

```bash
# 레포 최상위 디렉토리를 정규식 OR 그룹으로 변환
ROOTS=$(ls -d */ 2>/dev/null | tr -d '/' | paste -sd '|' -)

echo "$BODY" | grep -oE "(${ROOTS})/[A-Za-z0-9_./-]+" | sort -u
```

거짓 양성(실제로 존재하지 않는 경로)은 다음 (2)에서 걸러냅니다.

#### (2) ls / Glob으로 실제 존재 여부 검증

각 경로가 실제로 존재하는지 확인합니다 (없는 경로는 거짓 양성으로 제외).

```bash
ls -d <path> 2>/dev/null
# 또는 와일드카드
```

Glob 도구로 패턴 매칭을 보완합니다 (예: `plugins/**/<keyword>*`).

#### (3) git log로 해당 영역의 최근 활동 빈도 확인

per-path 반복 호출 대신, 단일 `git log` 호출로 최근 변경 파일을 모두 출력한 뒤 클라이언트 측에서 경로별 카운트를 lookup합니다.

```bash
# 한 번에 최근 3개월 변경 파일 → 경로별 카운트 맵
git log --since='3 months ago' --name-only --pretty=format: \
  | grep -v '^$' | sort | uniq -c | sort -rn
```

각 후보 경로(또는 그 prefix)에 해당하는 카운트를 합산해 활동 점수로 사용합니다. 활동 빈도가 높을수록 "활발하게 개발 중인 영역"으로 가산점.

#### (4) 현재 작업과의 교차 분석

현재 브랜치의 변경 영역과 issue 관련 경로를 교차하여 연관성을 평가합니다.

```bash
# default 브랜치 동적 조회 — main 외 (master/develop 등) 레포 대응
DEFAULT_BRANCH=$(git symbolic-ref refs/remotes/origin/HEAD 2>/dev/null \
  | sed 's@^refs/remotes/origin/@@' \
  || echo main)

git diff "${DEFAULT_BRANCH}...HEAD" --name-only
```

issue 관련 경로와 위 결과의 교집합이 있으면 "현재 작업과 연속성 있는 issue"로 가산점.

이 단계의 산출물:

- issue별 관련 파일/모듈 리스트
- 최근 활동 빈도 점수
- 현재 작업과의 연관성 점수
- 구현 난이도 추정 (관련 파일 수 + 변경 영역 크기 기반)

### Step 4: 우선순위 결과 출력

상위 5개 issue를 다음 형식으로 출력합니다. 머지/PR-진행 중 issue는 별도 섹션에 분리해 표시합니다.

```
## 추천 작업 순위

### 1위: #{번호} {제목}
- 긴급도: ★★★
- 영향 범위: ★★☆
- 난이도: ★☆☆
- 관련 파일: plugins/git-utils/commands/...
- 최근 활동: 12 commits / 3 months
- 현재 작업과의 연관성: 높음 (겹치는 파일 2개)
- 의존성:
  ← #526, #527이 이 issue에 의존
  → 선행 요구 없음
- 추천 이유: ...

### 2위: ...

## 참고: 정리 권장 issue

### [머지됨] #NNN {제목}
최근 커밋 abc1234에서 처리됨. close 권장.

### [PR 진행 중 #MMM] #NNN {제목}
이미 작업 중이므로 우선순위에서 제외.

### [작업 시작됨 #<branch>] #NNN {제목}
연결된 작업 브랜치 존재, PR 미생성 상태.
```

### Step 5: 다음 액션 제안

`AskUserQuestion`으로 사용자에게 선택지를 제공합니다:

- 특정 issue 작업 시작 (브랜치 생성 포함)
- `[머지됨]` 마커 issue 일괄 close
- `[작업 시작됨]` 마커 issue의 기존 브랜치로 전환
- 특정 issue에 코멘트 추가
- 라벨 업데이트

## 에러 처리

**`gh` 인증 실패:**

- `gh auth login` 안내

**`gh issue list` 결과 0개:**

- "열린 issue가 없습니다" 메시지 출력 후 종료

**Step 1a 결과가 너무 커서 잘리는 경우 (방어적 처리):**

- `--limit`을 절반으로 줄여 재시도하고 사용자에게 알림

**`closedByPullRequests` 필드가 없는 gh 버전:**

- `gh pr list --search` 폴백으로 자동 전환

## Output Examples

### 성공 케이스

```
## 추천 작업 순위

### 1위: #642 epic store domain wiring
- 긴급도: ★★★
- 영향 범위: ★★★
- 난이도: ★★☆
- 관련 파일: plugins/github-autopilot/internal/epic/
- 최근 활동: 8 commits / 3 months
- 현재 작업과의 연관성: 높음 (겹치는 파일 1개)
- 의존성:
  ← #645, #646이 이 issue에 의존
  → 선행 요구 없음
- 추천 이유: 다수 후속 작업이 대기 중이며 현재 활발히 변경되는 영역

## 참고: 정리 권장 issue

### [머지됨] #641 skip idle check on first run
커밋 e7f745e에서 처리됨. close 권장.

### [PR 진행 중 #649] #549 expand task store trait per refined spec
이미 작업 중이므로 우선순위에서 제외.
```

### 실패 케이스 (인증)

```
gh CLI 인증이 필요합니다. 다음 명령으로 로그인하세요:
  gh auth login
```
