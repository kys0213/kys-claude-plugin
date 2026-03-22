# Flow 2: 스펙 생명주기 — 등록 → 이슈 분해 → 완료

> 사용자가 디자인 스펙을 등록하면, 이슈가 자동 생성되어 파이프라인에 진입하고, Cron 품질 루프가 스펙 완료까지 감시한다.

---

## Spec Lifecycle

```
Draft ──→ Active ←──→ Paused
              │
              ▼
          Completing
              │
              ▼
          Completed (terminal)

Any ──→ Archived (soft delete)
Archived ──resume──→ Active (복구)
```

| 상태 | 가능한 전이 | CLI |
|------|------------|-----|
| Draft | → Active | `spec add` |
| Active | → Paused, → Completing(자동), → Archived | `spec pause`, `spec remove` |
| Paused | → Active, → Archived | `spec resume`, `spec remove` |
| Completing | → Active(gap 발견), → Completed(HITL 승인) | 자동 |
| Completed | → Archived | `spec remove` |
| Archived | → Active | `spec resume` |

---

## 등록 → 이슈 분해

```
/spec add [file]
  → 필수 섹션 검증 (개요, 요구사항, 아키텍처, 테스트, 수용 기준)
  → DB에 저장 (status: Active)
  → Claw가 스펙 분해 → 이슈 자동 생성
  → 각 이슈에 trigger 라벨 (예: autodev:analyze) 부착
  → DataSource.collect()가 감지 → 파이프라인 진입
```

---

## 스펙 완료 판정

스펙 완료는 Cron 품질 루프가 담당:

```
gap-detection cron (1시간):
  → 스펙 vs 현재 코드 비교
  → gap 발견 → 새 이슈 생성 → 파이프라인 재진입
  → gap 없음 → Completing 상태로 전이

on_spec_completing:
  → TestRunner: spec.test_commands 실행
    → 실패 → 새 이슈 생성 → Active로 복귀
    → 성공 → HITL: 최종 확인 (approve / request-changes)
```

---

## 다중 스펙 우선순위

```
독립 스펙: 병렬 실행 (concurrency 제한 내)
의존 스펙: DependencyGuard가 실행 순서 강제
충돌 스펙: 같은 파일/모듈 → HITL 요청
```

---

## /spec 통합 커맨드

```
/spec                → 목록
/spec add [file]     → 등록
/spec update <id>    → 수정
/spec status <id>    → 진행도 상세
/spec remove <id>    → Archived
/spec pause <id>     → 일시정지
/spec resume <id>    → 재개
```

---

### 관련 문서

- [이슈 파이프라인](./03-issue-pipeline.md) — 분해된 이슈의 처리 흐름
- [실패 복구와 HITL](./04-failure-and-hitl.md) — 완료 확인 HITL
- [Cron 엔진](../concerns/cron-engine.md) — gap-detection 품질 루프
