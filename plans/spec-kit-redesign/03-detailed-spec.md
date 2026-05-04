# Detailed Spec — 출력 스키마, 인용 형식, 검증 규칙

## 1. L1 출력 스키마

### 1.1 전체 구조

```markdown
# Per-File Report: {spec_file_path}

## Metadata
- spec_file: {spec_file_path}
- spec_lines: {total}
- code_paths_examined: [{path1}, {path2}, ...]
- frontmatter_related_paths: [{path1}, ...]    # spec frontmatter 에 명시된 것
- autonomous_paths: [{path3}, ...]              # 자율 탐색으로 추가된 것
- generated_at: {ISO 8601}

## Spec Claims
- [S{n}] `{file}:{line_start}-{line_end}` — "{발췌}"
  ...

## Code Observations
- [C{n}] `{file}:{line_start}-{line_end}` — `{code 발췌 또는 시그니처}`
  ...

## Mismatches
- [{S id}] vs [{C id}] — {일치 / 차이 한 줄}
  ...

## Gaps
- [G{n}] {SPEC_ONLY | CODE_ONLY | PARTIAL} — {참조 ID 들} — {한 줄 설명}
  ...

## Notes
- [N{n}] `{file}:{line_start}-{line_end}` — "{발췌}" — {모호 사유 한 줄}
  ...
```

### 1.2 항목 ID 규칙

- `S` = Spec Claim, `C` = Code Observation, `G` = Gap, `N` = Note, `M` = Mismatch
- 각 카테고리별 1부터 순차. 한 리포트 내 ID 충돌 금지
- L2 가 인용할 때 `{report_filename}:{ID}` 형식 사용 (예: `database.md:S3`)

### 1.3 인용 형식 (file:line)

```
`{relative_path_from_repo_root}:{line_start}` — 단일 라인
`{relative_path_from_repo_root}:{line_start}-{line_end}` — 라인 범위
```

#### 발췌 (excerpt)

- spec 인용: 큰따옴표로 감싼 원문 그대로
- code 인용: 백틱 코드 스팬으로 감싼 원문 그대로
- 길이 제한: 발췌는 200자 이내. 초과 시 끝에 `...` (가공 금지, 단순 절단)

#### 예시

```markdown
- [S1] `spec/database.md:120` — "content_type 컬럼은 VARCHAR(255)"
- [C1] `migrations/001.sql:34` — `content_type VARCHAR(255) NOT NULL DEFAULT ''`
- [S5] `spec/auth.md:200-215` — "Refresh token 회전: 매 사용 시 새 토큰 발급, 이전 토큰 무효화..."
```

### 1.4 Gap 항목 분류 enum

| 값 | 의미 |
|----|------|
| `SPEC_ONLY` | spec 에 있지만 code 에 없음 |
| `CODE_ONLY` | code 에 있지만 spec 에 없음 |
| `PARTIAL` | 양쪽에 있으나 일부 누락 |

L2 는 이 enum 으로 자동 분류 가능.

### 1.5 빈 섹션 처리

- 항목이 0개여도 섹션 헤더는 유지 (스키마 일관성)
- 본문은 `(없음)` 표시

```markdown
## Mismatches
(없음)
```

## 2. L2 출력 스키마

### 2.1 전체 구조

```markdown
# Spec Review Report

## Summary
- spec_files_reviewed: {N}
- l1_reports_received: {N} (drop: {M})
- code_spec_gaps: {count by severity}
- spec_spec_gaps: {count by severity}
- generated_at: {ISO 8601}

## Code ↔ Spec Gaps

### [{HIGH|MEDIUM|LOW}] {제목}
- 증거:
  - {report_filename}:{S/C/G id} — "{원 인용 그대로}"
  - {report_filename}:{S/C/G id} — "{원 인용 그대로}"
- 분류: {SPEC_ONLY | CODE_ONLY | PARTIAL | DIVERGENT}
- 권장: {1-2 문장}

## Spec ↔ Spec Gaps

### [{severity}] {제목}
- 증거:
  - {report_a}:{id} — "{인용}"
  - {report_b}:{id} — "{인용}"
- 분류: {DEFINITION_CONFLICT | INTERFACE_DRIFT | TERM_AMBIGUITY | ...}
- 권장: {1-2 문장}

## Notes (모호함)

### [{severity}] {제목}
- 증거: {report}:{N id} — "{인용}"
- 권장: {1-2 문장}
```

### 2.2 Severity 기준

| 등급 | 기준 |
|------|------|
| HIGH | 사용자 영향 직접 / 보안 / 데이터 무결성 / spec 전제와 code 가 정반대 |
| MEDIUM | 기능 영향 있으나 우회 가능 / 부분 구현 / 모호함이 다중 해석 야기 |
| LOW | 문서화 누락 / 미세 표기 차이 / 스타일 |

### 2.3 분류 (Code↔Spec) enum

| 값 | 의미 |
|----|------|
| `SPEC_ONLY` | spec 약속, code 없음 |
| `CODE_ONLY` | code 동작, spec 미언급 |
| `PARTIAL` | 부분 구현 |
| `DIVERGENT` | 양쪽 다 있으나 동작이 다름 |

### 2.4 분류 (Spec↔Spec) enum

| 값 | 의미 |
|----|------|
| `DEFINITION_CONFLICT` | 같은 용어/개념을 다르게 정의 |
| `INTERFACE_DRIFT` | 같은 인터페이스를 다르게 명세 |
| `TERM_AMBIGUITY` | 한 spec 이 다른 spec 의 용어를 가정만 함 |
| `REQUIREMENT_OVERLAP` | 요구사항이 중복/모순 |

### 2.5 인용 형식 (L1 참조)

```
{report_filename}:{ID} — "{원 인용 그대로}"
```

L2 는 새 발췌를 만들지 않음. L1 항목의 발췌를 그대로 가져옴.

## 3. 인용 검증 규칙 (오케스트레이터)

### 3.1 L1 인용 검증

각 L1 출력에 대해:

1. **파일 존재**: `{file}` 가 실재하는가
2. **라인 범위 유효**: `{line_start}` ≤ `{line_end}` ≤ 파일 총 라인 수
3. **substring 매치**: 발췌(따옴표/백틱 안 텍스트)가 `{file}:{line_start}-{line_end}` 범위 텍스트에 substring 으로 존재하는가 (공백 정규화 후)

검증 알고리즘 (의사 코드):

```python
def verify_l1_citation(citation):
    file_path, line_range, excerpt = parse(citation)
    if not exists(file_path):
        return DROP, "file not found"
    file_lines = read_lines(file_path)
    line_start, line_end = line_range
    if line_end > len(file_lines):
        return DROP, "line out of range"
    text = "\n".join(file_lines[line_start-1 : line_end])
    if normalize_ws(excerpt.rstrip("...")) not in normalize_ws(text):
        return DROP, "excerpt mismatch"
    return PASS
```

검증 실패 항목은 L1 리포트에서 제거. 리포트 메타에 drop 카운트 기록.

### 3.2 L1 자체 출력 검증

- 모든 항목이 ID 부여되었는가 (S{n}, C{n}, G{n}, N{n})
- ID 충돌 없는가
- Mismatches 의 참조 ID 가 같은 리포트 내 실재하는가
- Gaps 의 참조 ID 들이 같은 리포트 내 실재하는가

위반 항목은 drop.

### 3.3 L2 인용 검증

각 L2 finding 의 증거에 대해:

1. **리포트 존재**: `{report_filename}` 이 검증 통과한 L1 리포트 중 있는가
2. **항목 존재**: `{ID}` 가 그 리포트에 실재하는가
3. **발췌 일치**: L2 가 인용한 발췌가 L1 의 해당 항목 발췌와 일치하는가 (공백 정규화 후 동일)

검증 실패 finding 은 drop. 사용자에게 경고.

### 3.4 검증 실패 처리 정책

- **L1 항목 단위 drop**: 환각 의심 항목만 제거. 해당 리포트의 다른 항목은 살림
- **전체 리포트 drop**: 한 리포트의 drop 비율이 50% 초과 시 전체 drop + 재실행 (Haiku 재시도)
- **재시도 한계**: 동일 spec 파일 3회 연속 실패 시 사용자 confirm 요청
- **드롭 로그**: 모든 drop 을 사용자에게 노출 (silent fail 금지)

## 4. spec 파일 frontmatter 컨벤션

### 4.1 권장 프론트매터

```yaml
---
title: {스펙 제목}
related_paths:
  - {레포 루트 기준 경로 1}
  - {레포 루트 기준 경로 2}
spec_kit:
  level: {detailed | overview}    # 선택
  category: {database | api | ...}   # 선택
---
```

### 4.2 `related_paths` 의미

- 디렉토리: 그 디렉토리의 모든 source 파일 (텍스트 파일, 빌드 산출물 제외)
- 파일: 정확히 그 파일
- 와일드카드 미허용 (Glob 은 자율 탐색이 담당)

### 4.3 누락 처리

frontmatter 가 없거나 `related_paths` 가 비면 L1 에이전트가 자율 탐색. 자율 탐색 결과는 출력 메타에 명시되어 사후 검증 가능.

## 5. 출력 사이즈 제어

### 5.1 L1 사이즈 가이드

- 한 리포트 당 항목 100개 이내 권장. 초과 시 spec 을 분할하라는 경고
- 발췌 200자 제한
- 빈 섹션은 `(없음)` 한 줄

### 5.2 L2 사이즈 가이드

- finding 50개 이내. 초과 시 severity LOW 부터 잘라냄 (제거된 것은 메타에 기록)
- 한 finding 의 증거는 최대 5개 인용. 초과 시 대표 5개만

## 6. 결정 사항 정리

| 항목 | 결정 |
|------|------|
| L1 모델 | haiku |
| L2 모델 | sonnet |
| L1 tools | Read, Glob, Grep |
| L2 tools | (none) |
| 인용 형식 | `file:line` + excerpt |
| 입력 결정 | frontmatter `related_paths` + 자율 보강 |
| 검증 위치 | 오케스트레이터 (커맨드 레벨) |
| Drop 정책 | 항목 단위 drop, 50% 초과 시 전체 재실행 |

다음 문서(`04-test-scenarios.md`)에서 회귀 / 정확도 / 성능 테스트 계획.
