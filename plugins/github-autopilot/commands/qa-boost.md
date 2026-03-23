---
description: "최근 변경사항의 테스트 커버리지를 분석하고 테스트를 보강합니다"
argument-hint: "[interval: 1h, 2h, ...]"
allowed-tools: ["Bash", "Glob", "Read", "Grep", "Agent", "CronCreate"]
---

# QA Boost

최근 변경사항을 QA 전문가 관점에서 분석하고, 누락된 테스트를 draft 브랜치에서 작성한 뒤 PR을 생성합니다.

## 사용법

```bash
/github-autopilot:qa-boost           # 1회 실행
/github-autopilot:qa-boost 1h        # 1시간마다 반복
```

## Context

- 설정 파일: !`cat github-autopilot.local.md 2>/dev/null | head -20 || echo "설정 파일 없음 - 기본값 사용"`
- 현재 브랜치: !`git branch --show-current`
- 최근 커밋: !`git log --oneline -5`

## 작업 프로세스

### Step 1: 인자 파싱

`$ARGUMENTS`에서 interval을 추출합니다.
- `/^\d+[smh]$/` 패턴 매칭 → interval 모드
- 비어있으면 → 1회 실행 모드

### Step 2: 최신 상태 동기화

```bash
git fetch origin
git pull --rebase origin $(git branch --show-current) 2>/dev/null || true
```

### Step 3: 최근 변경사항 분석

```bash
# 최근 20개 커밋의 변경 파일 수집
git log --oneline -20 --format="%H %s"
git diff --name-only HEAD~20..HEAD 2>/dev/null || git diff --name-only $(git rev-list --max-parents=0 HEAD)..HEAD
```

변경 파일에서 테스트 파일(`*_test.*`, `*_spec.*`, `test_*`, `tests/`)을 제외한 소스 파일만 추출합니다.

### Step 4: 테스트 매핑 분석

각 소스 파일에 대해 대응하는 테스트 파일이 있는지 Glob으로 확인합니다:

```
src/auth/mod.rs      → tests/auth_test.rs ✅ (존재)
src/auth/refresh.rs  → tests/refresh_test.rs ❌ (없음)
src/api/handler.rs   → tests/api_test.rs ⚠️ (있지만 handler 관련 테스트 없음)
```

테스트가 충분한 파일은 제외하고, 보강이 필요한 파일을 그룹화합니다.

### Step 5: 테스트 작성 (Agent Team)

테스트 보강이 필요한 파일 그룹별로 qa-test-writer 에이전트를 호출합니다.

각 에이전트는 `isolation: "worktree"`로 실행하여 독립적으로 작업합니다.

에이전트에게 전달:
- changed_files: 해당 그룹의 변경 파일 목록
- commit_messages: 관련 커밋 메시지
- branch_name: `draft/qa-{short-hash}` (HEAD의 short hash 사용)

**그룹 수가 3개 이하**: 순차 호출
**그룹 수가 4개 이상**: 병렬 호출 (background=true)

### Step 6: 승격 (Agent)

테스트 작성에 성공한 각 worktree에 대해 branch-promoter 에이전트를 호출합니다:

전달 정보:
- draft_branch: `draft/qa-{short-hash}`
- issue_number: 0 (QA 보강은 특정 이슈 없음)
- issue_title: "add missing tests for recent changes"
- branch_strategy: 설정에서 로딩
- label_prefix: 설정에서 로딩
- pr_type: "qa"

### Step 7: CronCreate (interval 모드)

interval이 지정된 경우에만 실행합니다:

CronCreate를 호출하여 `/github-autopilot:qa-boost`를 지정된 interval로 등록합니다.

### Step 8: 결과 보고

```
## QA Boost 결과
- 분석한 변경 파일: 15개
- 테스트 보강 필요: 5개
- 테스트 작성 완료: 4개 (PR 생성)
- 테스트 작성 실패: 1개
- 생성된 PR: #55 (feature/qa-a1b2c3)
```

## 주의사항

- 기존 테스트를 수정하지 않음 (추가만)
- 동일한 커밋 범위에 대해 중복 실행 방지 (이전 QA PR이 열려있으면 skip)
- 프로젝트의 테스트 컨벤션을 따름
