---
description: 비대화형 자동 개발 워크플로우 - 외부 분석 리포트 기반으로 설계 → 리뷰 → 구현 → 머지를 완전 자동 실행합니다
argument-hint: "implement based on analysis: [분석 리포트]"
allowed-tools: ["Task", "Glob", "Grep", "Read", "Write", "Edit", "Bash"]
---

# 비대화형 자동 개발 워크플로우 (/develop-auto)

`claude -p` 비대화형 환경에서 실행되는 완전 자동 개발 파이프라인입니다.
외부에서 제공된 분석 리포트를 요구사항으로 사용하며, 모든 판단을 에이전트가 자율적으로 수행합니다.

```
Phase 1: DESIGN ─→ Phase 2: REVIEW ─→ Phase 3: IMPLEMENT ─→ Phase 4: MERGE
 (자동 설계)         (자동 리뷰)         (자동 구현)             (자동 머지)
```

> **대화형 워크플로우와의 차이**: `/develop`는 HITL(Human-in-the-Loop) 2곳에서 사용자 확인을 받습니다.
> `/develop-auto`는 HITL 없이 완전 자동으로 실행됩니다. 품질은 Multi-LLM 리뷰와 Gate 시스템으로 보장합니다.

## 핵심 원칙

1. **완전 자동**: 사용자 개입 없이 전체 파이프라인 실행. AskUserQuestion 사용 금지
2. **외부 분석 기반**: Consumer가 제공한 분석 리포트를 요구사항으로 직접 사용
3. **Multi-LLM 합의**: 설계와 리뷰에서 Claude + Codex + Gemini 3개 LLM의 관점 활용
4. **상황별 구현 전략**: 태스크 규모에 따라 Direct / Subagent / Agent Teams 자동 선택
5. **git-utils 연동**: 브랜치, PR, 머지를 git-utils 플러그인에 위임
6. **상태 지속성**: `.develop-workflow/state.json`으로 세션 재개 및 compaction 대응
7. **Gate 기반 Phase 차단**: Hook이 `state.json`의 gates를 검증하여 Phase 전환을 물리적으로 제어
8. **Fail-fast**: 자동 재시도 실패 시 에스컬레이션 없이 즉시 중단

---

## HITL 대체 정책

`/develop`의 HITL 지점을 다음과 같이 자동화합니다:

| `/develop` HITL 지점 | `/develop-auto` 대체 | 근거 |
|----------------------|---------------------|------|
| Step 0: 세션 재개 질문 | 항상 새로 시작 (state.json 삭제) | 비대화형에서 재개 판단 불가 |
| Step 1.1: 요구사항 수집 (반복 질문) | Consumer 분석 리포트를 요구사항으로 직접 사용 | 분석 리포트에 요구사항 포함 |
| Step 1.5: 설계 승인 | 자동 승인 (Phase 2 리뷰에서 품질 보장) | Multi-LLM 리뷰가 품질 게이트 역할 |
| Step 3.4: 3회 실패 에스컬레이션 | 실패 기록 후 즉시 abort (exit code 1) | 비대화형에서 사용자 판단 불가 |
| Step 4.1: 최종 리뷰 확인 | clean pass 시 자동 승인 | Gate 시스템이 품질 보장 |

---

## Step 0: 세션 초기화

워크플로우 시작 시 **항상 새 세션**으로 시작합니다.

```
/develop-auto 실행
    │
    ├── .develop-workflow/state.json 존재?
    │   ├── Yes → 삭제 후 새 워크플로우 시작
    │   └── No  → 새 워크플로우 시작
    │
    ▼
Phase 1부터 실행
```

> **재개 없음**: 비대화형 환경에서는 이전 세션을 판단할 수 없으므로 항상 fresh start합니다.

### state.json 형식

```json
{
  "phase": "IMPLEMENT",
  "strategy": "subagent",
  "feature": "실시간 채팅 기능",
  "started_at": "2026-02-16T10:00:00",
  "updated_at": "2026-02-16T11:30:00",
  "gates": {
    "review_clean_pass": true,
    "architect_verified": false,
    "re_review_clean": false
  },
  "checkpoints": {
    "cp-1": { "status": "passed", "iteration": 2 },
    "cp-2": { "status": "in_progress", "iteration": 1 },
    "cp-3": { "status": "pending", "iteration": 0 }
  }
}
```

### Gates (Phase Gate)

`develop-phase-gate.cjs` hook이 PreToolUse 시점에 `state.json`의 gates를 검증합니다.
Gate 조건이 미충족이면 Write/Edit/Bash가 물리적으로 차단됩니다.

| Gate | 설정 시점 | 용도 |
|------|----------|------|
| `review_clean_pass` | Phase 2 리뷰에서 Blocking 이슈 0개 | Phase 3 IMPLEMENT 진입 허용 |
| `architect_verified` | Phase 3 Architect 검증 통과 | Phase 4 MERGE 진입 허용 |
| `re_review_clean` | Phase 4 코드 리뷰 clean pass | PR 생성/push 허용 |

### 상태 기록 시점

state.json은 다음 시점에 **Write/Edit tool로 갱신**합니다:

| 시점 | 기록 내용 |
|------|----------|
| Phase 전환 | `phase` 필드 업데이트 |
| Gate 충족 | `gates.{gate_name}: true` |
| Checkpoint 시작 | 해당 CP `status: in_progress` |
| RALPH iteration 완료 | `iteration` 카운트 증가, `status` 업데이트 |
| Checkpoint 통과 | `status: passed` |
| Checkpoint 실패 (재시도 초과) | `status: failed` → 워크플로우 abort |
| 워크플로우 완료 | `phase: DONE` |

---

## 전체 워크플로우

```
Consumer 분석 리포트
    │
    ▼
┌─────────────────────────────────────────────────────────────┐
│  Phase 1: DESIGN (자동)                                      │
│  ├── 분석 리포트에서 요구사항 추출                            │
│  ├── Multi-LLM 아키텍처 설계 (Claude + Codex + Gemini)      │
│  ├── 컨센서스 빌딩 + ASCII 다이어그램                        │
│  ├── Contract 정의 (Interface + Test Code)                   │
│  └── 자동 승인 → Phase 2로 진행                              │
└─────────────────────┬───────────────────────────────────────┘
                      │ (자동)
                      ▼
┌─────────────────────────────────────────────────────────────┐
│  Phase 2: REVIEW (자동)                                      │
│  ├── Multi-LLM 스펙/설계 리뷰 (3개 LLM 병렬)               │
│  ├── 합의점 / 분쟁점 분석                                    │
│  └── Critical 피드백 → Phase 1.2 자동 재진입                 │
└─────────────────────┬───────────────────────────────────────┘
                      │ (자동)
                      ▼
┌─────────────────────────────────────────────────────────────┐
│  Phase 3: IMPLEMENT (자동)                                   │
│  ├── 태스크 분석 → 전략 자동 선택                            │
│  │   ├── Direct: 단일/소규모 태스크                          │
│  │   ├── Subagent: 복수 독립 태스크 (Task tool)             │
│  │   └── Agent Teams: 대규모 병렬 + 소통 필요               │
│  ├── 구현 실행 (RALPH 패턴, 자동 재시도)                     │
│  └── 자동 검증 (Contract 기반, 3회 실패 시 abort)            │
└─────────────────────┬───────────────────────────────────────┘
                      │ (자동)
                      ▼
┌─────────────────────────────────────────────────────────────┐
│  Phase 4: MERGE (자동)                                       │
│  ├── Multi-LLM 코드 리뷰 → clean pass 시 자동 승인          │
│  ├── git-utils: /commit-and-pr                               │
│  ├── git-utils: /check-ci                                    │
│  └── git-utils: /merge-pr                                    │
└─────────────────────────────────────────────────────────────┘
```

---

## Phase 1: DESIGN

**진입 시 상태 기록**:
```json
// Write .develop-workflow/state.json
{
  "phase": "DESIGN",
  "feature": "{분석 리포트 요약}",
  "started_at": "{현재 시각}",
  "updated_at": "{현재 시각}",
  "gates": {
    "review_clean_pass": false,
    "architect_verified": false,
    "re_review_clean": false
  },
  "checkpoints": {}
}
```

### Step 1.1: 요구사항 추출 (자동)

Consumer가 제공한 분석 리포트에서 요구사항을 추출합니다. **사용자에게 질문하지 않습니다.**

**추출 절차**:
1. 분석 리포트의 summary, affected files, implementation direction, checkpoints, risks를 파싱
2. CLAUDE.md/rules + 코드베이스를 분석하여 맥락 보강
3. 모호한 부분은 **보수적 판단**으로 결정 (안전한 쪽 선택)
4. 추출된 요구사항을 구조화하여 설계 입력으로 전달

**판단 원칙**:
- 분석 리포트에 명시된 사항은 그대로 따름
- 리포트에 없는 비기능 요구사항은 코드베이스 컨벤션에서 추론
- 불확실한 선택지는 보수적/안전한 옵션 선택
- 과도한 추측 금지 — 리포트와 코드에서 확인 가능한 범위만

### Step 1.2: Multi-LLM 아키텍처 설계

요구사항을 기반으로 3개 LLM에 설계 요청을 **병렬** 전달합니다.

```
Task(subagent_type="architect-claude", prompt=DESIGN_PROMPT, run_in_background=true)
Task(subagent_type="architect-codex", prompt=DESIGN_PROMPT, run_in_background=true)
Task(subagent_type="architect-gemini", prompt=DESIGN_PROMPT, run_in_background=true)
```

### Step 1.3: 통합 분석

3개 결과를 취합하여:
- **합의 사항**: 3개 LLM 공통 제안
- **의견 차이**: LLM별 다른 접근
- **최종 권장**: 종합 추천 + ASCII 다이어그램

### Step 1.4: Contract 정의

병렬 구현을 위한 계약을 정의합니다:
- **Interface**: 컴포넌트 간 인터페이스 (함수 시그니처, 타입)
- **Test Code**: Contract를 검증하는 테스트 코드
- **Checkpoints**: 독립 실행 가능한 작업 단위

```yaml
checkpoints:
  - id: "checkpoint-1"
    description: "사용자 인증 모듈"
    interface:
      - "src/auth/types.ts"
      - "src/auth/auth-service.ts"
    tests:
      - "tests/auth/auth-service.test.ts"
    validation:
      command: "npm test -- --testPathPattern=auth"
      expected: "all tests pass"
    dependencies: []

  - id: "checkpoint-2"
    description: "API 엔드포인트"
    dependencies: ["checkpoint-1"]
```

### Step 1.5: 자동 승인

설계 결과를 **자동 승인**하고 Phase 2로 진행합니다.

> **품질 보장**: Phase 2의 Multi-LLM 리뷰가 설계 품질을 검증합니다.
> Blocking 이슈 발견 시 Phase 1.2로 자동 재진입하여 수정됩니다.

---

## Phase 2: REVIEW

`/multi-review` 커맨드로 설계 문서를 검증합니다.

**진입 시 상태 기록**:
```json
// Edit .develop-workflow/state.json → phase 업데이트
{
  "phase": "REVIEW",
  "updated_at": "{현재 시각}",
  "checkpoints": {
    "cp-1": { "status": "pending", "iteration": 0 },
    "cp-2": { "status": "pending", "iteration": 0 }
  }
}
```

### Step 2.1: Multi-LLM 스펙 리뷰

설계 결과물에 대해 3개 LLM 리뷰를 **병렬** 실행합니다.

```
Task(subagent_type="reviewer-claude", prompt=REVIEW_PROMPT, run_in_background=true)
Task(subagent_type="reviewer-codex", prompt=REVIEW_PROMPT, run_in_background=true)
Task(subagent_type="reviewer-gemini", prompt=REVIEW_PROMPT, run_in_background=true)
```

각 리뷰어는 이슈를 다음 3단계로 분류합니다:

| 레벨 | 설명 | 처리 |
|------|------|------|
| **Blocking** | 구현 불가 / 보안 / 성능 심각 | Phase 3 진입 차단 |
| **Warning** | 개선 권장하지만 구현 가능 | 에이전트가 자동 판단 |
| **Info** | 참고 사항 | 기록만 |

### Step 2.2: 컨센서스 분석

- **3/3 합의**: 높은 신뢰도 → 반드시 반영
- **2/3 동의**: 중간 신뢰도 → 반영 (에이전트 판단)
- **1/3 지적**: 낮은 신뢰도 → 참고만, 반영하지 않음

### Step 2.3: 자동 피드백 루프

```
리뷰 결과
    │
    ├── Blocking 이슈 있음 → Phase 1.2로 돌아가서 설계 자동 수정
    │   │                    (최대 2회 반복)
    │   │
    │   └── 수정 후 Step 2.1 재실행 (리뷰 재실행)
    │
    ├── Warning만 있음 → 에이전트가 자동 판단하여 반영/무시
    │
    └── Blocking 0개 (clean pass)
        │
        ▼
    ✅ gates.review_clean_pass = true → Phase 3 진입 허용
```

**Gate 설정**: Blocking 이슈가 0개일 때만 `review_clean_pass` gate를 `true`로 설정합니다.

```json
// Edit .develop-workflow/state.json → gate 업데이트
{ "gates": { "review_clean_pass": true } }
```

---

## Phase 3: IMPLEMENT

`/implement` 커맨드로 구현을 실행합니다.

**진입 시 상태 기록**:
```json
// Edit .develop-workflow/state.json → phase + strategy 업데이트
{
  "phase": "IMPLEMENT",
  "strategy": "{선택된 전략}",
  "updated_at": "{현재 시각}"
}
```

> **Gate 검증**: `review_clean_pass`가 `true`여야 Phase 3에서 파일 수정 가능.

### Step 3.1: 브랜치 생성

git-utils `/git-branch` 활용:

```bash
/git-branch feat/<feature-name>
```

### Step 3.2: 태스크 분석 및 전략 선택

Checkpoint 목록을 분석하여 구현 전략을 자동 결정합니다:

| 조건 | 전략 |
|------|------|
| Checkpoint 1개 | Direct |
| Checkpoint 2-4개, 의존성 없음, 파일 독립 | Subagent |
| Checkpoint 5+개 또는 팀원 간 소통 필요 | Agent Teams |
| 모든 Checkpoint가 동일 파일 수정 | Direct (순차) |

### Step 3.3a: Direct 구현

메인 에이전트가 직접 Checkpoint를 순차 실행합니다.

각 Checkpoint에 대해 RALPH 패턴 적용:
1. **R**ead: Contract와 테스트 코드 읽기
2. **A**nalyze: 요구사항 분석
3. **L**earn: 기존 코드베이스 패턴 학습
4. **P**atch: 구현 코드 작성
5. **H**alt: 검증 명령어 실행 → Pass면 다음, Fail이면 R부터 반복

### Step 3.3b: Subagent 구현

Task tool로 독립 태스크를 병렬 실행합니다.

```
# 의존성 없는 Checkpoint들을 병렬 실행
Task(prompt="Checkpoint 1 구현: ...", run_in_background=true)
Task(prompt="Checkpoint 2 구현: ...", run_in_background=true)

# 의존성 있는 Checkpoint는 선행 완료 후 실행
Task(prompt="Checkpoint 3 구현: ...", run_in_background=true)  # depends on 1,2
```

### Step 3.3c: Agent Teams 구현

Claude Code 공식 Agent Teams 기능을 활용합니다.
환경변수 미설정 시 Subagent로 자동 폴백합니다.

### Step 3.4: 자동 검증 (Fail-fast)

모든 Checkpoint 구현 완료 후:
1. 전체 테스트 실행
2. 실패 시 Test Oracle 에이전트로 분석 → 피드백 → 자동 재시도
3. 최대 3회 자동 재시도
4. **3회 실패 시 워크플로우 abort** (exit code 1로 종료)

```
검증 결과
    │
    ├── Pass → Step 3.5로 진행
    │
    └── Fail
        │
        ├── 재시도 < 3회 → Test Oracle 분석 → 자동 수정 → 재검증
        │
        └── 재시도 >= 3회
            │
            ▼
        ❌ state.json에 실패 기록 → 워크플로우 abort
           Consumer가 exit code로 실패 감지
```

> **에스컬레이션 없음**: `/develop`와 달리 사용자에게 묻지 않고 즉시 실패 처리합니다.
> Consumer가 exit code와 stdout/stderr로 실패를 감지하고 후속 처리합니다.

### Step 3.5: Architect 검증 + Gate 설정

전체 검증 통과 후 `architect_verified` gate를 설정합니다:

```json
// Edit .develop-workflow/state.json → gate 업데이트
{ "gates": { "architect_verified": true } }
```

---

## Phase 4: MERGE

**진입 시 상태 기록**:
```json
// Edit .develop-workflow/state.json → phase 업데이트
{
  "phase": "MERGE",
  "updated_at": "{현재 시각}"
}
```

### Step 4.1: Multi-LLM 코드 리뷰 + 자동 승인

구현 결과물에 대해 `/multi-review`로 코드 리뷰 실행:

```
/multi-review "구현된 코드를 엔지니어 관점으로 리뷰해줘"
```

```
리뷰 결과
    │
    ├── Blocking 이슈 있음
    │   │
    │   ▼
    │   자동 수정 시도 (RALPH 패턴)
    │   │
    │   └── 수정 후 Step 4.1 재실행 (리뷰 재실행, 최대 2회)
    │
    └── Blocking 0개 (clean pass)
        │
        ▼
    ✅ gates.re_review_clean = true → 자동 승인 → Step 4.2로 진행
```

> **HITL 없음**: clean pass 시 자동으로 머지 프로세스를 진행합니다.

```json
// Edit .develop-workflow/state.json → gate 업데이트
{ "gates": { "re_review_clean": true } }
```

### Step 4.2: Commit & PR

git-utils 활용:

```bash
/commit-and-pr
```

> **Gate 필수**: `re_review_clean`이 `true`여야 커밋/PR 생성이 가능합니다.

### Step 4.3: CI 확인

```bash
/check-ci
```

CI 실패 시:
1. 실패 원인 분석
2. 수정 후 재커밋
3. 최대 3회 재시도
4. 3회 실패 시 워크플로우 abort

### Step 4.4: 최종 확인

```bash
/unresolved-reviews
/merge-pr
```

### Step 4.5: 워크플로우 완료

```json
// Edit .develop-workflow/state.json → 완료 기록
{
  "phase": "DONE",
  "updated_at": "{현재 시각}"
}
```

---

## 사용 예시

```bash
# Consumer에서 호출 (분석 리포트 포함)
/develop-auto implement based on analysis:

{
  "summary": "실시간 채팅 기능 추가",
  "affected_files": ["src/chat/", "src/api/routes.ts"],
  "implementation_direction": "WebSocket 기반 실시간 통신",
  "checkpoints": ["인증 모듈", "WebSocket 서버", "UI 컴포넌트"],
  "risks": ["동시접속 부하", "메시지 순서 보장"]
}

This is for issue #42 in my-project.
```

## 설정

`/develop`와 동일한 `.develop-workflow.yaml` 설정을 공유합니다.

```yaml
# .develop-workflow.yaml (프로젝트 루트)
develop:
  design:
    multi_llm: true
  review:
    multi_llm: true
    auto_feedback: true
    max_iterations: 2
  implement:
    strategy: auto
    max_retries: 3
    validate_each: true
  merge:
    code_review: true
    auto_ci_check: true
```

## git-utils 의존성

| Phase | git-utils 커맨드 | 용도 |
|-------|-----------------|------|
| Phase 3 | `/git-branch` | feature 브랜치 생성 |
| Phase 3 | `/branch-status` | 브랜치 상태 확인 |
| Phase 4 | `/commit-and-pr` | 커밋 + PR 생성 |
| Phase 4 | `/check-ci` | CI 결과 확인 |
| Phase 4 | `/unresolved-reviews` | 미해결 리뷰 확인 |
| Phase 4 | `/merge-pr` | PR 머지 |
| Phase 4 | `/git-resolve` | 충돌 해결 |
