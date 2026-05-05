# Architecture — gap-auditor 에이전트와 L2 ↔ auditor 루프

## 컴포넌트

| 이름 | 역할 | 모델 | tools |
|------|------|------|-------|
| `file-pair-observer` (L1) | per-file 관찰 | haiku | Read, Glob, Grep |
| `gap-aggregator` (L2) | cross-file 종합 | sonnet | (없음) |
| **`gap-auditor` (신규)** | L2 finding 의 인용 + 의미 통합 감사 | sonnet | (없음) |

gap-auditor 는 L2 와 동일하게 **raw 파일 미접근**. L1 reports + L2 findings 만 입력.

## 호출 흐름 (전체)

```
[Step 3] L1 (haiku × N, 병렬)
[Step 4] L1 인용 검증 + L1 feedback loop (현행 유지)
[Step 5] L2 (sonnet × 1)
[Step 6] gap-auditor 단일 게이트 (NEW — mechanical L2 검증을 흡수)
            ├─ major 0 → 종료, finalize
            ├─ iter < 3 && 진전 있음 → L2 fix request → Step 5 재진입
            └─ 그 외 → 잔여 major 를 drop log 로 노출, finalize
[Step 7] 최종 리포트
```

기존 Step 6 (mechanical L2 인용 검증) 은 **삭제**. gap-auditor 가 인용 정확성 + 의미 적합성을 통합 감사한다.

## 통합 게이트 결정

mechanical L2 인용 검증 단계를 별도로 두지 않는 이유:

- L2 인용은 raw 파일 발췌가 아니라 `{report}:{ID}` 메타참조 → string compare 검증의 가치가 낮음
- `M-0 INVALID_CITATION` 카테고리로 gap-auditor 가 흡수 → 검증 분기 단일화
- L2 fix request 후 재호출 시 별도 mechanical 검증을 다시 돌릴 필요 없음
- 인용 오류 + 의미 오류가 한 finding 에 동시에 존재할 때 통합 판단 가능

대신 gap-auditor 의 입력 프롬프트에 **인용 검증 책임이 명시** 되어야 한다 (03-detailed-spec.md §"입력" 참조).

## gap-auditor 입력

```
# Gap Audit Request

## L1 Reports (검증 통과분)
{Step 4 결과의 정제본 — 그대로 첨부}

## L2 Output (감사 대상)
{Step 5 결과 — Code↔Spec / Spec↔Spec / Notes 섹션}

## 감사 책임
1. 인용 정확성: 모든 finding 의 {report}:{ID} 인용이 L1 에 실재하는가, 발췌가 L1 항목과 일치하는가
2. 인용-결론 매칭: 인용된 L1 항목이 실제로 finding 의 결론을 뒷받침하는가
3. 분류 정확성: SPEC_ONLY / DEFINITION_CONFLICT 등 분류가 정의에 맞는가
4. 심각도 정확성: HIGH/MEDIUM/LOW 가 기준에 맞는가
5. 중복 / 누락 / false positive

## 분류 기준 (요약)
{03-detailed-spec.md §"분류 기준" 섹션 발췌}

## 출력
gap-auditor 의 출력 스키마 엄수.
```

## gap-auditor 출력

```markdown
# Gap Audit Report

## Metadata
- l2_findings_total: N
- iteration: K (현재 반복 회차)
- generated_at: ISO 8601

## Major Issues (수정 필요 — 루프 발생)
- [R1] {finding_id} — {category: M-0|M-1|M-2|M-3|M-4|M-5|M-6}
  - 사유: {한 줄}
  - 근거 L1 인용: {report:ID, ...}
  - 권장: {분류 변경 / severity 변경 / finding 제거 / finding 추가 / dedupe / 인용 정정}

## Minor Issues (수정 권장 — 루프 미발생)
- [r1] {finding_id} — {category: m-1|m-2|m-3}
  - 사유: {한 줄}

## Notes
- (없음 또는 gap-auditor 의 컨텍스트 관찰)
```

## 루프 통합

```python
iter = 0
prev_major_ids = None
l2_output = call_l2(l1_reports)

while iter < 3:
    audit = call_gap_auditor(l1_reports, l2_output)
    major = audit.major_issues
    if not major:
        break
    if iter > 0 and {m.finding_id for m in major} == prev_major_ids:
        # 진전 없음 — 동일 finding 들이 같은 사유로 반복
        break
    fix_prompt = build_l2_fix_request(l1_reports, l2_output, major)
    l2_output = call_l2_fix(fix_prompt)
    prev_major_ids = {m.finding_id for m in major}
    iter += 1

# 잔여 major 는 drop log 로 노출
if major:
    log_audit_drop(major)
```

## L2 fix request 형식

```
# Gap Analysis Fix Request

## 이전 L2 출력
{prev L2 output 본문 그대로}

## L1 Reports (재첨부)
{변경 없음 — 동일}

## Audit Major Issues

### [R1] {finding_id} — {category}
- 현재 finding: {원문 발췌}
- 사유: {auditor 의 reasoning}
- 근거 L1 인용: {report:ID}
- 권장 조치: {구체적 수정 방향}

{... 모든 major 반복 ...}

## 지시
- Major 이슈만 수정.
- 통과 finding 은 변경 금지.
- M-0 (인용 오류) 의 경우 인용을 정정하거나 finding 제거.
- M-5 (누락) 의 경우 새 finding 추가 가능 — L1 인용은 auditor 가 제시한 것 사용.
- M-4 (중복) 의 경우 finding 통합/제거.
- 출력 스키마는 gap-aggregator 의 기존 스키마 그대로.
```

## 종료 조건 (3가지)

1. **major == 0** — finalize
2. **동일 major finding ID 집합 반복** — 진전 없음. 잔여 major 는 drop log
3. **iter == 3 도달** — 강제 종료. 잔여 major 는 drop log

## drop log 노출 (사용자 가시)

```
🔍 의미 + 인용 통합 감사 결과
  - L2 호출: K회 (초회 1 + 재호출 K-1)
  - gap-auditor 호출: K회
  - major drop: J건 (분류별 breakdown)
    - M-0 invalid citation: 1건
    - M-1 evidence-conclusion mismatch: 1건
    - M-3 severity overestimate: 1건
  - minor: I건 (루프 미발생, 정보 제공)
```

minor 는 drop 이 아니라 "참고" 수준으로 사용자에게 노출 (선택적 출력).

## 호출자 변경 (커맨드)

`/spec-kit:spec-review` Step 6 을 다음으로 교체:

```markdown
### Step 6: gap-auditor 단일 게이트 (인용 + 의미 통합 감사 + 루프)

L2 출력 후 gap-auditor 에이전트를 spawn (sonnet, run_in_background=false).
입력: L1 reports + L2 output. 출력: major/minor 분류된 audit report.

루프 정책:
- major == 0 → 종료
- iter < 3 && 진전 있음 → L2 에 fix request 보내고 재호출 (Step 5 재진입)
- 그 외 → 잔여 major 를 drop log 로 사용자에게 노출

상세 알고리즘은 plans/spec-kit-l2-reviewer/02-architecture.md §"루프 통합" 참조.
```

`/spec-kit:gap-detect` 도 동일한 Step 6 교체 (백본 공유).

## 실패 모드 처리

| 실패 모드 | 처리 |
|-----------|------|
| gap-auditor 자체 호출 실패 | retry 1회, 그래도 실패 시 audit 단계 skip + 사용자 알림 |
| auditor 가 잘못된 L1 인용 사용 | M-0 사유로 자기 audit item 자체가 무효 — 다른 audit item 은 살림 |
| L2 가 fix request 후에도 동일 major 반복 | "진전 없음" 종료 조건. 사용자 가시 drop |
| auditor 가 minor 만 무한 생성 | major 0 이면 종료. minor 는 정보 제공일 뿐 |

## 결정 사항

- gap-auditor 는 **raw 파일 접근 권한 없음** (L2 와 동일 isolation) — L1 reports 가 단일 ground truth
- gap-auditor 는 **수정 권한 없음** — 비평만, 수정은 L2 가 다음 호출에서
- mechanical L2 인용 검증 단계는 **제거** — gap-auditor 의 M-0 으로 흡수
- **major / minor 만 출력** — severity 4단계 같은 세분화는 노이즈
- 최대 3회는 사용자 제안 그대로
- **진전 없음 종료 조건 추가** — L1 루프와 같은 패턴
