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

열린 GitHub issue 들을 분석하여 우선순위를 매기고 다음에 작업할 항목을 추천합니다.

> 단계별 판단 로직(가중치·의존성 그래프·코드베이스 연관성 4단계 분석)·출력 형식·에러 처리는 `git` skill 의 `references/issue-prioritization.md` 에 있습니다. 이 커맨드는 진입점만 담습니다.

## 실행 흐름

tool result 크기 폭발을 막기 위해 메타데이터 → 필터링 → 후보 압축 → 본문 조회 → 분석 순으로 진행합니다.

```
Step 1a: 메타데이터 수집 (body 제외)
   ↓ Step 1b: 머지/PR 상태로 필터링
   ↓ Step 1c: 상위 N개 후보 선정 후 body 조회
   ↓ Step 2: 우선순위 분석 + 의존성 그래프
   ↓ Step 3: 코드베이스 연관성 분석 (4단계)
   ↓ Step 4: 결과 출력 → Step 5: 다음 액션 제안
```

각 단계의 구체 절차(gh/git 명령, 가중치 표, 4단계 연관성 분석, Output 형식)는 `git` skill 의 `references/issue-prioritization.md` 를 로드하여 수행합니다. `--limit N` 인자는 Step 1c 후보 압축 개수(기본 10)에 적용됩니다.

## 에러 처리

- gh 인증 실패 → `gh auth login` 안내
- 열린 issue 0개 → 메시지 출력 후 종료
- Step 1a 결과 truncation → `--limit` 절반으로 재시도 + 알림
- `closedByPullRequests` 미지원 gh 버전 → `gh pr list --search` 폴백

상세 판단 로직·Output Examples 는 `git` skill 의 `references/issue-prioritization.md` 참조.
