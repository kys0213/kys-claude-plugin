# Team Claude Plugin

멀티 에이전트 협업 시스템 - Claude Code 플러그인으로 구현하는 병렬 개발 파이프라인

## 핵심 가치

| 가치 | 설명 |
|------|------|
| **컨텍스트 엔지니어링** | 단순 프롬프트가 아닌, 지속적인 맥락 공유와 피드백 루프 |
| **적절한 개입** | 모호한 부분은 사람이 판단, 명확한 부분은 AI가 실행 |
| **병렬 실행** | Contract 기반으로 독립적인 Task를 동시에 진행 |
| **시각적 확인** | Worker 진행 상황을 터미널에서 실시간 확인 |

## Commands

| Command | 설명 |
|---------|------|
| `/team-claude:init` | 프로젝트 초기 설정 |
| `/team-claude:setup` | 설정 변경 위자드 |
| `/team-claude:config` | 개별 설정 조회/수정 |
| `/team-claude:agent` | 에이전트 관리 (추가/활성화/커스터마이징) |
| `/team-claude:plan` | 요구사항 → 스펙 정제 |
| `/team-claude:spawn` | Worker 생성 및 실행 |
| `/team-claude:status` | Worker 상태 조회 |
| `/team-claude:review` | 완료된 Task 리뷰 |
| `/team-claude:feedback` | Worker에 피드백 전달 |
| `/team-claude:merge` | PR 머지 |
| `/team-claude:cleanup` | Worktree 정리 |

## 에이전트 계층 구조

에이전트는 `.claude` 파일처럼 계층화된 구조로 관리됩니다:

```
┌─────────────────────────────────────────────────────────────┐
│                    에이전트 해석 순서                         │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  1. 프로젝트 로컬 (최우선)                                   │
│     .team-claude/agents/my-custom-agent.md                 │
│                                                             │
│  2. 플러그인 기본                                            │
│     plugins/team-claude/agents/code-reviewer.md            │
│                                                             │
│  동일 이름 → 로컬이 플러그인 기본을 오버라이드               │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

### 커스텀 에이전트 추가

```bash
# 새 에이전트 생성 (대화형)
/team-claude:agent add payment-expert

# 기본 에이전트 커스터마이징 (로컬 복사)
/team-claude:agent customize code-reviewer

# 에이전트 활성화/비활성화
/team-claude:agent enable domain-expert
/team-claude:agent disable security-auditor

# 에이전트 목록
/team-claude:agent list
```

## 워크플로우

```
┌─────────────────────────────────────────────────────────────┐
│                         사람 (Architect)                     │
│  • 아키텍처 설계 • 모호한 부분 판단 • 최종 리뷰 승인           │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                    Main Claude (Orchestrator)                │
│  • 요구사항 → 스펙 구조화 • Task 분해 • 결과 리뷰            │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                    Worker Claude (Executor)                  │
│  • Contract 기반 구현 • 테스트 작성 • 완료 보고              │
└─────────────────────────────────────────────────────────────┘
```

## 사전 요구사항

- Git worktree 지원
- iTerm2 / tmux (터미널 분할용)
- macOS (알림용, 선택사항)

## 빠른 시작

```bash
# 1. 프로젝트 초기화
/team-claude:init

# 2. 요구사항 정제 및 Task 분해
/team-claude:plan "결제 시스템에 쿠폰 할인 기능 추가"

# 3. Worker 병렬 실행
/team-claude:spawn task-a task-b

# 4. 상태 확인
/team-claude:status

# 5. 리뷰 및 머지
/team-claude:review task-a
/team-claude:merge task-a
```
