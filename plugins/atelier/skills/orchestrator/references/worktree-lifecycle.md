---
name: worktree-lifecycle
description: isolation worktree 사용 패턴과 dispatch 사전 충돌 검증. 머지/정리 책임은 merge-coordinator로 위임. orchestrator 스킬 내부 참조 문서.
version: 0.1.0
user-invocable: false
---

# Worktree Lifecycle

병렬 작업의 격리와 dispatch 사전 검증을 다룬다. **결과 수령 후 머지/충돌/정리 책임은 `merge-coordinator.md`로 위임한다** (라이프사이클 파일은 격리 패턴까지만 다룸).

> **적용 범위**: 이 문서 전체가 **무거운 경로** 전용이다 (판정: `SKILL.md` §경로 판정 게이트).

## 토폴로지: epic 브랜치 + worktree 격리

오케스트레이터에서 worktree는 **항상 epic 브랜치 위의 sub-agent 격리 수단**이다. 메인은 worktree를 사용하지 않는다.

```
main
  └─ epic/<name>   ← 메인 에이전트 (read + dispatch + report)
       ├─ worktree A → epic/<name>/t1-<slug>  (sub-agent A, base = epic/<name>)
       ├─ worktree B → epic/<name>/t2-<slug>  (sub-agent B, base = epic/<name>)
       └─ worktree C → epic/<name>/t3-<slug>  (sub-agent C: ...)
```

- 모든 sub-agent worktree는 **현재 epic 브랜치 기준으로 동기화된 상태에서 작업**한다 (자동 보장 아님 — 확인·동기화는 `delegation-patterns.md §Prompt 작성 원칙 필수 포함 요소` 9번이 단일 출처). dispatch **이후** epic HEAD가 움직여 생기는 drift는 별도 정책이다 (`branch-strategy.md §base drift 전파`)
- sub-agent 결과는 **epic 브랜치로 머지** (main 직접 머지 X). 브랜치 이름은 자동 생성명을 쓰지 않고 `epic/<name>/t<task-id>-<slug>` 규약을 따른다 (`branch-strategy.md §브랜치 네이밍`)
- 메인은 epic 브랜치의 **메인 working tree**에 머문다 — EnterWorktree 금지

## 사용 방식: Agent isolation 한 가지만

`Agent`에 `isolation: "worktree"`와 `run_in_background: true`를 지정하고, prompt에 epic 브랜치 이름을 컨텍스트로 포함한다.

- Agent가 자동으로 worktree를 만들고 그 안에서 작업 (base가 dispatch 시점 epic 브랜치 HEAD라는 보장은 없다 — 확인·동기화는 `delegation-patterns.md §Prompt 작성 원칙 필수 포함 요소` 9번이 단일 출처)
- **변경이 없으면 자동 정리** — 메인이 신경 쓸 필요 없음
- **변경이 있으면** 결과에 worktree 경로와 브랜치명이 포함됨
- 병렬 fan-out에 가장 적합

**금지: 메인이 직접 EnterWorktree로 진입**. 메인은 편집을 안 하므로 worktree 진입 자체가 불필요하고, 진입하면 dispatch 토폴로지가 깨진다. sub-agent에게 위임된 worktree 상태를 봐야 할 때도 `git -C <worktree-path> ...` Bash 호출이나 새 read-only sub-agent로 처리한다.

---

## 병렬 dispatch 패턴

```
# 진입 검증: 메인이 epic 브랜치 + 메인 working tree인가?
assert `git branch --show-current` == "epic/<name>"
assert `git rev-parse --show-toplevel` == repo의 메인 working tree

# 사전 검증: 작업들의 변경 파일 집합이 disjoint인가?
files_A = analyze_files(task_A)  # Read/Glob/Grep으로 영향받을 파일 추정
files_B = analyze_files(task_B)
S = files_A ∩ files_B
if S and not all_hot_spot(S):     # S 가 전부 hot-spot 이면 병렬 유지 +
    → 순차로 전환 (worktree 병렬 X)  #   통합 task 분리 (branch-strategy.md §hot-spot 파일)

# Dispatch — 직렬로, 한 메시지에 하나씩 (같은 tool-call batch에 worktree dispatch를
# 2개 이상 싣지 않는다 — 아래 §dispatch 생성 가드. base는 자동 보장되지 않음 —
# prompt에 base 확인·동기화 지시 포함)
before = snapshot(`git worktree list --porcelain`)
Agent({description: "task A", isolation: "worktree", run_in_background: true,
       prompt: "<자기완결, epic 브랜치 이름 포함>"})
assert_worktree_created(before)   # 생성 확인 후에야 다음 dispatch

before = snapshot(`git worktree list --porcelain`)
Agent({description: "task B", isolation: "worktree", run_in_background: true,
       prompt: "<자기완결, epic 브랜치 이름 포함>"})
assert_worktree_created(before)

# 메인은 epic 브랜치에서 다른 일 진행 또는 사용자 응대
# (직렬화되는 것은 dispatch 행위뿐이다 — agent들은 background에서 여전히 병렬로 돈다)
# 완료 알림 자동 도착 — sleep/poll 금지
```

---

## dispatch 생성 가드 (assert_worktree_created — 단일 출처)

동일 tool-call batch에서 worktree-isolated agent를 2개 이상 동시 생성하면, worktree 생성이 직렬화되지 않아 한쪽이 누락되는 race가 알려져 있다. worktree를 받지 못한 agent는 **메인 working tree에서 직접 편집·커밋**해 격리 계약이 깨지고, 메인 shell cwd가 다른 agent의 worktree로 drift하는 부수효과도 관찰됐다. 완료 알림 시점의 토폴로지 가드로는 늦다 — 유출이 이미 커밋된 뒤다. 그래서 dispatch 시점에 두 가지를 지킨다:

1. **직렬화**: worktree dispatch는 **한 메시지에 하나씩**. 이전 dispatch의 worktree 생성을 확인한 뒤에야 다음을 dispatch한다. 병렬성은 잃지 않는다 — `run_in_background: true` agent는 dispatch가 직렬이어도 실행은 병렬이다.
2. **생성 검증**: 매 dispatch 직후 메인이 직접 Bash로 확인한다:

```bash
git worktree list --porcelain   # dispatch 전 스냅샷 대비 새 worktree가 실제 추가됐는가
git status --short              # 메인 working tree가 여전히 clean인가
git rev-parse --show-toplevel   # 메인 shell cwd가 메인 working tree인가 (worktree로 drift 감지)
```

위반 시 처리 (셋 중 하나라도):

- **새 worktree 미등장** → 해당 agent가 격리 없이 돌고 있을 수 있다. **후속 dispatch를 중단**하고 해당 agent를 정지시킨 뒤 메인 working tree 오염 여부를 확인한다. 자율 모드면 hard stop이다 (`autonomous-driving.md §에스컬레이션` 조건 2와 동급 — 토폴로지 위반).
- **메인 not clean** → 유출이 이미 시작된 것. 즉시 위와 동일하게 중단하고, 변경은 버리지 않고 보존한다 — 복구 절차는 `merge-coordinator.md §토폴로지 가드`가 단일 출처다.
- **cwd drift** → 메인 working tree로 복귀 후 재검증한다 (오염 검사를 worktree 안에서 하면 결과가 그 worktree 것으로 바뀌어 가드 자체가 무의미해진다).

주의: 검증 기준은 **worktree 등장 + 메인 clean + cwd**다. 규약 브랜치(`epic/<name>/t*`)는 agent가 작업 시작 후 스스로 전환하므로 dispatch 직후에는 아직 없을 수 있다 — 브랜치 존재를 생성 검증 기준으로 삼지 않는다.

---

## 결과 수령 후 처리

**모든 sub-agent 완료 알림 수신 직후, 다른 처리에 앞서 토폴로지 가드를 실행한다** — sub-agent가 격리를 이탈해 메인 working tree를 변형하면 메인 branch가 switch될 수 있고, 그 상태를 못 보고 진행하면 다음 dispatch의 worktree base가 잘못 잡힌다. 가드 명령과 복구 절차는 `merge-coordinator.md §토폴로지 가드`가 단일 출처다 (불일치는 자율 모드라도 hard stop).

가드 통과 후 결과를 후속 단계로 위임한다:

- **변경 없음** → 자동 정리됨. 추가 조치 불필요.
- **변경 있음 (성공/실패 무관)** → `merge-coordinator.md`로 이동. 머지 순서 결정, 충돌 처리, 정리 책임이 그쪽에 있다. 단 **아직 in-flight인 sub-agent가 남아 있으면 기본값은 배치 머지**라 여기서 바로 머지하지 않는다 — 언제 머지할지의 정책은 `branch-strategy.md §base drift 전파`가 단일 출처다.
- **재위임 판단 기준** → `agent-monitor.md` (외부 환경 vs prompt 결함 vs 불명확).

이 파일은 격리 패턴까지만 다루고, 결과 통합 로직은 의도적으로 가지고 있지 않다.

---

## 충돌 위험 사전 분석

병렬 가능성을 판단하기 위해 메인이 epic 브랜치에서 수행할 조사:

```
1. Glob으로 작업 영역 추정
   - "auth 관련 작업" → Glob("src/auth/**", "tests/auth/**")

2. Grep으로 cross-reference 식별
   - 작업 A가 건드릴 함수가 작업 B 영역에서 import되는가?

3. git log --stat main..epic/<name> 로 epic 누적 변경 영역 확인
   - 같은 파일이 반복적으로 수정되는 영역인가?

4. 의존성 그래프 추정
   - import 체인이 작업 간에 얽혀 있는가?
```

이 분석은 메인이 직접 수행한다 (Read/Glob/Grep/Bash) — 짧고 결정적이라 위임할 가치가 없다.

---

## 안티패턴

1. **모든 작업을 worktree로 격리**: 단일 작업이나 읽기 전용 분석에도 worktree → 오버헤드만 큼. disjoint 병렬에만 사용.
2. **검증 없이 병렬**: disjoint 분석 없이 worktree 병렬 던지기 → 머지 시 사람이 다 처리. 사전 분석 필수.
3. **메인이 worktree에 진입**: EnterWorktree로 메인이 들어가서 직접 편집 → 오케스트레이터 원칙 위반.
4. **worktree 누수**: 결과를 받은 뒤 머지/폐기 결정을 안 하고 방치 → 디스크/git 상태 오염.
5. **epic 브랜치 아닌 곳에서 dispatch**: main이나 임의 feature 브랜치에서 worktree sub-agent 호출 → worktree base가 epic이 아니게 되어 결과 머지 경로가 어긋남.
6. **완료 알림 후 가드 생략**: sub-agent 완료 직후 메인 branch/status 확인 없이 다음 단계 진행 → sub-agent의 격리 이탈(Edit 절대경로 트랩 등)로 변형된 메인 state 위에서 후속 작업이 진행됨.
7. **단일 batch 다중 worktree dispatch**: 한 메시지(tool-call batch)에 worktree-isolated dispatch를 2개 이상 → 생성 race로 한쪽 worktree가 누락되고 그 agent의 편집이 메인 트리로 유출. 직렬 dispatch + 생성 가드 필수 (§dispatch 생성 가드).

---

## 체크리스트

병렬 dispatch 전:

- [ ] 메인이 현재 epic 브랜치 + 메인 working tree에 있는가? (`git branch --show-current` / `git rev-parse --show-toplevel`)
- [ ] 작업들의 변경 파일 집합을 추정했는가?
- [ ] 겹치는 파일이 있다면 hot-spot 여부를 갈랐는가? (전부 hot-spot이면 병렬 유지 + 통합 task 분리 — `branch-strategy.md §hot-spot 파일`)
- [ ] disjoint가 명확한가? (의심스러우면 순차)
- [ ] 각 sub-agent prompt가 자기완결적이며 epic 브랜치 이름과 `epic/<name>/t<task-id>-<slug>` 전환 지시를 포함하는가?
- [ ] `isolation: "worktree"`와 `run_in_background: true`를 켰는가?
- [ ] worktree dispatch를 한 메시지에 하나씩 싣고 있는가? (같은 batch 다중 dispatch 금지 — §dispatch 생성 가드)

dispatch 후:

- [ ] 매 dispatch 직후 생성 가드를 실행했는가? (worktree 등장 + 메인 clean + cwd — §dispatch 생성 가드)
- [ ] 완료 알림을 기다리는 중에 sleep/poll을 하고 있지 않은가?
- [ ] 완료 알림 수신 직후 토폴로지 가드를 실행했는가? (`merge-coordinator.md §토폴로지 가드`)
- [ ] 각 결과의 worktree 상태(변경 유무)를 파악했는가?
- [ ] 변경 있는 결과를 `merge-coordinator.md` 단계로 넘겼는가? (이 파일의 책임은 여기서 끝)
