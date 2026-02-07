---
description: "PR의 CI/CD 실패 로그를 분석합니다"
argument-hint: "[PR_NUMBER]"
allowed-tools:
  - Bash
  - Read
---

# Check CI

PR의 CI/CD 체크 상태를 확인하고, 실패한 항목의 로그를 분석하여 원인과 수정 방향을 제안합니다.

## 실행 흐름

### Step 1: PR 번호 확인

인자로 받은 PR 번호를 사용하거나, 현재 브랜치의 PR을 자동 감지합니다.

```bash
PR_NUMBER="${ARGUMENTS:-$(gh pr view --json number --jq '.number' 2>/dev/null)}"
```

PR 번호를 찾을 수 없으면 에러 메시지 출력 후 종료합니다.

### Step 2: 체크 상태 조회

```bash
gh pr checks "$PR_NUMBER"
```

- 모든 체크가 통과했으면 "모든 CI 체크가 통과했습니다" 출력 후 종료
- 실패 또는 pending 항목이 있으면 다음 단계로 진행

### Step 3: HEAD SHA 가져오기

```bash
HEAD_SHA=$(gh pr view "$PR_NUMBER" --json headRefOid --jq '.headRefOid')
```

### Step 4: 실패한 run 식별

```bash
gh run list --commit "$HEAD_SHA" --status failure --json databaseId,name --limit 5
```

실패한 run이 없으면 pending 상태의 체크 목록만 출력 후 종료합니다.

### Step 5: 실패 로그 분석

각 실패한 run에 대해:

```bash
gh run view "$RUN_ID" --log-failed | tail -200
```

로그에서 다음 에러 패턴을 식별합니다:

| 패턴 | 카테고리 |
|------|----------|
| `TypeError`, `ReferenceError`, `SyntaxError` | 런타임 에러 |
| `build failed`, `compilation error` | 빌드 에러 |
| `lint error`, `eslint`, `prettier` | 린트 에러 |
| `test failed`, `FAIL`, `AssertionError` | 테스트 실패 |
| `timeout`, `ETIMEDOUT` | 타임아웃 |
| `permission denied`, `403`, `401` | 권한 에러 |
| `out of memory`, `heap` | 메모리 에러 |

### Step 6: 분석 결과 출력

다음 형식으로 결과를 출력합니다:

```
## CI 체크 결과: PR #123

### 체크 상태 요약
| 체크 | 상태 |
|------|------|
| build | ❌ 실패 |
| test | ❌ 실패 |
| lint | ✅ 통과 |

### 실패 분석

#### 1. build (Run #456789)
- **카테고리**: 빌드 에러
- **에러 메시지**: `Cannot find module './utils'`
- **파일**: src/index.ts:15
- **수정 방향**: import 경로 확인 필요

#### 2. test (Run #456790)
- **카테고리**: 테스트 실패
- **에러 메시지**: `Expected 200, received 404`
- **파일**: tests/api.test.ts:42
- **수정 방향**: API 엔드포인트 경로 변경 확인 필요

---
**추천 액션:**
- [ ] src/index.ts: import 경로 수정 (Line 15)
- [ ] tests/api.test.ts: 테스트 기대값 업데이트 (Line 42)
```
