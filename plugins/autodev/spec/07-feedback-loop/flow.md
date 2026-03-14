# Flow 7: 피드백 루프

### 시나리오

사용자가 구현 결과를 확인하고 수정이 필요하다고 판단한다.

### 핵심 원칙

모든 피드백은 **레포의 Claude 세션에서 대화로 처리**한다.
스펙 생성(`/add-spec`)과 동일한 패턴 — 맥락 위에서 대화하며 방향을 잡는다.

---

### 피드백 경로

```
Case 1: 개별 이슈/PR 수정
  → PR에 리뷰 코멘트 작성
  → autodev가 changes-requested 감지 → ImproveTask 실행
  (v3와 동일)

Case 2: 스펙 레벨 수정 (대화형)
  → 레포 Claude 세션에서 /update-spec <spec-id>
  → 대화로 변경 방향 논의 → 스펙 정제 → 이슈 재구성
  (아래 상세 참조)

Case 3: 새로운 문제 발견
  → GitHub에 이슈 수동 등록 + autodev:analyze
  → Claw가 활성 스펙과 연관성 판단 → spec_issues에 링크
  (Issue 모드와 Spec 모드의 자연스러운 공존)
```

---

### Case 2: /update-spec (대화형 스펙 수정)

`/add-spec`과 동일한 패턴. 레포 컨텍스트 + 기존 스펙 + 진행 상태를 기반으로 대화한다.

```
> /update-spec auth-v2

📋 Auth Module v2 현재 상태:
  진행도: 3/5 (60%)
  ✅ #42 JWT middleware (done)
  ✅ #43 Token API (done)
  🔄 #44 Session adapter (implementing)
  ⏳ #45 Error handling (pending)
  🔍 #46 Missing tests (gap, analyzing)

어떤 부분을 변경하고 싶으신가요?

> Redis 세션 대신 JWT stateless로 바꾸고 싶어. 세션 스토리지가 필요없게.

변경 영향을 분석합니다...

영향 범위:
  - #44 Session adapter (implementing) → 불필요해짐
  - #43 Token API (done) → refresh 로직 수정 필요
  - 아키텍처: auth/session.rs 제거, auth/token.rs 수정
  - Acceptance Criteria: 세션 관련 항목 제거, stateless 검증 추가

제안:
  1. #44 → autodev:skip (세션 어댑터 불필요)
  2. #43 → 새 이슈로 재작업 (refresh → stateless rotation)
  3. Acceptance Criteria 업데이트
  4. 아키텍처 섹션 수정

이대로 진행할까요?
[1: 진행 (Recommended)]
[2: 수정]

> 1

✅ 스펙 업데이트 완료.
  - #44 → autodev:skip 처리됨
  - 새 이슈 #47 "refactor: stateless token rotation" 생성됨
  - Acceptance Criteria 3개 항목 수정됨
  - Claw가 다음 틱에서 업데이트된 스펙 기반으로 재판단합니다.
```

### /update-spec이 수행하는 것

```
1. 현재 스펙 + 진행 상태 로드
2. 사용자와 대화하며 변경 방향 파악
3. 변경 영향 분석 (기존 이슈와 대조)
4. 제안:
   - 불필요해진 이슈 → skip
   - 수정 필요한 이슈 → 새 이슈 생성 또는 재분석
   - Acceptance Criteria 업데이트
   - 아키텍처 섹션 수정
5. 사용자 승인 후 autodev spec update CLI로 반영
```

---

### Spec 업데이트 vs Issue 등록 구분

| 변경 유형 | 적합한 경로 | 이유 |
|----------|-----------|------|
| 요구사항 추가/제거 | `/update-spec` (Case 2) | 계약서 자체가 변경됨 |
| Acceptance Criteria 변경 | `/update-spec` (Case 2) | 완료 기준이 변경됨 |
| 아키텍처 방향 전환 | `/update-spec` (Case 2) | 계약서의 구조가 변경됨 |
| 구현 중 발견된 버그 | Issue 등록 (Case 3) | 기존 계약 범위 내 |
| 기술적 개선/리팩토링 | Issue 등록 (Case 3) | 계약 변경 아님 |

핵심: **Spec = 계약서**. 계약 내용이 바뀌면 `/update-spec`, 계약 범위 내 작업이면 Issue 등록.

---

### Claw가 트리거하는 피드백 (Flow 9 연계)

실패가 누적되면 Claw가 **사용자에게 `/update-spec`을 제안**한다:

```
🔔 autodev HITL:

PR #44 (Session adapter)가 구현 실패 3회, 리뷰 실패 2회입니다.
근본 원인: 세션 스토리지 인터페이스와 Redis 어댑터 간 계약 불일치

권장: 스펙의 아키텍처 섹션을 재검토해주세요.
→ 레포 Claude 세션에서: /update-spec auth-v2
```

Claw는 스펙을 직접 수정하지 않는다. **항상 사용자와의 대화를 통해** 수정한다.
