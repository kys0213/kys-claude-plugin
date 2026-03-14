# Flow 9: 실패 복구

### 시나리오

자동 처리 중 실패가 발생하여 사용자 개입이 필요하다.

### 실패 유형별 대응

```
구현 실패 (ImplementTask 에러):
  → autodev:impl-failed 라벨 + worktree 보존
  → HITL 알림: "구현에 실패했습니다. worktree를 확인해주세요."
  → 사용자 선택: 수동 수정 후 PR 생성 / 이슈 재시도 / skip

리뷰 반복 실패 (review_iteration ≥ max):
  → autodev:skip 라벨
  → HITL 알림: "리뷰 사이클이 {max}회 반복되었습니다."
  → 사용자 선택: 직접 리뷰 / 스펙 수정 / skip 유지

분석 실패 (AnalyzeTask 에러):
  → 라벨 제거 (다음 scan에서 재시도)
  → 3회 연속 실패 시 → HITL 알림

Claw 판단 실패 (Claude Code 세션 오류 또는 의도 불명확):
  → daemon의 기계적 drain으로 fallback (v3 동작)
  → 세션 로그 보존 (디버깅용)
  → 연속 N회 실패 시 → HITL 알림
  → 원인: claw-workspace 규칙 충돌, 네트워크 오류, 토큰 초과 등
```

### 에스컬레이션 규칙

```
1단계: 자동 재시도 (1-2회)
2단계: 관련 이슈에 실패 코멘트
3단계: HITL 알림 (GitHub 코멘트 + Notifier 채널)
4단계: 해당 이슈/PR을 skip 처리
5단계: Claw가 /update-spec 제안 (사용자와 대화로 방향 전환)
```

### 5단계: /update-spec 제안 (Flow 7 연계)

Claw는 스펙을 **직접 수정하지 않는다**. 실패가 누적되면 사용자에게 대화형 스펙 수정을 제안한다.

```
Claw 판단:
  "PR #44가 구현 3회, 리뷰 2회 실패.
   근본 원인: 스펙의 세션 스토리지 설계가 현재 인프라와 불일치."

HITL 알림:
  "스펙의 아키텍처 섹션을 재검토해주세요.
   레포 Claude 세션에서: /update-spec auth-v2"
```

**Replan = Claw가 스펙 수정이 필요하다고 판단하는 것**이지, 직접 이슈를 재구성하는 것이 아니다.
실제 수정은 사용자가 `/update-spec`으로 대화하며 진행한다. (Flow 7 Case 2 참조)
