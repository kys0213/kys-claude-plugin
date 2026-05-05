---
description: 스펙 문서와 관련 코드를 대조하여 spec↔code 갭, spec↔spec 갭, 모호함을 통합 분석합니다
argument-hint: "<스펙파일 [스펙파일2 ...]>"
allowed-tools: ["Task", "Glob", "Read", "Grep", "AskUserQuestion"]
---

# Spec Review (/spec-review)

스펙 문서의 완성도를 **실제 코드와의 대조**로 검증합니다. 한 spec 파일과 그 관련 코드 영역을 묶어 per-file 관찰자(L1)가 사실을 나열하고, 종합 분석기(L2)가 cross-file 패턴을 찾습니다. 모든 결론은 `file:line` 인용으로 추적 가능합니다.

## 사용법

```bash
/spec-review "docs/auth-spec.md"                              # 단일 파일
/spec-review "docs/api-spec.md" "docs/data-model.md"          # 다중 파일 (명시적)
/spec-review "spec/v5.1/"                                      # 디렉터리 → 파일 목록 확인 후 진행
```

| 인자 | 필수 | 설명 |
|------|------|------|
| 스펙파일 | Yes | 하나 이상의 스펙 마크다운 경로 또는 디렉터리 |

## 작업 프로세스

### Step 1: 입력 파싱 및 파일 확정

#### Case A: 명시적 파일 경로

각 파일이 존재하는지 Glob 으로 확인. 미존재 시 즉시 에러:

```
Error: 스펙 파일을 찾을 수 없습니다: [경로]
```

#### Case B: 디렉터리 또는 Glob 패턴

매칭되는 `.md` 파일 목록을 Glob 으로 수집한 뒤 AskUserQuestion 으로 확인:

```
발견된 스펙 파일 (N개):
1. docs/overview.md
2. docs/api-spec.md
...

이 파일들을 모두 리뷰 대상으로 사용할까요?
제외할 파일이 있으면 번호를 알려주세요.
```

#### Case C: 인자 없음

AskUserQuestion 으로 경로 요청.

### Step 2: 각 spec 파일의 `related_paths` 결정

각 spec 파일에 대해:

1. **frontmatter 파싱**: 파일 상단 YAML frontmatter 의 `related_paths` 필드 추출
2. **자율 보강 (frontmatter 비었을 때)**: spec 본문에서 식별자/경로 패턴을 Grep 으로 추출 → 프로젝트 디렉터리 구조와 매칭하여 후보 추정. 추정 결과는 사용자에게 confirm 요청.
3. **결과**: 각 spec 파일별 `(spec_path, [related_paths])` 페어 확정

### Step 3: file-pair-observer (L1) 병렬 spawn

각 spec 파일마다 1개의 file-pair-observer 에이전트를 `run_in_background=true` 로 동시 spawn 한다 (모델: haiku). 입력 프롬프트:

```
# File Observation Request

## Spec 파일
- 경로: {spec_file_path}

## 관련 code 경로 (frontmatter Hint)
{related_paths}

## 자율 탐색 허용 범위
- 위 경로 + 그 경로에서 import/require 된 인접 파일

## 출력
file-pair-observer 의 출력 스키마를 엄수하여 per-file 리포트를 마크다운으로 반환.
```

모든 에이전트 완료까지 대기.

### Step 4: L1 인용 검증 + 피드백 루프 (오케스트레이터)

각 L1 리포트별로 인용을 검증하고, 실패 항목이 있으면 L1 에이전트에 **구체적 피드백을 보내 수정**한다 (전체 재실행이 아닌 targeted 수정). 최대 3회 반복.

#### 4.1 입력 파일 일괄 읽기

리포트가 인용하는 모든 file (spec 1개 + 인용된 code 파일 N개) 을 Read 도구로 한 번씩 읽어 메모리에 보관 (피드백 루프 내내 재사용, 중복 호출 회피).

#### 4.2 검증 절차

리포트의 각 항목 (`S{n}`, `C{n}`, `G{n}`, `M{n}`, `N{n}`) 에 대해:

1. **인용 파싱**: `` `path:line_start[-line_end]` `` 추출 + 발췌 (`"..."` 또는 `` `...` `` 안 텍스트)
2. **파일 검증**: 4.1 에서 읽은 파일 중에 있는지. 없으면 fail ("file not read or not exist")
3. **라인 범위 검증**: line_start/line_end 가 파일 라인 수 범위 내인지. 초과 시 fail ("line out of range")
4. **발췌 검증**: line_start ~ line_end 범위 텍스트와 발췌를 공백 정규화 (연속 공백 → 단일 공백, 양끝 trim) 후
   - 발췌 끝에 `...` 있으면 prefix match
   - 없으면 substring 포함
   - 실패 시 fail ("excerpt mismatch")
5. **ID 일관성**: 같은 카테고리에서 ID 충돌 → fail ("duplicate id"). Mismatches/Gaps 의 참조 ID 가 리포트 내 실재하지 않으면 fail ("dangling reference")

검증 실패 항목은 임시 보관 (즉시 drop 하지 않고 4.3 피드백에 사용).

#### 4.3 피드백 루프

```
iter = 0
prev_failure_ids = none
while iter < 3:
    failures = 4.2 검증 결과
    if failures 0:
        break
    if iter > 0 and {f.id for f in failures} == prev_failure_ids:
        # 진전 없음: 동일 항목이 연속 실패. 더 시도해도 같은 결과 가능성 높음.
        break
    fix_prompt = build_feedback(report, failures, file_contents)
    report = call_L1_fix(fix_prompt)
    prev_failure_ids = {f.id for f in failures}
    iter += 1
```

#### 4.4 피드백 프롬프트 형식

같은 file-pair-observer 에이전트에 다음 프롬프트로 재호출 (`run_in_background=false`, 단일 spec 단위 처리):

```
# File Observation Fix Request

## 이전 리포트
{이전 L1 리포트 본문 그대로 — 통과/실패 모두 포함}

## 검증 실패 항목 (수정 필요)

다음 항목들이 인용 검증에서 실패했다. 각 항목을 수정해라.

### [{item_id}] {failure_reason}
- 적힌 발췌: `{agent_excerpt}`
- {file}:{line_range} 의 실제 내용:
  \`\`\`
  {actual_file_lines_content}
  \`\`\`
- 실제 내용을 그대로 발췌로 사용하거나, 적합한 다른 라인으로 인용을 옮겨라.

{... 모든 실패 항목 반복 ...}

## 지시
- **실패 항목만 수정한 새 리포트**를 같은 출력 스키마로 반환해라.
- **통과한 항목은 절대 변경하지 마라.**
- 새 항목 추가 금지.
- 발췌는 반드시 원문 그대로 (paraphrasing/keyword 생략/prefix 추가 금지).
- 라인 범위는 발췌 위치와 일치해야 한다.
```

#### 4.5 종료 조건

루프 종료는 다음 중 하나:
1. 모든 항목 통과
2. 동일 ID 집합이 연속 실패 (진전 없음)
3. 3회 도달

루프 종료 시 마지막 리포트의 검증 통과 항목만 정제본으로 사용. 남은 실패 항목은 drop.

#### 4.6 리포트 단위 정책

- 정제본의 통과 항목 비율 ≥ 50%: 해당 리포트 사용 (Step 5 입력)
- < 50%: 해당 리포트 제외 + 사용자 confirm — 그 spec 을 빼고 진행할지, 중단할지

#### 4.7 Drop 로그 노출 (silent fail 금지)

검증 마지막에 사용자에게 표시:

```
🛡️ 인용 검증 결과
  - 통과 리포트: M / N
  - 피드백 루프 평균 반복: K.K회
  - 항목별 drop: J건 (이유별 breakdown)
```

모든 drop 은 사용자 가시.

### Step 5: gap-aggregator (L2) spawn

검증 통과한 L1 리포트들을 입력으로 gap-aggregator 에이전트를 `run_in_background=false` 로 spawn (모델: sonnet). 입력 프롬프트:

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

### Step 6: L2 인용 검증

L2 출력의 각 finding 에 대해 증거 인용을 검증한다.

#### 6.1 인용 파싱

`{report_filename}:{ID}` 형식 추출. 예: `database.md:S1`.

#### 6.2 검증

1. **report 존재**: 해당 filename 이 Step 4 통과 리포트 중에 있는가
2. **항목 ID 존재**: 그 리포트에서 해당 ID 항목이 실재하는가
3. **발췌 일치**: L2 가 인용한 발췌가 L1 의 해당 항목 발췌와 동일한가 (공백 정규화 후)

검증 실패 finding 은 drop. drop 로그에 기록.

### Step 7: 최종 리포트 출력

사용자에게 다음 형식으로 출력:

```markdown
# Spec Review Report

{L2 출력 본문 그대로 — Summary / Code↔Spec Gaps / Spec↔Spec Gaps / Notes}

---

## 검증 통계
- spec 파일: N
- L1 리포트 통과: M / N
- L1 항목 drop: K건
- L2 finding drop: J건
- 분석 모델: file-pair-observer (haiku) × N + gap-aggregator (sonnet) × 1
```

## 주의사항

- **MainAgent 는 spec/code 파일 직접 읽지 않음** (인용 검증 시 Read 도구만 사용). 분석은 모두 sub-agent.
- **인용 검증 silent fail 금지** — 모든 drop 은 사용자에게 노출.
- **frontmatter `related_paths` 권장** — 자율 보강은 fallback. 정확한 영역은 명시 필수.
- **재시도는 같은 spec 으로만** — drop 비율이 높다고 다른 spec 까지 영향 주지 않음.
- **출력은 마크다운만** — JSON 출력 금지.

## 에러 처리

**spec 파일 미존재**: Step 1 에서 즉시 에러.

**L1 모두 50% 이상 drop (3회 재시도 후)**: 사용자 confirm 으로 일부 spec 제외하고 진행 또는 중단.

**L2 finding 100% drop**: 매우 드문 경우. drop 로그 + 사용자 알림 후 빈 리포트 출력.

**gh / Glob 에러**: 표준 에러 메시지 출력 후 종료.

## Output Examples

### 성공

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

---

## 검증 통계
- spec 파일: 2
- L1 리포트 통과: 2 / 2
- L1 항목 drop: 0건
- L2 finding drop: 0건
```

### Drop 발생

```markdown
🛡️ 인용 검증 결과
  - 통과 리포트: 2 / 2
  - 항목별 drop: 3건
    - excerpt mismatch: 2건 (database.md:S5, database.md:C7)
    - line out of range: 1건 (proxy.md:S2)
  - 재시도: 0회

# Spec Review Report
...
```
