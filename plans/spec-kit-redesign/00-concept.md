# spec-kit 재설계 — 2-layer Grounded Extraction

## 배경

현재 spec-kit 은 다음 6개 에이전트로 구성되어 있다:

| 에이전트 | 역할 |
|----------|------|
| `spec-parser` | spec 마크다운 → 구조화 JSON |
| `cross-reference-checker` | spec ↔ spec 일관성 (D1-D5) |
| `spec-quality-checker` | spec 자체 품질 (A/B/C) |
| `gap-analyzer` | spec ↔ code gap |
| `reverse-gap-analyzer` | code ↔ spec reverse gap |
| `structure-mapper` | code 구조 매핑 |

이 구조의 핵심 문제는 **spec-parser 가 N개 파일을 종합 요약** 한다는 것이다. 종합 = 해석 = 환각 진입점.

## 트리거 사건 (#639)

`/spec-kit:spec-review` 실행 중 `cross-reference-checker` 가:

1. 실제 Read 도구를 호출하지 않고 텍스트로 `<tool_call>` / `<tool_response>` 블록 생성
2. 가짜 본문 생성 (`content_type ENUM('json','multipart','form')` — 실제는 `VARCHAR(255)`)
3. 이를 다른 spec 의 실제 내용과 대조해 "불일치 6건" 보고

근본 원인은 **에이전트가 종합 요약된 입력만 보고 결론을 내려야 하는 구조** 자체에 있다. 입력에 없는 내용을 만들어 내도 검증할 메커니즘이 없다.

## 임시 처치 (#650)

prompt-level 강제 조항을 추가했다:

- "전달받은 구조화 입력에만 근거하여 검증한다"
- "`<tool_call>` / `<tool_response>` 같은 가짜 블록을 출력하지 않는다"

부정형 지시("거짓말 하지 마라")는 LLM 이 어기기 쉽다. 같은 환각이 다른 형태로 재발할 수 있다.

## 새 설계 방향

### 핵심 원칙

1. **종합 요약 단계 제거** — spec-parser 처럼 "여러 파일을 한번에 요약하는" 에이전트 없음
2. **하나의 에이전트는 하나의 파일만** — 발언권의 범위 = 자기 파일
3. **인용 by construction** — 각 에이전트는 직접 Read 한 파일의 라인을 인용. 인용 없으면 발언 불가
4. **Code 를 외부 ground truth 로 활용** — spec 만 보지 말고 code 와 대조. "spec 의 모호함" 이 "code 와의 불일치" 라는 객관적 증거로 드러남

### 2-Layer 구조

```
[Layer 1] file-pair-observer  (Haiku × N, 병렬)
  Input  : spec 파일 1개 + 관련 code 영역
  Output : 사실 나열만 (인용 필수)
           - Spec Claims        : "이 라인에 이렇게 쓰여 있다"
           - Code Observations  : "이 라인에 이렇게 구현돼 있다"
           - Mismatches         : 둘이 다른 곳 (판단 없이 병치)
           - Gaps               : 한쪽에만 있는 것
  특성   : 종합/추론 금지. 적시만. 인용은 file:line 범위 + 발췌

         ↓ N개 markdown 리포트

[Layer 2] gap-analyzer  (Sonnet/Opus, 1개)
  Input  : L1 리포트 전체 (raw 파일 안 봄)
  Output :
    A. code ↔ spec gaps  : 구현 불일치/누락
    B. spec ↔ spec gaps  : 여러 L1 리포트가 같은 code 를 다른 spec 주장으로
                            가리킬 때 자동 발견
  특성   : 새 사실 만들지 않음. L1 인용을 그대로 통과시키며 패턴 매칭 + 우선순위
```

### 환각 면적 비교

| 단계 | 기존 구조 | 새 구조 |
|------|-----------|---------|
| 입력 종합 | spec-parser 가 N 파일 종합 → JSON (환각 진입점) | 없음 |
| Per-file 처리 | 없음 | L1 (Haiku, 자기 파일만, 인용 필수) |
| Cross-file 검증 | 6개 checker 가 JSON 종합 결론 (환각 surface 큼) | L2 (Sonnet, L1 인용만 처리) |
| 인용 가능성 | 약함 (JSON 경로 인용은 사용자 친화 X) | 강함 (file:line + excerpt, 클릭 가능) |
| 검증 메커니즘 | 없음 | L1: 인용 라인이 실파일에 존재하는지 확인. L2: 인용된 L1 리포트 항목이 실재하는지 확인 |

### 모델 선택 근거

- **L1 = Haiku**: 사실 적시 작업. 종합 욕심을 부리는 강한 모델은 오히려 위험. Haiku 의 "literal" 성향이 적합. N개 병렬 → 비용도 낮음.
- **L2 = Sonnet/Opus**: 패턴 매칭, 우선순위, 종합 판단. 새 사실은 만들지 않지만 cross-report 추론 필요. 한 번만 호출.

### 기존 6개 에이전트 흡수 매핑

| 기존 | 새 구조 | 비고 |
|------|---------|------|
| `spec-parser` | **삭제** | 종합 요약 단계가 없어짐 |
| `cross-reference-checker` | **L2 흡수** | spec ↔ spec gap 이 L2 의 자연스런 결과 |
| `spec-quality-checker` | **L2 흡수** | spec 모호함이 code 와의 불일치로 드러남 |
| `gap-analyzer` | **L1 + L2 분산** | per-file 관찰은 L1, 종합 판단은 L2 |
| `reverse-gap-analyzer` | **L1 + L2 분산** | 양방향 관찰을 L1 이 한꺼번에 |
| `structure-mapper` | **L1 보조** | 각 L1 에이전트가 자기 영역 매핑 → L2 가 합침 |

6 → 2 단순화.

## #650 의 처리

이 재설계가 합의되면 #650 은 **임시방편으로 머지** 한다:

- 새 구조 구현/마이그레이션은 다중 단계 작업이라 시간이 걸림
- 그 사이에도 #639 같은 환각이 재발할 수 있으므로 약한 가드라도 있는 게 낫다
- 새 구조가 안정화되면 cross-reference-checker / spec-quality-checker 자체가 사라지므로 자연스레 함께 제거됨

## 다음 단계

- `01-use-cases.md`: D1-D5, A/B/C, gap, reverse-gap, #639 시나리오가 새 구조에서 어떻게 풀리는지
- `02-architecture.md`: L1/L2 인터페이스, 입력 결정, 호출자 변경
- `03-detailed-spec.md`: 출력 스키마, 인용 형식, 검증 규칙
- `04-test-scenarios.md`: 환각 회귀, 마이그레이션 호환성, 정확도 측정
