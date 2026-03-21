# Flow 3: 스펙 등록 (Spec 모드)

### 시나리오

사용자가 디자인 스펙을 등록하여 자율 구현 루프를 시작한다.

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
   Plugin: 누락 시 대화형 보완 (workspace 컨텍스트 기반 자동 제안)

2. DB에 저장 (status: Active)

3. 코어 on_spec_active 이벤트:
   → ForceClawEvaluate: claw-evaluate cron 즉시 트리거

4. Claw evaluate:
   → decompose skill로 스펙 분해 → 이슈 자동 생성
   → 각 이슈에 autodev:analyze 라벨
   → convention 기반 이슈 템플릿 적용

5. 생성된 이슈들이 큐에 진입 → Flow 2 파이프라인
```

### /spec 통합 커맨드

```
/spec                → 목록 (autodev spec list)
/spec add [file]     → 등록 (기존 /add-spec)
/spec update <id>    → 수정 (기존 /update-spec, Flow 7)
/spec status <id>    → 진행도 상세
/spec remove <id>    → Archived
/spec pause <id>     → 일시정지
/spec resume <id>    → 재개 (Archived에서도 복구)
```

---

### 관련 플로우

- [Flow 7: 피드백 루프](../07-feedback-loop/flow.md) — /spec update
- [Flow 8: 스펙 완료 판정](../08-spec-completion/flow.md)
- [Flow 11: 컨벤션 부트스트랩](../11-convention-bootstrap/flow.md) — 이슈 템플릿
