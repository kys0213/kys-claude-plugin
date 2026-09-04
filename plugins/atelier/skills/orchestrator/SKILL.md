---
name: orchestrator
description: Use this skill for any multi-unit work delegated to sub-agents, agent-teams, or worktrees — parallel fan-out, sequential pipelines, long-running agent teams, autonomous self-driving runs (decompose→dispatch→merge without human intervention), document deliverables (writing reports, specs, or write-up docs is also delegated Write work), multi-branch research and investigation (codebase surveys, side-effect analysis, comparing options — read-only work counts too), or any moment the main agent is about to use Edit/Write directly (delegate instead). Scope is set by scale, not by kind. Triggers include "자율주행", "자율주행모드", "자율 모드", "자율 주행으로", "알아서 끝까지", "여러 작업 병렬로", "동시에 처리", "에이전트 나눠서", "worktree로 분리", "위임해서", "팀으로 작업", "리포트 작성", "보고서로 정리", "스펙 문서 작성", "분석 결과 문서화", "조사해줘", "리서치", "코드베이스 파악", "영향 범위 분석", "사이드이펙트 조사", "원인 분석", "감사해줘", "다시 검토해줘", "여러 방안 비교", "autonomous mode", "self-driving", "hands-off run", "delegate", "parallel agents", "fan-out", "agent team", "sub-agent", "dispatch multiple", "split into tasks", "run in parallel", "write a report", "draft a spec", "write up findings", "research", "investigate", "survey the codebase", "analyze impact", "root cause", "audit", "re-review", "compare approaches".
version: 0.1.0
---

# Orchestrator Skill

## When to use (트리거 케이스)

이 스킬을 트리거해야 하는 상황:

- 사용자가 **자율주행 모드를 언급** ("자율주행모드로", "자율 주행으로 진행해", "알아서 끝까지 해줘", "autonomous mode") — 자율 루프(분해→위임→머지 self-drive)는 오케스트레이터의 기본 동작이므로 이 스킬로 진입한다 (`references/autonomous-driving.md`)
- 사용자가 **2개 이상의 독립 작업**을 한 번에 요청 ("A랑 B랑 C 같이 해줘", "동시에 처리해줘") · **병렬 fan-out**이 가능해 보일 때 ("여러 파일 동시에", "병렬로", "parallel", "in parallel")
- **sub-agent / agent-team / worktree 위임**을 명시적으로 요청 ("나눠서", "팀으로", "에이전트 여러 개", "delegate", "dispatch") · **장기 진행 작업**에 식별 가능한 agent team이 필요할 때 (designer/implementer/reviewer 등) · **머지 조정**이 필요한 다중 변경 (여러 worktree 결과 통합, 충돌 해결 위임)
- **문서 산출물 작업** ("리포트로 정리해줘", "스펙 문서 만들어줘", "분석 보고서 작성") — 문서 작성도 Write 작업이므로 위임 대상 · **여러 갈래로 벌어지는 리서치·조사·분석** — 코드베이스 파악, 사이드이펙트·영향 범위 조사, 방안 비교, 자료 수집 ("조사해줘", "리서치", "어디에 영향 가는지 봐줘", "여러 방안 비교") — **read-only 라고 위임 대상에서 빠지지 않는다** (근거: `references/autonomous-driving.md §메인 컨텍스트 격리`)
- **메인 에이전트가 Edit/Write/NotebookEdit로 직접 코드를 수정하려는 모든 순간** — 위임으로 전환할지 먼저 검토

**적용 범위는 작업의 종류가 아니라 규모로 정한다.** 구현이냐 문서냐 조사냐로 가르지 않는다 — 여러 단위로 쪼개지거나, 병렬로 벌릴 수 있거나, 메인 컨텍스트를 크게 먹으면 위임 대상이다.

트리거하면 안 되는 상황: 단일 파일의 단순 편집(오버헤드만 늘어남) · 사용자가 직접 메인이 처리하라고 명시한 경우 · 1턴 안에 끝나는 단발 조회(파일 하나 확인, git 상태, 테스트 결과 등 결정적 사실 확인).

## 사고 모드 (Mental Model)

이 스킬을 트리거한 순간부터 메인 에이전트는 **편집자가 아니라 관리자**다 — Edit/Write로 직접 코드를 작성하지 않고, Read/Bash로 상태를 파악하고 Task로 일감을 분리·관리하며 Agent로 위임하고 SendMessage로 조율한다.

### 메인 에이전트가 해도 되는 일
- `Read`, `Glob`, `Grep`, `Bash(git status / git log / git diff --stat)` — 작업 분해와 위험도 판단에 필요한 **결정적 사실 확인**에 한정한다. 본격적인 조사·리서치(코드베이스 파악, 영향 범위 분석, 방안 비교)는 메인이 통독하지 않고 **위임**한다 — 그 원문이 메인 컨텍스트에 쌓이면 조율 판단 품질이 떨어진다 (`references/autonomous-driving.md §메인 컨텍스트 격리`)
- `Agent`, `SendMessage`, `Monitor` — 위임과 조율 (spawn한 agent에 **다시 말을 거는 유일한 수단이 `SendMessage`**다 — 가용 판정은 아래 §진입 시 체크 4, `TeamCreate`는 제거됨). 결과물은 취합해 사용자에게 보고한다
- `TaskCreate` / `TaskList` / `TaskGet` / `TaskUpdate` — 일감을 분리하고 상태를 관리하는 것은 **메인 에이전트의 핵심 룰**이다. 편집을 위임하는 관리자로서 메인의 본업은 일감을 추적 가능한 Task로 쪼개고 상태를 갱신하는 것 — 다중 작업이면 항상 적용하고 단발 1회만 예외다 (`references/agent-monitor.md §Task 시스템`)

> 위 조율 도구는 대부분 deferred tool이다 — `ToolSearch`로 스키마를 확보하기 전에는 호출할 수 없고(§진입 시 체크 0), 확보를 건너뛴 세션은 명시적 에러 없이 team 경로 전체를 조용히 잃는다.

### 메인 에이전트가 하면 안 되는 일
- `Edit`, `Write`, `NotebookEdit` — 코드 편집·작성은 항상 sub-agent에 위임한다. sub-agent 실패 시에도 편집권을 가져오지 않는다 → 사용자에게 보고
- `EnterWorktree` / `git checkout <other-branch>` 로 worktree 또는 다른 브랜치로 진입 — 메인은 진입 시점의 브랜치에 머문다 (무거운 경로에서는 그것이 epic 브랜치다)

## 진입 절차 (Entry Procedure)

**버전 관리 이력에 남을 변경을 만드는 런은 반드시 epic 브랜치 전략으로 동작한다** — 메인은 worktree가 아니라 epic 브랜치에 체크아웃된 상태로 작업하고, 위임된 sub-agent들만 worktree로 격리한다. 반대로 **tracked 변경을 만들지 않는 런에는 이 전략이 성립하지 않는다** — 머지할 대상도, 격리할 쓰기도 없다. 어느 쪽인지는 아래 §경로 판정 게이트가 먼저 정하고, 이 절의 토폴로지와 체크 1·2·3은 **무거운 경로에만** 적용된다.

### 토폴로지 (무거운 경로)

- **메인 = epic 브랜치 자체**(worktree 진입 금지) · **sub-agent = epic base의 worktree** — 통합은 rebase 후 `--ff-only` 고정, 브랜치 네이밍·머지 방식·drift 처리의 단일 출처는 `references/branch-strategy.md`다. 다이어그램·격리 상세: `references/worktree-lifecycle.md §토폴로지`.
- **epic → main 머지는 이 스킬 범위 밖** — 역방향 drift는 런 안에서 처리한다 (`references/branch-strategy.md §epic ← main 역방향 drift`).

### 진입 시 체크

0. **조율 도구의 스키마를 확보했는가?** (다른 모든 체크보다 먼저) — `ToolSearch({query: "select:SendMessage,Monitor,TaskCreate,TaskList,TaskGet,TaskUpdate"})`

   - **1회만 한다** (확보된 스키마는 세션 내내 유효). `SendMessage`가 확보되지 않으면 **왕복 조율 수단이 없는 것**이므로 체크 4는 자동으로 비가용이다 (아래 판정 트리의 0단계). 결과에 없는 도구는 이 런타임에 없는 것이다 — 이름을 추측해 호출하지 않는다.

### 경로 판정 게이트 (체크 0 직후, 체크 1 앞)

체크 0을 마치면 **이번 런이 버전 관리 이력에 남을 변경을 만드는가**를 먼저 정한다. 판정 기준은 "git 레포 안인가"가 아니다 — 레포 안이어도 이력에 남지 않으면 경량 경로다.

```
이번 런의 계획된 산출물에 tracked 파일 변경이 있는가?
  ├─ No  → 경량 경로 (체크 1·2·3 건너뛰고 체크 4·5로)
  │        예: 외부 시스템 산출물(이슈 등록·PR 코멘트), read-only fan-out 조사, repo 밖·gitignore 산출물
  └─ Yes → 무거운 경로 (체크 1~5 전부)
```

- **판정 시점의 산출물 계획을 기준으로 한다.** 계획 밖 편집이 생기면 아래 §경로 전환. 판정 결과 + 근거를 진입 보고 1줄과 decision log에 남긴다 — **생략은 판정이 아니다.**
- **git 레포가 아니면서 편집이 필요한 경우**는 경량 경로가 아니다 — 판정은 `references/delegation-patterns.md §경로 판정 경계 케이스`가 단일 출처다.
- 경량 경로에서 생략되는 것은 체크 1·2·3, 토폴로지 가드, 머지 조정뿐이다 — 조율·Task·게이트·preflight·복원력·취합 보고는 편집 유무와 무관하게 유지되고, 리뷰·QA는 쓰기 전 검토 1회로 축소된다. 전체 유지·생략 표와 경계 케이스 판정은 `references/delegation-patterns.md §경로 판정 경계 케이스`가 단일 출처다.

### 진입 시 체크 (이어서)

1. **현재 브랜치가 epic 브랜치인가?** — `git branch --show-current` 확인. `main` / 일반 feature 브랜치라면 epic 브랜치를 먼저 만들거나 사용자에게 어떤 epic 브랜치로 진입할지 물어본다 (`git` skill 의 브랜치 생성 또는 plain `git checkout -b epic/<name>`).
2. **현재 메인이 다른 worktree 안에 있지 않은가?** — `git rev-parse --show-toplevel` 가 repo의 메인 working tree여야 한다. worktree 안에서 시작했다면 즉시 메인 working tree로 빠져나오도록 사용자에게 보고.
3. **이후 모든 sub-agent dispatch는 `isolation: "worktree"` 로** — worktree의 base가 dispatch 시점 epic 브랜치 HEAD라는 보장은 없다. dispatch prompt에 base 확인·동기화 지시를 반드시 포함한다 (`references/delegation-patterns.md §Prompt 작성 원칙 필수 포함 요소` 9번이 단일 출처)
4. **왕복 조율(team)이 이번 세션에서 가용한가?** — 판정하는 대상은 "team이라는 기능이 켜져 있는가"가 아니라 **"spawn한 agent에게 다시 말을 걸 수 있는가"**다. 필수 등급이 요구하는 실질은 *직전 라운드를 기억하는 상대와의 왕복*이고(`references/delegation-patterns.md §team mode 강제 등급` 기준 1), 그것을 주는 것은 `name`이라는 파라미터가 아니라 `SendMessage`라는 채널이다.

```
왕복 조율(team) 가용 판정
  │
  ├─ [0 · 전제] SendMessage 스키마를 확보했는가? (§진입 시 체크 0)
  │     └─ No ──→ 비가용 확정. 왕복 수단 자체가 없다 (1·2차를 볼 필요 없음)
  │
  ├─ [1차 · 권위] spawn한 agent를 SendMessage로 다시 지목할 수 있는가?
  │     ├─ Agent 스키마에 `name` 있음 ──→ 가용 · 지목자 = name
  │     └─ `name` 없음 ────────────────→ 가용 · 지목자 = spawn 결과의 agentId (`a...` 형식)
  │
  └─ [2차 · 보조] printenv CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS
        └─ 1차를 확인할 수 없을 때만 참고. 단독으로 비가용을 확정하지 못한다.

  기록  판정 결과 + 사용한 신호 → 진입 보고 1줄 + decision log
        (근거 없는 "플래그 off"는 판정이 아니다)
  시점  진입 시 1회 확정. 트리거 시점 재확인 없음
  정정  필수 등급 경로의 첫 spawn이 왕복 가능한 상대가 아니면 → 1회 재판정
  이후  비가용 시의 반응은 경로마다 다름 → §team mode 강제 등급
```

   - **name 부재는 비가용이 아니다** — agentId로 같은 왕복이 성립한다. 신호의 권위·env를 안 쓰는 이유는 `references/delegation-patterns.md §Agent team 사용 패턴`이 단일 출처다.

5. **위임 파이프라인이 의존하는 공유 전제가 살아 있는가?** (preflight — dispatch 전 마지막 확인) — 판정 기준은 하나다: 깨졌을 때 fan-out 전체가 죽는 공유 의존인가. read-only 확인만 하고, 실패 시 dispatch를 시작하지 않고 해제 방법과 함께 즉시 보고한다. 대상 도출·경계는 `references/delegation-patterns.md §공유 전제 preflight`가 단일 출처다.

### 경로 전환 (경량 → 무거운)

전환 트리거: 계획에 없던 tracked 파일 편집이 필요해진 순간 — 편집 dispatch를 시작하기 전에 생략했던 체크 1·2·3을 late gate로 실행한다. 5단계 절차·기록 규칙은 `references/delegation-patterns.md §경로 전환`이 단일 출처다. **역방향(무거운 → 경량) 전환은 없다.**

## 표준 절차 (Workflow)

```
0. 진입 확인 (Entry)        → 조율 도구 스키마 확보(ToolSearch) + 경로 판정 게이트 + §진입 시 체크
1. 분해 (Decompose)        → 독립 단위로 분해 — 복잡·모호한 요구는 아키텍트 협의체(설계 생성 →
                             별도 agent 의 심문·검증)에 위임해 검증된 task 도출 (`references/architect-council.md`)
2. 위험도 분석 (Analyze)    → 단위 간 충돌 위험 식별
3. 실행 계획 (Plan)         → 병렬/순차 결정 + 위임 형태(단발/team) 결정
4. 위임 (Dispatch)          → Agent 호출 (`references/delegation-patterns.md`)
5. 모니터링 (Monitor)       → 진행 추적, 정체 감지, 사용자 보고
6. 검토·QA 게이트 (Gate)    → 작업마다 검토 + QA(검증 테스트 추가) 필수 + DB 접촉 작업은 DBA 추가, 전부 pass여야 머지
7. 머지 조정 (Coordinate)   → 게이트 통과분만 epic 브랜치로 통합 + 충돌 위임 + worktree 정리
8. 보고 (Report)            → 사용자에게 결과 요약
```

각 단계의 상세 패턴은 아래 references에 있다. **경로별 차이는 §경로 판정 게이트의 요지와 `references/delegation-patterns.md §경로 판정 경계 케이스`의 유지·생략 표를 따른다.**

### 일감을 Task로 분리·관리하는 것은 메인의 핵심 룰

관리자의 본업은 일감을 추적 가능한 단위로 쪼개고 그 상태를 끝까지 관리하는 것이다(위 *사고 모드*) — 분해한 일감을 Task 시스템(`TaskCreate`/`TaskList`/`TaskGet`/`TaskUpdate`)으로 분리·등록·갱신하는 것은 선택이 아니라 메인의 핵심 룰이다. 다중 작업이거나 의존성이 있으면 **항상** 적용하고, 단발 1회 작업만 예외다. 상세 사용법(필드·의존성·owner)은 `references/agent-monitor.md §Task 시스템`이 단일 출처다. 자율 모드의 Task 추적 규칙은 `references/autonomous-driving.md`를 따른다.

### 디스패치 전·보고 수용 게이트 (Dispatch Preconditions)

reference를 읽지 않아도 성립해야 하는 게이트 여섯 개다. 발동 시점만 여기 두고, 각 계약의 단일 출처는 지목된 절이다.

- **spec 입력 구현 dispatch 전 — spec 확정 확인 (hard stop)**: 입력 spec에 미결(TBD) 항목이 하나라도 있으면 dispatch하지 않고 hard stop한다 (`references/autonomous-driving.md §spec 확정 게이트`가 단일 출처).
- **implementer dispatch 전 — 설계 승인 마커 확인**: 마커가 없으면 dispatch하지 않고 설계 단계로 회귀한다 (`references/architect-council.md §설계 승인 마커`).
- **테스트 작성이 포함된 구현 dispatch 전 — 테스트 인프라 발견**: 레포의 테스트 러너·픽스처·하네스·유사 기존 테스트가 file:line으로 인용되기 전에는 테스트 작성 단계에 진입시키지 않는다 (`references/delegation-patterns.md §테스트 인프라 발견`).
- **sub-agent 보고 수용 전 — 증거 계약 확인**: 증거 없는 claim은 수용하지 않고 재디스패치하며, 부재 주장(negative claim)은 교차 검증 후에만 수용한다 (`references/delegation-patterns.md §증거 계약`).
- **조사·리서치 위임 dispatch 전 — 탐색 예산 명시**: 예산 없는 조사형 prompt는 dispatch하지 않는다 (`references/delegation-patterns.md §탐색 예산`이 단일 출처).
- **dispatch 시·보고 수용 시 — 기대 완료 시간과 중복·idle 판정**: 기대 완료 시간 없이 dispatch하지 않는다. 동일 내용의 재전송 보고는 첫 수신만 취합하고, 기대 시간을 넘긴 무보고 agent는 대기 연장이 아니라 취소 후 폴백·재위임으로 회부한다 (`references/agent-monitor.md §중복 보고 감지`·`§idle 판정`).

### 작업 케이스마다 검토 에이전트·QA 에이전트는 필수 (Review & QA Gate)

- **검토 에이전트 (reviewer)** — `구현 ↔ 요구사항`, **QA 에이전트 (qa)** — `요구사항 ↔ 테스트`(누락 시 검증 테스트를 추가·보강), **DBA 에이전트 (dba)** — DB 접촉 작업만 조건부로 `구현 ↔ DB 안전성`.
- **AND 게이트**: 전부 `pass`여야 머지 후보로 승급. 하나라도 `reject`면 findings를 실어 재위임한다. 게이트 에이전트들은 **구현 sub-agent와 다른 agent**다 — 자기 코드 자기 검증 금지.
- QA의 테스트 추가도 편집이므로 **`isolation:"worktree"` subagent로 위임**한다 (메인은 직접 편집하지 않는다 — *사고 모드*). 예외는 Task 룰과 동일하게 **단발 1회·read-only 작업만**이다.

역할별 입력·검증 질문·출력 계약, DB 접촉 판정, 게이트 거부의 재위임 예산·기록 등 세부 규칙은 `references/autonomous-driving.md §리뷰어·QA 게이트`가 단일 출처다. spec 문서를 입력으로 구현하는 경우만 `references/spec-driven-review.md`(검토자=spec↔구현, QA 매니저=spec↔테스트)로 특수화된다.

### 병렬 fan-out 복원력 (Resilience)

대규모 fan-out에서는 일부 agent의 인프라 실패(504·API 에러)를 정상 케이스로 전제한다. 필수 규칙 4개 — **체크포인트**(완료 즉시 agent 자신이 파일 저장) · **재시도**(같은 prompt N회, 기본 3회) · **폴백**(소진 시 대체 경로로 취합 완성 — 미완성 종료 금지) · **투명 보고**(실패/재시도/폴백 명시). 상세 절차는 `references/agent-monitor.md §fan-out 복원력`이 단일 출처다.

## 분해는 충돌 경계로 쪼갠다 (병렬 판정 앞 단계)

병렬/순차 결정 트리는 **이미 쪼개진** 작업을 거르는 사후 필터다. 분해(1단계)가 충돌을 만들어 놓으면 트리는 그것을 전부 순차로 떨어뜨릴 수밖에 없다 — **병렬 이득은 판정이 아니라 분해에서 결정된다.**

- **수직 슬라이스로 쪼갠다**: 기능 단위(A 기능의 모델+서비스+API+테스트)로 자른다. 레이어 수평 분해(모델 전부 / 서비스 전부 / 컨트롤러 전부)는 작업마다 같은 파일들을 훑게 되어 disjoint가 애초에 나오지 않는다.
- **hot-spot은 별도 task로 뽑는다**: 작업들의 의도는 안 겹치는데 **같은 위치에 항목을 추가**하게 되는 파일은 병렬 작업에서 편집을 금지하고 마지막에 통합 task 1개로 순차 처리한다. 안 하면 **hot-spot 하나 때문에 fan-out 전체가 순차로 떨어진다** (사례·판정·계약은 `references/branch-strategy.md §hot-spot 파일`이 단일 출처).
- **공유 인터페이스는 선행 task로 앞세운다**: 여러 작업이 같은 타입·시그니처를 필요로 하면 그 정의를 먼저 한 task로 확정·머지한 뒤 나머지를 병렬로 띄운다. 각자 정의하게 두면 머지에서 의미 충돌이 되고, 그건 자동 해결 대상이 아니다.

## 병렬 vs 순차 판정 (요지)

- disjoint + 의존성 없음 → 병렬. overlap 또는 의심스러우면 → 순차 — 단, 겹침이 전부 hot-spot이면 병렬 유지 + 통합 task 분리 (`references/branch-strategy.md §hot-spot 파일`). 전체 결정 트리·경량 경로의 충돌 축(외부 리소스·rate limit)은 `references/delegation-patterns.md §병렬 vs 순차 결정 트리`가 단일 출처다.

### 조사·감사 작업의 기본값은 병렬 fan-out

read-only 조사·감사·원인분석은 충돌 비용이 없어 "의심스러우면 순차"가 적용되지 않는다 — **첫 행동이 관점별 병렬 fan-out**(특별한 근거 없으면 3관점 이상)이고, 메인이 순차 Bash/Read 탐색으로 조사를 시작하지 않는다. 각 조사 prompt에 탐색 예산을 명시한다(§디스패치 전·보고 수용 게이트). 출구 판정(1턴 단발·단일 관점)은 `references/delegation-patterns.md §병렬 vs 순차 결정 트리`를 따른다.

## 위임 형태 결정

| 상황 | 형태 | 도구 |
|------|------|------|
| 1회성 독립 작업, 결과물 단일 | 단발 sub-agent | `Agent({...})` |
| 여러 agent 협업·식별/제어 필요 (read-only 조율) | agent team | `Agent({name, ...})` — `name` 없는 런타임이면 `Agent({run_in_background: true})` + 반환 `agentId` — 에 `SendMessage` (가용 판정 §진입 시 체크 4·`team_name` 무시·편집 격리는 subagent) |
| 파일 충돌 위험 있는 병렬 | worktree-isolated | `Agent({isolation: "worktree", ...})` |

> **격리는 subagent만 보장** — teammate는 공유 checkout. 편집은 `isolation:"worktree"` subagent, team은 조율 전용 (`references/delegation-patterns.md §Agent team 사용 패턴`이 단일 출처).

자세한 판단 기준과 prompt 작성법은 `references/delegation-patterns.md`.

### team mode 강제 등급 (요지)

- **필수**: 자문 조회 · 아키텍트 협의체 — 실질이 왕복 대화이고 read-only다. **가용인데 단발로 대체 = 위반**, 비가용이면 폴백 없이 원래의 에스컬레이션으로 진행한다. **선호**: spec 검토·QA 게이트 · review→fix 루프 — 단발 폴백 허용.
- 등급 기준·경로별 표·필수 등급 가드(spawn 확인 + decision log 필드)는 `references/delegation-patterns.md §team mode 강제 등급`이 단일 출처다.

## 모델 라우팅 (요지)

- **집행 위임(자문 제외 전부 — 구현·문서·리서치·조사·리뷰·게이트)의 tier ≤ 메인 tier, 예외 없음.** 매 dispatch에 `model` 명시 필수(상속 금지), 문서에 모델명을 박지 않는다. **자문 조회만 상위 tier 허용** — 권고 + 근거(read-only)만 사오고 결정권은 메인에 100% 잔류, team member 전용(필수 등급).
- **team 비가용 = 자문 경로 차단**: 트리거에 도달해도 소집하지 않고, 무엇으로도 대체하지 않으며, 원래 하려던 에스컬레이션으로 진행하고 그 사실을 판정 근거와 함께 decision log에 남긴다. 사용자가 명시 요청해도 우회하지 못한다 (절차: `references/advisory-consult.md §게이트 0`, 자율 모드 `max_advisory_consults = 0`은 `references/autonomous-driving.md §자율 계약`이 단일 출처).
- 판정 트리·집행/자문 대비 표·역할별 모델 제약은 `references/model-routing.md`가 단일 출처다. 작업 유형 → 시작 tier 표는 `references/delegation-patterns.md §모델 선택`.

## References (필요할 때만 로드)

| 파일 | 언제 읽을지 |
|------|-------------|
| `references/architect-council.md` | 분해(1단계) 시 요구가 복잡·모호해 아키텍트 협의체(설계 생성 ↔ 심문 검증)로 분석·검증 후 task 를 도출할 때 |
| `references/delegation-patterns.md` | 위임 형태(단발 vs team)를 결정하거나 sub-agent prompt를 작성할 때, **경로 판정이 경계 케이스이거나 경로 전환·preflight·탐색 예산·병렬/순차 전체 트리를 적용할 때**(§경로 판정 경계 케이스·§경로 전환·§공유 전제 preflight·§탐색 예산·§병렬 vs 순차 결정 트리가 단일 출처), **team 강제 등급을 판정할 때**(§team mode 강제 등급이 단일 출처), **원인 불명 결함·회귀를 조사할 때**(§근본원인 swarm — 축 분해·증거 계약·가설 랭킹) — **작업 유형 → tier 표의 단일 출처** (역할 기준 원칙·역할별 모델 제약은 `references/model-routing.md`가 단일 출처) |
| `references/model-routing.md` | dispatch의 model/tier를 정할 때 — 역할 기준 원칙(집행 tier 상한)·자문 tier 예외·역할별 모델 제약의 단일 출처 (작업 유형 → tier 표는 `delegation-patterns.md §모델 선택`) |
| `references/branch-strategy.md` | 무거운 경로에서 **브랜치를 어떻게 운영할지** 정할 때 — worktree 브랜치 네이밍, hot-spot 파일 분리 계약, 머지 정책(배치 vs 즉시+전파), 통합 방식, epic ← main 역방향 흡수, 반복 충돌의 재분해 트리거. **위 여섯의 단일 출처** (분해 원칙은 위 §분해는 충돌 경계로 쪼갠다, 단일 rebase의 충돌 해결 전략은 `git` skill) |
| `references/worktree-lifecycle.md` | 병렬 dispatch 직전, 또는 worktree 정리/머지를 다룰 때 |
| `references/agent-monitor.md` | 백그라운드 agent 진행 추적, Task 시스템으로 다중 작업 상태·의존성을 추적할 때, 또는 대규모 fan-out에서 실패 대비 복원력(체크포인트·재시도·폴백) 절차를 적용할 때 |
| `references/merge-coordinator.md` | 병렬 결과를 통합할 때 (순서 결정, 충돌 처리) |
| `references/autonomous-driving.md` | 자율 루프(분해→위임→머지 self-drive)를 돌릴 때 — **오케스트레이터 기본 동작**. 계약·가드레일·종료 조건·에스컬레이션 + **작업마다 필수인 리뷰어·QA 게이트**(검토 + 검증 테스트 추가)의 단일 출처 (단발 fan-out 1회면 불필요) |
| `references/advisory-consult.md` | 상위 tier 자문을 소집할 때 (협의체 예산 소진 tie-break, 게이트 재위임 루프, 되돌리기 어려운 결정, 사용자 요청) — **소집 트리거·패킷 계약·출력 계약·수명의 단일 출처** (tier 예외 원칙 자체는 `references/model-routing.md`가 단일 출처). 진입 시 team 비가용으로 확정됐으면 읽을 필요 없다 |
| `references/spec-driven-review.md` | 검토·QA 게이트가 **spec 문서를 입력으로 구현**하는 경우의 특수화 — 팀 모드로 검토자(spec↔구현)·QA 매니저(spec↔테스트)를 상주시켜 worktree 코드를 계속 리뷰·개선 (spec 입력이 없으면 일반 게이트 사용). 진입 전제는 **미결 0으로 확정된 spec**이다 (`references/autonomous-driving.md §spec 확정 게이트`) |

## 사용자 보고 원칙

오케스트레이터는 **기본적으로 자율 주행**한다 — 진입 시 자율 계약을 1회 보고하고, 가드레일(종료 조건·예산·자동 중단) 안에서 자동 재위임·머지·충돌 해결을 사람 개입 없이 진행한다. 자율 계약·루프·에스컬레이션 규칙은 `references/autonomous-driving.md` 가 단일 소유한다.

- **시작 시**: 분해된 작업 목록 + 병렬/순차 결정 + 자율 계약(종료 조건·예산·hard stop·결정 기록 위치)을 한 번에 보고
- **진행 중**: 침묵 — 단, 에스컬레이션 조건(되돌리기 어려운 행위·토폴로지 위반·도메인 의미 충돌·예산 소진·spec 미결 발견 등)은 자율 모드라도 **항상** 멈추고 즉시 보고한다 (`references/autonomous-driving.md §에스컬레이션`)
- **종료 시**: 종료 사유(완료/예산 소진/에스컬레이션) + 머지 결과 + 미해결 항목 + **3분류 판정 요약(DONE/BLOCKED/NOT-STARTED) + 핸드오프 파일 경로** + 원격 최신화 상태(열린 PR이 있으면 push 완료 여부 — 상세는 `references/autonomous-driving.md §종료 조건`/`§종료 핸드오프`, 판정은 `git` skill `SKILL.md` §열린 PR 최신화 원칙) + 의사결정 요약 (핸드오프 계약은 `references/autonomous-driving.md §종료 핸드오프`)
- **opt-out — 휴먼-인-더-루프**: 사용자가 단계별 확인을 명시하면(예: "확인받으면서", "단계마다 물어봐", "babysit", "자동으로 머지하지 마") 자율 주행을 끄고 전환한다. 자동 개입(SendMessage 명령 주입·자동 머지·자동 충돌 해결)을 하지 않고, 정체·실패·머지 결정을 사용자에게 보고하고 결정을 받는다 (`agent-monitor.md` / `merge-coordinator.md` 의 HITL 규칙)

## 안티패턴

1. **편집권 회수**: sub-agent 실패 시 메인이 직접 Edit로 마무리 금지 — 다시 위임하거나 사용자에게 보고한다.
2. **충돌 위험 무시한 병렬화**: disjoint 검증 없이 병렬 금지 — 의심스러우면 순차다.
3. **컨텍스트 의존 prompt**: sub-agent는 메인 대화를 못 본다 — 자기완결적으로 작성한다 (`references/delegation-patterns.md §Prompt 작성 원칙`).
4. **Reference 일괄 로드**: 시작하자마자 모든 reference를 Read 금지 — 단계별로 필요할 때만.
5. **무한 폴링**: `Bash sleep` 루프 금지 — `run_in_background: true` + 완료 알림 사용.
6. **메인이 worktree에서 시작** (적용 경로는 §경로 판정 게이트): 메인은 epic 브랜치의 메인 working tree에서만 동작한다.
7. **epic 브랜치 우회** (적용 경로는 §경로 판정 게이트): 반드시 epic 브랜치를 만들고 거기서 dispatch한다.
8. **자문 흉내**: 자문 경로 비활성 시 단발 왕복·자기 판단을 자문으로 포장 금지 — 메인 자신의 판단으로 명시하고, 폴백은 decision log의 `실행 형태` 필드로 사후 탐지된다 (`references/advisory-consult.md §안티패턴` / §team mode 강제 등급).
9. **고무도장 메인**: 상위 tier 권고의 무검토 채택 금지 — 채택도 기각도 사유와 함께 기록한다 (`references/advisory-consult.md §안티패턴`).
10. **자문 tier가 다른 역할로 번짐**: 자문 외의 모든 위임은 집행 위임이며 메인 tier를 넘지 못한다 (`references/model-routing.md §역할 기준 원칙 / §역할별 모델 제약`).
11. **신호 하나로 team 비가용 단정**: 권위 신호는 **`SendMessage`로 다시 지목할 수 있는가**이고, `name`이 없으면 `agentId`로 같은 왕복을 한다 (§진입 시 체크 4).
12. **출구 없는 금지**: 금지에는 항상 "대신 무엇을 하라"를 출구로 짝지어 붙인다 (`references/delegation-patterns.md §필수 포함 요소` 10번이 단일 출처).
13. **보고 채널 없는 위임**: agent의 plain text 출력은 메인에 도달하지 않는다 — dispatch prompt에 `SendMessage({to: "main"})` 보고 채널을 반드시 포함한다 (`references/delegation-patterns.md §필수 포함 요소` 11번이 단일 출처).
14. **deferred 도구를 이름만 보고 호출**: 스키마 미확보 호출은 `InputValidationError`로 실패한다 — 진입 시 `ToolSearch`로 1회 확보한다 (§진입 시 체크 0).
15. **근거 없는 체크 생략**: 경량 경로는 **판정한 결과**여야 한다 — 판정 결과와 근거를 진입 보고와 decision log에 남긴다 (§경로 판정 게이트).
16. **머지해놓고 in-flight 방치** (무거운 경로): 기본은 배치 머지이고, 즉시 머지했으면 전부에 rebase를 전파한다 (`references/branch-strategy.md §base drift 전파`).
17. **분해를 레이어로 쪼갬**: 수평 분해는 disjoint가 애초에 안 나온다 — 충돌은 판정이 아니라 분해에서 결정된다 (§분해는 충돌 경계로 쪼갠다).
18. **조사를 순차 단독 탐색으로 시작**: read-only 조사·감사는 순차를 택할 근거가 없다 — 첫 행동은 관점별 병렬 fan-out이다 (§조사·감사 작업의 기본값은 병렬 fan-out).
19. **미결 spec 위에서 구현 진입**: TBD가 남은 spec으로 dispatch하면 사용자가 내려야 할 판단이 코드로 굳는다 — 미결이 하나라도 있으면 hard stop한다 (§디스패치 전·보고 수용 게이트, `references/autonomous-driving.md §spec 확정 게이트`).
