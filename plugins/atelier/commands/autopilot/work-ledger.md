---
description: "ledger의 Ready task를 claim하여 issue-implementer로 구현하고 PR을 생성합니다 (첫 reader)"
argument-hint: "[--epic <NAME>]"
allowed-tools: ["Bash", "Read", "Agent"]
---

# Work Ledger (/atelier:work-ledger)

결정적 ledger(SQLite)에 누적된 Ready task 를 epic 별로 claim 하여 `issue-implementer` 에이전트에 디스패치하는 첫 reader 파이프라인입니다. gap-watch / qa-boost / ci-watch 가 쓴 task 를 실제 코드로 옮기는 단일 경로입니다.

> 파이프라인 제어와 단계별 프로토콜은 `autopilot-pipeline` skill 에 있습니다. 이 커맨드는 진입점만 담습니다.

## 사용법

```bash
# 1) 이벤트 드리븐 모드 (autopilot Monitor가 TASK_READY 이벤트 수신 시 호출)
/atelier:work-ledger --epic <NAME>

# 2) 매뉴얼 / cron 모드 (인자 없음 — 모든 epic을 selection strategy로 순회)
/atelier:work-ledger
```

> hybrid 모드에서는 `autopilot watch` daemon 이 `TASK_READY epic=<E> task_id=<ID>` 이벤트를 emit 하면 Monitor 가 `--epic <E>`를 붙여 호출합니다. 해당 호출은 selection strategy 를 skip 하고 단일 epic 만 claim 합니다. cron / 매뉴얼 호출(인자 없음)은 by-depth selection strategy 를 사용합니다.

## Context

- 설정 파일: !`cat github-autopilot.local.md 2>/dev/null | head -20 || echo "설정 파일 없음 - 기본값 사용"`
- 현재 브랜치: !`git branch --show-current`

## 작업 프로세스

### Step 0: 인자 파싱

`$ARGUMENTS`에서 `--epic <NAME>` 을 추출합니다 (있으면 selection strategy skip, 단일 epic claim).

### Step 1: 전처리 (base 동기화)

`autopilot-pipeline` `references/pipeline-control.md` 의 Base 브랜치 동기화(`branch-sync` 스킬)를 수행합니다.

> work-ledger 는 idle/capacity/throttling 전처리를 사용하지 않습니다 (`references/ledger.md` §A 참조). base 동기화 후 곧바로 epic 부트스트랩 → selection → claim → dispatch 흐름입니다.

설정에서 `max_parallel_agents`(기본 3), `quality_gate_command`, `work_branch`/`branch_strategy`, `label_prefix`, `work_ledger.priority`(기본 `by-depth`) 를 읽습니다.

### Step 2: Ready task claim → 구현 → PR

`autopilot-pipeline` `references/ledger.md` §A(work-ledger) 절차를 수행합니다:
- Ledger Epic 부트스트랩(3 writer epic 멱등) → Selection Strategy(by-depth/by-age/round-robin/리스트) → Task Claim(epic 당 1개) → 디스패치(Agent Team) → 결과 수집 + PR 생성(branch-promoter) → 결과 보고 + 세션 통계

> 디스패치(서브그룹 분할·rate-limit 백오프·병렬 실행) 메커니즘은 `orchestrator` skill 에 위임합니다 (ledger.md 가 "무엇을 전달할지"만 정의).

## 주의사항

- 한 cycle 에서 epic 당 **최대 1개** task 만 claim (per-epic fairness). claim 순서는 selection strategy 가 결정 (default `by-depth`)
- task complete 은 **호출하지 않습니다** — pr-merger 의 close-the-loop 이 PR 머지 시 호출
- 실패 시 default 는 `task fail`(attempts 증가). transient 실패만 `task release`
- draft 브랜치는 `draft/task-{12-hex-id}` 형식이며 로컬 only (remote push 금지)
- ledger reader 는 GitHub 라벨/이슈 상태와 독립적으로 동작 — `:wip`, `:ready` 라벨 미사용

상세 책임 경계·selection 의사결정·에러 처리·Output Examples 는 `autopilot-pipeline` skill 의 references 참조.
