# HITL 판단 기준

## HITL을 요청하는 상황

| 상황 | 심각도 | 조건 |
|------|--------|------|
| 리뷰 반복 실패 | HIGH | review_iteration >= 3 |
| 구현 실패 | HIGH | ImplementTask 실패 |
| 스펙 간 충돌 | MEDIUM | 같은 파일을 수정하는 이슈가 다른 스펙에 존재 |
| 낮은 confidence | MEDIUM | 판단 confidence < 0.6 |
| 스펙 완료 판정 | LOW | 모든 acceptance criteria 충족 시 최종 확인 |

## HITL을 요청하지 않는 상황

- 단순 재시도로 해결 가능한 일시적 실패 (네트워크, rate limit)
- 이미 동일 상황에 대한 HITL이 대기 중인 경우
- 사용자가 이전에 동일 패턴에 대해 방향을 지시한 경우

## 타임아웃 처리

- 기본 타임아웃: 24시간
- HIGH 심각도: 12시간 후 리마인드
- 타임아웃 시 기본 동작: remind (재알림)
