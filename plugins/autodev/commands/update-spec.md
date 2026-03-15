---
description: 스펙 수정 — 대화형 영향 분석 후 스펙 업데이트
argument-hint: "<spec-id>"
allowed-tools: ["AskUserQuestion", "Bash", "Read", "Glob", "Grep"]
---

# 스펙 수정 (/update-spec)

레포의 Claude 세션에서 실행합니다. 기존 스펙의 내용을 대화형으로 수정하고, 변경 영향을 분석합니다.

> `cli-reference` 스킬을 참조하여 autodev CLI 명령을 호출합니다.

## 사용법

- `/update-spec <spec-id>` — 지정 스펙 수정

## 실행

### Step 0: CLI 바이너리 확인

```bash
bash ${CLAUDE_PLUGIN_ROOT}/scripts/ensure-binary.sh
```

### Step 1: 현재 스펙 + 진행 상태 로드

```bash
autodev spec status <spec-id> --json
```

현재 상태를 요약하여 출력합니다:

```
📋 <title> 현재 상태:
  진행도: N/M (X%)
  ✅ #42 JWT middleware (done)
  🔄 #44 Session adapter (implementing)
  ⏳ #45 Error handling (pending)
```

### Step 2: 변경 방향 파악

AskUserQuestion으로 어떤 부분을 변경하고 싶은지 물어봅니다.
사용자의 응답을 기반으로 변경 의도를 파악합니다.

### Step 3: 변경 영향 분석

코드베이스와 기존 이슈 상태를 대조하여 영향 범위를 분석합니다:

1. **이슈 영향**: 불필요해지는 이슈, 수정 필요한 이슈 식별
2. **코드 영향**: Glob/Grep으로 관련 파일 탐색
3. **Acceptance Criteria 영향**: 제거/추가/수정 항목 식별

분석 결과를 출력합니다:

```
영향 범위:
  - #44 Session adapter (implementing) → 불필요해짐
  - #43 Token API (done) → refresh 로직 수정 필요
  - 아키텍처: auth/session.rs 제거, auth/token.rs 수정
  - Acceptance Criteria: 세션 관련 항목 제거, stateless 검증 추가
```

### Step 4: 변경 제안 + 사용자 승인

제안 사항을 정리하여 AskUserQuestion으로 확인받습니다:

```
제안:
  1. #44 → autodev:skip (불필요)
  2. #43 → 새 이슈로 재작업
  3. Acceptance Criteria 업데이트
  4. 아키텍처 섹션 수정

이대로 진행할까요?
```

### Step 5: 스펙 업데이트 실행

승인 후 수정된 스펙을 임시 파일에 저장하고 CLI로 업데이트합니다:

```bash
SPEC_TMP=$(mktemp /tmp/autodev-spec-XXXXXX.md)
# ... 수정된 스펙을 $SPEC_TMP에 저장 ...
autodev spec update <spec-id> --file "$SPEC_TMP"
rm -f "$SPEC_TMP"
```

skip 처리가 필요한 이슈가 있으면:

```bash
autodev queue skip <work-id>
```

완료 메시지:

```
✅ 스펙 업데이트 완료.
  - 변경 사항 요약
  - Claw가 다음 틱에서 업데이트된 스펙 기반으로 재판단합니다.
```
