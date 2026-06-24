---
name: autonomous-driving
description: 사용자가 명시 opt-in 했을 때 분해→위임→모니터→머지→재위임 루프를 사람 개입 없이 끝까지 self-drive 하는 자율 실행 패턴. 종료 조건·예산·자동 중단 가드레일 포함. orchestrator 스킬 내부 참조 문서.
version: 0.1.0
user-invocable: false
---

# Autonomous Driving (자율 실행 루프)

기본 오케스트레이터는 **휴먼-인-더-루프**다 — 정체/실패/충돌을 사용자에게 보고하고 결정을 기다린다 (`agent-monitor.md`, `merge-coordinator.md`). 자율주행 모드는 사용자가 **명시적으로 opt-in** 했을 때, 분해→위임→모니터→머지→재위임 루프를 사람 개입 없이 끝까지 self-drive 한다. 단, **가드레일(종료 조건·예산·자동 중단) 안에서만**.

> **우선순위**: 자율 모드는 기본 원칙의 *opt-in 예외*다. `agent-monitor.md` / `merge-coordinator.md`의 "자동 개입 금지"는 기본 모드 규칙이고, 자율 모드가 활성화된 동안에는 이 문서의 규칙이 우선한다. 단 아래 **에스컬레이션 조건**은 자율 모드에서도 절대 무시하지 않는다.

---

## 활성화 (Opt-in)

자율 모드는 **절대 기본값이 아니다**. 다음 중 하나가 있어야만 진입한다:

- 사용자가 명시: "자율로", "끝까지 알아서", "사람 개입 없이", "babysit 없이 완주", "autonomous", "self-drive"
- 사용자가 종료 조건/예산을 함께 제시하거나, 메인이 진입 시 제안하고 사용자가 승인

진입 시 메인은 **자율 계약(autonomy contract)**을 한 번 보고하고 시작한다:

```
자율 계약:
- 종료 조건 (done_when):     무엇이 충족되면 끝인가 (검증 가능해야 함)
- 예산:                      max_loops, max_redispatch_per_task, (가능하면) 시간/턴 상한
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
contract = {done_when, max_loops, max_redispatch_per_task, hard_stops, log_dir}
loop_count = 0

while not satisfied(contract.done_when) and loop_count < contract.max_loops:
    loop_count += 1
    tasks = decompose(remaining_work)                 # 분해
    log_decision("분해", tasks, refs=[대화, CLAUDE.md, rules])
    dispatch(tasks, isolation="worktree",             # 위임 (base = epic 브랜치 — 편집은 격리 subagent)
             run_in_background=true,
             model=main_allocates_per_task)           # 모델 배분 — 메인 판단 (아래 모델 분배)
    log_decision("병렬/순차 + 위임 형태(subagent/team) + 모델 배분", ...)
    results = await_completion_notifications()        # 모니터 (sleep/poll 금지)
    assert_topology()                                 # 가드: branch == epic + status clean (아래)

    for r in results:
        if r.failed:
            handle_failure(r)                         # 자동 재위임 규칙 (아래)
            log_decision("재위임 판단", r, refs=[실패이력, agent-monitor.md])
            continue
        verdict = review(r)                           # 작업별 리뷰어 게이트 (reviewer subagent 또는 teammate)
        if verdict.rejected:
            handle_review_rejection(r, verdict)       # 격리 subagent 재위임(실패 맥락 포함), team이면 SendMessage로 조율 (max_redispatch 예산 소모)
            log_decision("리뷰 거부 → 재위임/조율", verdict, refs=[리뷰 findings])

    merge_coordinate(passed_results)                  # 리뷰 통과분만 머지 — 충돌은 자동 위임 (아래)
    assert_topology()                                 # 가드: 매 머지 직후에도 (#783)
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
- **고정 배분을 박지 않는다**: 모델이 더 똑똑해지면 같은 작업을 더 가벼운 tier로 내릴 수 있어야 하므로, 매 dispatch마다 "지금도 이 역량이 필요한가"를 재평가한다. 모델 tier 정의는 `delegation-patterns.md §모델 선택` 표가 단일 출처 — 시작 기준으로만 참조하고 여기서 중복 정의하지 않는다.

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

핵심 제약:

- **격리는 subagent만 보장한다**: agent team teammate는 공유 checkout이라 per-teammate worktree 격리가 없다 — 같은 파일을 편집하면 덮어쓴다. `team_name`은 받지만 무시된다. 따라서 **편집은 teammate가 직접 하지 않고 `isolation:"worktree"` subagent에 위임**한다.
- **team은 실험 기능이다**: `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1`이 없으면 teammate가 spawn되지 않는다. 플래그가 없는 환경에서는 team을 쓰지 않고 단발 격리 subagent 재위임으로 review→fix를 돈다.

team을 쓸 때의 이득 (실험 플래그 활성 시):

- **리뷰어 게이트 조율**: reviewer teammate + implementer teammate를 한 team에 두면, 리뷰 거부 findings를 **team 내부 SendMessage로 전달**해 다음 라운드를 조율한다 (실제 편집은 implementer가 격리 subagent로 위임).
- **장기 런에서 식별·제어 가능**: 이름으로 SendMessage해 정체 해소·단계 전환을 지시할 수 있다 (*자동 개입 규칙*이 허용).
- **컨텍스트 격리 강화**: 한 작업의 반복 맥락이 team 안에 머물러 메인으로 전문(全文)이 올라오지 않는다.

구성:

- **feature/task 하나 = team 하나**(실험 플래그 시). reviewer + implementer 역할. 구성·이름·수명은 `delegation-patterns.md §Agent team 사용 패턴`을 따른다. **편집 격리는 team이 아니라 그 안에서 띄우는 `isolation:"worktree"` subagent가 책임진다** — teammate에게 worktree 이동을 위임하지 않는다 (격리가 프롬프트 희망으로 격하 → 공유 checkout 오염, #783).
- **review→fix 조율**: reviewer reject → implementer에게 SendMessage → implementer가 격리 subagent로 수정 재위임 → 재리뷰. 이 사이클도 `max_redispatch_per_task` 예산을 동일하게 소모한다 (무한 반복 금지). 소진 → hard stop → 에스컬레이션.

플래그가 없거나 조율이 불필요하면 **단발 격리 subagent 재위임**(이전 실패 맥락 포함)으로 review→fix를 돈다. 의심스러우면 단발 subagent를 고른다 — 격리가 항상 보장되기 때문이다.

---

## 종료 조건 (Done)

종료 조건은 **명시적이고 검증 가능**해야 한다. 모호한 종료 조건은 루프를 영원히 돌리거나 환각으로 조기 종료시킨다.

- ✅ 검증 가능: "모든 작업이 리뷰 통과 후 머지 완료 + `cargo test` green + `cargo fmt --check`/`clippy -D warnings` 통과"
- ❌ 검증 불가: "코드가 좋아 보이면", "대충 다 되면"
- 매 루프 종료 시 종료 조건을 **결정적으로** 재평가한다 — Bash로 테스트/빌드/lint를 실행하고 git 상태를 확인해 판정. 메인의 주관적 "다 된 것 같다"에 맡기지 않는다.

---

## 자동 개입 규칙 (자율 모드에서 허용)

기본 모드에서 금지된 행위가 자율 모드에서는 **가드레일 안에서** 허용된다.

| 행위 | 기본 모드 | 자율 모드 (opt-in) |
|------|----------|-------------------|
| 자동 재위임 | 외부환경 원인 1회만 | 예산(`max_redispatch_per_task`) 한도 내 반복 |
| SendMessage 명령 주입 | 금지 | 계획된 단계 전환 + 정체 해소용 허용 |
| 자동 머지 | 보고 후 진행 | 충돌 없으면 자동 |
| 자동 충돌 해결 | 위임/보고 후 결정 | `git-resolve` sub-agent에 자동 위임 |

각 행위는 **예산을 소모**한다. 예산이 소진되거나 hard stop에 닿으면 그 즉시 멈춘다.

### 재위임 (자동)

```
실패 감지 → 원인 추정 (agent-monitor.md 재위임 판단 기준 활용)
          → prompt 보강 (누적 실패 이력을 자기완결적으로 포함)
          → 새 isolation worktree로 재위임
          → redispatch_count[task] += 1
```

- `redispatch_count[task] > max_redispatch_per_task` → hard stop → 에스컬레이션
- 재위임 prompt에는 **이전 시도가 어디까지 갔고 무엇이 실패했는지**를 반드시 담는다 (sub-agent는 메인 대화를 못 봄).

### 리뷰어 게이트 (자동, 머지 전)

자율 모드는 사람이 PR을 보지 않는다. 따라서 **각 작업의 결과를 epic 브랜치에 머지하기 전에 리뷰어 sub-agent가 검증**하는 게이트를 둔다. 구현 sub-agent가 스스로 "통과했다"고 말하는 것에 머지를 맡기지 않는다 (자기 검증 편향).

- **역할 분리**: 구현과 리뷰는 **다른 sub-agent**다. 리뷰어는 구현자의 worktree(또는 diff)를 입력으로 받아 epic 브랜치 base 기준 변경을 검토한다. 같은 agent가 자기 코드를 리뷰하게 하지 않는다.
- **검토 범위**: 요구사항 충족, 회귀 위험, CLAUDE.md 품질 게이트(테스트/lint/포맷), 설계 원칙(SOLID 등) 위반, worktree 격리 준수.
- **출력**: `pass` / `reject` + 구체적 사유(`파일:라인`). reject 사유는 재위임 prompt에 그대로 실을 수 있는 자기완결 형태로 받는다.

```
구현 완료 → 리뷰어 검증
  pass   → 머지 후보로 승급 (이후 merge_coordinate)
  reject → 재위임 (리뷰 findings를 prompt에 포함, max_redispatch_per_task 예산 소모)
            예산 소진 → hard stop → 에스컬레이션
```

- 리뷰(코드 품질·요구사항 충족)와 `integration_verify`(인프라 의존 동작)는 둘 다 머지 전 게이트이며 둘 다 통과해야 머지한다 — 리뷰는 작업별, integration_verify는 루프별로 돈다.
- 리뷰어 거부는 실패와 동일하게 **`max_redispatch_per_task`를 소모**한다 — 리뷰 전용 새 예산을 만들지 않는다. 무한 재위임을 막는다.
- 리뷰어 모델도 위 *모델 분배* 원칙으로 메인이 작업 리스크에 맞춰 정한다 (자동 머지의 유일한 안전장치이므로 보통 더 강한 역량을 둘 가치가 있으나, 고정은 아님).
- 컨텍스트 격리: 리뷰어의 상세 findings·diff는 리뷰어 컨텍스트에 남기고, 메인은 verdict + 압축 요약만 받는다.
- `done_when` 평가에 **"머지된 모든 작업이 리뷰 통과"**를 포함한다.

### 머지 / 충돌 (자동)

```
충돌 없음 → merge-coordinator 순서 규칙대로 자동 머지
충돌 발생 → git-resolve sub-agent에 자동 위임
            성공 → 계속
            실패 → 재시도 1회 → 그래도 실패면 hard stop → 에스컬레이션
도메인 의미 충돌 (코드로 판정 불가) → 즉시 에스컬레이션 (자동 해결 금지)
```

### 토폴로지 가드 (assert_topology)

sub-agent의 격리 이탈로 메인 working tree branch가 sub-agent 브랜치로 switch되는 사고가 실제 자율 런에서 3회 재현됐다 (#783). 자율 모드는 보고 없이 연속 진행하므로 오염이 후속 dispatch/머지로 전파되기 전에 잡아야 한다. **매 sub-agent 완료 알림 수신 직후 + 매 머지 직후** 실행:

```bash
git branch --show-current    # epic/<name> 이어야 함
git status --short           # clean 이어야 함
```

위반 시 **hard stop** — `merge-coordinator.md`의 복구 절차(rebase abort → epic checkout)로 복구한 뒤 즉시 에스컬레이션한다. 자율 재개는 사용자 결정.

### 통합 검증 (integration_verify)

worktree sub-agent는 인프라 의존 환경(내부 자격증명, live DB, 외부 서비스 토큰 등)에 접근할 수 없다 (#782). 따라서:

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
.orchestrator/<epic>/decisions/      ← gitignore됨 (.autopilot/ · .review-output/ 와 동일 패턴)
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
- 에스컬레이션 판단 (멈춤 vs 계속)
- 종료 조건 충족 판정

### 기록 형식 (한 결정 = 한 항목)

```markdown
## <ISO timestamp> · <결정 요약>
- 상황: 무엇을 결정해야 했는가
- 참고: 본 소스 (대화 / CLAUDE.md §책임경계 / rules/git-workflow.md / 코드 상태 …)
- 결정: 무엇을 선택했는가
- 근거: 왜 (참고 소스와 연결)
- 대안: 고려했으나 택하지 않은 것 + 이유
- 영향: 어떤 작업/브랜치에 적용됐는가
```

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

## 에스컬레이션 (자율 모드라도 멈추는 조건)

opt-in 전면 허용이라도 다음은 **항상** 멈추고 사람에게 보고한다 (예산과 무관, 우선 적용):

- **되돌리기 어렵거나 외부로 나가는 행위**: force push, main 브랜치 머지, 배포, 외부 서비스 호출, 데이터 삭제 — 자율 모드는 epic 브랜치 안에서만 자율이고, 그 경계를 넘는 행위는 자동화 대상이 아니다.
- **토폴로지 위반**: 메인 working tree branch가 epic 브랜치가 아니게 되거나 의도치 않은 변경이 발견됨 — 복구 후 즉시 보고 (#783).
- **integration_verify 실패**: 계약에 정의된 인프라 의존 검증이 실패 — 자동 머지/루프 진행 금지 (#782).
- **도메인 의미 충돌**: 의도가 갈리는 머지 충돌 (코드로 판정 불가).
- **예산 소진**: 루프 상한 / 재위임 한도 / no-progress 도달.
- **원인 불명확한 반복 실패**: 같은 실패가 재위임에도 계속됨.
- **종료 조건 자체가 검증 불가능해짐**: 테스트 인프라 붕괴 등으로 done 판정이 불가능.

에스컬레이션 = 멈추고 **현재 상태 + 남은 작업 + 막힌 지점 + 선택지**를 한 번에 보고.

---

## 보고 (자율 모드)

```
진입 시:   자율 계약 1회 보고 (종료 조건 / 예산 / hard stop / 결정 기록 위치)
진행 중:   침묵 (정상 루프는 보고하지 않음) — 단 결정은 log_dir에 계속 append
           — hard stop / 에스컬레이션 발생 시 즉시 보고
종료 시:   종료 사유 (완료 / 예산 소진 / 에스컬레이션)
           + 루프 횟수 + 머지 결과 + 미해결 항목
           + 의사결정 요약 (총 결정 수 / 주요 분기 / 전체 로그 경로)
```

---

## 안티패턴

1. **종료 조건 없는 자율**: "알아서 끝까지"만 받고 `done_when` 미정의 → 무한 루프. 진입을 거부하고 검증 가능한 종료 조건부터 합의한다.
2. **검증 불가능한 종료 판정**: 메인 주관으로 "다 된 것 같다" → 환각 종료. Bash 명령 결과로 판정.
3. **가드레일 없는 재위임**: 예산 없이 실패 → 재위임을 무한 반복 → 폭주. 카운트 필수.
4. **hard stop 무시**: 되돌리기 어려운 행위까지 자동화 → 사고. 에스컬레이션 조건은 예산과 무관하게 우선한다.
5. **무진전 방치**: 루프는 도는데 종료 조건에 가까워지지 않음 → 예산만 소모. no-progress 감지로 조기 중단.
6. **opt-in 없이 자율**: 사용자가 명시하지 않았는데 자동 모드로 진입 → 기본은 휴먼-인-더-루프. 반드시 opt-in 후에만.
7. **sleep / poll**: 자율 루프에서도 완료 알림을 사용. `Bash sleep` 루프 금지.
8. **epic 경계 이탈**: 자율이라는 이유로 main 머지/배포까지 자동 → 자율은 epic 브랜치 안에서만. 경계 밖은 에스컬레이션.
9. **결정 기록 누락**: 근거를 남기지 않고 자율 주행 → 사용자가 사후에 "왜"를 복원 불가. 모든 분기 결정은 참고 소스와 함께 `log_dir`에 기록.
10. **결정 로그 커밋**: 휘발성 자율 산출물을 epic 브랜치에 커밋 → repo 오염. `.orchestrator/`는 gitignore, 완료 시 요약으로만 공유.
11. **토폴로지 가드 생략**: 완료 알림/머지 후 메인 branch 확인 없이 연속 진행 → 오염된 HEAD 위에서 다음 dispatch의 worktree base가 잘못 잡힘 (#783).
12. **인프라 의존 테스트를 worktree 검증에 포함**: sub-agent가 접근 불가한 환경 의존 테스트를 worktree에서 실행 → 환경 실패 noise로 PR 검증 신뢰도 저하. 계약의 `integration_verify`로 분리해 메인이 실행 (#782).
13. **리뷰 없는 자동 머지**: 구현 sub-agent의 자기 보고만 믿고 머지 → 자기 검증 편향으로 결함 통과. 머지 전 별도 리뷰어 게이트 필수.
14. **자기 코드 자기 리뷰**: 구현한 agent가 자기 결과를 리뷰 → 게이트 무력화. 리뷰어는 항상 구현자와 다른 sub-agent.
15. **리뷰 전용 무한 재위임**: 리뷰 거부를 `max_redispatch_per_task` 예산 밖에서 반복 → 폭주. 리뷰 거부도 동일 예산을 소모하고 소진 시 에스컬레이션.
16. **고정 모델 매핑 박기**: "구현은 항상 X, 리뷰는 항상 Y"로 못 박음 → 모델이 똑똑해져도 비효율 유지. 매 dispatch마다 작업 리스크에 맞춰 재평가.
17. **메인 컨텍스트로 전문 끌어오기**: 전체 diff·파일 전문·리뷰 findings 전문을 메인이 직접 통독 → 긴 루프에서 메인 컨텍스트 포화. 메인은 압축 요약 + verdict만 수령.
18. **teammate에 편집 격리 기대**: team은 공유 checkout이라 worktree 격리가 없는데 teammate가 직접 편집 → 덮어쓰기/메인 오염(#783). 편집은 `isolation:"worktree"` subagent에 위임하고 team은 조율만. 또한 단발 재위임 시 이전 실패 맥락을 새 prompt에 포함해 컨텍스트 손실을 줄인다.

---

## 체크리스트

진입 전:

- [ ] 사용자가 자율 모드를 명시 opt-in 했는가?
- [ ] 종료 조건이 명령으로 판정 가능한 형태인가?
- [ ] 예산(`max_loops` / `max_redispatch_per_task` / no-progress)을 계약에 고정했는가?
- [ ] hard stop(에스컬레이션) 조건을 정의했는가?
- [ ] 결정 기록 위치(`.orchestrator/<epic>/decisions/`)를 계약에 고정했는가?
- [ ] 자율 계약을 사용자에게 1회 보고했는가?

진입 전 (계약):

- [ ] 인프라 의존 테스트가 있다면 `integration_verify` (command + run_at)를 계약에 정의했는가?

루프 중:

- [ ] 매 sub-agent 완료 직후 + 매 머지 직후 토폴로지 가드를 실행하는가?
- [ ] 계약의 integration_verify를 run_at 시점에 메인이 직접 실행하는가?
- [ ] 편집·격리가 필요한 작업을 **`isolation:"worktree"` subagent**에 위임했는가? (teammate 직접 편집 금지 — team은 공유 checkout)
- [ ] 작업이 多·의존성이 있으면 Task 시스템(`TaskCreate`/`addBlockedBy`/`owner`)으로 상태를 추적하는가? (`agent-monitor.md §Task 시스템`)
- [ ] 리스크 큰/되돌리기 어려운 편집은 **계획 우선 게이트**(계획 받아 승인 후 편집 재위임)를 거쳤는가? (`delegation-patterns.md §계획 우선 게이트`)
- [ ] team을 썼다면 실험 플래그가 활성이고, 편집은 그 안의 격리 subagent로 위임했는가? (조율은 SendMessage)
- [ ] 각 작업을 **머지 전 리뷰어**(구현자와 다른 역할/agent)로 검증하는가?
- [ ] 리뷰 거부가 `max_redispatch_per_task` 예산을 소모하며 카운트되는가?
- [ ] 각 작업의 모델을 작업 리스크에 맞춰 배분하고, 비표준 선택은 기록하는가?
- [ ] 메인이 전문 대신 압축 요약 + verdict만 수령하는가? (컨텍스트 격리)
- [ ] 매 루프 종료 시 종료 조건을 결정적으로 재평가하는가?
- [ ] 재위임 / 머지 / 충돌 해결이 예산을 소모하며 카운트되는가?
- [ ] 진전을 수치로 측정하는가? (체감 아님)
- [ ] 각 분기 결정을 참고 소스와 함께 `log_dir`에 기록하는가?
- [ ] hard stop 발생 시 예산과 무관하게 즉시 멈추는가?

종료 시:

- [ ] 종료 사유(완료 / 예산 소진 / 에스컬레이션)를 보고했는가?
- [ ] 의사결정 요약(총 수 / 주요 분기 / 로그 경로)을 보고에 포함했는가?
- [ ] 미해결 항목과 남은 worktree를 정리/보고했는가?
