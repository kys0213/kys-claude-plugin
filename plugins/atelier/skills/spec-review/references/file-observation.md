# L1 관찰 + 인용 검증 + 피드백 루프

`file-pair-observer` (L1) 를 spawn 하고, 그 출력의 인용을 검증한 뒤, 실패 항목을 targeted 피드백으로 수정하는 프로토콜. `spec-review`·`gap-detect` 가 공통으로 사용한다.

## L1 spawn

각 spec 파일마다 1개의 `file-pair-observer` 에이전트를 `run_in_background=true` 로 동시 spawn 한다 (모델: haiku). 다중 spec 이면 병렬, 단일 spec 이면 1개. 입력 프롬프트:

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

## 인용 검증 절차

### 입력 파일 일괄 읽기

리포트가 인용하는 모든 file (spec 1개 + 인용된 code 파일 N개) 을 Read 도구로 한 번씩 읽어 메모리에 보관 (피드백 루프 내내 재사용, 중복 호출 회피).

### 검증 규칙

리포트의 각 항목 (`S{n}`, `C{n}`, `G{n}`, `M{n}`, `N{n}`) 에 대해:

1. **인용 파싱**: `` `path:line_start[-line_end]` `` 추출 + 발췌 (`"..."` 또는 `` `...` `` 안 텍스트)
2. **파일 검증**: 읽은 파일 중에 있는지. 없으면 fail ("file not read or not exist")
3. **라인 범위 검증**: line_start/line_end 가 파일 라인 수 범위 내인지. 초과 시 fail ("line out of range")
4. **발췌 검증**: line_start ~ line_end 범위 텍스트와 발췌를 공백 정규화 (연속 공백 → 단일 공백, 양끝 trim) 후
   - 발췌 끝에 `...` 있으면 prefix match
   - 없으면 substring 포함
   - 실패 시 fail ("excerpt mismatch")
5. **ID 일관성**: 같은 카테고리에서 ID 충돌 → fail ("duplicate id"). Mismatches/Gaps 의 참조 ID 가 리포트 내 실재하지 않으면 fail ("dangling reference")

검증 실패 항목은 임시 보관 (즉시 drop 하지 않고 피드백에 사용).

## 피드백 루프

검증 실패 항목이 있으면 아래 "피드백 프롬프트 형식"으로 같은 L1 에이전트를 재호출해 수정을 반복한다. 반복은 무한하지 않으며, 종료 조건은 뒤의 "종료 조건" 절을 따른다.

### 피드백 프롬프트 형식

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

### 종료 조건

루프 종료는 다음 중 하나:
1. 모든 항목 통과
2. 동일 ID 집합이 연속 실패 (진전 없음)
3. 3회 도달

루프 종료 시 마지막 리포트의 검증 통과 항목만 정제본으로 사용. 남은 실패 항목은 drop.

## 리포트 단위 정책

- 정제본의 통과 항목 비율 ≥ 50%: 해당 리포트 사용 (L2 입력)
- < 50%: 해당 리포트 제외 + 사용자 confirm — 그 spec 을 빼고 진행할지, 중단할지

## Drop 로그 노출 (silent fail 금지)

검증 마지막에 사용자에게 표시:

```
🛡️ 인용 검증 결과
  - 통과 리포트: M / N
  - 피드백 루프 평균 반복: K.K회
  - 항목별 drop: J건 (이유별 breakdown)
```

모든 drop 은 사용자 가시.
