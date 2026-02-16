---
description: 통합 개발 워크플로우 - 설계 → 리뷰 → 구현 → 머지를 하나의 파이프라인으로 실행합니다
argument-hint: "[기능 설명 또는 요구사항]"
allowed-tools: ["Task", "Glob", "Grep", "Read", "Write", "Edit", "Bash", "AskUserQuestion"]
---

# 통합 개발 워크플로우 (/develop)

기능 개발의 전체 라이프사이클을 하나의 파이프라인으로 실행합니다.

```
Phase 1: DESIGN ─→ Phase 2: REVIEW ─→ Phase 3: IMPLEMENT ─→ Phase 4: MERGE
 (설계)              (리뷰)              (구현)                (머지)
```

## 핵심 원칙

1. **Human decides What & Why, Agent decides How**: 인간이 "무엇을"과 "왜"를 결정하고, 에이전트가 "어떻게"를 제안
2. **Multi-LLM 합의**: 설계와 리뷰에서 Claude + Codex + Gemini 3개 LLM의 관점 활용
3. **상황별 구현 전략**: 태스크 규모에 따라 Direct / Subagent / Agent Teams 자동 선택
4. **git-utils 연동**: 브랜치, PR, 머지를 git-utils 플러그인에 위임
5. **상태 지속성**: `.develop-workflow/state.yaml`로 세션 재개 및 compaction 대응

---

## Step 0: 세션 재개 확인

워크플로우 시작 시 **반드시** `.develop-workflow/state.yaml` 존재 여부를 확인합니다.

```
/develop 실행
    │
    ├── .develop-workflow/state.yaml 존재?
    │   │
    │   ├── Yes → 상태 읽기 → 사용자에게 보고
    │   │         "이전 세션이 있습니다: Phase IMPLEMENT, cp-2 진행 중 (iteration 1/3)"
    │   │         AskUserQuestion: "이어서 진행할까요?"
    │   │         ├── 이어서 → 해당 Phase/Checkpoint부터 재개
    │   │         └── 처음부터 → state.yaml 삭제 후 새로 시작
    │   │
    │   └── No → 새 워크플로우 시작
    │
    ▼
Phase 1부터 (또는 재개 지점부터) 실행
```

### state.yaml 형식

```yaml
# .develop-workflow/state.yaml
phase: IMPLEMENT                # DESIGN | REVIEW | IMPLEMENT | MERGE | DONE
strategy: subagent              # direct | subagent | agent-teams (Phase 3 진입 시 기록)
feature: "실시간 채팅 기능"       # 사용자 요청 요약
started_at: "2026-02-16T10:00:00"
updated_at: "2026-02-16T11:30:00"

checkpoints:
  cp-1: { status: passed, iteration: 2 }
  cp-2: { status: in_progress, iteration: 1 }
  cp-3: { status: pending, iteration: 0 }
```

### 상태 기록 시점

state.yaml은 다음 시점에 **Write tool로 갱신**합니다:

| 시점 | 기록 내용 |
|------|----------|
| Phase 전환 | `phase` 필드 업데이트 |
| Checkpoint 시작 | 해당 CP `status: in_progress` |
| RALPH iteration 완료 | `iteration` 카운트 증가, `status` 업데이트 |
| Checkpoint 통과 | `status: passed` |
| Checkpoint 실패 (재시도 초과) | `status: escalated` |
| 워크플로우 완료 | `phase: DONE` |

> **주의**: state.yaml 기록은 가볍게 유지합니다. 피드백 상세나 설계 내용은 기록하지 않습니다.
> 실패 원인은 다시 분석하면 되므로 "어디까지 했는지"만 추적합니다.

---

## 전체 워크플로우

```
사용자 요청
    │
    ▼
┌─────────────────────────────────────────────────────────────┐
│  Phase 1: DESIGN                                            │
│  ├── 요구사항 수집 (HITL)                                   │
│  ├── Multi-LLM 아키텍처 설계 (Claude + Codex + Gemini)     │
│  ├── 컨센서스 빌딩 + ASCII 다이어그램                       │
│  └── Contract 정의 (Interface + Test Code)                  │
└─────────────────────┬───────────────────────────────────────┘
                      │
                      ▼
┌─────────────────────────────────────────────────────────────┐
│  Phase 2: REVIEW                                            │
│  ├── Multi-LLM 스펙/설계 리뷰 (3개 LLM 병렬)              │
│  ├── 합의점 / 분쟁점 분석                                   │
│  └── 피드백 → Phase 1 재진입 (필요시)                       │
└─────────────────────┬───────────────────────────────────────┘
                      │
                      ▼
┌─────────────────────────────────────────────────────────────┐
│  Phase 3: IMPLEMENT                                         │
│  ├── 태스크 분석 → 전략 선택                                │
│  │   ├── Direct: 단일/소규모 태스크                         │
│  │   ├── Subagent: 복수 독립 태스크 (Task tool)            │
│  │   └── Agent Teams: 대규모 병렬 + 소통 필요              │
│  ├── 구현 실행 (RALPH 패턴)                                 │
│  └── 자동 검증 (Contract 기반)                              │
└─────────────────────┬───────────────────────────────────────┘
                      │
                      ▼
┌─────────────────────────────────────────────────────────────┐
│  Phase 4: MERGE                                             │
│  ├── Multi-LLM 코드 리뷰                                   │
│  ├── git-utils: /commit-and-pr                              │
│  ├── git-utils: /check-ci                                   │
│  └── git-utils: /merge-pr                                   │
└─────────────────────────────────────────────────────────────┘
```

---

## Phase 1: DESIGN

`/design` 커맨드의 전체 프로세스를 실행합니다.

**진입 시 상태 기록**:
```yaml
# Write .develop-workflow/state.yaml
phase: DESIGN
feature: "{사용자 요청 요약}"
started_at: "{현재 시각}"
updated_at: "{현재 시각}"
checkpoints: {}
```

### Step 1.1: 요구사항 수집

사용자 요청을 분석하고, 모호하거나 부정확한 부분이 있으면 `AskUserQuestion`으로 명확화합니다.

**수집 항목**:
- 기능 요구사항 (핵심 기능, 사용자 시나리오)
- 비기능 요구사항 (성능, 확장성, 보안)
- 제약조건 (기술 스택, 팀 규모, 일정)
- 우선순위 (Must-have vs Nice-to-have)

**질문 원칙**:
1. 맥락에 맞게: 사용자가 언급한 내용 기반으로 필요한 것만 질문
2. 최소한으로: 설계에 꼭 필요한 정보만 수집 (1-2개 질문)
3. 선택지 제공: 열린 질문보다 구체적 옵션 제공
4. 충분하면 진행: 핵심 정보가 모이면 바로 다음 단계로

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

### Step 1.5: 사용자 확인

설계 결과와 Checkpoint를 `AskUserQuestion`으로 사용자에게 확인받습니다.

**승인 시**: Phase 2로 진행
**수정 요청 시**: Step 1.2부터 반복 (수정 사항 반영)

---

## Phase 2: REVIEW

`/multi-review` 커맨드로 설계 문서를 검증합니다.

**진입 시 상태 기록**:
```yaml
# Edit .develop-workflow/state.yaml → phase 업데이트
phase: REVIEW
updated_at: "{현재 시각}"
checkpoints:              # Phase 1에서 정의된 Checkpoint들 초기화
  cp-1: { status: pending, iteration: 0 }
  cp-2: { status: pending, iteration: 0 }
```

### Step 2.1: Multi-LLM 스펙 리뷰

설계 결과물에 대해 3개 LLM 리뷰를 **병렬** 실행합니다.

```
Task(subagent_type="reviewer-claude", prompt=REVIEW_PROMPT, run_in_background=true)
Task(subagent_type="reviewer-codex", prompt=REVIEW_PROMPT, run_in_background=true)
Task(subagent_type="reviewer-gemini", prompt=REVIEW_PROMPT, run_in_background=true)
```

### Step 2.2: 컨센서스 분석

- **3/3 합의**: 높은 신뢰도 → 반드시 반영
- **2/3 동의**: 중간 신뢰도 → 사용자에게 제시
- **1/3 지적**: 참고 사항 → 정보 제공

### Step 2.3: 피드백 루프

Critical 이슈가 있으면:
1. 사용자에게 이슈 보고
2. `AskUserQuestion`으로 수정 방향 확인
3. Phase 1.2로 돌아가서 설계 수정

Critical 이슈가 없으면: Phase 3로 진행

---

## Phase 3: IMPLEMENT

`/implement` 커맨드로 구현을 실행합니다.

**진입 시 상태 기록**:
```yaml
# Edit .develop-workflow/state.yaml → phase + strategy 업데이트
phase: IMPLEMENT
strategy: "{선택된 전략}"
updated_at: "{현재 시각}"
```

> Phase 3에서는 RALPH iteration마다 state.yaml을 갱신합니다.
> 상세 로직은 `/implement` 커맨드의 "상태 관리" 섹션을 참조하세요.

### Step 3.1: 브랜치 생성

git-utils `/git-branch` 활용:

```bash
# feature 브랜치 생성
/git-branch feat/<feature-name>
```

### Step 3.2: 태스크 분석 및 전략 선택

Checkpoint 목록을 분석하여 구현 전략을 결정합니다:

```
┌─────────────────────────────────────────────────────────────┐
│  태스크 분석                                                │
│  ├── Checkpoint 수: N개                                     │
│  ├── 의존성 그래프: 독립 vs 종속                            │
│  ├── 파일 겹침: 있음 vs 없음                                │
│  └── 소통 필요: 있음 vs 없음                                │
└────────────────────────┬────────────────────────────────────┘
                         │
         ┌───────────────┼───────────────┐
         ▼               ▼               ▼
    ┌─────────┐    ┌──────────┐    ┌──────────────┐
    │ Direct  │    │ Subagent │    │ Agent Teams  │
    │         │    │          │    │              │
    │ • 1개   │    │ • 2-4개  │    │ • 5+개       │
    │ • 간단  │    │ • 독립적 │    │ • 소통 필요  │
    │ • 겹침O │    │ • 겹침X  │    │ • 대규모     │
    └─────────┘    └──────────┘    └──────────────┘
```

**자동 선택 기준**:

| 조건 | 전략 |
|------|------|
| Checkpoint 1개 | Direct |
| Checkpoint 2-4개, 의존성 없음, 파일 독립 | Subagent |
| Checkpoint 5+개 또는 팀원 간 소통 필요 | Agent Teams |
| 모든 Checkpoint가 동일 파일 수정 | Direct (순차) |

사용자가 `AskUserQuestion`으로 전략을 오버라이드할 수 있습니다.

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

각 Subagent에게 RALPH 패턴과 Contract(Interface + Test)를 프롬프트로 주입합니다.

**충돌 방지**:
- 각 Subagent에 `allowed_files` / `forbidden_files` 지정
- 공유 파일(index.ts, types.ts)은 마지막에 메인 에이전트가 통합

### Step 3.3c: Agent Teams 구현

Claude Code 공식 Agent Teams 기능을 활용합니다.

**팀 생성 요청**:
```
에이전트 팀을 만들어 다음 Checkpoint들을 병렬로 구현해주세요.

팀원 구성:
- [Checkpoint 1 담당] {contract, tests, allowed_files}
- [Checkpoint 2 담당] {contract, tests, allowed_files}
- ...

각 팀원은 다음 RALPH 패턴을 따르세요:
1. Contract과 테스트 코드를 먼저 읽으세요
2. 기존 코드베이스 패턴을 학습하세요
3. 구현 코드를 작성하세요
4. 검증 명령어를 실행하세요
5. 실패하면 분석하고 수정하세요

팀원별 계획 승인을 요구합니다.
서로 다른 파일을 소유하도록 합니다.
```

**Agent Teams 설정 필요**:
- `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1` 환경변수
- 또는 settings.json에 설정

**Agent Teams를 사용할 수 없는 경우**:
- 환경변수 미설정 시 Subagent로 자동 폴백
- 사용자에게 Agent Teams 활성화 방법 안내

### Step 3.4: 검증

모든 Checkpoint 구현 완료 후:
1. 전체 테스트 실행
2. 실패 시 Test Oracle 에이전트로 분석 → 피드백 → 재시도
3. 최대 3회 재시도 후 사용자에게 에스컬레이션

---

## Phase 4: MERGE

**진입 시 상태 기록**:
```yaml
# Edit .develop-workflow/state.yaml → phase 업데이트
phase: MERGE
updated_at: "{현재 시각}"
```

### Step 4.1: Multi-LLM 코드 리뷰

구현 결과물에 대해 `/multi-review`로 코드 리뷰 실행:

```
/multi-review "구현된 코드를 엔지니어 관점으로 리뷰해줘"
```

### Step 4.2: Commit & PR

git-utils 활용:

```bash
# 커밋 및 PR 생성
/commit-and-pr
```

### Step 4.3: CI 확인

```bash
# CI 상태 확인
/check-ci
```

CI 실패 시:
1. 실패 원인 분석
2. 수정 후 재커밋
3. 최대 3회 재시도

### Step 4.4: 최종 확인

```bash
# 미해결 리뷰 확인
/unresolved-reviews

# 머지
/merge-pr
```

### Step 4.5: 워크플로우 완료

```yaml
# Edit .develop-workflow/state.yaml → 완료 기록
phase: DONE
updated_at: "{현재 시각}"
```

> **정리**: 워크플로우 완료 후 `.develop-workflow/state.yaml`은 유지합니다.
> 다음 `/develop` 실행 시 DONE 상태를 감지하면 "이전 워크플로우가 완료되었습니다. 새로 시작합니다."로 처리합니다.

---

## 사용 예시

```bash
# 기본 사용 (전체 워크플로우)
/develop "실시간 채팅 기능을 추가해줘"

# 제약조건 포함
/develop "React + Node.js로 결제 모듈 구현. PostgreSQL 사용"

# 기존 코드 확장
/develop "현재 인증 시스템에 OAuth2 지원 추가"
```

## Phase 단독 실행

각 Phase를 독립적으로 실행할 수 있습니다:

```bash
/design "채팅 시스템 아키텍처 설계"     # Phase 1만
/multi-review "plans/*.md 리뷰"          # Phase 2만
/implement "checkpoints.yaml 기반 구현"  # Phase 3만
```

## 설정

```yaml
# .develop-workflow.yaml (프로젝트 루트)
develop:
  # Phase 1: Design
  design:
    multi_llm: true           # 3개 LLM 병렬 설계 (false면 Claude만)
    max_questions: 3           # 요구사항 수집 시 최대 질문 수

  # Phase 2: Review
  review:
    multi_llm: true           # 3개 LLM 병렬 리뷰
    auto_feedback: true       # Critical 이슈 자동 피드백
    max_iterations: 2         # 리뷰 ↔ 수정 최대 반복

  # Phase 3: Implement
  implement:
    strategy: auto            # auto | direct | subagent | agent-teams
    max_retries: 3            # RALPH 최대 재시도
    validate_each: true       # Checkpoint별 검증

  # Phase 4: Merge
  merge:
    code_review: true         # Multi-LLM 코드 리뷰
    auto_ci_check: true       # CI 자동 확인
```

## git-utils 의존성

이 워크플로우는 git-utils 플러그인의 다음 커맨드를 활용합니다:

| Phase | git-utils 커맨드 | 용도 |
|-------|-----------------|------|
| Phase 3 | `/git-branch` | feature 브랜치 생성 |
| Phase 3 | `/branch-status` | 브랜치 상태 확인 |
| Phase 4 | `/commit-and-pr` | 커밋 + PR 생성 |
| Phase 4 | `/check-ci` | CI 결과 확인 |
| Phase 4 | `/unresolved-reviews` | 미해결 리뷰 확인 |
| Phase 4 | `/merge-pr` | PR 머지 |
| Phase 4 | `/git-resolve` | 충돌 해결 |

> **주의**: git-utils 플러그인이 설치되어 있어야 합니다.
