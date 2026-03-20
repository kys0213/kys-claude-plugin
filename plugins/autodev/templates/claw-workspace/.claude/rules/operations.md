# 운영 가이드 — 상황별 처리 방법

## 큐 상태 전이 흐름

```
Pending → Ready → Running → Done
                      ↓
                   Skipped
```

### 각 상태에서의 처리

| 상태 | Claw 동작 |
|------|----------|
| Pending | 대기 중. `autodev queue advance <work-id>` → Ready로 전이 |
| Ready | 실행 준비 완료. `autodev queue advance <work-id>` → Running으로 전이 후 구현 시작 |
| Running | 구현/리뷰 진행 중. 완료 시 `autodev queue advance <work-id>` → Done |
| Skipped | 건너뜀. 원인 분석 후 재시도 또는 HITL 에스컬레이션 |
| Done | 다음 큐 아이템 처리로 진행 |

### 전이 전 확인 사항

- advance 전에 `autodev queue show <work-id> --json`으로 현재 상태 확인
- 이미 Done/Skipped인 아이템은 advance 불가
- skip은 `autodev queue skip <work-id> --reason "사유"` 사용

---

## HITL 에스컬레이션 처리 절차

### 1. HITL 발생 시

```bash
autodev hitl list --json              # 대기 중인 HITL 확인
autodev hitl show <id> --json         # 상세 상황 + 선택지 확인
```

### 2. 자동 응답 가능한 경우

이전 판단 이력에서 동일 패턴이 있는지 확인:
```bash
autodev decisions list --json -n 20   # 최근 판단 이력 조회
```

동일 패턴이 있으면 동일 방향으로 응답:
```bash
autodev hitl respond <id> --choice <N>
```

### 3. 사용자 개입이 필요한 경우

- HIGH 심각도: 구현 실패, 리뷰 3회 반복 → 사용자에게 알림
- MEDIUM 심각도: 스펙 충돌, 낮은 confidence → 24시간 대기
- LOW 심각도: 스펙 완료 판정 → 24시간 대기

### 4. 타임아웃 처리

`hitl-timeout` cron이 5분마다 자동 실행:
```bash
autodev hitl timeout                  # 기한 초과 HITL 처리 (리마인드)
```

---

## Cron 등록 및 관리

### 빌트인 Cron (자동 등록)

| 이름 | 스코프 | 주기 | 역할 |
|------|--------|------|------|
| claw-evaluate | per-repo | 60초 | 큐 평가 + 다음 작업 결정 |
| gap-detection | per-repo | 1시간 | 스펙-코드 갭 탐지 |
| daily-report | global | 매일 06:00 | 일간 리포트 생성 |
| hitl-timeout | global | 5분 | HITL 타임아웃 처리 |

### Cron 미등록 시 대응

cron이 등록되지 않은 상태에서 호출되면 실패한다. 확인 방법:
```bash
autodev cron list --json              # 등록된 cron 목록 확인
```

누락된 cron이 있으면:
```bash
autodev cron add --name <name> --repo <repo> --interval <seconds> --script <path>
```

### Cron 수동 트리거

급한 평가가 필요할 때:
```bash
autodev cron trigger claw-evaluate [--repo <name>]
# 또는
autodev spec evaluate <spec-id>
```

---

## 병렬 vs 순차 작업 판단

### 병렬 처리 조건 (모두 충족 시)

1. 서로 다른 파일을 수정하는 이슈
2. 의존 관계 없음 (이슈 간 depends_on 없음)
3. 레포의 concurrency 한도 미초과

### 순차 처리 조건 (하나라도 해당 시)

1. 같은 파일을 수정하는 이슈
2. 명시적 의존 관계 존재
3. 인터페이스 정의 → 구현 순서

### 충돌 감지

```bash
autodev spec conflicts <spec-id>      # 스펙 간 파일 충돌 탐지
```

충돌 발견 시 → MEDIUM 심각도 HITL 생성 또는 순차 처리로 전환

---

## 실패 복구 및 재시도

### 일시적 실패 (재시도 가능)

- 네트워크 오류, rate limit, 일시적 CLI 오류
- 1회 재시도 후 성공하면 계속 진행
- 2회 연속 실패 시 HITL 에스컬레이션

### 구현 실패

1. 에러 로그 확인: `autodev logs --repo <name> -n 5`
2. 실패 원인 분석 (컴파일 에러, 테스트 실패 등)
3. 자동 수정 가능하면 재시도
4. 불가능하면 HIGH 심각도 HITL 생성

### 리뷰 반복 실패

- 1~2회: 리뷰 코멘트 기반 자동 수정
- 3회: HITL 에스컬레이션 (review-policy.md 참조)
- 5회+: 스펙 수정 권고

---

## 스펙 라이프사이클

```
등록 → 분해(decompose) → 이슈 생성 → 구현 → 리뷰 → 완료
```

### 주요 명령어 흐름

```bash
# 1. 스펙 등록
autodev spec add --title "..." --file spec.md --repo <name>

# 2. 스펙 분해 (decompose 스킬 사용)
# Claw가 자동으로 이슈를 GitHub에 생성

# 3. 진행 상태 확인
autodev spec status <id> --json

# 4. 완료 판정
autodev spec complete <id>
```

### 스펙 일시정지/재개

우선순위 변경이나 블로커 발생 시:
```bash
autodev spec pause <id>               # 관련 이슈 진행 중단
autodev spec resume <id>              # 재개
```
