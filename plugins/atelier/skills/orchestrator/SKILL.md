---
name: orchestrator
description: Use this skill for any multi-unit work delegated to sub-agents, agent-teams, or worktrees — parallel fan-out, sequential pipelines, long-running agent teams, document deliverables (writing reports, specs, or write-up docs is also delegated Write work), multi-branch research and investigation (codebase surveys, side-effect analysis, comparing options — read-only work counts too), or any moment the main agent is about to use Edit/Write directly (delegate instead). Scope is set by scale, not by kind. Triggers include "여러 작업 병렬로", "동시에 처리", "에이전트 나눠서", "worktree로 분리", "위임해서", "팀으로 작업", "리포트 작성", "보고서로 정리", "스펙 문서 작성", "분석 결과 문서화", "조사해줘", "리서치", "코드베이스 파악", "영향 범위 분석", "사이드이펙트 조사", "여러 방안 비교", "delegate", "parallel agents", "fan-out", "agent team", "sub-agent", "dispatch multiple", "split into tasks", "run in parallel", "write a report", "draft a spec", "write up findings", "research", "investigate", "survey the codebase", "analyze impact", "compare approaches".
version: 0.1.0
---

# Orchestrator Skill

## When to use (트리거 케이스)

이 스킬을 트리거해야 하는 상황:

- 사용자가 **2개 이상의 독립 작업**을 한 번에 요청 ("A랑 B랑 C 같이 해줘", "동시에 처리해줘")
- **병렬 fan-out**이 가능해 보일 때 ("여러 파일 동시에", "병렬로", "parallel", "in parallel")
- **sub-agent / agent-team / worktree 위임**을 명시적으로 요청 ("나눠서", "팀으로", "에이전트 여러 개", "delegate", "dispatch")
- **장기 진행 작업**에 식별 가능한 agent team이 필요할 때 (designer/implementer/reviewer 등)
- **머지 조정**이 필요한 다중 변경 (여러 worktree 결과 통합, 충돌 해결 위임)
- **문서 산출물 작업** — 리포트·스펙·정리 문서 작성 요청 ("리포트로 정리해줘", "스펙 문서 만들어줘", "분석 보고서 작성") — 문서 작성도 Write 작업이므로 위임 대상
- **여러 갈래로 벌어지는 리서치·조사·분석** — 코드베이스 파악, 사이드이펙트·영향 범위 조사, 방안 비교, 자료 수집 ("조사해줘", "리서치", "어디에 영향 가는지 봐줘", "여러 방안 비교"). **read-only 라고 위임 대상에서 빠지지 않는다** — 오히려 병렬 fan-out 이 가장 잘 듣는 자리이고, 메인이 직접 다 읽으면 조율에 써야 할 컨텍스트가 조사 원문으로 채워진다 (`references/autonomous-driving.md §메인 컨텍스트 격리`)
- **메인 에이전트가 Edit/Write/NotebookEdit로 직접 코드를 수정하려는 모든 순간** — 위임으로 전환할지 먼저 검토

**적용 범위는 작업의 종류가 아니라 규모로 정한다.** 구현이냐 문서냐 조사냐로 가르지 않는다 — 여러 단위로 쪼개지거나, 병렬로 벌릴 수 있거나, 메인 컨텍스트를 크게 먹으면 위임 대상이다.

트리거하면 안 되는 상황:
- 단일 파일의 단순 편집 (오버헤드만 늘어남)
- 사용자가 직접 메인이 처리하라고 명시한 경우
- 1턴 안에 끝나는 단발 조회 (파일 하나 확인, git 상태, 테스트 결과 등 결정적 사실 확인)

## 사고 모드 (Mental Model)

이 스킬을 트리거한 순간부터 메인 에이전트는 **편집자가 아니라 관리자**다 — Edit/Write로 직접 코드를 작성하지 않고, Read/Bash로 상태를 파악하고 Task로 일감을 분리·관리하며 Agent로 위임하고 SendMessage로 조율한다.

### 메인 에이전트가 해도 되는 일
- `Read`, `Glob`, `Grep`, `Bash(git status / git log / git diff --stat)` — 작업 분해와 위험도 판단에 필요한 **결정적 사실 확인**에 한정한다. 본격적인 조사·리서치(코드베이스 파악, 영향 범위 분석, 방안 비교)는 메인이 통독하지 않고 **위임**한다 — 그 원문이 메인 컨텍스트에 쌓이면 조율 판단 품질이 떨어진다 (`references/autonomous-driving.md §메인 컨텍스트 격리`)
- `Agent`, `SendMessage`, `Monitor` — 위임과 조율 (spawn한 agent에 **다시 말을 거는 유일한 수단이 `SendMessage`**다 — 가용 판정은 아래 §진입 시 체크 4, `TeamCreate`는 제거됨)
- `TaskCreate` / `TaskList` / `TaskGet` / `TaskUpdate` — 일감을 분리하고 상태를 관리하는 것은 **메인 에이전트의 핵심 룰**이다. 편집을 위임하는 관리자로서 메인의 본업은 일감을 추적 가능한 Task로 쪼개고 상태를 갱신하는 것 — 다중 작업이면 항상 적용하고 단발 1회만 예외다 (`references/agent-monitor.md §Task 시스템`)
- 결과물 취합 후 사용자에게 보고

> **위 조율 도구는 대부분 deferred tool이다 — 이름만 노출돼 있고 스키마는 로드돼 있지 않다.** `SendMessage`·`Monitor`·`TaskCreate`/`TaskList`/`TaskGet`/`TaskUpdate`는 `ToolSearch`로 스키마를 **먼저 확보한 뒤에만** 호출할 수 있고, 확보 없이 호출하면 `InputValidationError`로 실패한다. 진입 시 1회 확보한다 (§진입 시 체크 0).
>
> **문서에 이름이 적혀 있다는 것과 지금 호출할 수 있다는 것은 다르다.** 이 착각의 결과는 "도구가 없다"는 명시적 에러가 아니라 **조율 경로가 조용히 사라지는 것**이다 — 왕복을 못 하니 단발로 대체하고, 판정은 비가용으로 떨어지고, 필수 등급 경로는 에스컬레이션만 남긴다. 확보를 건너뛴 세션은 team 경로 전체를 잃은 채로 진행된다.

### 메인 에이전트가 하면 안 되는 일
- `Edit`, `Write`, `NotebookEdit` — 코드 편집은 항상 sub-agent에 위임
- 코드 작성을 직접 수행 (sub-agent 실패 시에도 편집권을 가져오지 않음 → 사용자에게 보고)
- `EnterWorktree` / `git checkout <other-branch>` 로 worktree 또는 다른 브랜치로 진입 — 메인은 진입 시점의 브랜치에 머문다 (무거운 경로에서는 그것이 epic 브랜치다)

---

## 진입 절차 (Entry Procedure)

**버전 관리 이력에 남을 변경을 만드는 런은 반드시 epic 브랜치 전략으로 동작한다.** 메인 에이전트는 worktree가 아니라 epic 브랜치에 체크아웃된 상태로 작업하고, 위임된 sub-agent들만 worktree로 격리한다. 이 토폴로지를 어기면 머지 경로가 꼬이고 메인이 직접 편집하게 되어 오케스트레이터 원칙이 깨진다.

**tracked 변경을 만들지 않는 런에는 이 전략이 성립하지 않는다** — 머지할 대상도, 격리할 쓰기도 없다. 어느 쪽인지는 아래 §경로 판정 게이트가 먼저 정하고, 이 절의 토폴로지와 체크 1·2·3은 **무거운 경로에만** 적용된다.

### 토폴로지 (무거운 경로)

```
main
  └─ epic/<name>   ← 메인 에이전트 (read + dispatch + report)
       ├─ worktree A → epic/<name>/t1-<slug>  (sub-agent A, base = epic/<name>)
       ├─ worktree B → epic/<name>/t2-<slug>  (sub-agent B, base = epic/<name>)
       └─ worktree C → epic/<name>/t3-<slug>  (sub-agent C: ...)
```

- **메인 = epic 브랜치 자체**. 절대 worktree로 들어가지 않는다.
- **sub-agent = epic 브랜치를 base로 한 worktree**. 결과는 epic 브랜치로 머지한다 — 통합 방식은 **rebase 후 `--ff-only`** 로 고정이고, 브랜치 네이밍·머지 방식·drift 처리의 단일 출처는 `references/branch-strategy.md`다.
- **epic 브랜치 → main 머지는 이 스킬 범위 밖** (사용자 결정 / 별도 release 절차). 반대 방향(main이 움직여 epic이 뒤처짐)은 런 안에서 처리한다 (`references/branch-strategy.md §epic ← main 역방향 drift`).

### 진입 시 체크

오케스트레이터 트리거 직후, 위임을 시작하기 전에 메인이 확인할 것:

0. **조율 도구의 스키마를 확보했는가?** (다른 모든 체크보다 먼저)

```
ToolSearch({query: "select:SendMessage,Monitor,TaskCreate,TaskList,TaskGet,TaskUpdate"})
```

   - **1회만 한다.** 확보된 스키마는 세션 내내 유효하므로 매 dispatch마다 반복하지 않는다.
   - `SendMessage`가 확보되지 않으면 **왕복 조율 수단이 없는 것**이므로 체크 4는 자동으로 비가용이다 (아래 판정 트리의 0단계).
   - 결과에 없는 도구는 이 런타임에 없는 것이다. 이름을 추측해 호출하지 않는다.
   - 이 체크를 건너뛰면 실패가 dispatch 이후에야, 그것도 "조율이 왜 안 되지"라는 형태로 드러난다.

### 경로 판정 게이트 (체크 0 직후, 체크 1 앞)

체크 0을 마치면 **이번 런이 버전 관리 이력에 남을 변경을 만드는가**를 먼저 정한다. 판정 기준은 "git 레포 안인가"가 아니다 — 레포 안이어도 이력에 남지 않으면 경량 경로다.

```
이번 런의 계획된 산출물에 tracked 파일 변경이 있는가?
  ├─ No  → 경량 경로 (체크 1·2·3 건너뛰고 체크 4·5로)
  │        예: 외부 시스템 산출물(이슈 등록·PR 코멘트), read-only fan-out 조사,
  │            repo 밖·gitignore 경로에만 쓰는 산출물(scratchpad, .orchestrator/)
  └─ Yes → 무거운 경로 (체크 1~5 전부)
```

- **판정 시점의 산출물 계획을 기준으로 한다.** 계획 밖 편집이 생기면 아래 §경로 전환.
- **git 레포가 아니면서 편집이 필요한 경우**는 경량 경로가 아니다 — 판정은 `references/delegation-patterns.md §경로 판정 경계 케이스`가 단일 출처다.
- 판정 결과 + 근거를 진입 보고 1줄과 decision log에 남긴다. **생략은 판정이 아니다.**

| 규칙 | 경량 | 근거 |
|---|---|---|
| 조율 도구 스키마 확보(체크 0) · 왕복 조율 판정(체크 4) | 유지 | 조율 경로의 공유 전제이고, 자문·협의체는 read-only다 |
| Task 분리·상태 추적 · 응답 계약 + 보고 채널 · fan-out 복원력 · 취합 보고 | 유지 | 관리자 본업이라 편집 유무와 무관하다 (§안티패턴 13) |
| 병렬/순차 결정 트리 | 유지(축 변경) | 충돌 축이 "파일 집합"에서 "외부 리소스·rate limit"으로 바뀐다 |
| preflight (체크 5) | 유지·강화 | 외부 쓰기 인증이 곧 공유 의존이다 |
| 리뷰어·QA 게이트 | 조건부 유지 | 외부 쓰기는 되돌리기 비용이 있어 **쓰기 전 검토 1회**로 축소한다 |
| epic 브랜치 체크(체크 1) · worktree 격리(체크 2·3) | 생략 | 머지 대상도, 격리할 쓰기도 없다 |
| 토폴로지 가드 · 머지 조정 | 생략 | 가드 불변식(branch == epic)이 성립하지 않는다 |

경계 케이스(조사 리포트를 파일로 남김, 조사 → 구현 연결 등)의 판정은 `references/delegation-patterns.md §경로 판정 경계 케이스`가 단일 출처다.

### 진입 시 체크 (이어서)

1. **현재 브랜치가 epic 브랜치인가?**
   - `git branch --show-current` 확인
   - `main` / 일반 feature 브랜치라면 epic 브랜치를 먼저 만들거나 사용자에게 어떤 epic 브랜치로 진입할지 물어본다 (`git` skill 의 브랜치 생성 또는 plain `git checkout -b epic/<name>`).
2. **현재 메인이 다른 worktree 안에 있지 않은가?**
   - `git rev-parse --show-toplevel` 가 repo의 메인 working tree여야 함
   - worktree 안에서 오케스트레이터를 시작했다면 즉시 메인 working tree로 빠져나오도록 사용자에게 보고
3. **이후 모든 sub-agent dispatch는 `isolation: "worktree"` 로** — worktree의 base가 dispatch 시점 epic 브랜치 HEAD라는 보장은 없다. dispatch prompt에 base 확인·동기화 지시를 반드시 포함한다 (`references/delegation-patterns.md §Prompt 작성 원칙 필수 포함 요소` 9번이 단일 출처)
4. **왕복 조율(team)이 이번 세션에서 가용한가?**

**판정하는 대상은 "team이라는 기능이 켜져 있는가"가 아니라 "spawn한 agent에게 다시 말을 걸 수 있는가"다.** 필수 등급이 요구하는 실질은 *직전 라운드를 기억하는 상대와의 왕복*이고(§team mode 강제 등급 기준 1), 그것을 주는 것은 `name`이라는 파라미터가 아니라 `SendMessage`라는 채널이다.

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

   - **`name` 부재는 비가용이 아니다.** `name`이 없는 런타임에서는 `Agent({run_in_background: true})`가 돌려주는 `agentId`가 그대로 지목자가 되고, 그 agent에 `SendMessage`를 보내면 **직전까지의 transcript에서 이어서 재개**된다. 즉 필수 등급이 요구하는 "라운드 간 맥락 유지"가 agentId 경로에서도 그대로 성립한다 — 이름이 편할 뿐 실질은 같다. `name` 하나를 보고 비가용으로 단정하면 자문·협의체가 통째로 죽는데, 정작 왕복은 가능한 상태다.
   - **채널이 권위인 이유**: 왕복을 실제로 결정하는 것은 spawn 인자가 아니라 재개 채널이다. `team_name`처럼 받아만 놓고 무시되는 인자의 선례가 이미 있으므로, 인자의 존재는 능력의 증거가 못 된다.
   - **env를 권위로 쓰지 않는 이유**: **Bash는 메인과 다른 프로세스로 뜨므로** env가 메인 프로세스를 반영한다는 보장이 없다. 이 false negative가 team 전제 경로 전체를 한 번에 죽이는 가장 흔한 실패다.
   - 진입 시 확정하는 이유: 트리거 시점에 확인하면 이미 다른 경로를 다 태운 뒤라 늦다.

5. **위임 파이프라인이 의존하는 공유 전제가 지금 살아 있는가?** (preflight — dispatch 시작 전 마지막 확인)

   판정 기준은 하나다 — **깨졌을 때 fan-out 전체가 죽는 공유 의존인가?**

   - **대상**: 이번 런의 파이프라인이 실제로 거치는 것만 확인한다 — 위임·머지 경로가 쓰는 CLI의 인증 상태, 필수 외부 서비스 도달 여부 등. **고정된 명령 목록을 따르지 않는다.** 레포마다 파이프라인 의존이 다르고 목록은 곧 stale 해진다. 무엇에 의존하는지는 이번 런의 계획에서 도출한다
   - **방법**: read-only 확인만 한다. 상태를 바꾸는 행위(로그인 시도, 토큰 갱신, 리소스 생성)는 preflight가 아니다
   - **대상이 아닌 것**: 작업 하나만 죽이는 의존. 그건 재위임·게이트 경로가 처리한다 (§병렬 fan-out 복원력). 여기까지 넓히면 진입만 무거워지고 정작 공유 전제는 묻힌다
   - **실패 시**: dispatch를 **시작하지 않는다.** 무엇을 확인하다 실패했는지 + 해제 방법(예: 로그인 명령)을 함께 즉시 보고한다
   - 위 **체크 0이 이 판정의 한 사례다** — `SendMessage` 스키마가 없으면 조율 경로 전체가 죽으므로 공유 전제이고, 그래서 다른 모든 체크보다 먼저 온다
   - 진입에서 확인하는 이유: 이 부류의 실패는 dispatch 뒤에 드러나면 **이미 진행한 작업까지 함께 잃는다.** 같은 확인이 런 시작 시점에는 가장 싸다

### 경로 전환 (경량 → 무거운)

```
전환 트리거: 계획에 없던 tracked 파일 편집이 필요해진 순간
  1. 편집 dispatch를 시작하지 않는다 (선-dispatch 후-체크 금지)
  2. 생략했던 체크 1·2·3을 지금 실행한다 (late gate)
       비-git이면 → delegation-patterns.md §경로 판정 경계 케이스
  3. 기존 Task는 그대로 유지한다 — 재분해하지 않는다
  4. decision log에 `경로 전환` + 전환 사유 + 전환 시점을 남긴다
  5. 이후 dispatch는 전부 무거운 경로 규칙 (토폴로지 가드 재개)
```

**역방향(무거운 → 경량) 전환은 없다.** 브랜치가 이미 있으면 계속 쓴다 — 되돌려서 얻을 것이 없고, 만들어 둔 머지 경로만 잃는다.

---

## 표준 절차 (Workflow)

```
0. 진입 확인 (Entry)        → 조율 도구 스키마 확보(ToolSearch) + 경로 판정 게이트 + §진입 시 체크
1. 분해 (Decompose)        → 작업을 독립 단위로 쪼갠다 — 복잡·모호한 요구는 아키텍트 협의체
                             (설계 생성 → 별도 agent 의 심문·검증)에 위임해 검증된 task 를 도출
                             (`references/architect-council.md`)
2. 위험도 분석 (Analyze)    → 단위 간 충돌 위험 식별
3. 실행 계획 (Plan)         → 병렬/순차 결정 + 위임 형태(단발/team) 결정
4. 위임 (Dispatch)          → Agent 호출 (`references/delegation-patterns.md`)
5. 모니터링 (Monitor)       → 진행 추적, 정체 감지, 사용자 보고
6. 검토·QA 게이트 (Gate)    → 작업마다 검토 에이전트 + QA 에이전트(검증 테스트 추가) 필수 + DB 접촉 작업은
                             DBA 에이전트 추가, 전부 pass여야 머지
7. 머지 조정 (Coordinate)   → 게이트 통과분만 epic 브랜치로 통합 + 충돌 위임 + worktree 정리
8. 보고 (Report)            → 사용자에게 결과 요약
```

각 단계의 상세 패턴은 아래 references에 있다. **경로별 차이(어느 단계가 유지·축소·생략되는지)는 §경로 판정 게이트의 유지·생략 표가 단일 출처다.**

### 일감을 Task로 분리·관리하는 것은 메인의 핵심 룰

메인 에이전트는 편집자가 아니라 **관리자**다(위 *사고 모드*). 관리자의 본업은 일감을 추적 가능한 단위로 쪼개고 그 상태를 끝까지 관리하는 것이다. 따라서 1단계에서 분해한 일감 목록을 메인의 머릿속이나 대화 흐름에만 두지 말고 **Task 시스템(`TaskCreate`/`TaskList`/`TaskGet`/`TaskUpdate`)으로 분리·등록·갱신**하는 것은 선택이 아니라 메인의 핵심 룰이다. 일감이 목록이 아니라 추적 가능한 상태 객체가 되어야 메인과 사용자 모두 진행 상황을 한눈에 본다.

- **분리**: 분해된 각 독립 단위 = Task 1개. dispatch 전에 등록해 "무엇을 할 것인가"를 먼저 가시화한다.
- **상태 추적**: dispatch 시 `in_progress`, 머지/완료 시 `completed`로 갱신해 진행률이 목록에 드러나게 한다.
- **의존성·소유자**: 순차 의존이 있으면 Task의 의존성(blocked-by)으로, 어느 worktree/agent가 맡았는지는 owner로 표기해 병렬 상태를 추적한다.
- **적용 범위**: 다중 작업이거나 의존성이 있으면 **항상** 적용한다. 단발 1회 작업만 오버헤드라 예외로 생략한다.

상세 사용법(필드·의존성·owner)은 `references/agent-monitor.md §Task 시스템`이 단일 출처다. 자율 모드에서의 Task 추적 규칙은 `references/autonomous-driving.md`를 따른다.

### 디스패치 전·보고 수용 게이트 (Dispatch Preconditions)

reference를 읽지 않아도 성립해야 하는 게이트 세 개다. 발동 시점만 여기 두고, 각 계약의 단일 출처는 지목된 reference다.

- **implementer dispatch 전 — 설계 승인 마커 확인**: 마커가 없으면 dispatch하지 않고 설계 단계로 회귀한다 (`references/architect-council.md §설계 승인 마커`).
- **테스트 작성이 포함된 구현 dispatch 전 — 테스트 인프라 발견**: 레포의 테스트 러너·픽스처·하네스·유사 기존 테스트가 file:line으로 인용되기 전에는 테스트 작성 단계에 진입시키지 않는다 (`references/delegation-patterns.md §테스트 인프라 발견`).
- **sub-agent 보고 수용 전 — 증거 계약 확인**: 증거 없는 claim은 수용하지 않고 재디스패치하며, 부재 주장(negative claim)은 교차 검증 후에만 수용한다 (`references/delegation-patterns.md §증거 계약`).

### 작업 케이스마다 검토 에이전트·QA 에이전트는 필수 (Review & QA Gate)

코드를 바꾸는 각 작업(work case)은 구현이 끝나면 머지 전에 전용 게이트 에이전트를 **반드시** 거친다 — Task 분리가 핵심 룰인 것과 동급의 필수 규칙이다. 게이트가 references에 묻혀 누락되지 않도록, **적용 여부**만 본문에 항상 로드되는 규칙으로 둔다.

- **검토 에이전트 (reviewer)** — `구현 ↔ 요구사항`, **QA 에이전트 (qa)** — `요구사항 ↔ 테스트`(누락 시 검증 테스트를 추가·보강), **DBA 에이전트 (dba)** — DB 접촉 작업만 조건부로 `구현 ↔ DB 안전성`.
- **AND 게이트**: 전부 `pass`여야 머지 후보로 승급. 하나라도 `reject`면 findings를 실어 재위임한다.
- 게이트 에이전트들은 **구현 sub-agent와 다른 agent**다 — 자기 코드 자기 검증 금지.
- QA의 테스트 추가도 편집이므로 **`isolation:"worktree"` subagent로 위임**한다 (메인은 직접 편집하지 않는다 — *사고 모드*).
- 게이트 역할의 tier는 아래 §모델 라우팅 전략에 따라 dispatch 시점에 정한다 — 자동 머지의 유일한 안전장치라 보통 더 높은 역량을 둘 가치가 있다.
- 예외는 Task 룰과 동일하게 **단발 1회·read-only 작업만**이다.
- 경로별 차이는 §경로 판정 게이트의 유지·생략 표를 따른다.

역할별 입력·검증 질문·출력 계약, DB 접촉 판정, 게이트 거부의 재위임 예산·기록 등 세부 규칙은 `references/autonomous-driving.md §리뷰어·QA 게이트`가 단일 출처다. spec 문서를 입력으로 구현하는 경우만 `references/spec-driven-review.md`(검토자=spec↔구현, QA 매니저=spec↔테스트)로 특수화된다.

### 병렬 fan-out 복원력 — 실패한 조각도 누락 없이 (Resilience)

대규모 fan-out(예: 15개 이상 agent)에서는 일부 agent가 504 Gateway Time-out·API 에러로 죽는 것을 정상 케이스로 전제한다. 실패가 조용히 누락되면 최종 취합 리포트의 수치 일관성이 깨진다.

- **체크포인트**: 각 agent 결과는 완료 즉시 파일로 저장한다 — 전체 완료를 기다리지 않는다 (저장 주체는 agent 자신 — 메인의 Edit/Write 금지 유지).
- **재시도**: gateway/API 에러로 실패한 agent는 같은 prompt로 N회(기본 3회)까지 재시도한다.
- **폴백**: 재시도 소진 시 해당 조각은 메인의 read-only 직접 분석 또는 조건을 바꾼 새 agent 재위임으로 대체해 취합을 완성한다 — 미완성 상태로 종료하지 않는다. 폴백도 편집이 필요하면 위임한다 (*사고 모드*).
- **투명 보고**: 최종 보고에 어떤 agent가 실패/재시도/폴백되었는지 명시한다 — "실패한 건도 누락 없이 보고" 원칙의 fan-out 구체화다.

상세 절차(체크포인트 파일 규약·재시도 예산·폴백 판단)는 `references/agent-monitor.md §fan-out 복원력`이 단일 출처다.

---

## 분해는 충돌 경계로 쪼갠다 (병렬 판정 앞 단계)

아래 병렬/순차 결정 트리는 **이미 쪼개진** 작업을 거르는 사후 필터다. 분해(1단계)가 충돌을 만들어 놓으면 트리는 그것을 전부 순차로 떨어뜨릴 수밖에 없다 — **병렬 이득은 판정이 아니라 분해에서 결정된다.**

- **수직 슬라이스로 쪼갠다**: 기능 단위(A 기능의 모델+서비스+API+테스트)로 자른다. 레이어 수평 분해(모델 전부 / 서비스 전부 / 컨트롤러 전부)는 작업마다 같은 파일들을 훑게 되어 disjoint가 애초에 나오지 않는다.
- **hot-spot은 별도 task로 뽑는다**: 작업들의 의도는 안 겹치는데 **같은 위치에 항목을 추가**하게 되는 파일은 병렬 작업에서 편집을 금지하고 마지막에 통합 task 1개로 순차 처리한다. 안 하면 **hot-spot 하나 때문에 fan-out 전체가 순차로 떨어진다** (사례·판정·계약은 `references/branch-strategy.md §hot-spot 파일`이 단일 출처).
- **공유 인터페이스는 선행 task로 앞세운다**: 여러 작업이 같은 타입·시그니처를 필요로 하면 그 정의를 먼저 한 task로 확정·머지한 뒤 나머지를 병렬로 띄운다. 각자 정의하게 두면 머지에서 의미 충돌이 되고, 그건 자동 해결 대상이 아니다.

---

## 병렬 vs 순차 결정 트리

오케스트레이터의 가장 중요한 판단. **머지 시 충돌이 가장 적고 안정적인 쪽**을 선택한다.

```
작업 A, B의 변경 파일 집합을 식별
  │
  ├─ disjoint (겹치는 파일 없음)
  │    └─ 의존성 없음? → 병렬 (각자 worktree-isolated agent)
  │       의존성 있음? → 순차 (A 결과 → B 입력)
  │
  └─ overlap (같은 파일 수정)
       ├─ 겹치는 파일이 전부 hot-spot? → 병렬 유지 + hot-spot을 통합 task로 분리
       │                                  (`references/branch-strategy.md §hot-spot 파일`)
       └─ 그 외
            같은 라인 영역 가능성? → 순차 (단일 worktree에서 직렬)
            명확히 다른 영역? → 순차 권장 (안전), 병렬은 경험상 안전한 경우만
```

판단 근거:
- **병렬의 이득**: 시간 단축, 독립 컨텍스트
- **병렬의 비용**: 머지 시 충돌 → 사람 개입 필요
- **기본 규칙**: 의심스러우면 순차. 병렬은 disjoint가 명백할 때만.

경로별 차이(경량에서 disjoint 판정 축이 무엇으로 바뀌는지)는 §경로 판정 게이트의 유지·생략 표를 따른다. 축이 바뀌어도 규칙은 같다: 의심스러우면 순차다.

---

## 위임 형태 결정

| 상황 | 형태 | 도구 |
|------|------|------|
| 1회성 독립 작업, 결과물 단일 | 단발 sub-agent | `Agent({...})` |
| 여러 agent 협업·식별/제어 필요 (read-only 조율) | agent team | `Agent({name, ...})` — `name` 없는 런타임이면 `Agent({run_in_background: true})` + 반환 `agentId` — 에 `SendMessage` (가용 판정 §진입 시 체크 4·`team_name` 무시·편집 격리는 subagent) |
| 파일 충돌 위험 있는 병렬 | worktree-isolated | `Agent({isolation: "worktree", ...})` |

> **격리는 subagent만 보장**: agent team teammate는 공유 checkout이라 worktree 격리가 없다 — 편집·격리는 `isolation:"worktree"` subagent, team은 조율 전용 (`references/delegation-patterns.md §Agent team 사용 패턴`이 단일 출처).

자세한 판단 기준과 prompt 작성법은 `references/delegation-patterns.md`.

### team mode 강제 등급 (단일 출처)

team을 "쓰면 좋다"로 두면 폴백이 사실상 기본값이 되어 team 전제 경로가 조용히 사라진다. 경로별 **강제 등급을 2단계로 확정**하고, 등급별 반응을 이 절이 단일 소유한다.

**등급 기준 — 두 조건을 모두 만족하면 `필수`다**:

1. **대체 불가 (핵심)**: 경로의 본질이 **왕복 대화**인가. 단발 subagent는 매 왕복마다 패킷을 재합성해야 하고 **직전 라운드를 기억하지 못한다** — 반문·재답변·상호 반박이 성립하지 않으므로 폴백은 같은 일을 더 비싸게 하는 게 아니라 **아예 다른(그리고 실질 없는) 일**이 된다. 이것이 강제해야 하는 이유다.
2. **비용 없음**: 경로가 **read-only 조율(편집 없음)**인가. team의 유일한 실질 비용인 공유 checkout 오염 위험이 애초에 없다. 이것이 강제해도 되는 이유다.

둘 중 하나라도 어긋나면 `선호`다. 특히 **편집이 개입**하는 경로는 격리를 보장하는 것이 team이 아니라 `isolation:"worktree"` subagent이고, 검증 차원 자체는 단발로도 유지되므로 폴백이 본질을 잃지 않는다.

| 경로 | 등급 | team 가용인데 단발로 대체 | team 비가용 |
|------|------|---------------------------|-------------|
| 자문 조회 (`references/advisory-consult.md`) | **필수** | **위반** — 권고→반문→재답변이 성립하지 않으므로 그 1회 왕복은 자문이 아니다. 결과를 채택하지 않고 위반을 기록 | 자문 생략하고 원래 하려던 에스컬레이션으로 진행 |
| 아키텍트 협의체 (`references/architect-council.md`) | **필수** | **위반** — 라운드 간 맥락이 끊겨 상호 반박이 성립하지 않고, "협의체를 돌렸다"는 기록만 남는다 | 폴백 없이 즉시 에스컬레이션 |
| spec 검토·QA 게이트 (`references/spec-driven-review.md`) | 선호 | 허용 — 단발 subagent 2개로 두 검증 차원 유지 | 동일 폴백 |
| review→fix 루프 (`references/autonomous-driving.md`) | 선호 | 허용 — 단발 격리 subagent 재위임 | 동일 폴백 |

**필수 등급에는 가드 두 개가 붙는다** (없으면 강제가 아니라 권고다): **spawn 확인**(teammate로 실제 떴는가)과 **decision log 필수 필드**(`실행 형태`·`판정 근거`). 절차는 `references/delegation-patterns.md §spawn 확인`, 기록 형식은 `references/autonomous-driving.md §의사결정 기록`이 단일 출처다.

**등급별 기본값은 그 등급 안에서만 유효하다.** 선호 등급의 "의심스러우면 단발을 고른다"를 필수 등급으로 옮기지 않는다 — 거기서 단발은 경로의 실질 자체를 없앤다.

---

## 모델 라우팅 전략 (Model Routing)

모델 라우팅에는 **역할 기준 원칙**과 그 안에서의 **작업별 tier heuristic**이 있다. 원칙은 이 절이 단일 출처다.

### 역할 기준 원칙 (단일 출처)

```
위임 종류 판정 — 매 dispatch
  │
  └─ 이 dispatch가 §자문 조회의 네 트리거에 해당하는가?
       │
       ├─ Yes → 자문 조회 (advisory)
       │          tier   : 메인보다 상위 허용        ← 유일한 상위 예외
       │          실행   : team member 필수 (폴백 금지)
       │
       └─ No  → 집행 위임 (executive)  ※ 여집합 — 예외 없음
                  포함  : 구현 · 문서 생성 · 리서치/조사(discovery)
                          · 분석 · 리뷰 · 게이트 · 충돌 해결
                  tier   : ≤ 메인 tier                ← 상한, 예외 없음
                  산출물이 파일이냐 요약이냐로 가르지 않는다

모든 위임 공통 제약
  model 명시     필수 — 미지정(상속) 금지
  tier 재평가    매 dispatch (고정 매핑 아님)
  비표준 선택    decision log에 근거와 함께 기록
  문서에 모델명  박지 않음 — 역량 수준(최상위/중간/경량)만 표기
```

- **tier 상한의 근거**: 분해·위임·조율·머지 판단이 스웜 전체 결과의 **상한**을 결정한다. 메인보다 강한 집행 agent를 붙여도 그 결과를 판정하는 것은 메인이라 상한이 올라가지 않는다. 조사·discovery가 아무리 중요해 보여도 집행 위임이므로 이 상한을 넘지 못한다.
- **`model` 명시가 필수인 근거**: 상속에 맡기면 메인의 tier가 그대로 번져 배분 자체가 무의미해진다.
- **모델명을 안 박는 근거**: 세대가 바뀌어도 이 원칙이 그대로 성립해야 한다. 역량 수준 ↔ 실제 모델명 매핑은 dispatch 시점 판단에 맡긴다.

작업 유형 → 시작 tier 표는 `references/delegation-patterns.md §모델 선택`이 단일 출처다 (여기서 중복 정의하지 않는다). 자율 루프의 작업별 배분 원칙은 `references/autonomous-driving.md §모델 분배`.

### 자문 조회 — 상위 tier 예외 (단일 출처)

위임에는 두 종류가 있다. 위 역할 기준 원칙은 **집행 위임**의 규칙이고, **자문 조회**는 tier 방향이 반대인 별도 경로다.

| | 집행 위임 (executive) | 자문 조회 (advisory) |
|---|---|---|
| 범위 | **자문을 제외한 전부** — 구현·문서·리서치·조사·리뷰·게이트 | 아래 네 트리거에 해당하는 소집만 |
| 산출물 | 코드·문서·조사 결과 (맡긴 일의 수행) | 권고 + 근거 (read-only) |
| 결정권 | sub-agent 결과를 메인이 머지 판단 | **메인에 100% 잔류** — advisor는 `pass`/`reject` 권한이 없다 |
| tier | 메인 ≥ sub-agent | **메인 < advisor 허용** (유일한 예외) |
| 실행 형태 | `isolation:"worktree"` subagent | **team member 전용** — 필수 등급, 폴백 금지 (위 §team mode 강제 등급) |

예외가 원칙을 훼손하지 않는 이유: 역할 기준 원칙의 근거는 "메인의 판단이 스웜 결과의 **상한**을 결정한다"이고, 자문은 그 상한을 **올리는** 방향이기 때문이다. 결정권이 메인을 떠나는 순간 이 근거가 무너진다.

- **역할 제한은 계약이지 도구 보장이 아니다**: teammate는 도구 권한을 제한할 수단이 없다(agent 정의 지정은 단발 subagent의 경로다). 편집·`SendMessage`·재위임 금지는 **패킷의 금지 계약**으로 걸고, 위반은 **자문 소집 전후의 토폴로지 가드로 사후 탐지**한다. 왕복 조율(자문의 실질)과 도구 보장은 현재 동시에 얻을 수 없으며, 이 경로는 왕복을 택했다 — 없는 보장을 있다고 적지 않는다.

- **사용 조건**: 진입 시 확정한 team 가용 판정(위 §진입 시 체크 4). 자문은 **필수 등급**이므로 가용인데 단발 subagent로 대체하면 **위반**이다 — 등급 기준과 위반 처리는 위 §team mode 강제 등급이 단일 출처다.

- **team 비가용 = 자문 경로 차단** (이 한 줄은 `references/advisory-consult.md`를 읽지 않아도 성립해야 하므로 본문에 둔다): 트리거에 도달해도 **소집하지 않고, 무엇으로도 대체하지 않으며**, 원래 하려던 에스컬레이션으로 진행하고 그 사실을 판정 근거와 함께 decision log에 남긴다. **사용자가 명시 요청해도 우회하지 못한다** — 원한 것은 상위 tier의 관점이지 "자문했다는 서술"이 아니다. 절차는 `references/advisory-consult.md §게이트 0`, 자율 모드의 `max_advisory_consults = 0` 고정은 `references/autonomous-driving.md §자율 계약`이 단일 출처다.
- **고정하는 것은 계약, 관점·인원은 런타임 주입**: 어떤 렌즈로 몇 명을 붙일지는 문제를 보고 메인이 정한다 — 관점을 나눠 **자문 스웜**으로 소집할 수 있다 (협의체와 같은 정책·메커니즘 분리).
- **상시 경로가 아니다**: 자문은 에스컬레이션 직전 한 단계다. 소집 트리거·패킷 계약·출력 계약·수명은 `references/advisory-consult.md`가 단일 출처다.

### 역할별 모델 제약 — 지시로 받고, 설정 파일을 만들지 않는다

"이 모델은 리뷰어로 쓰지 마라" 같은 제약은 **사용자 지시**로 받는다. 받은 제약은 런 내내 일관되게 적용하고 근거와 함께 decision log에 남긴다.

- **역할을 지목한 제약은 그 역할에만 적용된다 — 다른 역할의 기본값이 되지 않는다.** "자문은 X 모델로"는 자문 dispatch 하나만 정한 것이지 "X가 이 런의 좋은 모델"이라는 뜻이 아니다. 특히 **자문 tier가 discovery·조사·구현으로 번지는 것**은 흔한 오적용이다 — 자문은 상위 tier 예외 경로이고 나머지는 전부 집행 위임이라 메인 tier 상한을 받는다(위 §역할 기준 원칙). 제약을 다른 역할로 넓히려면 사용자에게 확인받는다.
- **설정 파일 규약을 두지 않는다.** 파싱·검증하는 코드가 없는 설정 파일은 오타·스키마 위반을 조용히 삼켜 "걸린 줄 알았는데 안 걸린" 상태를 만든다. 강제력 없는 규약에 파일을 붙이면 강제력이 있는 것처럼 보이기만 한다.
- **세션을 넘겨 유지해야 하는 제약**은 이미 로드되는 문서(프로젝트 `CLAUDE.md`, `.claude/rules/*`)에 적는다 — 실제로 읽히는 경로에만 규약을 얹는다.
- **제약은 tier 선택 범위만 좁힌다**: 어떤 제약도 역할 기준 원칙을 뒤집지 못한다. 오케스트레이터가 집행 위임 sub-agent보다 낮은 tier로 내려가거나 `model` 미지정으로 상속하는 배정은 허용되지 않는다.
- 제약 때문에 표준 heuristic의 모델을 쓸 수 없으면 **인접 tier로 대체**한다 — 판정 품질이 게이트인 역할(reviewer/QA/DBA)은 인접 상위 우선, 그 외는 인접 하위 우선. 대체는 비표준 선택이므로 근거와 함께 기록한다.

---

## References (필요할 때만 로드)

메인 컨텍스트 절약을 위해 아래 파일은 **명시적으로 필요한 단계에서만** Read한다.

| 파일 | 언제 읽을지 |
|------|-------------|
| `references/architect-council.md` | 분해(1단계) 시 요구가 복잡·모호해 아키텍트 협의체(설계 생성 ↔ 심문 검증)로 분석·검증 후 task 를 도출할 때 |
| `references/delegation-patterns.md` | 위임 형태(단발 vs team)를 결정하거나 sub-agent prompt를 작성할 때, **경로 판정이 경계 케이스일 때**(§경로 판정 경계 케이스가 단일 출처), **원인 불명 결함·회귀를 조사할 때**(§근본원인 swarm — 축 분해·증거 계약·가설 랭킹) — **작업 유형 → tier 표의 단일 출처** (역할 기준 원칙·역할별 모델 제약은 위 §모델 라우팅 전략이 단일 출처) |
| `references/branch-strategy.md` | 무거운 경로에서 **브랜치를 어떻게 운영할지** 정할 때 — worktree 브랜치 네이밍, hot-spot 파일 분리 계약, 머지 정책(배치 vs 즉시+전파), 통합 방식, epic ← main 역방향 흡수, 반복 충돌의 재분해 트리거. **위 여섯의 단일 출처** (분해 원칙은 위 §분해는 충돌 경계로 쪼갠다, 단일 rebase의 충돌 해결 전략은 `git` skill) |
| `references/worktree-lifecycle.md` | 병렬 dispatch 직전, 또는 worktree 정리/머지를 다룰 때 |
| `references/agent-monitor.md` | 백그라운드 agent 진행 추적, Task 시스템으로 다중 작업 상태·의존성을 추적할 때, 또는 대규모 fan-out에서 실패 대비 복원력(체크포인트·재시도·폴백) 절차를 적용할 때 |
| `references/merge-coordinator.md` | 병렬 결과를 통합할 때 (순서 결정, 충돌 처리) |
| `references/autonomous-driving.md` | 자율 루프(분해→위임→머지 self-drive)를 돌릴 때 — **오케스트레이터 기본 동작**. 계약·가드레일·종료 조건·에스컬레이션 + **작업마다 필수인 리뷰어·QA 게이트**(검토 + 검증 테스트 추가)의 단일 출처 (단발 fan-out 1회면 불필요) |
| `references/advisory-consult.md` | 상위 tier 자문을 소집할 때 (협의체 예산 소진 tie-break, 게이트 재위임 루프, 되돌리기 어려운 결정, 사용자 요청) — **소집 트리거·패킷 계약·출력 계약·수명의 단일 출처** (집행/자문 분리와 tier 예외 원칙 자체는 위 §모델 라우팅 전략이 단일 출처). 진입 시 team 비가용으로 확정됐으면 읽을 필요 없다 |
| `references/spec-driven-review.md` | 검토·QA 게이트가 **spec 문서를 입력으로 구현**하는 경우의 특수화 — 팀 모드로 검토자(spec↔구현)·QA 매니저(spec↔테스트)를 상주시켜 worktree 코드를 계속 리뷰·개선 (spec 입력이 없으면 일반 게이트 사용) |

---

## 사용자 보고 원칙

오케스트레이터는 **기본적으로 자율 주행**한다 — 진입 시 자율 계약을 1회 보고하고, 가드레일(종료 조건·예산·자동 중단) 안에서 자동 재위임·머지·충돌 해결을 사람 개입 없이 진행한다. 자율 계약·루프·에스컬레이션 규칙은 `references/autonomous-driving.md` 가 단일 소유한다.

- **시작 시**: 분해된 작업 목록 + 병렬/순차 결정 + 자율 계약(종료 조건·예산·hard stop·결정 기록 위치)을 한 번에 보고
- **진행 중**: 침묵 (정상 루프는 보고하지 않음) — hard stop / 에스컬레이션 발생 시에만 즉시 보고
- **종료 시**: 종료 사유(완료/예산 소진/에스컬레이션) + 머지 결과 + 미해결 항목 + **3분류 판정 요약(DONE/BLOCKED/NOT-STARTED) + 핸드오프 파일 경로** + 의사결정 요약 (핸드오프 계약은 `references/autonomous-driving.md §종료 핸드오프`)

단, 에스컬레이션 조건(되돌리기 어려운 행위·토폴로지 위반·도메인 의미 충돌·예산 소진 등)은 자율 모드라도 **항상** 멈추고 보고한다 (`references/autonomous-driving.md §에스컬레이션`).

**opt-out — 휴먼-인-더-루프**: 사용자가 단계별 확인을 명시하면(예: "확인받으면서", "단계마다 물어봐", "babysit", "자동으로 머지하지 마") 자율 주행을 끄고 휴먼-인-더-루프로 전환한다. 이때는 자동 개입(SendMessage 명령 주입·자동 머지·자동 충돌 해결)을 하지 않고, 정체·실패·머지 결정을 사용자에게 보고하고 결정을 받는다 (`agent-monitor.md` / `merge-coordinator.md` 의 HITL 규칙).

---

## 안티패턴

1. **편집권 회수**: sub-agent가 실패하면 메인이 직접 Edit로 마무리 → 금지. 다시 위임하거나 사용자에게 보고.
2. **충돌 위험 무시한 병렬화**: 시간 단축에 끌려 disjoint 검증 없이 병렬 → 머지 지옥. 의심스러우면 순차.
3. **컨텍스트 의존 prompt**: "위에서 말한 그 파일을" 같은 prompt → sub-agent는 메인 대화를 못 봄. 자기완결적으로 작성.
4. **Reference 일괄 로드**: 시작하자마자 4개 reference를 모두 Read → 컨텍스트 낭비. 단계별로 필요할 때만.
5. **무한 폴링**: `Bash sleep` 루프로 agent 상태 확인 → 금지. `run_in_background: true` + 완료 알림 사용.
6. **메인이 worktree에서 시작** (적용 경로는 §경로 판정 게이트): 메인을 worktree에 진입시킨 채 오케스트레이션 → 머지 경로 꼬임. 메인은 epic 브랜치의 메인 working tree에서만 동작.
7. **epic 브랜치 우회** (적용 경로는 §경로 판정 게이트): main 또는 임의 feature 브랜치에서 sub-agent를 바로 dispatch → 결과를 어디로 모을지 모호. 반드시 epic 브랜치를 만들고 거기서 dispatch.
8. **자문 흉내**: 자문 경로가 비활성인데 단발 subagent 1회 왕복이나 메인 자신의 판단을 "자문 결과"로 포장 → 실질 없이 기록만 남는다. 경로가 없으면 없는 대로 진행하고, 그 시점의 판단은 **메인 자신의 판단으로 명시**한다. 필수 등급 경로의 폴백은 decision log의 `실행 형태` 필드로 사후 탐지된다 (§team mode 강제 등급).
9. **고무도장 메인**: 상위 tier라는 이유로 권고를 검토 없이 채택 → 실질 오케스트레이터가 advisor가 되고 메인은 전달자로 전락한다. 결정권은 메인에 있고, 채택도 기각도 사유와 함께 기록한다.
10. **자문 tier가 다른 역할로 번짐**: "자문은 X 모델로"라는 역할 지목 제약을 받고 discovery·조사·구현까지 X로 dispatch → 상위 tier 예외가 전역 기본값이 되어 집행 위임의 tier 상한이 무너지고 비용도 폭증한다. 자문 외의 모든 위임은 집행 위임이며 메인 tier를 넘지 못한다 (§역할 기준 원칙 / §역할별 모델 제약).
11. **신호 하나로 team 비가용 단정**: `printenv`가 비었다는 이유로, 또는 `Agent` 스키마에 `name`이 없다는 이유로 team 경로를 닫음 → 판정 하나가 틀리면 team 전제 경로가 동시에 죽는다. 권위 있는 신호는 **`SendMessage`로 다시 지목할 수 있는가**이고, `name`이 없으면 `agentId`로 같은 왕복을 한다 (§진입 시 체크 4).
12. **출구 없는 금지**: dispatch prompt에 금지만 넣고 "대신 무엇을 하라"를 안 줌 → sub-agent는 그 상황에서 뭐라도 해야 하므로 위반이 재발한다. 금지에는 항상 출구를 짝으로 붙인다 (`references/delegation-patterns.md §필수 포함 요소` 10번이 단일 출처).
13. **보고 채널 없는 위임**: background agent에 목적·범위만 주고 "무엇으로 보고하라"를 안 줌 → **agent의 plain text 출력은 메인에 도달하지 않으므로** 중간 보고·질의·부분 결과가 통째로 유실되고, 메인에는 침묵으로 보인다. 완료 알림만 남아 "일은 했는데 답이 없는" 상태가 된다. dispatch prompt에 `SendMessage({to: "main"})` 보고 채널을 반드시 포함한다 (`references/delegation-patterns.md §필수 포함 요소` 11번이 단일 출처).
14. **deferred 도구를 이름만 보고 호출**: 문서에 `SendMessage`가 적혀 있으니 쓸 수 있다고 가정 → 스키마 미확보 상태의 호출이 `InputValidationError`로 실패하고, 조율을 포기한 채 단발로 흘러간다. 진입 시 `ToolSearch`로 1회 확보한다 (§진입 시 체크 0).
15. **근거 없는 체크 생략**: 조사·이슈 등록처럼 편집이 없어 보인다는 이유로 경로 판정 없이 체크 1·2·3을 건너뜀 → 같은 생략이 tracked 편집이 섞인 런에서도 반복되고, 사후에는 판정한 것인지 빠뜨린 것인지 구분되지 않는다. 경량 경로는 **판정한 결과**여야 하며, 판정 결과와 근거를 진입 보고와 decision log에 남긴다 (§경로 판정 게이트).
16. **머지해놓고 in-flight 방치** (무거운 경로): 아직 돌고 있는 worktree가 있는데 먼저 끝난 결과를 머지하고 알리지 않음 → 옛 base 위의 작업이 **이미 머지된 수정을 되돌린 채 조용히 통과**할 수 있다. 기본은 배치 머지이고, 즉시 머지했으면 전부에 rebase를 전파한다 (`references/branch-strategy.md §base drift 전파`).
17. **분해를 레이어로 쪼갬**: 모델 전부 / 서비스 전부 / 컨트롤러 전부로 나눔 → 작업마다 같은 파일을 훑어 disjoint가 애초에 안 나오고, 병렬/순차 트리가 전부 순차로 떨어뜨린다. 충돌은 판정이 아니라 분해에서 결정된다 (§분해는 충돌 경계로 쪼갠다).
