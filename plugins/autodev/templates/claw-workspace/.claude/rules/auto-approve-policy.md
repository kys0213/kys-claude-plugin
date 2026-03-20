# 자동 승인 정책 (Auto-Approve Policy)

## 개요

분석(AnalyzeTask) 완료 후, 설정에 따라 사람의 리뷰 없이 자동 승인할 수 있다.
자동 승인 시 `approved-analysis` 라벨이 추가되어 즉시 구현 단계로 진행된다.

## 설정

| 키 | 타입 | 기본값 | 설명 |
|----|------|--------|------|
| `auto_approve` | bool | `false` | 자동 승인 기능 활성화 여부 |
| `auto_approve_threshold` | float | `0.8` | 자동 승인에 필요한 최소 confidence (0.0~1.0) |

## 자동 승인 조건

다음 조건을 **모두** 충족하면 자동 승인한다:

1. `auto_approve` 설정이 `true`
2. 분석 결과의 `confidence` >= `auto_approve_threshold`
3. verdict가 `implement` (구현 가능 판정)

## HITL 에스컬레이션 조건

다음 중 **하나라도** 해당하면 자동 승인하지 않고 사람의 리뷰를 대기한다:

| 조건 | 사유 |
|------|------|
| `auto_approve` 가 `false` | 기능 비활성화 |
| `confidence` < `auto_approve_threshold` | 분석 확신도 부족 |
| verdict가 `needs_clarification` | 추가 정보 필요 |
| verdict가 `wontfix` | 구현 불필요 판정 (스킵 처리됨) |
| 보안 관련 변경 | review-policy에 따라 항상 HITL 요청 |
| 아키텍처 변경 | 설계 판단이 필요한 변경 |

## Claw의 판단 흐름

```
분석 완료
  ├─ auto_approve == false → 사람 리뷰 대기 (analyzed 라벨만 추가)
  ├─ confidence < threshold → 사람 리뷰 대기
  └─ confidence >= threshold
       └─ approved-analysis 라벨 추가 → 구현 단계로 즉시 진행
```

## 주의사항

- 자동 승인은 분석 단계에만 적용된다. 리뷰(code review) 단계의 승인과는 별개다.
- 자동 승인된 이슈도 구현 후 리뷰 단계에서 별도의 코드 리뷰를 받는다.
- threshold를 낮추면 더 많은 이슈가 자동 승인되지만, 부적절한 이슈가 구현될 위험이 높아진다.
