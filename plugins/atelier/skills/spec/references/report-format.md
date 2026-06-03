# 최종 리포트 출력 포맷

`spec-review` 와 `gap-detect` 의 최종 출력 구조. 둘은 백본(L1→L2→audit)이 같고 **emphasis 만 다르다** — spec-review 는 다중 spec 의 spec↔spec gap 을 동등 비중으로, gap-detect 는 단일 spec 의 code↔spec gap 을 우선 표시.

## 측정 가이드 (footer 입력용)

분석 진행 중 다음 값을 자체 측정해 누적한다 (별도 측정 도구 없음, LLM 이 직접 카운트):

- **시작/종료 시각** (`t_start`/`t_end`): 첫 Step 직전 / 출력 직전 timestamp. wall-clock = 차이(초, 정수 반올림)
- **호출 횟수** (재시도/fix 포함):
  - `n_l1_initial`: L1 초회 spawn 수 (= spec 파일 수)
  - `n_l1_fix`: L1 피드백 fix 호출 수 (리포트별 합)
  - `n_l2_refix`: audit 루프에서 L2 재호출 수
  - `n_auditor`: gap-auditor 호출 수 (초회 + 루프 재호출 + retry)

토큰 측정은 Task 도구가 노출할 때만 추가 (미노출 시 항목 생략).

## spec-review 출력

```markdown
# Spec Review Report

{최종 L2 출력 본문 그대로 — Summary / Code↔Spec Gaps / Spec↔Spec Gaps / Notes}

---

## 검증 통계
- spec 파일: N
- L1 리포트 통과: M / N
- L1 항목 drop: K건
- gap-auditor 반복: I회 (major issue 0 까지)
- gap-auditor drop (잔여 major): L건 (분류별)
- 호출 횟수:
  - L1 (file-pair-observer): {n_l1_initial}회 + 인용 fix {n_l1_fix}회
  - L2 (gap-aggregator): 1회 + 재호출 {n_l2_refix}회
  - gap-auditor: {n_auditor}회
- wall-clock: ~{wall_clock_sec}초
- 분석 모델: L1 haiku × N + L2 sonnet + gap-auditor sonnet
```

## gap-detect 출력

Code ↔ Spec Gaps 를 우선 표시. 부속 섹션(Spec↔Spec gaps, Notes)은 발견 시에만 노출.

```markdown
# Gap Detection Report

## Summary
- spec_file: {경로}
- code_paths_examined: {목록}
- code_spec_gaps: HIGH=N, MEDIUM=N, LOW=N
- generated_at: ...

## Code ↔ Spec Gaps
{최종 L2 의 Code↔Spec Gaps 섹션 그대로}

{spec↔spec 또는 notes 가 있을 경우에만:}

## 부수 발견

### Spec ↔ Spec (다중 spec 비교 시 발견된 일관성 이슈)
{최종 L2 의 Spec↔Spec Gaps 섹션 — 단일 spec 분석에서는 보통 비어 있음}

### Notes (모호한 spec 표현)
{최종 L2 의 Notes 섹션}

---

## 검증 통계
{spec-review 와 동일 형식}
```

## Output Examples

### spec-review 성공

```markdown
# Spec Review Report

## Summary
- spec_files_reviewed: 2
- l1_reports_received: 2
- code_spec_gaps: HIGH=1, MEDIUM=2, LOW=0
- spec_spec_gaps: HIGH=1, MEDIUM=0, LOW=0
- generated_at: 2026-05-05T...

## Code ↔ Spec Gaps
### [HIGH] Rate limiting 미구현
- 증거:
  - auth.md:G1 — "all writes are rate-limited"
- 분류: SPEC_ONLY
- 권장: rate limit middleware 추가 또는 spec 에서 제거

## Spec ↔ Spec Gaps
### [HIGH] content_type 의미 충돌
- 증거:
  - database.md:S1 — "content_type 컬럼은 VARCHAR(255)"
  - proxy.md:S2 — "content_type 은 ENUM('json','multipart','form')"
- 분류: DEFINITION_CONFLICT
- 권장: ...

## Notes (모호함)
(없음)
```

### gap-detect 갭 발견

```markdown
# Gap Detection Report

## Summary
- spec_file: docs/auth-spec.md
- code_paths_examined: [internal/auth/]
- code_spec_gaps: HIGH=1, MEDIUM=1, LOW=0

## Code ↔ Spec Gaps

### [HIGH] Refresh token 회전 미구현
- 증거:
  - auth-spec.md:G1 — SPEC_ONLY — "Refresh token 회전: 매 사용 시 새 토큰 발급" (auth-spec.md:200-215) → 해당 영역 code 미발견
- 분류: SPEC_ONLY
- 권장: refresh 핸들러에 회전 로직 추가 또는 spec 에서 제거
```

### gap-detect 갭 없음

```markdown
# Gap Detection Report

## Summary
- spec_file: docs/auth-spec.md
- code_spec_gaps: HIGH=0, MEDIUM=0, LOW=0

✅ 검출된 갭 없음. spec 과 code 가 일치.
```

### drop 발생 시 (공통)

```markdown
🛡️ 인용 검증 결과
  - 통과 리포트: 2 / 2
  - 항목별 drop: 3건
    - excerpt mismatch: 2건 (database.md:S5, database.md:C7)
    - line out of range: 1건 (proxy.md:S2)
  - 재시도: 0회

🔍 의미 + 인용 통합 감사 결과
  - L2 호출: 2회 (초회 1 + 재호출 1)
  - gap-auditor 호출: 2회
  - major drop: 1건 (분류별 breakdown)
    - M-3 severity misjudgment: 1건
  - minor: 2건 (루프 미발생, 정보 제공)
```
