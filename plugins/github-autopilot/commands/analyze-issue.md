---
description: "GitHub 이슈를 분석하여 autopilot 구현 가능 여부를 판단하고 autopilot:ready 라벨을 추가합니다"
argument-hint: "<issue-numbers: #42 #43, 42 43, ...>"
allowed-tools: ["Bash", "Read", "Agent", "CronList", "CronDelete"]
---

# Analyze Issue

GitHub 이슈를 분석하여 autopilot으로 자동 구현 가능한지 판단합니다. 가능하면 `autopilot:ready` 라벨을 추가하고, 분석 결과를 코멘트로 남깁니다.

## 사용법

```bash
/github-autopilot:analyze-issue             # 자동 탐색 (라벨 없는 open 이슈)
/github-autopilot:analyze-issue 42          # 단일 이슈
/github-autopilot:analyze-issue 42 43 44    # 복수 이슈
/github-autopilot:analyze-issue #42 #43     # # 접두사 허용
```

> 반복 실행은 `/github-autopilot:autopilot`이 `CronCreate`로 관리합니다.

## Context

- 설정 파일: !`cat github-autopilot.local.md 2>/dev/null | head -20 || echo "설정 파일 없음 - 기본값 사용"`
- 현재 브랜치: !`git branch --show-current`

## 작업 프로세스

### Step 1: 인자 파싱

`$ARGUMENTS`에서 이슈 번호를 추출합니다.
- `#` 접두사 제거
- 숫자만 추출하여 목록 생성
- 이슈 번호가 없으면 → **자동 탐색 모드** (Step 1.5로 진행)

### Step 1.5: 자동 탐색 모드

인자가 없으면, autopilot 라벨이 없는 open 이슈를 자동으로 탐색합니다.

```bash
gh issue list --state open --json number,title,labels,comments --limit 50
```

필터 조건:
- `{label_prefix}` 로 시작하는 라벨이 **없음** (ready, wip, ci-failure 등 모두 제외)
- 코멘트에 "Autopilot 분석 결과"가 **없음** (이미 분석된 이슈 제외)

탐색 결과에서 이슈 번호 목록을 추출하여 Step 2로 진행합니다.
탐색 결과가 없으면 "분석 대상 이슈 없음" 출력 후 Step 8(Idle Detection)으로 진행합니다.

### Step 2: Base 브랜치 동기화

**branch-sync** 스킬의 절차를 수행합니다.

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

#### skip 판정

```bash
# 분석 코멘트 추가 (라벨은 부여하지 않음)
gh issue comment ${ISSUE_NUMBER} --body "${ANALYSIS_COMMENT}"
```

라벨을 추가하지 않습니다. 코멘트에 사유(모호한 요구사항 / 분할 필요)를 명시하여 이슈 작성자가 수정할 수 있도록 합니다.

### Step 7: 결과 보고

```
## Analyze Issue 결과

| Issue | Title | 판정 |
|-------|-------|------|
| #42 | Add user auth | ✅ Ready |
| #43 | Redesign entire system | ⏭️ Skip (분할 필요) |
| #44 | Fix something | ⏭️ Skip (요구사항 모호) |

- Ready: 1건 (autopilot:ready 라벨 추가됨)
- Skip: 2건 (코멘트만 게시)
- Skipped: 0건
```

### Step 8: Idle Detection (cron 자동 종료)

`/loop`으로 실행 중일 때, 분석 대상이 없는 상태가 지속되면 cron을 자동 종료합니다.

- **idle 파일**: `IDLE_FILE=.autopilot-idle-count`
- **종료 임계값**: `MAX_IDLE=3`

#### 판정 기준

- Step 4에서 **유효한 분석 대상 이슈가 0건**이면 해당 사이클을 **idle**로 판정
  - 존재하지 않는 이슈, 이미 라벨이 있는 이슈, closed 이슈를 제외한 후 남은 이슈가 0건인 경우
  - 인자 자체가 없어서 Step 1에서 종료된 경우도 idle로 판정
- 분석 대상이 **1건이라도 있으면** idle count를 **0으로 리셋**

#### 연속 idle 시 cron 종료

연속 idle 횟수를 추적합니다:

```bash
IDLE_COUNT=$(cat "${IDLE_FILE}" 2>/dev/null || echo "0")
```

- **분석 대상 있음**: `echo "0" > "${IDLE_FILE}"`
- **분석 대상 없음**: `IDLE_COUNT`를 1 증가시켜 저장

**연속 `MAX_IDLE`회 idle**이면:

1. `CronList`로 현재 등록된 cron 목록을 조회하여 `analyze-issue` 관련 cron ID를 찾음
2. `CronDelete`로 해당 cron을 삭제
3. idle count 파일을 삭제: `rm -f "${IDLE_FILE}"`
4. 메시지 출력:

```
분석할 이슈가 없어 cron을 종료합니다 (연속 3회 idle)
```

**`MAX_IDLE`회 미만 idle**이면:

```
분석할 이슈가 없습니다 (연속 idle: {IDLE_COUNT}회 / {MAX_IDLE}회 시 cron 자동 종료)
```

## 주의사항

- 토큰 최적화: MainAgent는 이슈 메타데이터만 조회, 코드 분석은 Agent에 위임
- 이미 autopilot 파이프라인에 있는 이슈는 중복 처리하지 않음
- 분석 코멘트는 이력으로 남아 issue-implementer가 참조할 수 있음
