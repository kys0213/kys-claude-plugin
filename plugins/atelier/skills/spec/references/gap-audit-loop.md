# L2 종합 + gap-auditor 단일 게이트 루프

검증 통과한 L1 리포트를 `gap-aggregator` (L2) 로 종합하고, `gap-auditor` 가 단일 게이트로 감사하며 major == 0 이 될 때까지 L2 를 fix 하는 프로토콜. `spec-review`·`gap-detect` 공통.

## L2 종합 (gap-aggregator)

검증 통과한 L1 리포트들을 입력으로 `gap-aggregator` 에이전트를 `run_in_background=false` 로 spawn (모델: sonnet). 단일 spec 인 경우 spec↔spec gaps 섹션은 비고, code↔spec gaps 가 핵심 출력.

```
# Gap Analysis Request

## L1 Reports (검증 통과분)

### Report: {filename_1}
{L1 리포트 본문 1, drop 항목 제외 정제본}

### Report: {filename_2}
{L1 리포트 본문 2, drop 항목 제외 정제본}

...

## 출력
gap-aggregator 의 출력 스키마를 엄수.
```

## audit 단일 게이트 (gap-auditor)

L2 출력 후 `gap-auditor` 에이전트를 spawn (sonnet, `run_in_background=false`).
입력: L1 reports (검증 통과 정제본) + L2 output. 출력: major / minor 분류된 audit 리포트.

mechanical L2 인용 검증은 두지 않는다 — gap-auditor 가 인용 정확성 (M-0) + 의미 적합성 (M-1~M-6) 을 단일 게이트로 흡수한다.

### 입력 프롬프트

```
# Gap Audit Request

## L1 Reports (검증 통과분)

### Report: {filename_1}
{L1 리포트 정제본}

...

## L2 Output (감사 대상)

{gap-aggregator 출력 본문 — Code↔Spec Gaps / Spec↔Spec Gaps / Notes 섹션 그대로}

## 감사 책임
1. 인용 정확성 (M-0): 모든 finding 의 {report}:{ID} 인용이 L1 에 실재하는가, 발췌가 L1 항목과 일치하는가
2. 인용-결론 매칭 (M-1): 인용된 L1 항목이 finding 의 결론을 뒷받침하는가
3. 분류 정확성 (M-2): SPEC_ONLY / DEFINITION_CONFLICT 등 분류가 정의에 맞는가
4. 심각도 정확성 (M-3): HIGH/MEDIUM/LOW 가 기준에 맞는가
5. 중복 / 누락 / false positive (M-4, M-5, M-6)

## 출력
gap-auditor 의 출력 스키마를 엄수.
```

분류 기준 / 카테고리 / 출력 스키마 상세는 `gap-auditor` 에이전트 명세 참조.

### 루프 정책

```
iter = 0
prev_major_ids = None
l2_output = L2 종합 결과

while iter < 3:
    audit = call_gap_auditor(l1_reports, l2_output, iter)
    major = audit.major_issues
    if not major:
        break  # 종료 조건 1: major == 0
    if iter > 0 and {m.finding_id for m in major} == prev_major_ids:
        break  # 종료 조건 2: 동일 finding 들이 같은 사유로 반복 (진전 없음)
    fix_prompt = build_l2_fix_request(l1_reports, l2_output, major)
    l2_output = call_l2_fix(fix_prompt)  # L2 재진입
    prev_major_ids = {m.finding_id for m in major}
    iter += 1

# 종료 조건 3: iter == 3 도달 — 강제 종료
# 잔여 major 는 drop log 로 사용자 가시 노출
if major:
    log_audit_drop(major)
```

상세 알고리즘은 `plans/spec-kit-l2-reviewer/02-architecture.md` §"루프 통합" 참조.

### L2 fix request 형식

```
# Gap Analysis Fix Request

## 이전 L2 출력
{prev L2 output 본문 그대로 — 통과/실패 finding 모두}

## L1 Reports (재첨부)
{변경 없음}

## Audit Major Issues

### [R1] {finding_id} — M-{N}: {category}
- 현재 finding 본문:
  {원문 발췌}
- 사유: {auditor reasoning}
- 근거 L1 인용: {report:ID}
- 권장 조치: {구체적}

{... 모든 major 반복 ...}

## 지시
- Major 이슈만 수정.
- 통과 finding (major 표기 없는 것) 은 절대 변경 금지.
- M-0 (INVALID_CITATION) 의 경우 인용 정정 또는 finding 제거.
- M-5 (MISSED_OBVIOUS_GAP) 의 경우 새 finding 추가 가능 — L1 인용은 auditor 가 제시한 것 사용.
- M-4 (DUPLICATE_FINDING) 의 경우 finding 통합/제거.
- 분류 변경 시 auditor 가 권장한 분류 사용.
- severity 변경 시 auditor 가 권장한 값 사용.
- 출력 스키마는 gap-aggregator 의 기존 스키마 그대로.
```

### 종료 조건 (3가지)

1. **major == 0** — finalize
2. **동일 major finding ID 집합 반복** — 진전 없음. 잔여 major 는 drop log
3. **iter == 3 도달** — 강제 종료. 잔여 major 는 drop log

### drop log 노출 (사용자 가시)

```
🔍 의미 + 인용 통합 감사 결과
  - L2 호출: K회 (초회 1 + 재호출 K-1)
  - gap-auditor 호출: K회
  - major drop: J건 (분류별 breakdown)
    - M-0 invalid citation: {n}건
    - M-1 evidence-conclusion mismatch: {n}건
    - ...
  - minor: I건 (루프 미발생, 정보 제공)
```

minor 는 drop 이 아니라 "참고" 수준으로 사용자에게 노출 (선택적 출력).

### 실패 모드

- gap-auditor 자체 호출 실패: 1회 retry, 그래도 실패 시 audit 단계 skip + 사용자에게 "감사 단계 미수행" 알림. L2 의 raw output 을 그대로 최종 리포트에 사용 (이때는 검증 부재를 명시적으로 노출)
- auditor 가 잘못된 L1 인용 사용: 다른 audit item 은 살림
- L2 가 fix request 후에도 동일 major 반복: "진전 없음" 종료 조건 발동 → drop log
