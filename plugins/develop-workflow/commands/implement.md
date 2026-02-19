---
description: 구현 실행 - Contract 기반으로 Direct/Subagent/Agent Teams 중 최적 전략을 선택하여 구현합니다
argument-hint: "[구현 요청 또는 checkpoint 파일 경로]"
allowed-tools: ["Task", "Glob", "Grep", "Read", "Write", "Edit", "Bash", "AskUserQuestion"]
---

# 구현 커맨드 (/implement)

Contract(Interface + Test Code)를 기반으로 구현을 실행합니다. 태스크 규모와 특성에 따라 최적 전략을 자동 선택합니다.

> `/develop` 워크플로우의 Phase 3을 단독 실행합니다.

## 핵심 원칙

1. **Contract 기반**: Interface와 Test Code가 있으면 활용, 없으면 요구사항에서 추론
2. **상황별 전략**: 태스크 특성에 따라 Direct / Subagent / Agent Teams 자동 선택
3. **RALPH 패턴**: 모든 전략에서 동일한 구현-검증 루프 적용
4. **자동 폴백**: Agent Teams 불가 시 Subagent로, Subagent 불가 시 Direct로

## 전략 선택

```
태스크 분석
    │
    ├─ 단일/소규모 태스크           → Direct
    │   "버튼 색상 변경"
    │   "API 1개 추가"
    │
    ├─ 복수 태스크, 파일 겹침 적음  → Subagent (Task tool 병렬)
    │   "독립 API 3개 각각 추가"
    │   "모듈 A, B, C 리팩토링"
    │
    └─ 대규모, 독립적, 소통 필요    → Agent Teams
        "프론트+백엔드+인프라 동시"
        "5개 이상 독립 모듈 병렬 구현"
```

### 자동 선택 기준

| 조건 | 전략 |
|------|------|
| Checkpoint 1개 | Direct |
| Checkpoint 2-4개, 의존성 없음, 파일 독립 | Subagent |
| Checkpoint 5+개 또는 팀원 간 소통 필요 | Agent Teams |
| 모든 Checkpoint가 동일 파일 수정 | Direct (순차) |
| Agent Teams 환경 미설정 | Subagent로 폴백 |

사용자가 `AskUserQuestion`으로 전략을 오버라이드할 수 있습니다.

---

## 상태 관리

RALPH 루프의 진행 상황을 `.develop-workflow/state.yaml`에 기록합니다.
compaction이나 세션 재개 시 "어디까지 했는지"를 복원하기 위함입니다.

### 상태 기록 규칙

```
Checkpoint 시작
    │ → state.yaml: cp-N status를 in_progress로, iteration을 1로
    │
    ▼
RALPH iteration 실행
    │
    ├── Pass
    │   → state.yaml: cp-N status를 passed로
    │   → 다음 Checkpoint
    │
    └── Fail
        → state.yaml: cp-N iteration 증가
        │
        ├── iteration < max_retries
        │   → 다음 RALPH iteration
        │
        └── iteration >= max_retries
            → state.yaml: cp-N status를 escalated로
            → 사용자에게 에스컬레이션
```

### 기록 예시

Checkpoint 시작 시:
```yaml
# Edit .develop-workflow/state.yaml
checkpoints:
  cp-1: { status: in_progress, iteration: 1 }
```

RALPH 실패 후 재시도:
```yaml
checkpoints:
  cp-1: { status: in_progress, iteration: 2 }
```

Checkpoint 통과:
```yaml
checkpoints:
  cp-1: { status: passed, iteration: 2 }
  cp-2: { status: in_progress, iteration: 1 }   # 다음 CP 시작
```

### 세션 재개 시

```
state.yaml 읽기
    │
    ├── passed 항목 → 건너뜀
    ├── in_progress 항목 → 해당 iteration부터 RALPH 재개
    ├── escalated 항목 → 사용자에게 재확인
    └── pending 항목 → 순서대로 시작
```

---

## RALPH 패턴

`ralph-pattern` Skill을 따릅니다. 각 전략에서 Skill의 루프와 재시도 정책을 적용하세요.

- Pass → state.yaml 갱신 (`passed`) → 다음 Checkpoint
- Fail → state.yaml 갱신 (`iteration++`) → 원인 분석 → R부터 재시작
- 최대 재시도(기본 3회) 초과 → state.yaml에 `escalated` 기록 후 사용자에게 에스컬레이션

---

## Strategy A: Direct 구현

메인 에이전트가 직접 순차 실행합니다.

### 실행 절차

1. **Checkpoint 정보 로드**: Contract, 테스트, 의존성 확인
2. **의존성 순서 정렬**: 의존성 그래프에 따라 실행 순서 결정
3. **각 Checkpoint 실행** (RALPH 패턴):
   - Contract의 Interface 파일 읽기
   - 테스트 코드 읽기 (목표 이해)
   - 기존 코드베이스 패턴 학습
   - 구현 코드 작성
   - 검증 명령어 실행
   - 실패 시 분석 → 수정 → 재검증
4. **전체 검증**: 모든 Checkpoint 통과 확인

### 적합한 경우

- 단순 버그 수정, 소규모 기능 추가
- 파일 간 의존성이 높은 변경
- 같은 파일을 여러 번 수정해야 하는 경우

---

## Strategy B: Subagent 구현

Task tool로 독립 태스크를 병렬 실행합니다.

### 실행 절차

1. **태스크 독립성 검증**:
   - 파일 겹침 분석 (allowed_files / forbidden_files 할당)
   - 공유 파일 식별 (types.ts, index.ts 등)
   - 의존성 순서 파악 (Round 분류)

2. **Round별 병렬 실행**:
   ```
   Round 1: 의존성 없는 Checkpoint들
   ├── Task(prompt="CP-1 구현", run_in_background=true)
   └── Task(prompt="CP-2 구현", run_in_background=true)

   Round 2: Round 1에 의존하는 Checkpoint들
   └── Task(prompt="CP-3 구현", run_in_background=true)
   ```

3. **Subagent 프롬프트 템플릿**:
   ```
   다음 Checkpoint를 구현하세요.

   ## Contract
   {interface_content}

   ## 테스트 코드
   {test_content}

   ## 제약
   - 수정 허용 파일: {allowed_files}
   - 수정 금지 파일: {forbidden_files}

   ## 검증
   구현 후 다음 명령어로 검증하세요:
   {validation_command}

   실패 시 원인을 분석하고 수정하세요 (최대 3회).
   ```

4. **결과 통합**:
   - 모든 Subagent 완료 확인
   - 공유 파일 통합 (메인 에이전트가 처리)
   - 전체 테스트 실행

### 충돌 방지

- **파일 기반 분리**: Checkpoint별 allowed/forbidden 파일 목록
- **공유 파일 처리**: export, type 파일은 마지막에 메인 에이전트가 통합
- **의존성 순서**: Round 기반 실행으로 선행 조건 보장

---

## Strategy C: Agent Teams 구현

Claude Code 공식 Agent Teams 기능을 활용합니다.

### 사전 조건

Agent Teams가 활성화되어 있어야 합니다:
```json
// settings.json
{
  "env": {
    "CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS": "1"
  }
}
```

활성화되지 않은 경우 사용자에게 안내하고 Subagent로 폴백합니다.

### 실행 절차

1. **팀 생성 요청**:
   ```
   에이전트 팀을 만들어 다음 Checkpoint들을 병렬로 구현해주세요.

   ## 팀 구성
   {checkpoint별 팀원 정의}

   ## 각 팀원 지침
   - RALPH 패턴을 따르세요 (Read → Analyze → Learn → Patch → Halt)
   - Contract의 Interface와 Test Code를 먼저 읽으세요
   - 검증 명령어를 실행하여 통과를 확인하세요
   - 실패 시 원인을 분석하고 수정하세요

   ## 조율 규칙
   - 팀원별 계획 승인을 요구합니다
   - 서로 다른 파일을 소유하도록 합니다
   - 완료된 팀원은 리더에게 보고합니다
   ```

2. **팀원 구성** (Checkpoint 기반):
   - 각 팀원에게 1-2개 Checkpoint 할당
   - 의존성이 있는 Checkpoint는 작업 종속성 설정
   - 공유 파일 소유권 명확히 지정

3. **계획 승인**:
   - 각 팀원의 구현 계획을 리더가 검토
   - 파일 충돌 가능성 확인
   - 승인 후 구현 시작

4. **진행 모니터링**:
   - 공유 작업 목록으로 진행 상황 추적
   - 막힌 팀원에게 피드백 전달
   - 필요시 팀원 간 소통 유도

5. **팀 정리**:
   - 모든 Checkpoint 통과 확인
   - 전체 테스트 실행
   - 팀 정리 요청

### Agent Teams의 장점

- 팀원 간 직접 메시지로 협업 가능
- 공유 작업 목록으로 자동 조율
- 계획 승인으로 품질 보장
- tmux/iTerm2로 실시간 모니터링

---

## 검증 및 완료

### Checkpoint별 검증

각 Checkpoint의 `validation.command`를 실행하여 통과 확인.

### 전체 검증

모든 Checkpoint 완료 후 프로젝트 전체 테스트 실행.

### 실패 처리

1. **자동 재시도**: RALPH 패턴으로 최대 3회
2. **Test Oracle**: 실패 분석 에이전트 호출 (복잡한 실패)
3. **에스컬레이션**: 재시도 초과 시 사용자에게 보고

---

## 사용 예시

```bash
# Contract 기반 구현 (전략 자동 선택)
/implement "checkpoints.yaml에 정의된 구현을 실행해줘"

# 전략 지정
/implement "Direct로 인증 모듈 구현"
/implement "Subagent로 API 3개를 병렬 구현"
/implement "Agent Teams로 전체 모듈 병렬 구현"

# 자유 형식
/implement "React 컴포넌트 3개 만들어줘 - UserList, UserDetail, UserForm"
```

## 설정

```yaml
implement:
  strategy: auto              # auto | direct | subagent | agent-teams
  max_retries: 3              # RALPH 최대 재시도
  validate_each: true         # Checkpoint별 검증
  subagent:
    max_parallel: 4           # 동시 Subagent 수
    conflict_check: true      # 파일 충돌 사전 검사
  agent_teams:
    require_plan_approval: true  # 팀원 계획 승인 필수
    teammate_mode: auto          # auto | in-process | tmux
```
