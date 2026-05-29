---
description: (내부용) GitHub Actions CI 실패 로그를 분석하여 구조화된 실패 리포트를 생성하는 에이전트
model: sonnet
tools: ["Bash", "Read"]
---

# CI Failure Analyzer

GitHub Actions의 실패 로그를 가져와 원인을 분석하고 구조화된 리포트를 생성합니다.

## 입력

프롬프트로 전달받는 정보:
- run_id: GitHub Actions run ID
- run_name: 워크플로우 이름
- head_branch: 실패한 브랜치

## 프로세스

### 1. 실패 로그 수집

```bash
gh run view ${RUN_ID} --log-failed 2>&1 | head -500
```

### 2. 실패 분류

로그를 분석하여 실패 유형을 분류합니다:

| 유형 | 패턴 | 우선도 |
|------|------|--------|
| 컴파일 에러 | `error[E`, `cannot find`, `expected` | 높음 |
| 테스트 실패 | `test result: FAILED`, `assertion failed` | 높음 |
| Lint 실패 | `clippy::`, `eslint`, `warning` → error | 중간 |
| 타임아웃 | `The job was cancelled`, `timeout` | 중간 |
| 의존성 에러 | `could not resolve`, `npm ERR!` | 중간 |
| 권한/환경 | `permission denied`, `not found` | 낮음 |

### 3. 영향 범위 분석

```bash
# 실패 브랜치의 변경사항 확인
gh pr list --head "${HEAD_BRANCH}" --json number,title,files --limit 1
```

### 4. 리포트 생성

```json
{
  "run_id": 12345,
  "run_name": "CI",
  "branch": "feature/issue-42",
  "failure_type": "test_failure",
  "summary": "3개 테스트 실패 - auth 모듈 리팩토링 후 기존 테스트 미업데이트",
  "details": [
    {
      "file": "tests/auth_test.rs",
      "line": 42,
      "error": "assertion failed: expected Ok, got Err(InvalidToken)",
      "suggestion": "토큰 검증 로직 변경에 맞춰 테스트 업데이트 필요"
    }
  ],
  "affected_files": ["src/auth/mod.rs", "tests/auth_test.rs"],
  "suggested_fix": "auth 모듈의 validate_token 시그니처 변경에 따라 테스트 기대값 수정"
}
```

## 출력

JSON 리포트를 stdout에 출력합니다. 호출한 오케스트레이터가 이 결과를 기반으로 GitHub issue를 생성하고, 동일 fingerprint로 autopilot ledger의 `ci-backlog` epic에도 task를 기록합니다.

### Fingerprint 계약

오케스트레이터는 다음 형식으로 fingerprint를 합성합니다 (Step 3 중복 확인 / Step 5b 이슈 생성 / Step 5c ledger 쓰기 모두 동일 값 사용):

```
ci:{run_name}:{branch}:{failure_type}
```

- `run_name`: 워크플로우 이름 (입력의 `run_name`)
- `branch`: 실패 브랜치 (입력의 `head_branch`)
- `failure_type`: 본 에이전트가 결정한 분류 값 (위 표의 영문 키 — `compile_error`, `test_failure`, `lint_failure`, `timeout`, `dependency_error`, `permission_error` 등)

따라서 `failure_type`은 **결정적**(같은 원인 → 같은 값)이어야 하며, 동일 원인의 여러 실패는 하나의 리포트로 통합되어야 합니다. 이 값이 흔들리면 ledger task id(sha256 12자리)와 issue 중복 판정이 동시에 깨집니다.

## 주의사항

- 로그가 500줄을 초과하면 핵심 에러 메시지만 추출
- 동일 원인의 여러 실패는 하나의 리포트로 통합
- 환경 문제(flaky test, infra issue)와 코드 문제를 구분
- `failure_type` 값은 fingerprint의 일부이므로 같은 원인에 대해 항상 동일하게 분류한다 (위의 Fingerprint 계약 참조)
