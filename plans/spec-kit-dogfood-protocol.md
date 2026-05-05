# Spec-Kit Dogfood Protocol

> spec-kit 변경 PR 의 회귀 게이트로 사용하는 표준 dogfood 절차. 본 문서는 절차 정의서이며, 실제 실행 결과는 별도 누적 파일에 기록한다 (§4 참조).

## 1. 목적

spec-kit 의 회귀 검증을 자동 unit test 로 충분히 다루기 어렵다 — L1/L2 출력은 LLM 산출물이라 단일 fixture 비교가 부적합하다. 따라서 **운영 dogfood 를 정형 회귀 게이트로 격상**한다.

본 protocol 은 다음을 목표로 한다.

- spec-kit 의 agents/commands/prompt 변경 PR 이 머지되기 전에 동일한 시나리오로 회귀 검증을 강제한다.
- baseline (Phase 3 dogfood, `plans/spec-kit-redesign/05-validation-and-followups.md`) 대비 정량 회귀 (drop, finding 수, wall-clock) 를 포착한다.
- 결과를 누적 파일에 시계열로 기록하여 변경 이력을 추적 가능하게 한다.

검증 baseline 의 출처는 `plans/spec-kit-redesign/05-validation-and-followups.md` 의 dogfood iteration table 이다. 본 protocol 의 시나리오 정의 (§2) 와 게이트 기준 (§5) 은 그 표를 직접 인용한다.

## 2. 표준 시나리오

Phase 3 dogfood 와 동일한 3종을 표준 시나리오로 고정한다. 시나리오는 임의 변경하지 않는다 — 변경 시 baseline 비교가 무력해진다.

| ID | 입력 spec | 도메인 | 검증 의도 |
|----|-----------|--------|-----------|
| **A** | belt `agent-runtime.md` | 에이전트 런타임 | 단일 spec — L1 정확도, 피드백 루프 수렴, L2 단일 spec finding |
| **B** | belt `cron-engine.md` | 스케줄링/orchestration | 다른 도메인의 단일 spec — 도메인 다양성에 대한 견고성 |
| **C** | belt `agent-runtime.md` + `daemon.md` | 에이전트 런타임 + 데몬 | 다중 spec — spec ↔ spec gap 자동 검출 능력 |

belt 프로젝트 경로는 사용자 환경에 따라 다르다 (예: `/Users/gong-yora/Documents/belt`). belt 에 접근 불가능한 환경에서는 동등한 spec-driven 프로젝트 (예: 본 레포의 `plans/github-autopilot/`) 를 대체로 사용할 수 있으나, 그 경우 baseline 과 직접 비교 불가능하므로 누적 로그에 명시한다.

## 3. 실행 절차

### 3.1 사전 준비

- 검증 대상 변경이 적용된 worktree 또는 branch 를 체크아웃한다.
- belt 프로젝트 (또는 동등 spec 프로젝트) 의 절대경로를 확인한다.
- spec-kit plugin 이 활성화될 plugin directory 경로를 확인한다 (본 레포의 경우 `plugins/spec-kit/`).

### 3.2 시나리오 실행

각 시나리오마다 spec-driven 프로젝트의 작업 디렉터리에서 spec-kit 을 활성화한 채 `/spec-kit:spec-review` 를 호출한다.

```bash
# 예: belt 프로젝트에서 spec-kit 활성화 후 실행
cd /Users/gong-yora/Documents/belt
claude --plugin-dir /Users/gong-yora/Documents/kys-claude-plugin2/plugins/spec-kit
# 세션 내에서:
# 시나리오 A
/spec-kit:spec-review specs/agent-runtime.md
# 시나리오 B
/spec-kit:spec-review specs/cron-engine.md
# 시나리오 C
/spec-kit:spec-review specs/agent-runtime.md specs/daemon.md
```

> spec 파일의 실제 경로는 belt 의 디렉터리 구조에 따른다. 본 표는 파일명 단위 시나리오 정의이며 경로 prefix 는 환경마다 다를 수 있다.

### 3.3 결과 캡처

각 시나리오마다 다음 4종을 캡처한다 (`/spec-kit:spec-review` 의 §검증 통계 footer 와 본문 finding 목록에서 직접 추출).

1. **L1 drop**: `K건 / N항목` 또는 `M / N` 리포트 통과
2. **피드백 루프 횟수**: `iter` 평균 또는 합계
3. **L2 drop**: L2 finding 단계의 drop 건수
4. **Finding 목록**: severity 별 카운트 + spec ↔ spec gap 발견 시 분류 (DEFINITION_CONFLICT / INTERFACE_DRIFT / REQUIREMENT_OVERLAP / TERM_AMBIGUITY 등)

추가로 wall-clock 측정이 가능한 환경에서는 시나리오별 실행 시간을 부수적으로 기록한다 (게이트 기준에 직접 포함되지는 않으나 추세 관찰용).

## 4. 결과 누적 위치

본 protocol 의 실행 결과는 **별도 파일** 에 누적한다.

- **누적 파일 경로**: `plans/spec-kit-dogfood-log.md` (별도 파일)
- **본 PR 에서는 생성하지 않는다** — 위치만 정의. 첫 dogfood 실행 PR 에서 신규 생성하면 된다.
- **PR 본문**: 누적 파일에 추가한 행을 그대로 복사하여 PR body 에도 포함 (리뷰어가 PR 단위에서 결과를 즉시 확인 가능).

### 4.1 결정 근거

PR 본문 단독 누적 (별도 파일 없음) 도 검토했으나 다음 이유로 별도 파일 채택:

- PR 본문 누적은 시계열 조회 시 PR 을 일일이 열어야 한다. 누적 파일은 단일 파일에서 회차 비교 가능.
- baseline 표 (`05-validation-and-followups.md`) 와 동일한 형식의 표로 누적하면 회귀 발견이 시각적으로 즉시 가능.
- PR 본문에도 동일 행을 함께 적으면 PR 단위 가시성과 시계열 추적성을 모두 확보.

### 4.2 누적 파일 스키마 (제안)

`plans/spec-kit-dogfood-log.md` 는 다음 형식의 표를 회차별로 append 한다 (실제 파일 생성 시 첫 행에서 합의).

| 회차 | PR | 시나리오 | L1 drop | 피드백 루프 | L2 drop | Finding (HIGH/MED/LOW) | spec↔spec gap | 비고 |
|------|----|----------|---------|-------------|---------|------------------------|---------------|------|

각 회차는 시나리오 A/B/C 3행을 한 묶음으로 추가한다.

## 5. 회귀 게이트 기준

### 5.1 Baseline (Phase 3 dogfood, `05-validation-and-followups.md`)

| 시나리오 | L1 drop | 피드백 루프 | L2 drop | Finding |
|----------|---------|-------------|---------|---------|
| A (`agent-runtime.md`) | 0 / 54 (0%) | 1회 | 0 | spec↔spec gap 측정 대상 아님 (단일) |
| B (`cron-engine.md`) | 1 / 39 (2.6%) | 1회 | 0 | HIGH 1, MED 2, LOW 1, Notes 4 (총 substantive 7건 상당) |
| C (`agent-runtime.md` + `daemon.md`) | 0 / 81 (0%) | 0회 | 0 | spec↔spec gap 4건 (HIGH 1, MED 2, LOW 1) |

### 5.2 게이트 조건 (PR 머지 전 충족 필요)

- **Finding 수**: 시나리오 B 의 substantive finding 수가 baseline 7건 대비 ±2 이내. 시나리오 C 의 spec↔spec gap 수가 baseline 4건 대비 ±2 이내.
- **Drop 비율**: 모든 시나리오의 L1 drop 비율이 baseline 유지 또는 개선. 시나리오 A/C 의 0% drop 은 strict (regression 발생 시 즉시 fail). 시나리오 B 의 2.6% drop 은 5% 이내 유지.
- **L2 drop**: 모든 시나리오에서 0 유지 (1건 이상 발생 시 fail).
- **Wall-clock**: 의도적 회귀 없음. 측정 시 baseline 대비 1.5배 이내. (정성 판단 — baseline 자체에 절대값이 없으므로 추세 관찰 후 명백한 회귀일 때만 fail.)

게이트 미충족 시 PR 은 머지 보류하고 변경을 보완한다. baseline 자체를 갱신해야 하는 경우 (예: 모델 교체로 정상 finding 분포가 변동) 별도 PR 에서 baseline 재정의를 합의한 뒤 게이트를 갱신한다.

### 5.3 게이트 면제 — 의도적 baseline 변경

다음 경우는 게이트 ±2 범위를 위반해도 정상 처리한다 (PR 본문에 사유 명시 필수).

- spec-kit 이 새로운 finding 분류를 추가하여 finding 수가 의도적으로 증가
- L1 prompt 강화로 drop 이 의도적으로 증가 (이후 피드백 루프에서 회복)
- 새 모델 프로필 도입으로 baseline 자체가 갱신됨

## 6. 트리거 조건

다음 변경을 포함하는 PR 은 머지 전 본 protocol 의 3개 시나리오 전부 실행을 의무로 한다.

1. **spec-kit 의 agents 변경**: `plugins/spec-kit/agents/` 하위 파일 추가/수정/삭제
2. **spec-kit 의 commands 변경**: `plugins/spec-kit/commands/` 하위 (`spec-review.md`, `gap-detect.md` 등)
3. **L1 / L2 / auditor prompt 변경**: 위 commands/agents 안의 prompt 본문 변경
4. **새 모델 프로필 추가**: file-pair-observer / gap-aggregator / gap-auditor 의 모델 변경 또는 새 변형 추가

다음은 본 protocol 의 트리거 대상이 **아니다**.

- spec-kit 외 다른 plugin 의 변경
- spec-kit 의 문서 (`README.md`, `plans/spec-kit-redesign/*.md` 등) 만 변경
- spec-kit 의 frontmatter 메타데이터만 변경 (description, argument-hint 등 prompt 본문 외)

## 7. Fixture 회귀 테스트 미도입 결정

본 protocol 은 운영 dogfood 만을 회귀 메커니즘으로 정의하며, 정형 fixture 회귀 테스트는 도입하지 않는다.

### 7.1 결정 근거

- L1/L2 출력은 LLM 산출물이라 단일 fixture 와의 byte-equal 비교가 무의미하다 — 동일 입력에서도 표현이 다를 수 있다.
- semantic 비교 (예: finding 분류 카운트) 는 가능하지만, 그 자체가 dogfood 의 §5 게이트와 동일하다. fixture 로 분리할 이득이 없다.
- 의도적 환각 주입 회귀 테스트 (`plans/spec-kit-redesign/04-test-scenarios.md` §1.3) 는 별개 가치가 있으나 본 protocol 의 범위 밖이다.

### 7.2 후속 처리

의도적 환각 주입 등 mock 기반 회귀 시나리오는 `05-validation-and-followups.md` 의 **F6 (환각 회귀 자동 테스트)** 백로그로 남아 있다. 본 protocol 은 F6 와 보완 관계이며 충돌하지 않는다.

## 8. 변경 이력 (본 protocol 자체의)

본 protocol 자체를 변경할 때는 다음 원칙을 따른다.

- §2 의 시나리오 정의 변경: baseline 무효화. baseline 갱신 PR 에서만 변경한다.
- §5 의 게이트 수치 변경: 명시적 사유 + 직전 회차 데이터 근거를 PR 본문에 첨부한다.
- §6 의 트리거 조건 추가/완화: 누적 로그에서 false positive/negative 가 관찰된 경우에만 변경한다.
