# Architecture — L1/L2 인터페이스, 호출 흐름

## 전체 흐름

```
/spec-kit:spec-review (커맨드)
  │
  ├─ Step 1: spec 파일 목록 수집 + frontmatter 파싱 (related_paths)
  │
  ├─ Step 2: L1 에이전트 N개 병렬 spawn (Haiku)
  │            ├─ file-pair-observer(spec_file=A.md, code_paths=...)
  │            ├─ file-pair-observer(spec_file=B.md, code_paths=...)
  │            └─ ...
  │
  ├─ Step 3: 각 L1 출력에 대한 인용 검증 (오케스트레이터)
  │            ├─ file:line 범위가 실재하는가
  │            ├─ 인용 excerpt 가 그 라인의 substring 인가
  │            └─ 검증 실패 항목 drop + 경고
  │
  ├─ Step 4: L2 에이전트 spawn (Sonnet) — L1 검증 통과 리포트만 입력
  │            └─ gap-analyzer
  │
  ├─ Step 5: L2 출력에 대한 인용 검증 (오케스트레이터)
  │            └─ L2 가 인용한 L1 항목 ID 가 실재하는가
  │
  └─ Step 6: 최종 리포트 출력
```

## L1: file-pair-observer

### Frontmatter

```yaml
---
description: (내부용) /spec-kit:spec-review 가 호출하는 per-file 관찰 에이전트. spec 파일 1개와 관련 code 영역을 읽고 사실만 나열.
model: haiku
tools: ["Read", "Glob", "Grep"]
---
```

`tools` 에 Read/Glob/Grep 부여. spec 파일과 code 를 직접 읽어야 인용 가능.

### 입력 형식

오케스트레이터로부터 다음 형식의 프롬프트:

```
# File Observation Request

## Spec 파일
- 경로: {spec_file_path}

## 관련 code 경로 (Hint)
{related_paths_from_frontmatter, 없으면 비움}

## 자율 탐색 허용 범위
- 위 경로 + 그 경로에서 import/require 된 인접 파일

## 출력 (아래 스키마 엄수)
[출력 스키마 보기 → 03-detailed-spec.md]
```

### 출력 형식 요약 (상세는 03)

```markdown
# Per-File Report: {spec_file_path}

## Spec Claims
- [S{n}] `{file}:{line_start}-{line_end}` — "{원문 인용, 50자 이내 요약 가능}"

## Code Observations
- [C{n}] `{file}:{line}` — `{code 단편 또는 시그니처}`

## Mismatches
- [S{n}] vs [C{n}] — {일치 / 차이 한 줄}

## Gaps
- [G{n}] {Spec 만 / Code 만 / 부분 구현} — {참조 ID + 한 줄 설명}

## Notes (선택)
- [N{n}] `{file}:{line}` — "{모호한 표현 인용}" (모호 사유 한 줄)
```

### 제약

- 종합/추론 금지. **나열만**
- 모든 항목에 file:line 인용 필수
- 인용은 원문을 그대로 (자유 의역 금지)
- 원문 발췌가 너무 길면 "..." 으로 생략 표시 (가공 금지)

## L2: gap-analyzer

### Frontmatter

```yaml
---
description: (내부용) /spec-kit:spec-review 의 종합 분석 에이전트. L1 리포트들을 받아 code↔spec gap 과 spec↔spec gap 을 식별.
model: sonnet
tools: []
---
```

`tools: []` — L2 는 raw 파일 안 봄. L1 리포트만 처리.

### 입력 형식

```
# Gap Analysis Request

## L1 Reports (검증 통과분)

### Report 1
{database.md report 본문}

### Report 2
{proxy.md report 본문}
...

## 출력 (아래 스키마 엄수)
[출력 스키마 보기 → 03-detailed-spec.md]
```

### 출력 형식 요약

```markdown
# Spec Review Report

## Code ↔ Spec Gaps

### [{severity}] {제목}
- 증거 (L1 인용):
  - {report_name} {claim_id}
  - ...
- {판단/권장}

## Spec ↔ Spec Gaps

### [{severity}] {제목}
- 증거 (L1 인용):
  - {report_a} {S/C/G id}
  - {report_b} {S/C/G id}
- {판단/권장}

## Notes (모호한 spec 항목)

### [{severity}] {제목}
- 증거: {report} {N id}
- {권장}
```

### 제약

- L1 에 없는 사실 추가 금지
- 모든 결론에 L1 인용 (report_name + claim_id)
- 우선순위/판단은 자유롭게 (L2 의 본업) — 단 사실은 L1 인용에서만

## 입력 결정 — `related_paths` 처리

L1 의 "관련 code 영역" 결정 방식 (옵션 C 채택):

### 1차: spec 파일 frontmatter

```yaml
---
title: Database Schema
related_paths:
  - migrations/
  - internal/dao/
---
```

명시되어 있으면 그대로 사용.

### 2차: 자율 보강

frontmatter 가 비어 있거나 불충분할 때 L1 에이전트가 Glob/Grep 으로 보강:

- spec 파일 본문에 등장하는 식별자/경로 패턴을 grep
- 프로젝트 루트의 디렉토리 구조 (Glob `*/`) 와 spec 헤딩을 매칭
- 발견된 경로를 입력에 추가하고 출력 메타에 기록

### 3차: 사용자 확인 (선택)

자율 보강 결과가 너무 많거나 적으면 오케스트레이터가 사용자에게 confirm 요청.

## 호출자 변경 (commands)

### `/spec-kit:spec-review` (개정)

기존: `spec-parser` → 6개 checker 호출 → 통합

신: 위 "전체 흐름" 그대로

### `/spec-kit:spec-quality` (있다면)

L2 만 단독 호출하는 모드 추가 가능. 기존 `spec-quality-checker` 단독 호출 use case 대체.

### 기타 의존자

`gap-analyzer`, `reverse-gap-analyzer`, `structure-mapper`, `cross-reference-checker`, `spec-quality-checker`, `spec-parser` 를 직접 호출하는 곳:

```bash
grep -rn "spec-parser\|cross-reference-checker\|spec-quality-checker\|gap-analyzer\|reverse-gap-analyzer\|structure-mapper" plugins/
```

마이그레이션 단계에서 호출 지점을 모두 새 흐름으로 전환.

## 마이그레이션 단계

플러그인 배포가 자동 버전 범프를 처리하므로 내부 에이전트/커맨드의 점진 cohabitation 은 불필요하다. v2 suffix 와 deprecate-then-rename 단계를 생략하고 **3-Phase 아토믹 마이그레이션**으로 진행한다.

### Phase 1 — 설계 (이 PR)

설계 합의. 코드 변경 없음.

### Phase 2 — 신규 에이전트 추가 (additive)

- `plugins/spec-kit/agents/file-pair-observer.md` (L1) 추가
- `plugins/spec-kit/agents/gap-aggregator.md` (L2) 추가 (역할 기반 이름 — legacy `gap-analyzer.md` 와 자연스레 공존)
- 단위 검증: 단일 spec 파일에 대해 L1 호출 → 출력 스키마 검증, 수기 작성 L1 리포트로 L2 호출 → 출력 검증
- legacy 에이전트는 그대로 유지 (커맨드들이 여전히 호출)

### Phase 3 — 아토믹 마이그레이션

한 PR 에서 다음을 동시에 수행. 부분 적용 시 깨지므로 분할 불가:

- `plugins/spec-kit/commands/spec-review.md` 새 흐름으로 재작성 (file-pair-observer × N → 인용 검증 → gap-aggregator)
- `plugins/spec-kit/commands/gap-detect.md` 도 동일 흐름으로 재작성 (또는 spec-review 로 흡수)
- 인용 검증 로직 추가 (L1 인용이 실파일 substring 매치, L2 인용이 검증 통과 L1 항목 ID 실재)
- legacy 에이전트 6개 삭제:
  - `spec-parser.md`
  - `cross-reference-checker.md`
  - `spec-quality-checker.md`
  - `gap-analyzer.md` (gap-aggregator 가 대체)
  - `reverse-gap-analyzer.md`
  - `structure-mapper.md`
- `plugins/github-autopilot/agents/gap-detector.md` 의 doc 참조 (`spec-parser → structure-mapper → gap-analyzer`) 갱신
- e2e 회귀: 동일 spec-set 으로 v1 결과와 비교, `04-test-scenarios.md` 의 통과 기준 충족 확인

### Phase 후속 — 사용자 노출 변경

CI 의 자동 버전 범프가 `feat:` prefix 로 minor 범프를 트리거. 릴리스 노트에 "spec-kit 가 2-layer 구조로 재작성됨, 사용자 노출 인터페이스(`/spec-kit:spec-review`, `/spec-kit:gap-detect`)는 동일하나 내부 동작/출력 형식이 변경됨" 명시.

## 위험 평가

| 위험 | 영향 | 완화 |
|------|------|------|
| L1 토큰 비용 증가 | 중 | 병렬 호출 + Haiku 단가. 실측 후 결정 |
| `related_paths` frontmatter 미설정 spec 다수 | 중 | 자율 보강 fallback. 점진 추가 |
| 기존 호출자 마이그레이션 누락 | 고 | grep 기반 의존성 매트릭스 작성 후 체크 |
| L2 가 L1 사실을 왜곡 | 중 | 인용 검증으로 차단 |
| Haiku 가 인용 형식 어김 | 중 | few-shot example + 사후 검증 + 재시도 |

다음 문서(`03-detailed-spec.md`)에서 출력 스키마 / 인용 형식 / 검증 알고리즘을 정확히 정의.
