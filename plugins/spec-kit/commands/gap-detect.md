---
description: 스펙 문서와 구현 코드 간 갭(미구현, 부분 구현, 발산, spec 부재 코드)을 file:line 인용으로 검출합니다
argument-hint: "<스펙파일> [관련코드경로 ...]"
allowed-tools: ["Task", "Glob", "Read", "Grep", "AskUserQuestion"]
---

# Gap Detect (/gap-detect)

스펙 ↔ 코드 갭을 검출합니다. `/spec-review` 와 동일한 file-pair-observer (L1) + gap-aggregator (L2) 백본을 사용하지만, 출력에서 **Code ↔ Spec Gaps** 섹션을 우선 표시하고 다른 관점은 보조로 노출합니다.

> **새 흐름**: file-pair-observer 가 spec 과 code 양방향 관찰을 한꺼번에 수행하므로 별도 `--reverse` 옵션은 더 이상 필요하지 않습니다 (`SPEC_ONLY` / `CODE_ONLY` / `PARTIAL` / `DIVERGENT` 분류로 자동 표현됨).

## 사용법

```bash
/gap-detect "docs/auth-spec.md"
/gap-detect "docs/auth-spec.md" "src/auth"
/gap-detect "plans/design.md" "src/**/*.rs" "internal/auth/"
```

| 인자 | 필수 | 설명 |
|------|------|------|
| 스펙파일 | Yes | 분석 대상 스펙 마크다운 경로 |
| 관련코드경로 | No | 구현 코드 경로/패턴 (미지정 시 spec frontmatter `related_paths` 또는 자율 보강) |

## 작업 프로세스

### Step 1: 입력 파싱

- **스펙 파일 경로** 추출 (필수). Glob 으로 존재 확인. 없으면 즉시 에러.
- **관련 code 경로** 추출 (선택). 명시되면 spec frontmatter 보다 우선 사용.

### Step 2: 관련 코드 경로 결정

- 사용자가 명시한 경로 → 그대로 사용
- 없으면 spec 파일 frontmatter `related_paths` 사용
- 둘 다 없으면 spec 본문에서 식별자/경로 패턴을 Grep 으로 추출 → 후보 추정 → AskUserQuestion 으로 확인

### Step 3: file-pair-observer (L1) 호출

`run_in_background=false` (단일 spec). 입력 프롬프트:

```
# File Observation Request

## Spec 파일
- 경로: {spec_file_path}

## 관련 code 경로
{resolved_paths}

## 자율 탐색 허용 범위
- 위 경로 + 그 경로에서 import 된 인접 파일

## 출력
file-pair-observer 의 출력 스키마를 엄수하여 per-file 리포트 반환.
```

### Step 4: L1 인용 검증 + 피드백 루프

`/spec-review` Step 4 와 동일한 절차:

- 인용된 file:line 범위 실재 확인
- 발췌가 substring/prefix 매치 (공백 정규화)
- ID 일관성 / dangling reference 검증
- 실패 항목이 있으면 **피드백 루프**: 실패 ID + 적힌 발췌 + 실제 파일 내용을 묶어 L1 에 fix request 전송 → 재검증 → 진전 있으면 반복 (최대 3회), 진전 없으면 종료
- 정제본 통과 비율 < 50% 시 사용자 confirm (제외 또는 중단)
- 모든 drop 을 사용자에게 노출

상세 알고리즘과 피드백 프롬프트 형식은 `/spec-review` Step 4.3 ~ 4.7 참조.

### Step 5: gap-aggregator (L2) 호출

검증 통과 L1 리포트를 입력으로 spawn (`run_in_background=false`, sonnet). 단일 spec 인 경우 spec↔spec gaps 섹션은 비게 되고, code↔spec gaps 섹션이 핵심 출력이 된다.

```
# Gap Analysis Request

## L1 Reports (검증 통과분)

### Report: {spec_filename}
{L1 리포트 본문, drop 정제본}

## 출력
gap-aggregator 의 출력 스키마를 엄수.
```

### Step 6: L2 인용 검증

`/spec-review` Step 6 과 동일.

### Step 7: 최종 리포트 출력

Code ↔ Spec Gaps 를 우선 표시. 부속 섹션(Spec↔Spec gaps, Notes) 은 발견 시에만 노출.

```markdown
# Gap Detection Report

## Summary
- spec_file: {경로}
- code_paths_examined: {목록}
- code_spec_gaps: HIGH=N, MEDIUM=N, LOW=N
- generated_at: ...

## Code ↔ Spec Gaps
{L2 의 Code↔Spec Gaps 섹션 그대로}

{spec↔spec 또는 notes 가 있을 경우에만:}

## 부수 발견

### Spec ↔ Spec (다중 spec 비교 시 발견된 일관성 이슈)
{L2 의 Spec↔Spec Gaps 섹션 — 단일 spec 분석에서는 보통 비어 있음}

### Notes (모호한 spec 표현)
{L2 의 Notes 섹션}

---

## 검증 통계
- L1 리포트 통과: M / N
- L1 항목 drop: K건
- L2 finding drop: J건
- 분석 모델: file-pair-observer (haiku) + gap-aggregator (sonnet)
```

## 주의사항

- **`/spec-review` 와 백본 동일**, 출력 emphasis 만 다름. 다중 spec 분석은 `/spec-review` 사용 권장.
- **`--reverse` 옵션 제거**: 새 백본이 spec→code, code→spec 방향을 모두 자연스럽게 표현 (`SPEC_ONLY` / `CODE_ONLY` / `PARTIAL` / `DIVERGENT`).
- **frontmatter `related_paths` 권장** — 정확한 코드 영역 지정으로 자율 보강 노이즈를 줄임.
- **인용 검증 silent fail 금지** — 모든 drop 은 사용자에게 노출.

## 에러 처리

**spec 파일 미존재**: Step 1 에서 즉시 에러.

**code 경로 미해결 (frontmatter 없음 + 자율 보강 실패)**: 사용자에게 명시적 경로 입력 요청.

**L1 50% drop (3회 재시도 후)**: 사용자 confirm — 진행 또는 중단.

**L2 finding 0개**: "갭 없음" 메시지와 함께 정상 종료.

## Output Examples

### 갭 발견

```markdown
# Gap Detection Report

## Summary
- spec_file: docs/auth-spec.md
- code_paths_examined: [internal/auth/]
- code_spec_gaps: HIGH=1, MEDIUM=1, LOW=0
- generated_at: 2026-05-05T...

## Code ↔ Spec Gaps

### [HIGH] Refresh token 회전 미구현
- 증거:
  - auth-spec.md:G1 — SPEC_ONLY — "Refresh token 회전: 매 사용 시 새 토큰 발급" (auth-spec.md:200-215) → 해당 영역 code 미발견
- 분류: SPEC_ONLY
- 권장: refresh 핸들러에 회전 로직 추가 또는 spec 에서 제거

### [MEDIUM] API key 인증 spec 누락
- 증거:
  - auth-spec.md:G3 — CODE_ONLY — `internal/auth/api_key.go:20` 존재, spec 미언급
- 분류: CODE_ONLY
- 권장: spec 에 추가 또는 code 제거

---

## 검증 통계
- L1 리포트 통과: 1 / 1
- L1 항목 drop: 0건
```

### 갭 없음

```markdown
# Gap Detection Report

## Summary
- spec_file: docs/auth-spec.md
- code_spec_gaps: HIGH=0, MEDIUM=0, LOW=0

✅ 검출된 갭 없음. spec 과 code 가 일치.
```
