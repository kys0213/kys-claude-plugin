# Automated Team Workflow

> oh-my-claudecode 스타일의 자동화된 팀 워크플로우

## Overview

기존 워크플로우를 자동화하여 HITL(Human-In-The-Loop) 개입을 최소화합니다.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  기존 워크플로우 (10단계)           →    자동화 워크플로우 (5단계)           │
├─────────────────────────────────────────────────────────────────────────────┤
│  1. LLM과 초벌 스펙                      1. 스펙 설계                        │
│  2. 상세 스펙 잡기                          (자동 리뷰 루프 포함)             │
│  3. 스펙 리뷰                                                                │
│  4. 2~3 반복                            2. 스펙 리뷰 (HITL)                  │
│  5. 합의된 스펙 리뷰 (HITL)                                                  │
│  6. 구현 (worktree 분할)                3. 스펙 구현                         │
│  7. 코드 리뷰                              (worktree, 병렬 에이전트)          │
│  8. 코드 리뷰 개선                                                           │
│  9. 7~8 반복                            4. 코드 리뷰 (HITL)                  │
│  10. 합의된 코드 리뷰 (HITL)                                                 │
│  11. 머지                               5. 머지 또는 3~4 반복                │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Core Features

### 1. /team-claude:flow - 통합 워크플로우

```bash
# 전체 자동화 (autopilot 모드)
/team-claude:flow "기능 설명" --mode autopilot

# 스펙 단계만
/team-claude:flow "기능 설명" --mode spec

# 구현 단계만 (기존 세션 사용)
/team-claude:flow --session abc123 --mode impl

# 스펙 + 구현 (리뷰는 HITL)
/team-claude:flow "기능 설명" --mode assisted
```

### 2. PSM (Parallel Session Manager)

git worktree 기반 병렬 세션 관리:

```bash
# 새 세션 생성 (worktree 자동 생성)
psm new "feature-name"

# 세션 목록
psm list

# 세션 상태 확인
psm status

# 특정 세션으로 전환
psm switch feature-name

# 병렬 실행
psm parallel session1 session2 session3

# 세션 정리
psm cleanup [session-name]
```

### 3. Magic Keywords

메시지 시작에 키워드를 붙여 실행 모드 제어:

| Keyword | 설명 | 예시 |
|---------|------|------|
| `autopilot:` | 전체 자동화 | `autopilot: 쿠폰 기능 추가` |
| `spec:` | 스펙 단계만 | `spec: 결제 시스템 설계` |
| `impl:` | 구현 단계만 | `impl: --session abc123` |
| `review:` | 리뷰만 수행 | `review: 코드 품질 검토` |
| `parallel:` | 병렬 실행 | `parallel: task1, task2, task3` |
| `ralph:` | 자율 루프 | `ralph: 테스트 통과까지 반복` |

### 4. Auto-Review Loop

스펙과 코드에 대한 자동 리뷰 루프:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  Auto-Review Loop                                                            │
│                                                                              │
│    ┌──────────┐     ┌──────────┐     ┌──────────┐     ┌──────────┐         │
│    │  작성    │ ──▶ │  리뷰    │ ──▶ │  피드백  │ ──▶ │  수정    │ ──┐     │
│    └──────────┘     └──────────┘     └──────────┘     └──────────┘   │     │
│         ▲                                                             │     │
│         └─────────────────────────────────────────────────────────────┘     │
│                                                                              │
│    종료 조건:                                                               │
│    • 리뷰 통과 (no issues)                                                  │
│    • 최대 반복 횟수 도달                                                    │
│    • 사용자 개입 요청                                                       │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Architecture

```
plugins/team-claude/
├── commands/
│   ├── flow.md                    # 통합 워크플로우 명령
│   └── psm.md                     # PSM 명령
│
├── scripts/
│   ├── tc-psm.sh                  # PSM CLI
│   ├── tc-flow.sh                 # Flow orchestrator
│   └── tc-review.sh               # Auto-review runner
│
├── agents/
│   ├── spec-reviewer.md           # 스펙 자동 리뷰어
│   ├── code-reviewer.md           # 코드 자동 리뷰어
│   └── flow-orchestrator.md       # 워크플로우 오케스트레이터
│
├── skills/
│   └── auto-review/
│       └── SKILL.md               # 자동 리뷰 스킬
│
└── docs/
    └── AUTOMATED-WORKFLOW.md      # 이 문서
```

## Workflow States

```
                                    ┌─────────────────────────────────────┐
                                    │         AUTOMATED WORKFLOW          │
                                    └─────────────────────────────────────┘
                                                     │
                    ┌────────────────────────────────┼────────────────────────────────┐
                    │                                │                                │
                    ▼                                ▼                                ▼
            ┌───────────────┐              ┌───────────────┐              ┌───────────────┐
            │   autopilot   │              │   assisted    │              │   manual      │
            │               │              │               │              │               │
            │ 전체 자동화   │              │ 단계별 확인   │              │ 기존 방식     │
            └───────────────┘              └───────────────┘              └───────────────┘
                    │                                │                                │
                    ▼                                ▼                                ▼
┌─────────────────────────────────────────────────────────────────────────────────────────────┐
│                                     SPEC PHASE                                               │
│  ┌─────────┐    ┌─────────┐    ┌─────────┐    ┌─────────┐                                   │
│  │ analyze │ ─▶ │ design  │ ─▶ │ review  │ ─▶ │ approve │  ◀── HITL (assisted/manual)      │
│  └─────────┘    └─────────┘    └─────────┘    └─────────┘                                   │
│                                     │                                                        │
│                              ┌──────┴──────┐                                                │
│                              │ auto-review │  ◀── 자동 반복 (autopilot/assisted)            │
│                              │    loop     │                                                │
│                              └─────────────┘                                                │
└─────────────────────────────────────────────────────────────────────────────────────────────┘
                                        │
                                        ▼
┌─────────────────────────────────────────────────────────────────────────────────────────────┐
│                                     IMPL PHASE                                               │
│  ┌─────────┐    ┌─────────┐    ┌─────────┐    ┌─────────┐                                   │
│  │ prepare │ ─▶ │ execute │ ─▶ │ review  │ ─▶ │ approve │  ◀── HITL (assisted/manual)      │
│  │worktree │    │ (RALPH) │    │  code   │    │         │                                   │
│  └─────────┘    └─────────┘    └─────────┘    └─────────┘                                   │
│       │              │              │                                                        │
│       ▼              ▼              ▼                                                        │
│  ┌─────────┐    ┌─────────┐    ┌─────────┐                                                  │
│  │ parallel│    │  auto   │    │  auto   │  ◀── 자동 반복 (autopilot/assisted)              │
│  │ workers │    │ feedback│    │ review  │                                                  │
│  └─────────┘    └─────────┘    └─────────┘                                                  │
└─────────────────────────────────────────────────────────────────────────────────────────────┘
                                        │
                                        ▼
┌─────────────────────────────────────────────────────────────────────────────────────────────┐
│                                     MERGE PHASE                                              │
│  ┌─────────┐    ┌─────────┐    ┌─────────┐                                                  │
│  │ collect │ ─▶ │  merge  │ ─▶ │  final  │                                                  │
│  │   PRs   │    │ (auto)  │    │   PR    │                                                  │
│  └─────────┘    └─────────┘    └─────────┘                                                  │
│                      │                                                                       │
│               ┌──────┴──────┐                                                               │
│               │  conflict   │  ◀── 충돌 시만 HITL                                           │
│               │  resolver   │                                                               │
│               └─────────────┘                                                               │
└─────────────────────────────────────────────────────────────────────────────────────────────┘
```

## Configuration

```yaml
# .claude/team-claude.yaml
flow:
  # 기본 실행 모드
  defaultMode: assisted   # autopilot | assisted | manual

  # 자동 리뷰 설정
  autoReview:
    enabled: true
    maxIterations: 5
    # 리뷰어 에이전트
    specReviewer: spec-reviewer
    codeReviewer: code-reviewer

  # PSM 설정
  psm:
    parallelLimit: 4      # 최대 병렬 세션 수
    autoCleanup: true     # 완료 후 자동 정리

  # Magic Keywords
  keywords:
    enabled: true
    aliases:
      auto: autopilot
      ap: autopilot
      sp: spec
      im: impl
      rv: review
      pl: parallel
      rl: ralph

# 실행 모드별 설정
modes:
  autopilot:
    specReview: auto
    codeReview: auto
    merge: auto
    escalateOn:
      - conflict
      - maxIterations

  assisted:
    specReview: auto
    codeReview: auto
    merge: hitl
    escalateOn:
      - phaseComplete
      - conflict

  manual:
    specReview: hitl
    codeReview: hitl
    merge: hitl
```

## Usage Examples

### Example 1: Full Autopilot

```bash
# 전체 자동화로 기능 구현
/team-claude:flow "결제 시스템에 쿠폰 기능 추가" --mode autopilot

# 또는 magic keyword 사용
autopilot: 결제 시스템에 쿠폰 기능 추가
```

### Example 2: Assisted Mode

```bash
# 단계별 확인하며 진행
/team-claude:flow "알림 시스템 리팩토링" --mode assisted

# 스펙 승인 후 자동 구현
→ 스펙 자동 리뷰 완료, 검토해주세요
→ [사용자 승인]
→ 구현 시작 (3개 병렬 워커)
→ 코드 자동 리뷰 완료, 검토해주세요
→ [사용자 승인]
→ 머지 완료
```

### Example 3: Parallel Sessions

```bash
# 여러 기능 병렬 개발
psm new coupon-feature
psm new notification-system
psm new user-profile

# 병렬 실행
psm parallel coupon-feature notification-system user-profile

# 상태 모니터링
psm status
```

### Example 4: Magic Keywords

```bash
# 스펙만 설계
spec: 쿠폰 할인 기능 설계

# 기존 세션으로 구현
impl: --session abc123

# 병렬 태스크 실행
parallel: coupon-model, coupon-service, coupon-api

# 자율 반복 모드
ralph: 모든 테스트 통과할 때까지 수정
```

## API Reference

### Flow Command

```typescript
interface FlowOptions {
  mode: 'autopilot' | 'assisted' | 'manual';
  session?: string;          // 기존 세션 재개
  checkpoints?: string[];    // 특정 체크포인트만
  parallel?: boolean;        // 병렬 실행 활성화
  maxIterations?: number;    // 최대 반복 횟수
  dryRun?: boolean;          // 시뮬레이션만
}

interface FlowResult {
  sessionId: string;
  mode: string;
  phases: {
    spec: PhaseResult;
    impl: PhaseResult;
    merge: PhaseResult;
  };
  totalDuration: number;
  success: boolean;
}
```

### PSM Commands

```bash
psm new <name> [--from <session>]   # 새 세션
psm list [--status <status>]        # 목록
psm status [session]                # 상태
psm switch <session>                # 전환
psm parallel <sessions...>          # 병렬 실행
psm cleanup [session]               # 정리
psm export <session> <path>         # 내보내기
```

### Magic Keywords

```typescript
interface KeywordHandler {
  keyword: string;
  aliases: string[];
  handler: (args: string) => Promise<void>;
  description: string;
}
```

## Integration with Existing Commands

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  기존 명령어와의 관계                                                        │
│                                                                              │
│  /team-claude:flow                                                          │
│        │                                                                    │
│        ├──▶ /team-claude:architect (스펙 단계)                             │
│        │         └──▶ auto-review loop                                     │
│        │                                                                    │
│        ├──▶ /team-claude:delegate (구현 단계)                              │
│        │         └──▶ RALPH loop + auto-review                            │
│        │                                                                    │
│        └──▶ /team-claude:merge (머지 단계)                                 │
│                  └──▶ conflict-resolver                                    │
│                                                                              │
│  PSM                                                                        │
│        │                                                                    │
│        └──▶ tc-worktree.sh (git worktree 관리)                             │
│        └──▶ tc-session.sh (세션 관리)                                      │
│        └──▶ tc-server.sh (태스크 서버)                                     │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Roadmap

- [x] 기본 아키텍처 설계
- [ ] /team-claude:flow 명령어 구현
- [ ] PSM 스크립트 구현
- [ ] Magic Keywords 처리
- [ ] Auto-review 에이전트
- [ ] 테스트 및 문서화
