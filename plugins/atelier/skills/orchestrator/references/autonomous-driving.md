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
    dispatch(tasks, isolation="worktree",             # 위임 (base = epic 브랜치)
             run_in_background=true)
    log_decision("병렬/순차 + 위임 형태", ...)
    results = await_completion_notifications()        # 모니터 (sleep/poll 금지)
    assert_topology()                                 # 가드: branch == epic + status clean (아래)

    for r in results:
        if r.failed:
            handle_failure(r)                         # 자동 재위임 규칙 (아래)
            log_decision("재위임 판단", r, refs=[실패이력, agent-monitor.md])

    merge_coordinate(results)                         # 머지 — 충돌은 자동 위임 (아래)
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

## 종료 조건 (Done)

종료 조건은 **명시적이고 검증 가능**해야 한다. 모호한 종료 조건은 루프를 영원히 돌리거나 환각으로 조기 종료시킨다.

- ✅ 검증 가능: "모든 작업 머지 완료 + `cargo test` green + `cargo fmt --check`/`clippy -D warnings` 통과"
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
- [ ] 매 루프 종료 시 종료 조건을 결정적으로 재평가하는가?
- [ ] 재위임 / 머지 / 충돌 해결이 예산을 소모하며 카운트되는가?
- [ ] 진전을 수치로 측정하는가? (체감 아님)
- [ ] 각 분기 결정을 참고 소스와 함께 `log_dir`에 기록하는가?
- [ ] hard stop 발생 시 예산과 무관하게 즉시 멈추는가?

종료 시:

- [ ] 종료 사유(완료 / 예산 소진 / 에스컬레이션)를 보고했는가?
- [ ] 의사결정 요약(총 수 / 주요 분기 / 로그 경로)을 보고에 포함했는가?
- [ ] 미해결 항목과 남은 worktree를 정리/보고했는가?
