# Flow 3: 이슈 파이프라인 — 컨베이어 벨트

> 이슈가 DataSource의 상태 정의에 따라 자동으로 처리되고, Done이 다음 단계를 트리거한다.

---

## 컨베이어 벨트 흐름

```
autodev:analyze 감지 → [analyze handlers] → Claw: Done? → autodev:implement 부착
                                                              │
autodev:implement 감지 → [implement handlers] → Claw: Done? → autodev:review 부착
                                                              │
autodev:review 감지 → [review handlers] → Claw: Done? → autodev:done 부착
```

각 구간은 독립적인 QueueItem. 되돌아가지 않고, 항상 새 아이템으로 다음 구간에 진입.

---

## 단일 구간 상세

```
DataSource.collect(): trigger 조건 매칭 (예: autodev:analyze 라벨)
    │
    ▼
  Pending → Ready → Running
    │
    │  handlers 순차 실행:
    │    prompt → AgentRuntime.invoke()
    │    command → slash command 호출
    │    script → sh -c 실행 (exit code 판정)
    │
    ├── 전부 성공
    │     │
    │     ▼
    │   Claw evaluate: "완료? 추가 검토?"
    │     ├── Done → on_done 액션 (다음 state trigger 활성화)
    │     └── HITL → HITL 이벤트 생성 → 사람 대기
    │
    └── 실패 → escalation
          ├── Retry → Pending 재진입 (같은 구간)
          ├── HITL → HITL 이벤트 생성
          └── Skip → Skipped
```

---

## DataSource 설정 예시

```yaml
sources:
  github:
    states:
      analyze:
        trigger: { label: "autodev:analyze" }
        handlers:
          - prompt: "이슈를 분석하고 구현 가능 여부를 판단해줘"
        on_done: { label: "autodev:implement" }

      implement:
        trigger: { label: "autodev:implement" }
        handlers:
          - command: "/implement"
          - script: hooks/lint.sh
        on_done: { label: "autodev:review" }

      review:
        trigger: { label: "autodev:review" }
        handlers:
          - prompt: "PR을 리뷰하고 품질을 평가해줘"
        on_done: { label: "autodev:done" }
```

사용자가 단계를 추가/제거/교체하려면 yaml만 수정.

---

## 피드백 루프

### PR review comment (changes-requested)

```
DataSource.collect()가 changes-requested 감지
  → 새 아이템 생성 → handlers 실행 → 수정 반영
```

### /spec update

```
스펙 변경 → on_spec_active → Cron(gap-detection) 재평가
  → gap 발견 시 새 이슈 생성 → 파이프라인 재진입
```

### 핵심 원칙

**스펙 = 계약**. 계약이 바뀌어야 하면 `/spec update`. 계약 범위 내 작업이면 이슈 등록.

---

### 관련 문서

- [DataSource](../concerns/datasource.md) — 상태 기반 워크플로우 정의
- [실패 복구와 HITL](./04-failure-and-hitl.md) — escalation 정책
- [Cron 엔진](../concerns/cron-engine.md) — 품질 루프로 새 아이템 생성
