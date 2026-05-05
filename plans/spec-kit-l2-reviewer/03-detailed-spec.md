# Detailed Spec — gap-auditor 에이전트

## 에이전트 frontmatter

```yaml
---
description: (내부용) /spec-kit:spec-review · /spec-kit:gap-detect 가 호출하는 통합 감사 에이전트. L2 finding 의 인용 정확성 + 의미 적합성을 단일 게이트로 검증한다.
model: sonnet
tools: []
---
```

`tools: []` — raw 파일 접근 금지. L1 reports + L2 output 만 입력으로 받는다.

## 분류 기준 (gap-auditor 가 사용)

### Code↔Spec Gap 분류

| 분류 | 정의 |
|------|------|
| `SPEC_ONLY` | spec 명시, code 미발견. L1 의 어떤 Code Observation 도 해당 spec claim 영역에 매핑 안 됨 |
| `CODE_ONLY` | code 존재, spec 미언급. L1 의 어떤 Spec Claim 도 해당 code 영역을 다루지 않음 |
| `PARTIAL` | spec 의 일부 약속만 code 에 구현. 양쪽에 evidence 있으나 범위/조건이 부족 |
| `DIVERGENT` | 양쪽에 구현 있으나 동작/계약이 다름. 인터페이스/시그니처/상수가 차이 |

### Spec↔Spec Gap 분류

| 분류 | 정의 |
|------|------|
| `DEFINITION_CONFLICT` | 같은 도메인 개념을 두 spec 이 다른 값/타입/범위로 정의 |
| `INTERFACE_DRIFT` | 두 spec 이 같은 컴포넌트의 인터페이스/책임 경계를 다르게 기술 |
| `TERM_AMBIGUITY` | 같은 용어가 두 spec 에서 다른 의미로 사용. 도메인 미일치 |
| `REQUIREMENT_OVERLAP` | 두 spec 이 같은 요구사항을 다른 표현으로 중복 기술 (충돌 아님) |

### Severity 기준

| Severity | 기준 |
|----------|------|
| HIGH | production 동작/데이터/보안에 직접 영향. spec 의 핵심 약속 위반. 사용자 가시 결함 |
| MEDIUM | 동작 영향은 제한적이지만 spec 와 code 가 의미 있게 발산. 향후 유지보수에 영향 |
| LOW | 표현/명명 불일치, 누락된 doc, 무해한 변동. 정보 가치 |

## Major / Minor 카테고리

### Major (루프 발생)

| 코드 | 카테고리 | 정의 | gap-auditor 가 권장하는 조치 |
|------|----------|------|-----------------------------|
| `M-0` | INVALID_CITATION | 인용 ID 미실재 또는 발췌가 L1 항목과 의미적으로 어긋남 | 인용 정정 또는 finding 제거 |
| `M-1` | EVIDENCE_CONCLUSION_MISMATCH | 인용된 L1 항목이 L2 결론을 뒷받침하지 않음 | 결론 수정 또는 finding 제거 |
| `M-2` | MISCLASSIFICATION | 분류가 정의에 부합하지 않음 | 분류 변경 |
| `M-3` | SEVERITY_MISJUDGMENT | severity 가 기준과 어긋남 | severity 변경 |
| `M-4` | DUPLICATE_FINDING | 다른 ID 와 동일 사안 | finding 통합 또는 제거 |
| `M-5` | MISSED_OBVIOUS_GAP | L1 에 명백한 evidence 가 있으나 finding 없음 | finding 추가 |
| `M-6` | FALSE_POSITIVE | 같은 L1 의 다른 항목으로 반박됨 | finding 제거 |

### Minor (정보 제공)

| 코드 | 카테고리 | 정의 |
|------|----------|------|
| `m-1` | UNCLEAR_RECOMMENDATION | 권장 액션 불명확 |
| `m-2` | EVIDENCE_AUGMENTATION | 추가 인용으로 강화 가능 |
| `m-3` | ALTERNATIVE_ACTION | 더 나은 권장 액션 존재 |

## 에이전트 입력 (orchestrator → gap-auditor)

```
# Gap Audit Request

## L1 Reports (검증 통과분)

### Report: {filename_1}
{본문 그대로}

### Report: {filename_2}
{본문 그대로}

...

## L2 Output (감사 대상)

{Code↔Spec Gaps / Spec↔Spec Gaps / Notes 섹션 그대로}

## 감사 책임

다음을 모두 검증한다:

1. **인용 정확성 (M-0)**:
   - 모든 finding 의 {report}:{ID} 인용이 L1 reports 에 실재하는가
   - L2 가 발췌한 텍스트가 해당 L1 항목의 발췌와 의미적으로 일치하는가

2. **인용-결론 매칭 (M-1)**:
   - 인용된 L1 항목이 finding 의 결론을 실제로 뒷받침하는가
   - 도메인/컨텍스트가 같은 사안을 가리키는가

3. **분류 정확성 (M-2)**: 아래 분류 기준에 따른 분류인가

4. **심각도 정확성 (M-3)**: 아래 severity 기준에 따른 등급인가

5. **중복 / 누락 / false positive (M-4, M-5, M-6)**

## 분류 기준 (요약)

### Code↔Spec
- SPEC_ONLY: spec 명시, code 미발견
- CODE_ONLY: code 존재, spec 미언급
- PARTIAL: 일부만 구현
- DIVERGENT: 양쪽 구현 차이

### Spec↔Spec
- DEFINITION_CONFLICT: 같은 개념 다른 정의
- INTERFACE_DRIFT: 책임 경계 표류
- TERM_AMBIGUITY: 용어 도메인 mismatch
- REQUIREMENT_OVERLAP: 중복 기술

### Severity
- HIGH: production 영향, 핵심 약속 위반
- MEDIUM: 의미 있는 발산
- LOW: 표현/명명

## 출력
gap-auditor 의 출력 스키마를 엄수.
```

## 에이전트 출력

```markdown
# Gap Audit Report

## Metadata
- l2_findings_total: N
- generated_at: {ISO 8601}

## Major Issues
- [R1] {finding_id} — M-{N}: {category}
  - 사유: {1-2 문장. L1 evidence 와 L2 결론의 어긋남을 구체적으로}
  - 근거 L1 인용: {report:ID}, {report:ID}
  - 권장: {분류 변경 / severity 변경 / finding 제거 / finding 추가 / dedupe / 인용 정정}

## Minor Issues
- [r1] {finding_id} — m-{N}: {category}
  - 사유: {한 줄}

## Notes
- (없음 또는 gap-auditor 가 추가로 관찰한 메타 정보)
```

빈 섹션은 `(없음)` 한 줄. 스키마 일관성 유지.

## 에이전트 제약 (gap-auditor 본문에 포함)

- **raw 파일 접근 금지**: L1 reports 의 인용을 그대로 신뢰. raw 파일 인용 검증은 별도 단계에서 끝남.
- **L1 인용 외 새 인용 금지**: 모든 evidence 는 `{report}:{ID}` 형식.
- **수정 권한 없음**: 지적만. finding 의 본문을 다시 쓰지 마라. 수정은 L2 가 다음 호출에서 수행.
- **무한 nitpicking 금지**: 같은 finding 에 대해 동일한 minor 사유를 여러 번 보고하지 마라.
- **분류 변경 권장 시 정확한 타깃 분류 명시**: "DEFINITION_CONFLICT 가 아니다" 가 아니라 "INTERFACE_DRIFT 로 변경" 처럼.
- **severity 변경 권장 시 정확한 타깃 명시**: "HIGH → MEDIUM" 처럼.
- **major / minor 분류 신중**: 확신이 없으면 minor. major 는 명백한 오류만.
- **인용 검증은 의미 일치까지** : 글자 그대로의 string compare 가 아니라 의미 동치성. paraphrasing 이라도 같은 사실을 가리키면 통과. 다른 사실을 가리키면 M-0.
- **`<tool_call>` / `<tool_response>` 같은 가짜 블록 출력 금지** (L1 의 #639 환각 회귀 방지).

## L2 fix request 형식 (orchestrator → L2 재호출)

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

## 진전 판정 알고리즘

```python
def has_progress(prev_major_ids, curr_major):
    curr_ids = {m.finding_id for m in curr_major}
    if not curr_ids:
        return True  # 모두 해결
    if curr_ids == prev_major_ids:
        return False  # 같은 finding 이 같은 사유로 반복
    return True  # 일부라도 변동 있음
```

`finding_id + category` 조합으로 비교하면 더 엄밀하지만, dogfood 결과를 보고 결정. 우선 finding_id 만으로 시작.

## 최종 리포트 통합

`/spec-kit:spec-review` Step 7 출력에 audit 통계 추가:

```markdown
# Spec Review Report

{L2 최종 본문}

---

## 검증 통계
- spec 파일: N
- L1 리포트 통과: M / N
- L1 항목 drop: K건
- gap-auditor 반복: I회 (major issue 0 까지)
- gap-auditor drop (잔여 major): L건 (분류별)
- 분석 모델: L1 haiku × N + L2 sonnet + gap-auditor sonnet
```

## 호환성

- gap-aggregator 의 출력 스키마는 변경하지 않음 → gap-auditor 추가는 backward-compatible
- L1 (file-pair-observer) 변경 없음
- mechanical L2 인용 검증 단계는 **제거** — `/spec-review` 와 `/gap-detect` 에서 Step 6 을 gap-auditor 호출로 교체
- 호출 커맨드 (`/spec-review`, `/gap-detect`) 의 Step 6 만 변경
