# Automated Feedback Loop Plugin

자동화된 피드백 루프를 통한 인간-에이전트 협업 시스템

## Quick Start

```bash
# 1. 초기화 (프로젝트 분석)
/afl:init

# 2. 설계 시작 (인간-에이전트 협업)
/afl:architect "쿠폰 할인 기능 개발"

# 3. 구현 위임 (자율 에이전트)
/afl:delegate --all

# 4. 상태 확인
/afl:loop-status
```

---

## 전체 워크플로우

```mermaid
flowchart TB
    subgraph Phase1["🧑‍💻 PHASE 1: 설계 (인간 + 에이전트)"]
        A["/afl:architect 요구사항"] --> B["에이전트: 스펙 초안 제안"]
        B --> C{"인간: 검토"}
        C -->|피드백| B
        C -->|승인| D["Checkpoint 생성 및 저장"]
    end

    subgraph Phase2["🤖 PHASE 2: 구현 (자율 에이전트)"]
        E["/afl:delegate"] --> F["Worker: 구현 시작"]
        F --> G["자동 검증<br/>(validation command)"]
        G --> H{"통과?"}
        H -->|Yes| I["다음 Checkpoint"]
        H -->|No| J["Test Oracle: 실패 분석"]
        J --> K["자동 피드백 생성"]
        K --> L{"재시도<br/>횟수?"}
        L -->|"< max"| F
        L -->|">= max"| M["⚠️ 에스컬레이션"]
    end

    subgraph Phase3["📋 PHASE 3: 완료"]
        N["모든 Checkpoint 통과"]
        O["Impl Reviewer: 품질 검토"]
        P["완료 보고"]
    end

    D --> E
    I --> G
    I -->|"모두 완료"| N
    N --> O
    O --> P
    M -->|"설계 재검토"| A

    style Phase1 fill:#e1f5fe
    style Phase2 fill:#fff3e0
    style Phase3 fill:#e8f5e9
```

---

## Checkpoint 생명주기

### 언제 생성되나?

```mermaid
sequenceDiagram
    participant H as 인간
    participant A as 에이전트
    participant F as 파일시스템

    H->>A: /afl:architect "요구사항"

    loop 설계 루프
        A->>H: 아키텍처 옵션 제안
        H->>A: 피드백/선택
        A->>H: 수정된 스펙 제안
    end

    A->>H: Checkpoint 초안 제안
    Note over A,H: criteria + validation 정의

    H->>A: Checkpoint 승인
    A->>F: .claude/afl.yaml 저장
    A->>F: .afl/sessions/{id}/specs/checkpoints.yaml 저장

    Note over F: Checkpoint 확정!
```

### 저장 위치

```
.claude/
└── afl.yaml                    # 전역 설정

.afl/
└── sessions/
    └── {session-id}/
        ├── meta.json           # 세션 정보
        ├── specs/
        │   ├── architecture.md # 아키텍처 설계
        │   ├── contracts.md    # 인터페이스 정의
        │   └── checkpoints.yaml # ⭐ Checkpoint 정의
        └── delegations/
            └── {checkpoint-id}/
                ├── status.json
                └── iterations/
                    └── {n}/
                        ├── result.json
                        └── feedback.md
```

---

## 자동 피드백 루프 상세

### 어떻게 동작하나?

```mermaid
flowchart LR
    subgraph Trigger["트리거"]
        A["Worker 구현 완료"]
    end

    subgraph Validation["자동 검증"]
        B["validation.command 실행"]
        C["expected 결과와 비교"]
    end

    subgraph Analysis["실패 분석 (Test Oracle)"]
        D["테스트 출력 파싱"]
        E["실패 원인 분류"]
        F["관련 코드 분석"]
    end

    subgraph Feedback["피드백 생성"]
        G["구체적 수정 제안"]
        H["코드 예시 포함"]
    end

    subgraph Retry["재시도"]
        I["Worker에 피드백 전달"]
        J["재구현"]
    end

    A --> B --> C
    C -->|실패| D --> E --> F --> G --> H --> I --> J
    J --> B
    C -->|성공| K["✅ 완료"]

    style Trigger fill:#e3f2fd
    style Validation fill:#fff8e1
    style Analysis fill:#fce4ec
    style Feedback fill:#f3e5f5
    style Retry fill:#e8f5e9
```

### 실패 원인 분류

```mermaid
flowchart TD
    A["검증 실패"] --> B{"원인 분류"}

    B --> C["NOT_IMPLEMENTED<br/>구현 누락"]
    B --> D["LOGIC_ERROR<br/>로직 오류"]
    B --> E["TYPE_ERROR<br/>타입/컴파일 오류"]
    B --> F["DESIGN_MISMATCH<br/>설계 불일치"]
    B --> G["ENV_ISSUE<br/>환경 문제"]

    C --> H["자동 재시도"]
    D --> H
    E --> H
    F --> I["⚠️ 에스컬레이션<br/>(설계 재검토 필요)"]
    G --> I

    style C fill:#c8e6c9
    style D fill:#c8e6c9
    style E fill:#c8e6c9
    style F fill:#ffcdd2
    style G fill:#ffcdd2
```

---

## Checkpoint 구조

```yaml
# .afl/sessions/{session-id}/specs/checkpoints.yaml

session: abc12345
created_at: 2024-01-15T10:00:00Z
approved_at: 2024-01-15T11:30:00Z

checkpoints:
  - id: coupon-model
    name: "쿠폰 도메인 모델"
    type: implementation
    dependencies: []              # 의존성 없음 = 먼저 실행
    criteria:                     # 성공 기준 (사람이 정의)
      - "Coupon 엔티티 생성"
      - "CouponRepository 구현"
    validation:                   # 자동 검증 방법
      command: "pytest tests/test_coupon_model.py"
      expected: "passed"
      timeout: 30000

  - id: coupon-service
    name: "쿠폰 서비스 로직"
    type: implementation
    dependencies: [coupon-model]  # coupon-model 완료 후 실행
    criteria:
      - "validate() 구현"
      - "apply() 구현"
      - "중복 적용 방지"
    validation:
      command: "pytest tests/test_coupon_service.py"
      expected: "3 passed"
      timeout: 30000

  - id: coupon-api
    name: "쿠폰 API"
    type: api
    dependencies: [coupon-service]
    criteria:
      - "POST /coupons/validate - 200/400"
      - "POST /coupons/apply - 200/409"
    validation:
      command: "pytest tests/test_coupon_api.py -v"
      expected: "passed"
      timeout: 60000
```

---

## 실행 순서 (의존성 기반)

```mermaid
flowchart LR
    subgraph Round1["Round 1 (병렬 가능)"]
        A["coupon-model"]
    end

    subgraph Round2["Round 2"]
        B["coupon-service"]
    end

    subgraph Round3["Round 3"]
        C["coupon-api"]
    end

    subgraph Round4["Round 4"]
        D["coupon-integration"]
    end

    A --> B --> C --> D

    style Round1 fill:#e3f2fd
    style Round2 fill:#e8f5e9
    style Round3 fill:#fff3e0
    style Round4 fill:#fce4ec
```

---

## 명령어 요약

| 명령어 | 용도 | 인간 개입 |
|--------|------|----------|
| `/afl:init` | 프로젝트 분석, 설정 생성 | 초기 1회 |
| `/afl:config` | 설정 조회 및 변경 | 필요시 |
| `/afl:setup` | 대화형 설정 변경 | 필요시 |
| `/afl:architect` | 설계 루프 (스펙, Checkpoint 정의) | **적극 참여** |
| `/afl:delegate` | 구현 위임 | 시작만 |
| `/afl:checkpoint` | Checkpoint 관리 | 필요시 |
| `/afl:loop-status` | 피드백 루프 상태 확인 | 모니터링 |

---

## 에이전트 역할

```mermaid
flowchart TB
    subgraph Design["설계 단계"]
        SV["Spec Validator<br/>스펙 일관성 검증"]
    end

    subgraph Implementation["구현 단계"]
        TO["Test Oracle<br/>실패 분석 + 피드백 생성"]
    end

    subgraph Review["검토 단계"]
        IR["Impl Reviewer<br/>품질 검토"]
    end

    SV -->|"Checkpoint 승인 전"| TO
    TO -->|"모든 Checkpoint 통과"| IR

    style SV fill:#e1f5fe
    style TO fill:#fff3e0
    style IR fill:#e8f5e9
```

| 에이전트 | 역할 | 호출 시점 |
|----------|------|----------|
| **Spec Validator** | 스펙 문서 일관성 검증 | Checkpoint 승인 전 |
| **Test Oracle** | 테스트 실패 분석, 피드백 생성 | 검증 실패 시 |
| **Impl Reviewer** | 구현 품질 검토 | Checkpoint 통과 후 |

---

## 설정 파일

```yaml
# .claude/afl.yaml

project:
  language: python              # 자동 감지
  framework: fastapi
  test_command: pytest
  build_command: poetry build

feedback_loop:
  mode: auto                    # auto | semi-auto | manual
  max_iterations: 5             # 최대 재시도 횟수

validation:
  method: test                  # test | script | manual
  timeout: 120000

notification:
  method: system                # system | slack | none

agents:
  spec_validator: true
  test_oracle: true
  impl_reviewer: true
```

---

## 사용 시나리오

### 정상 플로우

```mermaid
sequenceDiagram
    participant H as 인간
    participant A as 에이전트
    participant W as Worker

    H->>A: /afl:architect "쿠폰 기능"

    loop 설계 협업
        A->>H: 옵션 제안
        H->>A: 선택/피드백
    end

    A->>H: Checkpoint 제안
    H->>A: 승인 ✅

    H->>A: /afl:delegate --all
    A->>W: Checkpoint 1 구현 요청
    W->>A: 구현 완료
    A->>A: 자동 검증 ✅

    A->>W: Checkpoint 2 구현 요청
    W->>A: 구현 완료
    A->>A: 자동 검증 ✅

    A->>H: 🎉 모든 Checkpoint 완료!
```

### 실패 → 자동 재시도 플로우

```mermaid
sequenceDiagram
    participant A as 에이전트
    participant W as Worker
    participant TO as Test Oracle

    A->>W: Checkpoint 구현 요청
    W->>A: 구현 완료
    A->>A: 자동 검증 ❌ 실패

    A->>TO: 실패 분석 요청
    TO->>A: 피드백 생성

    A->>W: 피드백 + 재구현 요청
    W->>A: 수정 완료
    A->>A: 자동 검증 ✅ 통과

    A->>A: 다음 Checkpoint로...
```

### 실패 → 에스컬레이션 플로우

```mermaid
sequenceDiagram
    participant H as 인간
    participant A as 에이전트
    participant W as Worker

    loop 3회 재시도
        A->>W: 구현 요청
        W->>A: 완료
        A->>A: 검증 ❌
        A->>W: 피드백 + 재시도
    end

    A->>H: ⚠️ 에스컬레이션!<br/>설계 문제 가능성

    Note over H,A: 실패 분석 리포트 제공

    H->>A: /afl:architect --resume
    Note over H,A: 설계 재검토...
```

---

## 핵심 원칙

| 원칙 | 설명 |
|------|------|
| **인간은 "무엇"과 "왜"** | 스펙, 아키텍처, 성공 기준 정의 |
| **에이전트는 "어떻게"** | 구현, 테스트, 자동 검증 |
| **명시적 Checkpoint** | 모호한 "완료"를 검증 가능한 기준으로 |
| **자동 피드백 루프** | 실패 시 자동 분석 + 재시도 |
| **적절한 에스컬레이션** | 설계 문제는 인간에게 |
