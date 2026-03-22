# Flow 2: 스펙 생명주기 — 등록 → 우선순위 → 이슈 분해 → 완료

> 사용자가 디자인 스펙을 등록하면, 자동으로 이슈가 분해되고 순서대로 실행되어 스펙 완료까지 진행된다.

---

## 1. 스펙 등록

### 입력

```
/spec add [file]            # Plugin command (대화형 보완)
autodev spec add --title ... --file ...   # CLI (경고만)
```

### Spec Lifecycle

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
| Draft | → Active | `spec add` (등록 시 바로 Active) |
| Active | → Paused, → Completing(자동), → Archived | `spec pause`, `spec remove` |
| Paused | → Active, → Archived | `spec resume`, `spec remove` |
| Completing | → Active(테스트 실패), → Completed(HITL 승인) | 자동 |
| Completed | → Archived | `spec remove` |
| Archived | → Active | `spec resume` (복구) |

### 기대 동작

```
1. 스펙 본문 파싱 → 필수 섹션 검증
   필수 섹션: 개요, 요구사항, 아키텍처, 테스트 환경, 수용 기준
   CLI: 누락 시 경고 (--force로 진행 가능)
   Plugin: 누락 시 대화형 보완 (레포 컨텍스트 기반 자동 제안)

2. DB에 저장 (status: Active)

3. 코어 on_spec_active 이벤트:
   → ForceClawEvaluate: claw-evaluate cron 즉시 트리거

4. Claw evaluate:
   → decompose skill로 스펙 분해 → 이슈 자동 생성
   → 각 이슈에 autodev:analyze 라벨
   → convention 기반 이슈 템플릿 적용

5. 생성된 이슈들이 큐에 진입 → 이슈 파이프라인
```

### /spec 통합 커맨드

```
/spec                → 목록 (autodev spec list)
/spec add [file]     → 등록 (기존 /add-spec)
/spec update <id>    → 수정 (피드백 루프 참조)
/spec status <id>    → 진행도 상세
/spec remove <id>    → Archived
/spec pause <id>     → 일시정지
/spec resume <id>    → 재개 (Archived에서도 복구)
```

---

## 2. 다중 스펙 우선순위

하나의 레포에 여러 스펙이 Active 상태로 존재할 때:

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

## 3. 스펙 완료 판정

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

### 관련 문서

- [이슈 파이프라인](./03-issue-pipeline.md) — 분해된 이슈의 실행 흐름
- [실패 복구와 HITL](./04-failure-and-hitl.md) — 충돌 시 HITL, 완료 확인 HITL
- [모니터링](./05-monitoring.md) — 스펙 진행률 시각화
