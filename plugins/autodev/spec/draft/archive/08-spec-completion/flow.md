# Flow 8: 스펙 완료 판정

### 시나리오

스펙에 연결된 모든 이슈가 완료되어 스펙의 목표 달성 여부를 판정한다.

### 자동 감지 (코어 on_done)

```
Task 완료 → 코어 on_done:
  → SpecCompletionCheck:
    1. 이 아이템에 linked spec이 있는가?
    2. 해당 spec의 모든 linked issues가 Done인가?
    3. 모두 Done → on_spec_completing 이벤트
```

### on_spec_completing 파이프라인

```
1. TestRunner: spec.test_commands 순차 실행
   → "cargo test -p auth", "cargo test -p auth --test integration" 등
   → 각 명령의 exit code + stdout/stderr 수집
   → 실패 항목: 이슈 자동 생성 → on_enter_pending → DataSource hook
   → 모두 성공: 다음 단계

2. ForceClawEvaluate: gap detection (Claw에게 위임)
   → Claw가 스펙 vs 현재 코드 비교
   → gap 발견: 이슈 생성 → 루프 계속
   → gap 없음: 다음 단계

3. HitlCreator: 최종 확인 HITL (Low severity)
   → 선택지:
     approve → on_spec_completed
     request-changes → Active로 복귀
```

### on_spec_completed

```
1. spec.status = Completed
2. ReportGenerator: 완료 리포트
   → 소요 시간, 토큰 사용량, 완료 이슈 목록, 생성된 PR 목록
```

### linked issues 없는 스펙 처리

```
spec remove <id>              → Archived (소프트 삭제)
spec complete <id> --force    → HITL 없이 직접 완료
```

---

### 관련 플로우

- [Flow 3: 스펙 등록](../03-spec-registration/flow.md) — spec lifecycle
- [Flow 5: HITL 알림](../05-hitl-notification/flow.md) — 최종 확인
