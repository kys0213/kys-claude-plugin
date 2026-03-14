# Flow 2: 이슈 등록 (Issue 모드)

### 시나리오

사람이 GitHub에 이슈를 만들고 `autodev:analyze` 라벨을 추가한다.

### 기대 동작

```
1. 사람: GitHub 이슈 생성 + autodev:analyze 라벨 추가
2. autodev: 라벨 감지 → AnalyzeTask 실행
3. 분석 리포트를 이슈 코멘트로 게시
4. auto_approve 조건 충족 시 → 자동 구현 진행
   미충족 시 → 사람 리뷰 대기
5. 승인 후 → ImplementTask → PR 생성
6. ReviewTask → ImproveTask → (반복) → done
```

### v3와의 관계

기본 파이프라인은 v3와 동일하다. Claw 활성화 여부에 따라 추가 동작이 발생한다.

### Claw 비활성 시 (claw.enabled: false)

v3와 100% 동일. daemon이 기계적으로 슬롯 기반 drain 수행.

### Claw 활성 시 (claw.enabled: true)

Claw가 Issue 모드에서 추가로 수행하는 판단:

```
1. 이슈 간 의존성 확인
   → 같은 파일/모듈을 수정하는 이슈가 있으면 순차 처리 판단
2. 활성 스펙과의 연관성 확인
   → 스펙이 존재하면 해당 이슈를 spec_issues에 자동 링크
3. 지능적 drain
   → FIFO가 아닌 의존성/충돌 기반 우선순위로 phase 전이
4. 패턴 감지
   → 리뷰 반복, 낮은 confidence 등 → HITL 요청 (Flow 5)
```

이 추가 동작은 Claw(autodev agent)가 내부적으로 CLI 도구를 호출하여 수행한다:
```bash
# (Claw 내부 도구 호출 — 사용자가 직접 실행하지 않음)
autodev queue list --json          # 큐 상태 확인
autodev spec show <id> --json      # 관련 스펙 확인
autodev hitl respond ...           # HITL 응답
```
