# Flow 2: 이슈 등록 (Issue 모드)

### 시나리오

사람이 GitHub에 이슈를 만들고 `autodev:analyze` 라벨을 추가한다.

### 기대 동작

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

### 관련 플로우

- [Flow 0: DataSource](../00-datasource/flow.md)
- [Flow 4: 다중 스펙 우선순위](../04-spec-priority/flow.md)
- [Flow 5: HITL 알림](../05-hitl-notification/flow.md)
- [Flow 9: 실패 복구](../09-failure-recovery/flow.md)
