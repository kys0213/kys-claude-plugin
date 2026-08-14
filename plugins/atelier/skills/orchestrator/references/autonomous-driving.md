---
name: autonomous-driving
description: 오케스트레이터 기본 동작인 자율 실행 루프(분해→위임→모니터→머지→재위임)를 사람 개입 없이 끝까지 self-drive 하는 패턴. 종료 조건·예산·자동 중단 가드레일·에스컬레이션 포함. 사용자가 단계별 확인을 명시하면 휴먼-인-더-루프로 opt-out. orchestrator 스킬 내부 참조 문서.
version: 0.1.0
user-invocable: false
---

# Autonomous Driving (자율 실행 루프)

오케스트레이터는 **기본적으로 자율 주행**한다 — 분해→위임→모니터→머지→재위임 루프를 사람 개입 없이 끝까지 self-drive 한다. 단, **가드레일(종료 조건·예산·자동 중단) 안에서만**. 사용자가 단계별 확인을 명시하면(opt-out) 휴먼-인-더-루프로 전환해 정체/실패/충돌을 보고하고 결정을 기다린다 (`agent-monitor.md`, `merge-coordinator.md`).

> **우선순위**: 자율 주행이 기본이다. `agent-monitor.md` / `merge-coordinator.md`의 "자동 개입 금지"는 사용자가 HITL 로 opt-out 했을 때 적용되는 규칙이고, 자율 주행(기본) 동안에는 이 문서의 규칙이 우선한다. 단 아래 **에스컬레이션 조건**은 자율 모드에서도 절대 무시하지 않는다.

---

## 기본 동작과 opt-out

자율 주행은 오케스트레이터의 **기본 동작**이다. 별도 opt-in 없이, 다중 작업 위임이 시작되면 자율 계약을 세우고 진입한다. 단 **검증 가능한 종료 조건(`done_when`)을 세울 수 없으면** 자율로 들어가지 않고 먼저 사용자와 합의한다 (안티패턴 #1).

**opt-out (휴먼-인-더-루프로 전환)**: 사용자가 단계별 확인을 명시하면 자율 주행을 끄고 HITL 로 전환한다 — "확인받으면서", "단계마다 물어봐", "babysit", "자동으로 머지하지 마". 이때는 `agent-monitor.md` / `merge-coordinator.md`의 HITL 규칙(자동 개입 금지·보고 후 결정)을 따른다.

진입 시 메인은 **자율 계약(autonomy contract)**을 한 번 보고하고 시작한다:

```
자율 계약:
- 종료 조건 (done_when):     무엇이 충족되면 끝인가 (검증 가능해야 함)
- 예산:                      max_loops, max_redispatch_per_task, (가능하면) 시간/턴 상한
- 자문 (advisory):           가용 여부 + max_advisory_consults
                             — 가용 = team 가용 판정 (SKILL.md §진입 시 체크 4,
                               권위 신호는 `SendMessage` 왕복 도달 가능성.
                               자문은 team 필수 등급)
                             — 가용이면 기본 2, **비가용이면 0 (경로 차단)**
                               예산 0 = 트리거에 도달해도 쓸 수 있는 것이 없다는 뜻이며,
                               폴백 여지를 남기지 않는다 (advisory-consult.md §게이트 0)
- 경로 (path):               경량 | 무거운 — 진입 시 1회 판정 (SKILL.md §경로 판정 게이트)
                             경량이면 isolation·토폴로지 가드·머지 조정이 빠진다.
                             계획 밖 tracked 편집이 생기면 §경로 전환으로 무거운 경로로 올린다
- 자동 중단 (hard_stops):    무엇이 발생하면 예산과 무관하게 멈추고 보고하는가
- 결정 기록 위치 (log_dir):  .orchestrator/<epic>/decisions/ (gitignore, 완료 시 요약 공유)
- 통합 검증 (integration_verify): (선택) worktree에서 실행 불가한 인프라 의존 테스트
                             - command: 실행할 명령 (예: "PROFILE=local-dev pnpm test src/storage/__test__/*.e2e.test.ts")
                             - run_at: before_merge | after_merge
```

진입 후에는 이 계약 범위 안에서 **보고 없이** 진행한다. 계약을 벗어나는 순간(예산 소진 또는 hard stop)에만 멈추고 보고한다.

---

## 자율 실행 루프

```
contract = {done_when, max_loops, max_redispatch_per_task, hard_stops, log_dir, path}
heavy = (contract.path == "무거운")                    # 진입 판정 — SKILL.md §경로 판정 게이트
loop_count = 0

while not satisfied(contract.done_when) and loop_count < contract.max_loops:
    loop_count += 1
    tasks = decompose(remaining_work)                 # 분해 — 복잡·모호한 요구는 아키텍트 협의체
                                                      # (설계 생성 → 별도 agent 심문·검증,
                                                      #  architect-council.md)로 위임하고,
                                                      # 자명한 작업만 메인이 직접 쪼갠다
    log_decision("분해", tasks, refs=[대화, CLAUDE.md, rules, 협의체 산출물])
    dispatch(tasks,                                   # 위임 — isolation은 경로 판정이 정한다
             isolation=("worktree" if heavy else None),  #   무거운: base = epic 브랜치, 편집은 격리 subagent
             run_in_background=true,                     #   경량: 격리할 tracked 쓰기가 없다.
             model=main_allocates_per_task)              #         대신 산출 경로 계약을 prompt에 싣는다
    log_decision("병렬/순차 + 위임 형태(subagent/team) + 모델 배분", ...)
    results = await_completion_notifications()        # 모니터 (sleep/poll 금지)
    if heavy: assert_topology()                       # 가드: branch == epic + status clean (아래)
                                                      # 경량은 불변식이 성립하지 않아 생략

    for r in results:
        if r.failed:
            handle_failure(r)                         # 자동 재위임 규칙 (아래)
            log_decision("재위임 판단", r, refs=[실패이력, agent-monitor.md])
            continue
        verdict = review_and_qa(r)                    # 작업별 검토·QA 게이트 (검토 + QA(검증 테스트 추가), 구현자와 다른 agent)
        if verdict.rejected:                           # 검토 reject 또는 QA reject (AND 게이트)
            handle_gate_rejection(r, verdict)         # 격리 subagent 재위임(실패 맥락 포함), team이면 SendMessage로 조율 (max_redispatch 예산 소모)
            log_decision("게이트 거부 → 재위임/조율", verdict, refs=[검토/QA findings])

    if heavy:                                         # 경량은 머지 대상이 없다 (SKILL.md §경로 판정 게이트)
        merge_coordinate(passed_results)              # 리뷰 통과분만 머지 — 충돌은 자동 위임 (아래)
        assert_topology()                             # 가드: 매 머지 직후에도
    run_integration_verify(contract)                  # 인프라 의존 테스트 — 메인이 직접 Bash (아래)
    log_decision("머지 순서 / 충돌 처리", ...)
    remaining_work = recompute_remaining()            # 진전 측정

    if no_progress(loop_count) or hit_hard_stop():
        log_decision("에스컬레이션", reason)
        break

escalate_or_report(reason, decision_log=contract.log_dir)   # 완료 / 예산소진 / 에스컬레이션
```

루프의 각 단계는 기존 references를 그대로 따른다 (`delegation-patterns`, `worktree-lifecycle`, `merge-coordinator`). 자율 모드가 바꾸는 것은 **실패/충돌을 만났을 때 사용자에게 묻지 않고 가드레일 안에서 스스로 처리한다**는 점, 그리고 **모든 자율 결정을 사후 검토 가능하도록 기록한다**는 점이다 (아래 *의사결정 기록* 참조).

---

## 모델 분배 (Model Allocation)

자율 루프는 분해·구현·리뷰를 여러 sub-agent로 나눠 돌린다. 메인은 **각 작업의 성격에 맞게 모델을 배분**한다 — 고정 매핑이 아니라 메인의 판단이다. CLAUDE.md 책임 경계상 모델 선택은 컨텍스트 의존 *결정(judgment)*이지 결정적 *변환(transform)*이 아니므로, 고정표/CLI에 박지 않고 메인이 작업마다 정한다.

원칙:

- **역량을 작업에 맞춘다**: 작업의 난이도·리스크·되돌리기 비용에 모델 역량을 맞춘다. 판단·설계·미묘한 리뷰는 더 강한 모델, 기계적·반복적 구현은 더 가벼운 모델.
- **비싼 모델은 품질을 좌우하는 지점에 아낀다**: 분해/조율(메인 자신), 자동 머지의 유일한 안전장치인 리뷰 게이트처럼 판단이 결과 품질을 결정하는 곳에 집중한다.
- **고정 배분을 박지 않는다**: 모델이 더 똑똑해지면 같은 작업을 더 가벼운 tier로 내릴 수 있어야 하므로, 매 dispatch마다 "지금도 이 역량이 필요한가"를 재평가한다. 작업 유형 → 시작 tier 표는 `delegation-patterns.md §모델 선택`이, 역할 기준 원칙(오케스트레이터는 위임 sub-agent보다 낮은 tier로 내려가지 않는다 / 위임 dispatch에는 항상 `model` 명시)은 `SKILL.md §모델 라우팅 전략`이 단일 출처다 — 여기서 재서술하지 않는다.

기록: 모델 배분도 자율 결정이므로, 표준 heuristic을 벗어난 선택(예: 평소 가벼운 tier에 맡기던 구현을 더 강한 모델로 올림)은 근거와 함께 decision log에 남긴다.

---

## 메인 컨텍스트 격리 (Main Context Isolation)

긴 자율 루프에서 메인이 매 작업의 파일 내용·전체 diff·리뷰 전문을 자기 컨텍스트에 쌓으면, 루프가 길어질수록 메인 컨텍스트가 포화되어 조율 판단 품질이 떨어진다. 자율 모드에서 메인은 **조율에 필요한 최소 상태만** 보유한다.

원칙:

- **무거운 일은 sub-agent 컨텍스트에서**: 읽기·편집·리뷰는 전부 sub-agent가 자기 컨텍스트에서 수행한다. 메인은 **구조화된 압축 요약**(작업 ID, 변경 파일 목록, pass/reject, 다음 행동)만 받고, 전체 diff·파일/리뷰 findings 전문은 끌어오지 않는다.
- **근거는 외부에 남기고 경로만 참조**: 자세한 판단 근거는 decision log / worktree에 남기고 메인은 경로만 보유한다.
- **메인 직접 Read는 결정적 사실로 제한**: 메인이 직접 Read/Bash하는 것은 조율 판단에 필요한 결정적 사실(git 상태, 테스트 exit code, 토폴로지 가드)로 한정한다 — 코드 본문 통독은 sub-agent 몫이다.

---

## 위임 형태: 편집은 격리 subagent, 조율은 team

자율 루프는 본질적으로 **구현 → 리뷰 → 수정**을 반복하는 구조다. 두 책임을 분리한다: **편집·격리는 `isolation:"worktree"` subagent가**(하베스트 보장), **조율은 team이**(공유 checkout).

핵심 제약 (team의 격리 특성·가용 전제는 `delegation-patterns.md §Agent team 사용 패턴`이 단일 출처):

- **편집은 teammate가 직접 하지 않고 `isolation:"worktree"` subagent에 위임**한다.
- review→fix 루프는 `SKILL.md §team mode 강제 등급`의 **선호** 등급이다 — 편집이 개입하고 격리를 보장하는 것은 team이 아니라 격리 subagent이므로, team이 비가용이면 단발 격리 subagent 재위임으로 돈다(폴백 허용). **이 폴백 허용은 선호 등급 경로에만 적용된다** — 자문·협의체는 필수 등급이라 폴백이 위반이다.

team을 쓸 때의 이득 (team 가용 시):

- **리뷰어·QA 게이트 조율**: reviewer/qa teammate + implementer teammate를 한 team에 두면, 게이트 거부 findings를 **team 내부 SendMessage로 전달**해 다음 라운드를 조율한다 (실제 편집은 implementer가 격리 subagent로 위임).
- **장기 런에서 식별·제어 가능**: 이름으로 SendMessage해 정체 해소·단계 전환을 지시할 수 있다 (*자동 개입 규칙*이 허용).
- **컨텍스트 격리 강화**: 한 작업의 반복 맥락이 team 안에 머물러 메인으로 전문(全文)이 올라오지 않는다.

구성:

- **feature/task 하나 = team 하나**(team 가용 시). reviewer + implementer 역할. 구성·이름·수명은 `delegation-patterns.md §Agent team 사용 패턴`을 따른다. **편집 격리는 team이 아니라 그 안에서 띄우는 `isolation:"worktree"` subagent가 책임진다** — teammate에게 worktree 이동을 위임하지 않는다.
- **review→fix 조율**: reviewer reject → implementer에게 SendMessage → implementer가 격리 subagent로 수정 재위임 → 재리뷰. 이 사이클도 `max_redispatch_per_task` 예산을 동일하게 소모한다 (무한 반복 금지). 소진 → hard stop → 에스컬레이션.

team이 비가용이거나 조율이 불필요하면 **단발 격리 subagent 재위임**(이전 실패 맥락 포함)으로 review→fix를 돈다. 이 선호 등급 안에서는 의심스러우면 단발 subagent를 고른다 — 격리가 항상 보장되기 때문이다.

---

## 종료 조건 (Done)

종료 조건은 **명시적이고 검증 가능**해야 한다. 모호한 종료 조건은 루프를 영원히 돌리거나 환각으로 조기 종료시킨다.

- ✅ 검증 가능: "모든 작업이 리뷰 통과 후 머지 완료 + `cargo test` green + `cargo fmt --check`/`clippy -D warnings` 통과"
- ❌ 검증 불가: "코드가 좋아 보이면", "대충 다 되면"
- 매 루프 종료 시 종료 조건을 **결정적으로** 재평가한다 — Bash로 테스트/빌드/lint를 실행하고 git 상태를 확인해 판정. 메인의 주관적 "다 된 것 같다"에 맡기지 않는다.

---

## 자동 개입 규칙 (자율 모드에서 허용)

HITL(opt-out) 모드에서 금지된 행위가 자율 모드(기본)에서는 **가드레일 안에서** 허용된다.

| 행위 | HITL (opt-out) | 자율 (기본) |
|------|----------|-------------------|
| 자동 재위임 | 외부환경 원인 1회만 | 예산(`max_redispatch_per_task`) 한도 내 반복 |
| **집행 agent에 대한** SendMessage 명령 주입 | 금지 | 계획된 단계 전환 + 정체 해소용 허용 |
| 자동 머지 | 보고 후 진행 | 충돌 없으면 자동 |
| 자동 충돌 해결 | 위임/보고 후 결정 | 충돌 해결 전담 sub-agent에 자동 위임 |

각 행위는 **예산을 소모**한다. 예산이 소진되거나 hard stop에 닿으면 그 즉시 멈춘다.

주어가 **집행 agent**인 데 유의한다 — 집행 중이 아닌 상대와의 왕복(자문 반문 등)은 애초에 이 표의 대상이 아니며, 허용 여부는 `agent-monitor.md §SendMessage`가 단일 소유한다.

### 재위임 (자동)

```
실패 감지 → 원인 추정 (agent-monitor.md 재위임 판단 기준 활용)
          → prompt 보강 (누적 실패 이력을 자기완결적으로 포함)
          → 새 isolation worktree로 재위임
          → redispatch_count[task] += 1
```

- `redispatch_count[task] > max_redispatch_per_task` → hard stop → 에스컬레이션
- **같은 자리를 맴돌면(게이트 reject → 재위임이 반복) 에스컬레이션 직전에 상위 tier 자문을 소집할 수 있다** (`advisory-consult.md` 트리거 2). 자문은 재위임이 아니므로 `max_redispatch_per_task`를 소모하지 않고 `max_advisory_consults`를 쓴다. 자문 경로가 비활성이면 그대로 에스컬레이션한다.
- 재위임 prompt에는 **이전 시도가 어디까지 갔고 무엇이 실패했는지**를 반드시 담는다 (sub-agent는 메인 대화를 못 봄).

### 리뷰어·QA 게이트 (자동, 머지 전 — 작업마다 필수)

자율 모드는 사람이 PR을 보지 않는다. 따라서 **각 작업의 결과를 epic 브랜치에 머지하기 전에 두 전용 sub-agent가 검증**하는 게이트를 둔다. 구현 sub-agent가 스스로 "통과했다"고 말하는 것에 머지를 맡기지 않는다 (자기 검증 편향). 이 게이트는 코드를 바꾸는 **모든 작업에 예외 없이 적용**한다 (단발 1회·read-only만 예외 — `SKILL.md §작업 케이스마다 검토 에이전트·QA 에이전트는 필수`).

검증 차원마다 **서로 다른 agent**로 분리해 둔다 (한 agent가 여럿을 보면 전부 얕아진다). 기본 두 차원(검토·QA)에 더해, 작업이 DB에 접촉하면(스키마·마이그레이션·쿼리) **DBA 차원이 조건부로 추가**된다:

| 역할 | 입력 | 검증 질문 | 출력 |
|------|------|-----------|------|
| **검토 에이전트 (reviewer)** | 요구사항 + worktree diff (epic base 기준) | 구현이 요구사항을 빠짐없이·과하지 않게 충족하나? 회귀·설계원칙(SOLID)·품질 게이트(test/lint/format) 위반은? | `pass`/`reject` + `파일:라인` 사유 |
| **QA 에이전트 (qa)** | 요구사항 + worktree 테스트 코드 | 각 요구사항·flow·엣지케이스에 대응 검증 테스트가 있나? 테스트가 의도를 실제로 검증하나(빈 assert·항상 통과 아님)? 없으면 **검증용 테스트를 추가·보강** | `pass`/`reject` + 누락 케이스 목록 + 추가한 테스트 |
| **DBA 에이전트 (dba)** — DB 접촉 작업만 | 요구사항 + worktree diff 중 스키마·마이그레이션·쿼리·ORM 모델 변경 | 스키마 변경이 안전한가(하위 호환·락·롤백 경로)? 쿼리가 인덱스를 타나, N+1·풀스캔은 없나? 제약조건·데이터 정합성 위험은? | `pass`/`reject` + `파일:라인` 사유 + 위험 쿼리/마이그레이션 목록 |

- **역할 분리**: 구현·검토·QA·DBA는 **서로 다른 sub-agent**다. 같은 agent가 자기 코드를 리뷰하거나 자기 테스트만으로 통과시키지 않는다.
- **QA의 테스트 추가도 편집**이므로 `isolation:"worktree"` subagent로 위임한다 — 메인이 직접 작성하지 않는다.
- **DBA는 조건부 게이트**: 작업이 DB에 접촉할 때만 붙는다. 판정은 결정적으로 — 협의체가 task 도출 시 표시한 **DB 접촉 플래그**(`architect-council.md §Task 도출 계약`)와, 게이트 시점의 **변경 파일 검사**(마이그레이션 디렉토리·`.sql`·스키마 정의·ORM 모델·쿼리 빌더 호출부) 중 하나라도 걸리면 DBA를 소집한다. 메인의 체감으로 생략하지 않는다.
- **AND 게이트**: 검토 `pass` + QA `pass`(추가한 검증 테스트 green) + (DB 접촉 시) DBA `pass` 모두여야 머지 후보로 승급한다. 하나라도 `reject`면 findings를 자기완결 prompt에 실어 재위임한다.
- **병렬 검증**: 검토·QA·DBA는 서로 독립이므로 동시에 돌린다 (한쪽이 다른 쪽을 기다리지 않음).
- **spec 기반 구현이면** 이 게이트를 `spec-driven-review.md`로 특수화한다 — 검토자(spec↔구현 적합성) + QA 매니저(spec↔테스트 적합성) 두 차원을 팀 모드로 상주시켜 worktree 코드를 계속 검증·개선한다 (예산·재위임·기록은 이 문서 규칙을 그대로 따른다). spec이 없어도 위 일반 검토·QA 게이트는 생략하지 않는다.
- 게이트(코드 품질·요구사항 충족 + 검증 테스트)와 `integration_verify`(인프라 의존 동작)는 둘 다 머지 전 게이트이며 둘 다 통과해야 머지한다 — 게이트는 작업별, integration_verify는 루프별로 돈다.
- 게이트 거부는 실패와 동일하게 **`max_redispatch_per_task`를 소모**한다 — 게이트 전용 새 예산을 만들지 않는다. 무한 재위임을 막는다.
- 검토·QA·DBA 모델도 위 *모델 분배* 원칙으로 메인이 작업 리스크에 맞춰 정한다 — 자동 머지의 유일한 안전장치이므로 보통 더 강한 역량을 둘 가치가 있으나 고정은 아니다.
- 컨텍스트 격리: 게이트 에이전트들의 상세 findings·diff는 각자 컨텍스트에 남기고, 메인은 verdict + 압축 요약만 받는다.
- `done_when` 평가에 **"머지된 모든 작업이 검토 통과 + QA 검증 테스트 green (+ DB 접촉 작업은 DBA 통과)"**을 포함한다.

### 머지 / 충돌 (자동)

```
충돌 없음 → merge-coordinator 순서 규칙대로 자동 머지
충돌 발생 → 충돌 해결 전담 sub-agent에 자동 위임 (`git` 스킬의 충돌 해결 정책을 입력으로)
            성공 → 계속
            실패 → 재시도 1회 → 그래도 실패면 hard stop → 에스컬레이션
도메인 의미 충돌 (코드로 판정 불가) → 즉시 에스컬레이션 (자동 해결 금지)
같은 파일 2회 충돌 → 재위임 중단. hot-spot 재분류 + 관련 task 직렬화
같은 파일 3회 충돌 → 에스컬레이션 (분해 자체를 보고)
```

위 사다리의 파일 단위 카운터는 **`max_redispatch_per_task`와 별개**이고(task를 갈아치워도 리셋되지 않는다), 머지 시점 정책·통합 방식도 자율 모드가 따로 정하지 않는다 — 전부 `branch-strategy.md`(§충돌 반복 / §base drift 전파 / §머지 방식)를 그대로 따른다. 자율 모드는 in-flight가 남아 있어도 보고 없이 진행하므로 **전파 누락이 조용히 누적된다** — 배치 머지 기본값이 여기서 특히 중요하다.

### 토폴로지 가드 (assert_topology)

자율 모드는 보고 없이 연속 진행하므로, sub-agent의 격리 이탈로 메인 working tree가 오염되면 그것이 후속 dispatch/머지로 전파되기 전에 잡아야 한다. **매 sub-agent 완료 알림 수신 직후 + 매 머지 직후** 실행한다 — 가드 명령과 복구 절차는 `merge-coordinator.md §토폴로지 가드`가 단일 출처다.

**적용 범위: 무거운 경로** (판정: `SKILL.md` §경로 판정 게이트). 경량에서 tracked 편집이 필요해지면 가드를 되살리는 것은 `SKILL.md §경로 전환`의 5단계다.

위반 시 **hard stop** — 복구 후 즉시 에스컬레이션하고, 자율 재개는 사용자 결정에 맡긴다.

### 통합 검증 (integration_verify)

worktree sub-agent는 인프라 의존 환경(내부 자격증명, live DB, 외부 서비스 토큰 등)에 접근할 수 없다. 따라서:

- 인프라 의존 테스트는 **처음부터 sub-agent worktree 검증 범위에서 제외**하고 dispatch prompt에 명시한다 — sub-agent의 테스트 결과에 환경 의존 실패 noise가 끼지 않도록.
- 계약에 `integration_verify`가 정의되어 있으면, `run_at` 시점(before_merge / after_merge)에 **메인이 epic 브랜치 메인 working tree에서 직접 Bash로 실행**한다 (메인의 Edit/Write 금지 정책에 해당 없음 — Bash 검증은 허용).
- 실패 시: `before_merge`면 해당 머지를 진행하지 않고, `after_merge`면 후속 루프를 진행하지 않는다. 두 경우 모두 hard stop → 에스컬레이션.
- `done_when` 평가에 integration_verify 통과를 포함한다 — 계약에 정의됐다면 이것이 통과하지 않은 채 "완료"를 선언하지 않는다.

---

## 의사결정 기록 (Decision Log)

자율 모드에서는 메인이 사람에게 묻지 않고 스스로 결정한다. CLAUDE.md의 "**결정(judgment)은 reasoning이 사람에게 검토 가능해야 한다**" 원칙에 따라, 모든 자율 결정은 **참고한 근거와 함께 기록**되어 사후 검토 가능해야 한다. 기록 없이 자율 주행하면 사용자가 "왜 그렇게 했는지"를 복원할 수 없다.

### 결정 시 참고 소스

각 자율 결정을 내리기 **전에** 메인은 다음을 참고한다 (그리고 어떤 소스를 봤는지 기록한다):

| 소스 | 무엇을 얻는가 |
|------|--------------|
| 대화 내용 | 사용자의 요구·제약·우선순위·이전 합의 |
| `CLAUDE.md` | 설계 최우선, 책임 경계(CLI vs Skill), SOLID/TDD, 품질 게이트 |
| `.claude/rules/*` | 커밋/브랜치 규칙, 플러그인 컨벤션 등 결정적 규칙 |
| spec / 설계 문서 | 작업 대상의 명세와 의도 |
| 코드·git 상태 | 현재 사실 (Read/Bash로 결정적 확인) |

### 기록 위치

```
.orchestrator/<epic>/decisions/      ← gitignore됨 (.review-output/ 와 동일 패턴)
```

- **커밋하지 않는다** — 자율 런의 휘발성 작업 산출물. 완료 시점에 요약해 사용자에게 공유한다.
- 파일 구성: append-only 단일 로그 `decisions/log.md` 또는 결정별 개별 파일 `decisions/NNNN-<slug>.md`. 결정적 파일명으로 재현성을 확보한다.
- epic마다 디렉토리를 분리해 런 간 기록이 섞이지 않게 한다.

### 기록 시점

다음 자율 결정이 발생할 때마다 append한다:

- 작업 분해 방식 (어떻게 쪼갰는가)
- 병렬 vs 순차 + 위임 형태(단발/team) 선택
- 재위임 여부 + prompt 보강 내용
- 머지 순서 + 자동 충돌 해결 위임
- 자문 요청 + 권고에 대한 채택/부분채택/기각 판단 (기각도 사유와 함께 — `advisory-consult.md §메인의 처리 의무`)
- 에스컬레이션 판단 (멈춤 vs 계속)
- 종료 조건 충족 판정

### 기록 형식 (한 결정 = 한 항목)

```markdown
## <ISO timestamp> · <결정 요약>
- 상황: 무엇을 결정해야 했는가
- 참고: 본 소스 (대화 / CLAUDE.md / .claude/rules/*.md / 코드 상태 …)
- 결정: 무엇을 선택했는가
- 근거: 왜 (참고 소스와 연결)
- 대안: 고려했으나 택하지 않은 것 + 이유
- 영향: 어떤 작업/브랜치에 적용됐는가
```

**team 필수 등급 경로(자문·협의체)의 기록에는 두 필드를 추가로 반드시 남긴다** (`SKILL.md §team mode 강제 등급` 가드 2):

```markdown
- 실행 형태: teammate | subagent      ← 폴백 여부를 사후에 판별할 유일한 근거
- 판정 근거: 왕복(name) | 왕복(agentId) | env   ← team 가용 판정을 어느 신호로 내렸는가
```

두 필드가 없으면 "team으로 돌렸다"는 서술을 검증할 방법이 없어 감사가 성립하지 않는다 — 필수 등급 경로의 기록으로 인정하지 않는다.

### 완료 시 공유

작업 완료(또는 에스컬레이션) 시점에 메인은 decision log를 **종료 보고에 함께 포함**한다:

```
의사결정 요약:
- 총 결정 수: N
- 주요 분기: <병렬화/재위임/에스컬레이션 등 핵심 결정 3~5개>
- 전체 로그: .orchestrator/<epic>/decisions/  (사후 검토용)
```

전체 로그는 gitignore되어 커밋되지 않으므로, 보고에 경로를 안내해 사용자가 직접 열어볼 수 있게 한다.

---

## 가드레일 (Guardrails)

폭주를 막는 핵심. 모두 진입 시 계약에 고정된다.

| 가드레일 | 의미 | 도달 시 |
|---------|------|---------|
| `max_loops` | 전체 while 반복 상한 | 멈추고 보고 |
| `max_redispatch_per_task` | 작업당 자동 재위임 횟수 (보통 2~3) | 해당 작업 hard stop |
| no-progress | 연속 N 루프 동안 진전 없음 (머지된 작업 0 / 테스트 통과 수 변화 없음) | 멈추고 보고 |
| 시간·턴 예산 | 가능하면 상한 설정 | 멈추고 보고 |
| hard stops | 아래 에스컬레이션 조건 | 예산이 남아도 즉시 멈춤 |

**진전(progress) 측정**은 결정적 신호로 한다 — 머지된 브랜치 수, 통과 테스트 수, 종료 조건 충족 항목 수. 메인의 체감이 아니라 수치로 판정.

---

## 모호한 seam — isolate-and-continue (hard-stop 대안)

자율 루프에서 마주치는 모든 모호함이 에스컬레이션 대상은 아니다. **경계(seam)는 분명한데 내부 구현/외부 계약만 비어 있는** 엣지(미확정 외부 webhook 계약, 아직 안 정한 DM/알림 채널 등)는 멈추는 대신 **격리하고 전진**한다. 멈춰야 진행되는 것이 아니라 빈자리만 채우면 되는 종류라면, 슬라이스 전체를 보류시키는 hard-stop 은 과한 대응이다.

- **contract / interface 로 격리**한 뒤 그 자리에 **loud stub** 을 둔다 — inert 임이 명백한 `Noop` 구현 또는 `throw new NotImplemented(...)`. 계약 형태까지 추측해야 하면 **minimal contract** 만, "확정본 아님" 표시와 함께 둔다 (예: name-only 테이블).
- 나머지 골격을 green 으로 끝까지 만든다. 미결 seam 은 **decision log + 종료 보고에 깃발**로 남긴다.

**가드 (Fail-Fast 정합):** stub 은 **silent fallback 이면 안 된다.** 조용히 기본값으로 "동작하는 척" 하면 스펙 불일치를 숨겨 디버깅을 망친다. 반드시 inert / throw 로 **드러내고** 보고한다. *"격리해서 계속하되, 그 자리에 깃발을 꽂는다."*

**여전히 에스컬레이션:** seam 자체가 아니라 **도메인 의미 결정**(틀리면 데이터/의미가 조용히 오염되는 것)·되돌리기 어려운 외부 행위는 stub 으로 가릴 수 없으므로 멈추고 보고한다 (아래 *에스컬레이션* 참조).

> 같은 패턴 선례: 미확정 외부 의존을 `interface` 로 격리하고 그 자리에 명백히 inert 한 `Noop` 구현(스캐너·알림 등)을 두는 contract-격리 + Noop stub 구성. seam 만 고정하고 구현은 비워둔 채 골격을 끝까지 가져간다.

적용 범위는 **이런 류의 엣지**(경계 명확·내부/계약만 미정)에 한정한다. 모든 모호함을 이 경로로 처리하지 않는다 — 도메인 의미 결정은 종전대로 에스컬레이션한다.

---

## 에스컬레이션 (자율 모드라도 멈추는 조건)

자율 주행이 기본이라도 다음은 **항상** 멈추고 사람에게 보고한다 (예산과 무관, 우선 적용):

```
에스컬레이션 판정
  │
  └─ 아래 7개 중 하나라도 해당하는가
       │  1. 되돌리기 어렵거나 외부로 나가는 행위
       │       force push · main 브랜치 머지 · 배포 · 외부 서비스 호출 · 데이터 삭제
       │  2. 토폴로지 위반
       │       메인 working tree branch ≠ epic 브랜치 / 의도치 않은 변경 발견
       │       → 복구 후 즉시 보고 (아래 참고)
       │  3. integration_verify 실패
       │       계약에 정의된 인프라 의존 검증 실패 → 자동 머지·루프 진행 금지
       │  4. 도메인 의미 충돌
       │       의도가 갈리는 머지 충돌 (코드로 판정 불가)
       │  5. 예산 소진
       │       루프 상한 / 재위임 한도 / no-progress 도달
       │  6. 원인 불명확한 반복 실패
       │       같은 실패가 재위임에도 계속됨
       │  7. 종료 조건 자체가 검증 불가능해짐
       │       테스트 인프라 붕괴 등으로 done 판정 불가
       │
       ├─ Yes ─→ 멈추고 보고
       └─ No  ─→ 루프 계속
```

- **조건 2의 미확정 지점**: 원문은 토폴로지 위반에만 "복구 후 즉시 보고"를 덧붙였는데, 이것이 보고 *순서*만 규정하는지 공통 보고 포맷을 대체하는지는 원문이 정하지 않았다. 여기서 확정하지 않는다.
- **1의 경계 근거**: 자율 모드는 epic 브랜치 안에서만 자율이고, 그 경계를 넘는 행위는 자동화 대상이 아니다.

에스컬레이션 = 멈추고 **현재 상태 + 남은 작업 + 막힌 지점 + 선택지**를 한 번에 보고 (아래 자문 경로의 에스컬레이션에도 동일 적용).

**자문 경로와의 관계**: 자문이 가용하면 일부 에스컬레이션(협의체 예산 소진·게이트 재위임 루프·되돌리기 어려운 결정) 직전에 상위 tier 권고를 한 번 받아볼 수 있다 — 그래도 **에스컬레이션 조건 자체는 유지된다**. 자문은 멈추는 판단을 대체하지 않고 보고에 실을 근거를 더할 뿐이다. 가용성 게이트와 예산 분기를 포함한 절차는 `advisory-consult.md §게이트 0`이 단일 출처다. 자문 경로가 비활성이면 종전대로 바로 에스컬레이션한다.

- **advisor `critical` 승격**: 자문 결과가 `critical`이면 그 자체가 위 7개 조건과 별개로 에스컬레이션 트리거로 승격된다 (`advisory-consult.md`).

---

## 종료 핸드오프 (Handoff)

**종료 상태는 대화가 아니라 파일로 남는다.** 다음 세션은 이번 대화를 보지 못한다 — 대화로만 보고하고 끝내면 재개하는 쪽이 상황을 처음부터 복원해야 한다. 완료로 끝나든 에스컬레이션·예산 소진·환경 블록으로 끝나든 동일하게 남긴다.

### 판정 — 작업 단위마다 하나씩

| 판정 | 함께 남기는 것 |
|------|----------------|
| `DONE` | 검증 증거 — 테스트·빌드·lint 결과, 커밋/머지 링크 |
| `BLOCKED` | 정확한 에러 메시지 + **해제 조건** (무엇이 갖춰지면 재개되는가) |
| `NOT-STARTED` | 착수하지 못한 이유 |

세 판정의 차이는 상태 이름이 아니라 **다음 행동을 정하는 데 필요한 정보가 다르다**는 것이다. 그 정보가 빠진 항목은 판정이 없는 것과 같다.

### 기록 위치

```
.orchestrator/<epic>/HANDOFF.md      ← decisions/ 와 같은 디렉토리, gitignore됨
```

- decision log와 **책임이 다르다**: decision log는 "왜 그렇게 했는가"(과정), 핸드오프는 "지금 어디이고 무엇이 막았는가"(상태). 한 파일에 섞으면 둘 다 어중간해진다
- 매 루프가 아니라 **종료 시점에 한 번** 쓴다
- **작성은 sub-agent에 위임한다.** 메인은 Write하지 않는다 (`SKILL.md §안티패턴` 1번 편집권 회수 금지). 판정 목록과 근거를 prompt에 실어 보낸다

### 성립 기준

다음 세션이 **이 파일만 읽고** "어디서 멈췄는지 / 무엇이 막았는지 / 무엇이 갖춰지면 재개되는지"를 알 수 있어야 한다. 대화를 봐야 이해되는 문장이 하나라도 있으면 아직 핸드오프가 아니다.

---

## 보고 (자율 모드)

```
진입 시:   자율 계약 1회 보고 (종료 조건 / 예산 / hard stop / 결정 기록 위치)
진행 중:   침묵 (정상 루프는 보고하지 않음) — 단 결정은 log_dir에 계속 append
           — hard stop / 에스컬레이션 발생 시 즉시 보고
종료 시:   종료 사유 (완료 / 예산 소진 / 에스컬레이션)
           + 루프 횟수 + 머지 결과 + 미해결 항목
           + 판정 요약 (DONE / BLOCKED / NOT-STARTED 건수) + 핸드오프 파일 경로
           + 의사결정 요약 (총 결정 수 / 주요 분기 / 전체 로그 경로)
```

---

## 안티패턴

1. **종료 조건이 없거나 검증 불가능**: `done_when` 미정의 상태로 "알아서 끝까지" 진입 → 무한 루프. 메인 주관의 "다 된 것 같다" 판정 → 환각 종료. 검증 가능한 종료 조건부터 합의하고, 판정은 명령 결과로 한다.
2. **예산 없는 반복**: 실패·게이트 거부를 예산 밖에서 무한 재위임하거나, 진전 없이 루프만 계속 → 폭주·예산 소모. 재위임과 게이트 거부는 **같은** `max_redispatch_per_task`를 소모하고, no-progress를 감지해 조기 중단한다.
3. **hard stop·경계 무시**: 되돌리기 어려운 행위(force push·main 머지·배포)까지 자동화 → 사고. 자율은 epic 브랜치 안에서만이고, 에스컬레이션 조건은 예산과 무관하게 우선한다.
4. **opt-out 무시**: 사용자가 HITL을 명시했는데 자율로 밀어붙임 → 자율은 기본이지만 opt-out은 존중한다. 자동 개입을 멈추고 보고 후 결정을 받는다.
5. **sleep / poll**: 자율 루프에서도 완료 알림을 사용. `Bash sleep` 루프 금지.
6. **결정 기록 누락 / 로그 커밋**: 근거 없이 자율 주행하면 사후에 "왜"를 복원 불가. 반대로 휘발성 로그를 epic 브랜치에 커밋하면 repo 오염. 분기 결정은 참고 소스와 함께 `log_dir`에 기록하되 `.orchestrator/`는 gitignore, 완료 시 요약으로만 공유.
7. **토폴로지 가드 생략** (무거운 경로): 완료 알림/머지 후 메인 branch 확인 없이 연속 진행 → 오염된 HEAD 위에서 다음 dispatch의 worktree base가 잘못 잡힘. 경량 경로에서의 생략은 판정 결과이므로 이 안티패턴이 아니다 — 대신 판정 자체를 빠뜨리는 것이 `SKILL.md §안티패턴 15`다.
8. **인프라 의존 테스트를 worktree 검증에 포함**: sub-agent가 접근 불가한 환경 의존 테스트를 worktree에서 실행 → 환경 실패 noise로 검증 신뢰도 저하. 계약의 `integration_verify`로 분리해 메인이 실행.
9. **게이트 무력화**: 구현 sub-agent의 자기 보고만 믿고 머지하거나, 구현한 agent가 자기 결과를 검토/QA → 자기 검증 편향으로 결함 통과. 게이트 에이전트는 항상 구현자와 다른 sub-agent다.
10. **검증 테스트·DBA 게이트 생략**: 구현만 머지하고 검증 테스트를 안 만들면 회귀를 잡을 그물이 없다(spec 유무와 무관). DB 접촉 판정에 걸렸는데 reviewer/QA만으로 통과시키면 락·하위 호환·인덱스 누락 같은 DB 특유 위험이 그대로 들어간다.
11. **고정 모델 매핑 박기**: "구현은 항상 X, 리뷰는 항상 Y"로 못 박음 → 모델 세대가 바뀌어도 비효율 유지. 매 dispatch마다 작업 리스크에 맞춰 재평가한다 (역할 기준 원칙은 `SKILL.md §모델 라우팅 전략`).
12. **메인 컨텍스트로 전문 끌어오기**: 전체 diff·파일 전문·리뷰 findings 전문을 메인이 직접 통독 → 긴 루프에서 메인 컨텍스트 포화. 메인은 압축 요약 + verdict만 수령.
13. **teammate에 편집 격리 기대**: teammate가 공유 checkout을 직접 편집 → 덮어쓰기/메인 오염. 편집은 `isolation:"worktree"` subagent에 위임하고 team은 조율만. 단발 재위임 시에는 이전 실패 맥락을 새 prompt에 포함해 컨텍스트 손실을 줄인다.
14. **seam 처리 실패 — 일괄 hard-stop 또는 silent stub**: 내부 구현/계약만 빈 엣지까지 전부 에스컬레이션하면 채우면 될 빈자리에 슬라이스 전체가 멈춘다. 반대로 격리 stub이 조용히 기본값으로 "동작하는 척"하면 스펙 불일치를 숨겨 디버깅을 망친다. contract/interface로 격리하고 **inert / throw 하는 loud stub**을 둔 채 전진, 미결 seam은 깃발로 보고 (*모호한 seam — isolate-and-continue*).

---

## 체크리스트

진입 전:

- [ ] 사용자가 HITL 로 opt-out 하지 않았는가? (opt-out 시 자율 진입 금지)
- [ ] 조율 도구(`SendMessage`/`Monitor`/`Task*`) 스키마를 `ToolSearch`로 확보했는가? (`SKILL.md §진입 시 체크 0` — 미확보면 왕복 경로가 전부 죽는다)
- [ ] 종료 조건이 명령으로 판정 가능한 형태인가?
- [ ] 예산(`max_loops` / `max_redispatch_per_task` / no-progress)과 hard stop 조건을 계약에 고정했는가?
- [ ] 결정 기록 위치(`.orchestrator/<epic>/decisions/`)를 고정하고 자율 계약을 1회 보고했는가?
- [ ] 인프라 의존 테스트가 있다면 `integration_verify` (command + run_at)를 계약에 정의했는가?
- [ ] 파이프라인이 의존하는 **공유 전제**(인증·필수 외부 서비스)를 dispatch 전에 read-only로 확인했는가? (`SKILL.md §진입 시 체크` 5)
- [ ] 자문 가용 여부(team 가용 판정 — 권위 신호는 `SendMessage` 왕복 도달 가능성)를 진입 시 확정하고 `max_advisory_consults`를 계약에 고정했는가?
- [ ] 필수 등급 경로(자문·협의체)를 team으로 돌렸고, spawn 확인 + `실행 형태`·`판정 근거` 필드를 기록했는가?

루프 중:

- [ ] 복잡·모호한 요구의 분해를 아키텍트 협의체에 위임했는가? (`architect-council.md` — 자명한 작업만 메인 직접 분해)
- [ ] 작업이 다중이거나 의존성이 있으면 Task 시스템(`TaskCreate`/`addBlockedBy`/`owner`)으로 상태를 추적하는가? (`agent-monitor.md §Task 시스템`)
- [ ] 편집·격리가 필요한 작업을 **`isolation:"worktree"` subagent**에 위임했는가? (team을 썼다면 조율만 — SendMessage)
- [ ] 모든 background dispatch의 prompt에 **보고 채널**(`SendMessage({to: "main", ...})`)을 넣었는가? (`delegation-patterns.md §필수 포함 요소` 11번 — 빠지면 agent가 막혀도 묻지 못하고 임의 가정으로 진행한다)
- [ ] 리스크 큰/되돌리기 어려운 편집은 **계획 우선 게이트**를 거쳤는가? (`delegation-patterns.md §계획 우선 게이트`)
- [ ] 각 작업을 머지 전 **검토 + QA (+ DB 접촉 시 DBA)** 게이트로, 구현자와 다른 agent가 검증하고 전부 pass(AND)여야 머지하는가? (QA가 추가한 검증 테스트 green 포함)
- [ ] 재위임·게이트 거부·충돌 해결이 `max_redispatch_per_task` 예산을 소모하며 카운트되는가?
- [ ] (무거운 경로) 같은 파일의 충돌 횟수를 task 예산과 **별개로** 세고 있는가? (`branch-strategy.md §충돌 반복`)
- [ ] (무거운 경로) 매 sub-agent 완료 직후 + 매 머지 직후 토폴로지 가드를 실행하는가? (`merge-coordinator.md §토폴로지 가드`)
- [ ] 자율 계약에 경로 판정(경량/무거운)과 그 근거를 실었는가? (`SKILL.md §경로 판정 게이트`)
- [ ] 계약의 integration_verify를 run_at 시점에 메인이 직접 실행하는가?
- [ ] 각 작업의 모델을 리스크에 맞춰 배분하고(dispatch에 `model` 명시), 비표준 선택은 기록하는가?
- [ ] 메인이 전문 대신 압축 요약 + verdict만 수령하는가? (컨텍스트 격리)
- [ ] 매 루프 종료 시 종료 조건을 결정적으로 재평가하고 진전을 수치로 측정하는가? (체감 아님)
- [ ] 자문을 소집했다면 트리거·예산 안이었고, 권고의 채택/부분채택/기각을 사유와 함께 기록했는가? (`critical`은 에스컬레이션으로 승격)
- [ ] 각 분기 결정을 참고 소스와 함께 `log_dir`에 기록하는가?
- [ ] hard stop 발생 시 예산과 무관하게 즉시 멈추는가?

종료 시:

- [ ] 종료 사유(완료 / 예산 소진 / 에스컬레이션)를 보고했는가?
- [ ] 작업 단위마다 DONE / BLOCKED / NOT-STARTED 를 판정하고, 각 판정이 요구하는 정보(증거 / 해제 조건 / 미착수 사유)를 채워 **핸드오프 파일을 sub-agent에 위임해** 남겼는가? (§종료 핸드오프)
- [ ] 의사결정 요약(총 수 / 주요 분기 / 로그 경로)을 보고에 포함했는가?
- [ ] 미해결 항목과 남은 worktree를 정리/보고했는가?
