# Flow 4: 다중 스펙 우선순위

### 시나리오

하나의 레포에 여러 스펙이 Active 상태로 존재한다.

### Claw의 판단

```
독립 스펙: 병렬 실행 (concurrency 제한 내)
의존 스펙: 선행 스펙 먼저 처리 (Claw가 순서 결정)
충돌 스펙: 같은 파일/모듈 → HITL 요청 (사용자가 우선순위 결정)
```

### DependencyGuard (코어, advance 시)

Claw가 의존성을 판단하면, 코어의 DependencyGuard가 실행 순서를 강제한다.

```
queue advance B 요청 시:
  1. B의 dependency 메타데이터 확인
  2. 선행 아이템 A가 Done 아니면 → advance 차단
  3. A가 Done이면 → 통과 → DataSource.on_phase_enter(Ready)
```

Claw는 "A가 B보다 먼저"라는 판단만 기록. 나머지는 상태 머신이 강제.

### CLI

```bash
autodev spec prioritize <id1> <id2> ...   # 순서 지정
autodev queue dependency add <work_id> --depends-on <work_id>
```

---

### 관련 플로우

- [Flow 2: 이슈 등록](../02-issue-registration/flow.md) — DependencyAnalyzer
- [Flow 5: HITL 알림](../05-hitl-notification/flow.md) — 충돌 시 HITL
