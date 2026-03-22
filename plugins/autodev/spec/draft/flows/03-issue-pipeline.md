# Flow 3: 이슈 파이프라인 — 감지 → 분석 → 실행 → 피드백

> 이슈가 큐에 진입하여 자동 분석·구현되고, 사용자 피드백으로 보정되는 순환 흐름.

---

## 1. 이슈 등록 (Issue 모드)

사람이 GitHub에 이슈를 만들고 `autodev:analyze` 라벨을 추가한다.

### 파이프라인

```
1. DataSource.collect(): 라벨 감지 → QueueItem 생성
2. Daemon: DB에 Pending 저장
3. 코어 on_enter_pending:
   a. DependencyAnalyzer: 파일 단위 의존성 분석
   b. SpecLinker: spec_issues 자동 링크
   c. DecisionRecorder: 수집 기록
4. DataSource.on_phase_enter(Pending): 라벨 부착 (autodev:queued)
5. Claw evaluate: advance 판단 → Pending → Ready
6. Daemon drain: Ready → Running
   → DataSource.before_task(): 시작 코멘트
   → AgentRuntime.invoke(): Task 실행
   → DataSource.after_task(): 결과 코멘트
7. 완료 시 → 코어 on_done + DataSource.on_done()
8. 실패 시 → DataSource.on_failed() → EscalationAction → 코어 처리
```

### DependencyAnalyzer (코어, on_enter_pending)

파일 단위 의존성 분석. DataSource 무관, 모든 소스에 공통 적용.

```
1. 이슈 본문에서 파일 경로 추출
   - 코드 블록 내 경로
   - 명시적 경로 (src/auth/login.rs)
   - 스펙 참조 내 파일 목록
2. 큐의 다른 Pending/Running 아이템과 파일 경로 비교
3. 겹침 발견 → dependency 메타데이터에 선행 work_id 기록
```

Claw가 다음 evaluate에서 의미론적 보정 (같은 모듈이지만 다른 파일 등).

### SpecLinker (코어, on_enter_pending)

```
1. 이슈 라벨에서 [XX-spec-name] 패턴 매칭
2. 이슈 본문에서 스펙 참조 추출
3. 매칭 시 spec_issues 테이블 링크
4. 실패 시 로그만 (Claw가 보정)
```

### DataSource hook 예시 (GitHub)

```
on_phase_enter(Pending)  → autodev:queued 라벨
on_phase_enter(Ready)    → autodev:ready 라벨
on_phase_enter(Running)  → autodev:wip 라벨
before_task(Analyze)     → "🔍 분석을 시작합니다..." 코멘트
after_task(Analyze, Ok)  → 분석 리포트 코멘트
on_done()                → "✅ 완료" 코멘트 + autodev:done 라벨
on_failed(count=3)       → HITL 이벤트 생성 + GitHub 코멘트
```

---

## 2. 피드백 루프

사용자가 구현 결과를 확인하고 수정이 필요하다고 판단한다.

### 3가지 경로

#### Case 1: PR review comment

```
GitHub에서 changes-requested
  → DataSource.collect(): 큐에 Review/Improve 아이템 추가
  → 표준 파이프라인 (DataSource hook + AgentRuntime 실행)
```

기존 v3 흐름과 동일. DataSource hook이 자동 적용.

#### Case 2: /spec update

```
/spec update <id>
  → 현재 스펙 + 진행 상태 로드
  → 대화형 impact analysis (어떤 이슈가 영향받는지)
  → 사용자가 변경 승인
  → autodev spec update CLI 실행
  → 코어 on_spec_active 이벤트 재발행
  → ForceClawEvaluate → Claw 재평가
```

#### Case 3: Replan (HITL 응답)

```
on_failed Level 5 → DataSource.on_failed() → EscalationAction::Replan
  → 코어: HITL 생성
  → DataSource.after_hitl_created(): 알림
  → 사용자 "replan" 응답
  → 코어 on_hitl_responded → Claw에게 스펙 수정 제안 위임
  → 사용자 승인 → spec update → on_spec_active
```

### 핵심 원칙

**스펙 = 계약**. 계약이 바뀌어야 하면 `/spec update`. 계약 범위 내 작업이면 이슈 등록.

---

### 관련 문서

- [스펙 생명주기](./02-spec-lifecycle.md) — 스펙 등록, 이슈 분해
- [실패 복구와 HITL](./04-failure-and-hitl.md) — 실패 시 escalation, replan 경로
- [DataSource](../concerns/datasource.md) — collect, hook 상세
- [AgentRuntime](../concerns/agent-runtime.md) — Task 실행
