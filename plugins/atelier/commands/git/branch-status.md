---
description: "현재 브랜치의 변경사항을 분석하고 남은 작업을 리포트합니다"
argument-hint: "[--diff-only|--tasks-only]"
allowed-tools:
  - Bash
  - Read
  - Grep
  - Glob
  - TaskList
---

# Branch Status Analysis

현재 브랜치의 변경사항을 기본 브랜치와 비교 분석하고, 남은 작업을 정리합니다.

## 실행 흐름

### Step 1: 기본 브랜치 및 현재 브랜치 확인

```bash
CURRENT_BRANCH=$(git branch --show-current)
DEFAULT_BRANCH=$(git remote show origin 2>/dev/null | grep 'HEAD branch' | awk '{print $NF}')
# fallback: main or master
```

현재 브랜치명과 기본 브랜치명을 출력합니다.

### Step 2: 변경사항 요약

기본 브랜치 대비 현재 브랜치의 변경사항을 분석합니다:

```bash
# 커밋 목록
git log ${DEFAULT_BRANCH}..HEAD --oneline

# 변경된 파일 통계
git diff ${DEFAULT_BRANCH}...HEAD --stat

# 아직 커밋되지 않은 변경사항
git status --short
```

`--tasks-only` 인자가 있으면 이 단계를 건너뜁니다.

### Step 3: 변경 분류

변경된 파일들을 카테고리별로 분류합니다:

| 카테고리 | 패턴 |
|----------|------|
| Source | `src/**`, `lib/**` |
| Tests | `**/*.test.*`, `**/*.spec.*`, `__tests__/**` |
| Config | `*.config.*`, `.*rc`, `package.json`, `tsconfig.*` |
| Docs | `*.md`, `docs/**` |
| Other | 위에 해당하지 않는 파일 |

### Step 4: 남은 작업 분석

`--diff-only` 인자가 있으면 이 단계를 건너뜁니다.

다음을 확인하여 남은 작업을 리포트합니다:

1. **미커밋 변경사항**: `git status`에서 staged/unstaged 파일
2. **TODO/FIXME**: 변경된 파일 내 `TODO`, `FIXME`, `HACK`, `XXX` 주석
3. **타입 에러**: `npx tsc --noEmit 2>&1` (TypeScript 프로젝트인 경우)
4. **테스트 커버리지 갭**: 새로 추가된 소스 파일에 대응하는 테스트 파일 존재 여부

### Step 5: 결과 출력

```markdown
## Branch Status: {CURRENT_BRANCH}

**Base**: {DEFAULT_BRANCH} | **Commits**: N개 | **Files changed**: N개

### 변경사항 요약

| 카테고리 | 파일 수 | +lines | -lines |
|----------|---------|--------|--------|
| Source   | 5       | +120   | -30    |
| Tests    | 2       | +80    | -0     |
| Config   | 1       | +3     | -1     |

### 커밋 히스토리

- abc1234 feat: add user authentication
- def5678 fix: handle edge case in parser

### 남은 작업

- [ ] 미커밋 변경: 3개 파일
- [ ] TODO 주석: 2개 발견
- [ ] 테스트 미작성: src/utils/helper.ts
- [x] 타입 체크: 통과
```
