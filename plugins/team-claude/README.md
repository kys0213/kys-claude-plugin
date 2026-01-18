# Team Claude Plugin

멀티 에이전트 협업 시스템 - Claude Code 플러그인으로 구현하는 병렬 개발 파이프라인

## 핵심 가치

| 가치 | 설명 |
|------|------|
| **컨텍스트 엔지니어링** | 단순 프롬프트가 아닌, 지속적인 맥락 공유와 피드백 루프 |
| **적절한 개입** | 모호한 부분은 사람이 판단, 명확한 부분은 AI가 실행 |
| **병렬 실행** | Contract 기반으로 독립적인 Task를 동시에 진행 |
| **시각적 확인** | Worker 진행 상황을 터미널에서 실시간 확인 |
| **지속적 개선** | 회고를 통해 에이전트/스킬/문서를 점진적으로 개선 |

## Commands

| Command | 설명 |
|---------|------|
| `/team-claude:init` | 프로젝트 초기 설정 |
| `/team-claude:setup` | 설정 변경 위자드 |
| `/team-claude:config` | 개별 설정 조회/수정 |
| `/team-claude:agent` | 에이전트 관리 (추가/활성화/커스터마이징) |
| `/team-claude:plan` | 요구사항 → 스펙 정제 (taskId 기반) |
| `/team-claude:spawn` | Worker 생성 및 실행 |
| `/team-claude:status` | Worker 상태 조회 |
| `/team-claude:review` | 완료된 Task 리뷰 |
| `/team-claude:feedback` | Worker에 피드백 전달 |
| `/team-claude:merge` | PR 머지 |
| `/team-claude:cleanup` | 회고 분석 및 Worktree 정리 |

---

## 전체 워크플로우

```mermaid
flowchart TB
    subgraph INIT["1. 초기화"]
        init["/team-claude:init"]
    end

    subgraph PLAN["2. 스펙 정제"]
        plan["/team-claude:plan"]
    end

    subgraph EXECUTE["3. 병렬 실행"]
        spawn["/team-claude:spawn"]
        status["/team-claude:status"]
        feedback["/team-claude:feedback"]
    end

    subgraph REVIEW["4. 리뷰 & 머지"]
        review["/team-claude:review"]
        merge["/team-claude:merge"]
    end

    subgraph CLEANUP["5. 회고 & 정리"]
        cleanup["/team-claude:cleanup"]
    end

    init --> plan
    plan --> spawn
    spawn --> status
    status --> feedback
    feedback --> status
    status --> review
    review --> merge
    merge --> cleanup
    cleanup -.->|개선된 에이전트/스킬| plan
```

---

## 커맨드별 워크플로우

### /team-claude:init

프로젝트 분석 및 Team Claude 환경 초기화

```mermaid
flowchart TD
    START([시작]) --> ANALYZE[프로젝트 자동 분석]

    ANALYZE --> |package.json| PKG[언어/프레임워크 감지]
    ANALYZE --> |tsconfig.json| TS[TypeScript 설정]
    ANALYZE --> |.eslintrc| LINT[린트 규칙]
    ANALYZE --> |디렉토리 구조| STRUCT[모놀리스/모노레포]

    PKG & TS & LINT & STRUCT --> INTERVIEW

    INTERVIEW[AskUserQuestion 인터뷰]
    INTERVIEW --> Q1{도메인?}
    Q1 --> |이커머스/금융/SaaS| DOMAIN[도메인 에이전트 선택]

    INTERVIEW --> Q2{품질 우선순위?}
    Q2 --> |성능/보안/안정성| QUALITY[품질 에이전트 선택]

    INTERVIEW --> Q3{터미널?}
    Q3 --> |iTerm2/tmux| TERMINAL[터미널 설정]

    DOMAIN & QUALITY & TERMINAL --> GENERATE[설정 파일 생성]

    GENERATE --> CONFIG[".team-claude/config.json"]
    GENERATE --> AGENTS[".team-claude/agents/"]
    GENERATE --> HOOKS[".team-claude/hooks/"]
    GENERATE --> CRITERIA[".team-claude/criteria/"]

    CONFIG & AGENTS & HOOKS & CRITERIA --> DONE([초기화 완료])
```

---

### /team-claude:plan

요구사항을 스펙으로 정제하는 반복 워크플로우 (taskId 기반 관리)

```mermaid
flowchart TD
    START([시작]) --> INPUT{입력 타입?}

    INPUT --> |새 요구사항| NEW[taskId 생성<br/>8자리 UUID]
    INPUT --> |--resume taskId| RESUME[기존 계획 로드]
    INPUT --> |--list| LIST[계획 목록 표시]

    NEW --> PHASE1
    RESUME --> PHASE1

    subgraph PHASE1["PHASE 1: 요구사항 정리"]
        REQ[요구사항 분석] --> REQ_SAVE[requirements.md 저장]
    end

    PHASE1 --> PHASE2

    subgraph PHASE2["PHASE 2: 아웃라인 설계"]
        OUTLINE[아웃라인 작성] --> REVIEW1{리뷰}
        REVIEW1 --> |피드백| ASK1[AskUserQuestion]
        ASK1 --> |수정 필요| OUTLINE
        REVIEW1 --> |승인| OUTLINE_SAVE[outline.md 저장]
    end

    PHASE2 --> PHASE3

    subgraph PHASE3["PHASE 3: 계약 설계"]
        CONTRACT[Interface/Payload 정의] --> VALIDATE{아웃라인 일관성?}
        VALIDATE --> |불일치| CONTRACT
        VALIDATE --> |일치| REVIEW2{리뷰}
        REVIEW2 --> |피드백| ASK2[AskUserQuestion]
        ASK2 --> |수정 필요| CONTRACT
        REVIEW2 --> |승인| CONTRACT_SAVE[contracts/*.md 저장]
    end

    PHASE3 --> PHASE4

    subgraph PHASE4["PHASE 4: Task 분배"]
        PARALLEL[병렬 분석] --> TASK[Task 스펙 생성]
        TASK --> REVIEW3{리뷰}
        REVIEW3 --> |피드백| ASK3[AskUserQuestion]
        ASK3 --> |수정 필요| TASK
        REVIEW3 --> |승인| TASK_SAVE[tasks/*.md 저장]
    end

    PHASE4 --> COMPLETE

    subgraph COMPLETE["완료"]
        SUMMARY[summary.md 생성] --> RECOMMEND[recommendations.md 생성]
        RECOMMEND --> DONE([계획 완료])
    end

    style PHASE1 fill:#e1f5fe
    style PHASE2 fill:#fff3e0
    style PHASE3 fill:#f3e5f5
    style PHASE4 fill:#e8f5e9
    style COMPLETE fill:#fce4ec
```

---

### /team-claude:agent

에이전트 관리 (계층화된 구조)

```mermaid
flowchart TD
    START([시작]) --> ACTION{액션?}

    ACTION --> |list| LIST[에이전트 목록 표시]
    ACTION --> |show| SHOW[에이전트 상세 보기]
    ACTION --> |add| ADD[새 에이전트 생성]
    ACTION --> |enable| ENABLE[에이전트 활성화]
    ACTION --> |disable| DISABLE[에이전트 비활성화]
    ACTION --> |customize| CUSTOM[기본 에이전트 커스터마이징]
    ACTION --> |remove| REMOVE[에이전트 삭제]

    LIST --> RESOLVE[에이전트 해석]

    subgraph HIERARCHY["계층 구조"]
        LOCAL[".team-claude/agents/<br/>(프로젝트 로컬)"]
        PLUGIN["plugins/team-claude/agents/<br/>(플러그인 기본)"]
        LOCAL --> |우선| MERGE[병합]
        PLUGIN --> MERGE
    end

    RESOLVE --> HIERARCHY

    ADD --> INTERVIEW[AskUserQuestion<br/>역할, 전문 분야, 체크리스트]
    INTERVIEW --> GENERATE[에이전트 파일 생성]
    GENERATE --> SAVE_LOCAL[".team-claude/agents/{name}.md"]
    SAVE_LOCAL --> UPDATE_CONFIG[config.json 업데이트]

    CUSTOM --> COPY[플러그인 → 로컬 복사]
    COPY --> EDIT[수정]
    EDIT --> SAVE_LOCAL

    ENABLE --> UPDATE_CONFIG
    DISABLE --> UPDATE_CONFIG
    REMOVE --> DELETE[파일 삭제] --> UPDATE_CONFIG

    UPDATE_CONFIG --> DONE([완료])
```

---

### /team-claude:spawn

Worker 생성 및 Git Worktree 기반 병렬 실행

```mermaid
flowchart TD
    START([시작]) --> INPUT[Task ID 입력]
    INPUT --> LOAD[Task 스펙 로드<br/>.team-claude/plans/*/tasks/]

    LOAD --> VALIDATE{스펙 검증}
    VALIDATE --> |실패| ERROR([스펙 오류])
    VALIDATE --> |성공| PREPARE

    subgraph PREPARE["준비 단계"]
        BRANCH[브랜치 생성<br/>feature/{task-id}]
        WORKTREE[Git Worktree 생성<br/>../worktrees/{task-id}/]
        HOOKS[Worker용 hooks.json 복사]
        BRANCH --> WORKTREE --> HOOKS
    end

    PREPARE --> TERMINAL{터미널 타입?}

    TERMINAL --> |iTerm2| ITERM[새 탭에서 실행]
    TERMINAL --> |tmux| TMUX[새 pane에서 실행]
    TERMINAL --> |manual| MANUAL[명령어 출력]

    ITERM & TMUX & MANUAL --> EXECUTE

    subgraph EXECUTE["Worker 실행"]
        CLAUDE["claude --worktree<br/>Task 스펙 + Contract 전달"]
        CLAUDE --> WORKING[작업 수행]
        WORKING --> |Hook: PreToolUse| NEEDS_HELP{도움 필요?}
        NEEDS_HELP --> |Yes| NOTIFY_HELP[Main에 알림]
        NEEDS_HELP --> |No| WORKING
        WORKING --> |Hook: Stop| COMPLETE[작업 완료]
        COMPLETE --> NOTIFY_DONE[Main에 완료 알림]
    end

    NOTIFY_DONE --> STATE[상태 저장<br/>.team-claude/state/]
    STATE --> DONE([Spawn 완료])
```

---

### /team-claude:status

Worker 상태 실시간 조회

```mermaid
flowchart TD
    START([시작]) --> INPUT{입력?}

    INPUT --> |task-id| SINGLE[단일 Task 조회]
    INPUT --> |--all| ALL[전체 조회]
    INPUT --> |없음| ALL

    SINGLE & ALL --> COLLECT[상태 수집]

    subgraph COLLECT_SOURCES["상태 소스"]
        STATE[".team-claude/state/*.json"]
        WORKTREE["git worktree list"]
        BRANCH["git branch -r"]
    end

    COLLECT --> COLLECT_SOURCES

    COLLECT_SOURCES --> AGGREGATE[상태 집계]

    AGGREGATE --> DISPLAY

    subgraph DISPLAY["상태 표시"]
        RUNNING["🔄 running<br/>현재 실행 중"]
        WAITING["⏳ waiting<br/>피드백 대기"]
        COMPLETED["✅ completed<br/>작업 완료"]
        FAILED["❌ failed<br/>오류 발생"]
        MERGED["🔀 merged<br/>머지됨"]
    end

    DISPLAY --> SUMMARY[요약 통계]
    SUMMARY --> DONE([완료])
```

---

### /team-claude:review

완료된 Task 에이전트 리뷰

```mermaid
flowchart TD
    START([시작]) --> INPUT[Task ID 입력]
    INPUT --> LOAD[변경사항 로드]

    subgraph LOAD_DATA["데이터 수집"]
        DIFF["git diff main...feature/{task-id}"]
        LOG["git log 커밋 히스토리"]
        SPEC["Task 스펙 로드"]
    end

    LOAD --> LOAD_DATA

    LOAD_DATA --> CHECK_MODE{리뷰 모드?}

    CHECK_MODE --> |auto| AUTO[자동 리뷰]
    CHECK_MODE --> |semi-auto| SEMI[반자동 리뷰]
    CHECK_MODE --> |manual| MANUAL[수동 리뷰]

    AUTO & SEMI --> AGENTS

    subgraph AGENTS["에이전트 리뷰"]
        CR[Code Reviewer<br/>코드 품질]
        QA[QA Agent<br/>테스트 커버리지]
        SEC[Security Auditor<br/>보안 취약점]
        DOMAIN[Domain Expert<br/>도메인 로직]
    end

    AGENTS --> AGGREGATE[리뷰 결과 집계]

    AGGREGATE --> RESULT{결과?}

    RESULT --> |모두 승인| APPROVE[승인]
    RESULT --> |이슈 발견| ISSUES[이슈 목록]

    ISSUES --> ASK[AskUserQuestion<br/>피드백 전달?]
    ASK --> |Yes| FEEDBACK["/team-claude:feedback"]
    ASK --> |No| SAVE

    APPROVE --> SAVE

    SAVE[리뷰 결과 저장<br/>.team-claude/reviews/]
    SAVE --> DONE([리뷰 완료])

    MANUAL --> HUMAN[사람이 직접 리뷰]
    HUMAN --> SAVE
```

---

### /team-claude:feedback

Worker에 피드백 전달 (Hook 기반)

```mermaid
flowchart TD
    START([시작]) --> INPUT[Task ID + 피드백 입력]

    INPUT --> TYPE{피드백 타입?}

    TYPE --> |revision| REVISION[수정 요청]
    TYPE --> |question| QUESTION[질문 응답]
    TYPE --> |approve| APPROVE[승인]
    TYPE --> |abort| ABORT[작업 중단]

    REVISION & QUESTION --> WRITE[피드백 파일 작성]

    WRITE --> FEEDBACK_FILE[".team-claude/feedback/{task-id}.md"]

    FEEDBACK_FILE --> HOOK[Hook 트리거]

    subgraph HOOK_FLOW["Hook 실행"]
        SIGNAL["worker-feedback.sh<br/>Worker에 신호 전달"]
        SIGNAL --> WORKER[Worker가 피드백 확인]
        WORKER --> RESUME[작업 재개]
    end

    HOOK --> HOOK_FLOW

    APPROVE --> UPDATE_STATE[상태 → approved]
    ABORT --> KILL[Worker 종료]
    KILL --> UPDATE_STATE_ABORT[상태 → aborted]

    HOOK_FLOW --> DONE([피드백 전달 완료])
    UPDATE_STATE --> DONE
    UPDATE_STATE_ABORT --> DONE
```

---

### /team-claude:merge

PR 생성 및 머지

```mermaid
flowchart TD
    START([시작]) --> INPUT[Task ID 입력]

    INPUT --> CHECK{상태 확인}

    CHECK --> |not approved| NEED_REVIEW["리뷰 먼저 필요<br/>/team-claude:review"]
    CHECK --> |approved| PREPARE

    subgraph PREPARE["PR 준비"]
        DIFF["변경사항 요약"]
        COMMITS["커밋 메시지 수집"]
        SPEC["Task 스펙에서 설명 추출"]
    end

    PREPARE --> CREATE_PR["gh pr create"]

    CREATE_PR --> PR_CREATED[PR 생성됨]

    PR_CREATED --> CHECKS{CI 체크?}

    CHECKS --> |실패| CI_FAIL[CI 실패<br/>수정 필요]
    CHECKS --> |성공| READY

    CI_FAIL --> FEEDBACK["/team-claude:feedback"]

    READY --> MERGE_TYPE{머지 방식?}

    MERGE_TYPE --> |squash| SQUASH["gh pr merge --squash"]
    MERGE_TYPE --> |merge| MERGE_COMMIT["gh pr merge --merge"]
    MERGE_TYPE --> |rebase| REBASE["gh pr merge --rebase"]

    SQUASH & MERGE_COMMIT & REBASE --> MERGED[머지 완료]

    MERGED --> UPDATE_STATE[상태 → merged]
    UPDATE_STATE --> SUGGEST["다음 단계 제안<br/>/team-claude:cleanup"]
    SUGGEST --> DONE([머지 완료])
```

---

### /team-claude:cleanup

회고 분석 및 리소스 정리

```mermaid
flowchart TD
    START([시작]) --> INPUT{입력?}

    INPUT --> |task-id| SINGLE[단일 Task]
    INPUT --> |--completed| COMPLETED[완료된 모든 Task]
    INPUT --> |--all| ALL[모든 Task]
    INPUT --> |--analyze| ANALYZE_ONLY[분석만]
    INPUT --> |--improve| IMPROVE[분석 + 개선 + 정리]

    SINGLE & COMPLETED & ALL --> PHASE1

    subgraph PHASE1["PHASE 1: 작업 분석"]
        COLLECT[데이터 수집]
        COLLECT --> COMMITS["커밋 히스토리"]
        COLLECT --> DIFFS["파일 변경"]
        COLLECT --> REVIEWS["리뷰 피드백"]
        COLLECT --> PLANS["계획 문서"]

        COMMITS & DIFFS & REVIEWS & PLANS --> DETECT[패턴 감지]
        DETECT --> STATS["통계 생성<br/>반복 패턴, 이슈 유형"]
    end

    PHASE1 --> PHASE2

    subgraph PHASE2["PHASE 2: 개선 제안"]
        SUGGEST_AGENT["🤖 에이전트 제안<br/>신규 생성 / 기존 개선"]
        SUGGEST_SKILL["⚡ 스킬 제안<br/>반복 작업 자동화"]
        SUGGEST_DOC["📚 문서 제안<br/>가이드라인 추가"]
        SUGGEST_CONFIG["⚙️ 설정 제안<br/>config.json 최적화"]
    end

    ANALYZE_ONLY --> PHASE2
    PHASE2 --> SAVE_REPORT[분석 보고서 저장]

    IMPROVE --> PHASE3

    subgraph PHASE3["PHASE 3: 개선 적용"]
        ASK[AskUserQuestion<br/>적용할 항목 선택]
        ASK --> APPLY[선택 항목 적용]
        APPLY --> CREATE_AGENT[에이전트 생성]
        APPLY --> CREATE_SKILL[스킬 템플릿 생성]
        APPLY --> CREATE_DOC[문서 생성/수정]
        APPLY --> UPDATE_CONFIG[config.json 수정]
    end

    PHASE3 --> PHASE4

    subgraph PHASE4["PHASE 4: 리소스 정리"]
        REMOVE_WORKTREE["Worktree 제거<br/>../worktrees/{task-id}"]
        REMOVE_BRANCH["브랜치 삭제<br/>feature/{task-id}"]
        ARCHIVE_STATE["상태 아카이브<br/>.team-claude/archive/"]
    end

    PHASE4 --> RETROSPECTIVE[회고 보고서 저장<br/>.team-claude/retrospectives/]
    RETROSPECTIVE --> DONE([정리 완료])

    style PHASE1 fill:#e3f2fd
    style PHASE2 fill:#fff8e1
    style PHASE3 fill:#f3e5f5
    style PHASE4 fill:#e8f5e9
```

---

## 에이전트 계층 구조

에이전트는 `.claude` 파일처럼 계층화된 구조로 관리됩니다:

```mermaid
flowchart TD
    subgraph LOCAL["프로젝트 로컬 (최우선)"]
        L1[".team-claude/agents/code-reviewer.md"]
        L2[".team-claude/agents/my-custom-agent.md"]
    end

    subgraph PLUGIN["플러그인 기본"]
        P1["plugins/team-claude/agents/code-reviewer.md"]
        P2["plugins/team-claude/agents/qa-agent.md"]
        P3["plugins/team-claude/agents/security-auditor.md"]
    end

    L1 --> |오버라이드| RESOLVE[최종 에이전트]
    P1 -.-> |로컬 없으면| RESOLVE
    L2 --> RESOLVE
    P2 --> RESOLVE
    P3 --> RESOLVE

    RESOLVE --> ENABLED{config.json<br/>agents.enabled}
    ENABLED --> ACTIVE[활성화된 에이전트]
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

---

## 디렉토리 구조

```
.team-claude/
├── config.json                 # 메인 설정
├── agents/                     # 로컬 에이전트 (오버라이드)
├── criteria/                   # 완료 기준
├── hooks/                      # Worker Hook 설정
├── plans/                      # 계획 문서 (taskId별)
│   ├── index.json
│   └── {taskId}/
│       ├── meta.json
│       ├── requirements.md
│       ├── outline/
│       ├── contracts/
│       ├── tasks/
│       └── completion/
├── state/                      # Worker 상태
├── reviews/                    # 리뷰 결과
├── feedback/                   # 피드백 파일
├── retrospectives/             # 회고 보고서
└── archive/                    # 아카이브

../worktrees/                   # Git Worktree (프로젝트 외부)
└── {task-id}/
```

---

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
/team-claude:spawn task-coupon-service task-coupon-api

# 4. 상태 확인
/team-claude:status

# 5. 리뷰 및 머지
/team-claude:review task-coupon-service
/team-claude:merge task-coupon-service

# 6. 회고 및 정리 (에이전트/스킬 개선 제안)
/team-claude:cleanup task-coupon-service --improve
```

---

## 라이선스

MIT
