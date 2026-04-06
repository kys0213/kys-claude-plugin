---
description: "최근 변경사항의 테스트 커버리지를 분석하고 누락된 테스트를 이슈로 발행합니다"
argument-hint: "[commit_hash]"
allowed-tools: ["Bash", "Glob", "Read", "Grep"]
---

# QA Boost

최근 변경사항을 QA 관점에서 분석하고, 누락된 테스트를 GitHub 이슈로 발행합니다. 테스트 구현은 build-issues 파이프라인이 처리합니다.

## 사용법

```bash
/github-autopilot:qa-boost                    # 최근 20커밋 기준, 1회 실행
/github-autopilot:qa-boost abc1234            # 특정 커밋 이후 변경 분석
```

> 반복 실행은 `/github-autopilot:autopilot`이 `run-loop.sh`로 관리합니다.

## Context

- 설정 파일: !`cat github-autopilot.local.md 2>/dev/null | head -20 || echo "설정 파일 없음 - 기본값 사용"`
- 현재 브랜치: !`git branch --show-current`
- 최근 커밋: !`git log --oneline -5`

## 작업 프로세스

### Step 1: 인자 파싱

`$ARGUMENTS`에서 commit_hash를 추출합니다.
- `/^[0-9a-f]{7,40}$/` 패턴 매칭 → commit_hash
- 비어있으면 → 최근 20커밋 기준

### Step 2: 최신 상태 동기화

```bash
git fetch origin
git pull --rebase origin $(git branch --show-current) 2>/dev/null || true
```

### Step 2.5: Pipeline Idle Check

```bash
autopilot pipeline idle --label-prefix "{label_prefix}"
```

- **exit 0 (idle)**: `notification` 설정이 있으면 "autopilot 파이프라인 완료 — qa-boost cycle 중단" 알림 발송 후 종료합니다.
- **exit 2 (error)**: 스크립트 실행 환경 오류. 에러 메시지를 출력하고 이번 cycle을 skip합니다.
- **exit 1 (active)**: Step 3부터 정상 진행.

### Step 3: 변경사항 수집

#### commit_hash가 있는 경우

```bash
git diff ${commit_hash}..HEAD --name-only
git log ${commit_hash}..HEAD --oneline --format="%H %s"
```

#### commit_hash가 없는 경우

```bash
git log --oneline -20 --format="%H %s"
git diff --name-only HEAD~20..HEAD 2>/dev/null || git diff --name-only $(git rev-list --max-parents=0 HEAD)..HEAD
```

변경 파일에서 테스트 파일(`*_test.*`, `*_spec.*`, `test_*`, `tests/`, `__tests__/`, `e2e/`, `bench/`, `benches/`)을 제외한 소스 파일만 추출합니다.

### Step 4: 테스트 매핑 분석

각 소스 파일에 대해 대응하는 테스트 파일이 있는지 Glob으로 확인합니다:

```
src/auth/mod.rs      → tests/auth_test.rs ✅ (존재)
src/auth/refresh.rs  → tests/refresh_test.rs ❌ (없음)
src/api/handler.rs   → tests/api_test.rs ⚠️ (있지만 handler 관련 테스트 없음)
```

테스트가 충분한 파일은 제외하고, 보강이 필요한 파일을 그룹화합니다.

보강 대상이 없으면 "테스트 보강 대상 없음" 출력 후 Step 7로 이동.

### Step 5: 중복 이슈 확인

각 테스트 갭에 대해 fingerprint를 생성하고, CLI로 중복을 확인합니다 (issue-label 스킬 참조):

```bash
# fingerprint 형식: qa:{source_file_path}:{test_type}
FINGERPRINT="qa:src/auth/refresh.rs:unit"

autopilot issue check-dup --fingerprint "$FINGERPRINT"
```

중복인 갭은 skip합니다.

### Step 6: 이슈 발행

테스트 보강이 필요한 각 항목에 대해 autopilot CLI로 이슈를 생성합니다:

```bash
autopilot issue create \
  --title "test(scope): add missing tests for [source file/module]" \
  --label "{label_prefix}qa-suggestion" \
  --fingerprint "qa:${SOURCE_FILE}:${TEST_TYPE}" \
  --body "$(cat <<'EOF'
## 테스트 보강 대상

- **소스 파일**: [변경된 소스 파일 경로]
- **변경 내용**: [변경된 함수/메서드 요약]

## 누락된 테스트

| 테스트 타입 | 대상 | 설명 |
|------------|------|------|
| unit | [함수명] | [정상 경로 / 에러 경로 / 경계값 등] |
| e2e | [워크플로우] | [엔드투엔드 시나리오] |

## 기존 테스트 현황

- 기존 테스트 파일: [경로 또는 없음]
- 커버리지 갭: [구체적으로 빠진 부분]

## 구현 가이드

- 프로젝트 테스트 컨벤션 따름
- 기존 테스트 수정 금지 (추가만)
EOF
)"
```

> **참고**: fingerprint HTML 주석은 CLI가 body 하단에 자동 삽입합니다.

### Step 7: 결과 보고

```
## QA Boost 결과

### 분석
- 기준 커밋: abc1234 (또는 HEAD~20)
- 분석한 변경 파일: 15개
- 테스트 보강 필요: 5개

### 발행된 이슈
| # | 소스 파일 | 테스트 타입 |
|---|----------|-----------|
| #60 | src/auth/refresh.rs | unit |
| #61 | src/api/handler.rs | unit, e2e |

### 건너뛴 항목
- src/auth/mod.rs: 이미 이슈 존재 (#45)
```

## 주의사항

- 테스트를 직접 구현하지 않음 — 이슈로 발행하여 사용자가 검토 후 처리
- `{label_prefix}qa-suggestion` 라벨을 사용하여 build-issues의 자동 처리 큐(`:ready`)와 분리
- 사용자가 이슈를 검토 후 `{label_prefix}ready`로 라벨을 변경하면 build-issues가 처리
- issue-label 스킬의 라벨 필수 규칙과 fingerprint 규칙을 반드시 따른다
- 기존 이슈와 중복되지 않도록 fingerprint로 검사
