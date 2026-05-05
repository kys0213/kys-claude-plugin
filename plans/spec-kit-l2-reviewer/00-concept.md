# spec-kit L2 Reviewer — Semantic Validation Layer

## 배경

Phase 3 (#667) 이후 spec-kit 의 검증 구조는 다음과 같다:

```
L1 (haiku × N, 병렬)
  └─ orchestrator: 인용 검증 (file:line 실재 + 발췌 일치 + ID 일관성)
     └─ 실패 시 targeted feedback loop (최대 3회)

L2 (sonnet × 1)
  └─ orchestrator: L2 인용 검증 ({report}:{ID} 가 통과 L1 항목인지)
     └─ 실패 시 drop
```

`05-validation-and-followups.md` 의 5회 dogfood 로 인용-수준 환각은 구조적으로 차단됨이 검증됐다. 그러나 L2 의 **의미 수준 (semantic) 정확성** 은 여전히 검증되지 않는다:

- 인용은 정확하지만 **결론이 증거를 뒷받침하지 않음**
- 분류 오류 (`DEFINITION_CONFLICT` 인데 `INTERFACE_DRIFT` 로 분류)
- 심각도 오판 (실제 HIGH 인데 MEDIUM 으로 보고)
- 중복 finding (서로 다른 ID 가 동일 사안)
- L1 에 명백히 보이는 갭을 L2 가 누락

현재 orchestrator 검증은 **기계적 (mechanical) 검증** 만 수행한다. 의미 정확성은 사용자에게 떠넘긴다.

## 트리거

dogfood Test 2 (#690 직후) 의 4건 spec↔spec gap 발견은 L2 의 **장점** 을 보여주었지만, 동시에:

- 4건 중 1건은 사용자 검토 시 "이건 단순 정의 차이가 아니라 인터페이스 책임 분담 문제" 로 reclassify 가 필요했다.
- 다른 1건은 severity HIGH 였으나 spec 의 의도를 더 자세히 보면 MEDIUM 이 적절했다.

즉 **L2 가 "패턴 매칭은 잘 하지만 분류/심각도 판단은 흔들린다"** 는 정성적 관찰이 있었다. 이 흔들림은 인용 검증으로 잡히지 않는다.

## 핵심 아이디어 (사용자 제안)

> "리뷰어를 별도로 만들고.. major, minor 이슈를 판별하게하고.. major 이슈가 0 건이 될때까지 반복하는건 어때? 최대 횟수는 3회로 조정하고"

L2 finding 에 대해 **별도 reviewer 에이전트** 가 의미 수준 비평을 수행하고, **major 이슈가 0 이 될 때까지 (최대 3회)** L2 에 fix request 를 돌린다.

## 왜 별도 에이전트인가

L2 가 자기 자신을 review 하면 동일한 인지 편향을 반복한다. Reviewer 는:

- **다른 prompt 컨텍스트** 에서 출발 (회의적 비평자 역할)
- **L1 reports + L2 findings 만** 입력 (L2 의 sonnet 호출 컨텍스트 미공유)
- **수정 권한 없음** — 분석/지적만, 실제 수정은 L2 가 다음 호출에서 수행

이는 L1 에서 사용한 **per-item targeted feedback loop** 패턴을 L2 층에 동일하게 적용하는 것이다. L1 은 인용을 정정, L2 는 해석을 정정.

## 새 흐름

```
L1 reports (검증 통과)
  ↓
L2 (sonnet × 1) → findings
  ↓ orchestrator: 인용 검증 (현재와 동일)
  ↓
L3 reviewer (sonnet × 1) → major/minor 이슈 목록
  ├─ major == 0 → finalize
  ├─ iter < 3 && 진전 있음 → L2 에 fix request → 재호출 → 다시 review
  └─ 그 외 → 잔여 major 이슈를 사용자 가시 drop log 로 노출
```

## 환각/오류 면적 비교

| 단계 | 현재 | 새 구조 |
|------|------|---------|
| 인용 정확성 | mechanical 검증 + L1 feedback loop | (그대로) |
| 분류 정확성 | 검증 없음 | reviewer 의 major 사유로 잡힘 |
| 심각도 정확성 | 검증 없음 | reviewer 의 major 사유로 잡힘 |
| false positive | 검증 없음 | reviewer 가 증거-결론 매칭 검사 |
| false negative (누락) | 검증 없음 | reviewer 가 L1 evidence 대비 확인 |
| 중복 finding | 검증 없음 | reviewer 의 dedupe 사유 |

## 비용 영향

- 추가 호출: reviewer (sonnet × 1) + L2 재호출 (최악 sonnet × 3)
- 평균 시나리오: reviewer 1회, L2 재호출 0~1회 예상
- 절대 비용 증가는 있으나, **사용자가 결과를 다시 검토하는 시간** 을 줄이는 절감과 교환

## 다음 단계

- `01-use-cases.md`: major/minor 가 잡히는 시나리오
- `02-architecture.md`: reviewer 에이전트 정의 + L2 ↔ reviewer 루프 통합
- `03-detailed-spec.md`: major/minor 분류 기준, 출력 스키마, prompt
- `04-test-scenarios.md`: 의미 검증 회귀 케이스, 게이트
