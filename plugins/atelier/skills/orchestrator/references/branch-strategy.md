---
name: branch-strategy
description: 충돌을 전제로 한 브랜치 운영 전략 — worktree 브랜치 네이밍, hot-spot 파일 분리, base drift 전파, 머지 방식(rebase 후 ff), epic←main 역방향 동기화, 충돌 반복의 재분해 신호. orchestrator 스킬 내부 참조 문서.
version: 0.1.0
user-invocable: false
---

# Branch Strategy

**충돌은 병렬 fan-out의 예외가 아니라 정상 산물이다.** 이 문서는 *터진 충돌을 어떻게 푸는가*(그건 `git` skill 의 `references/conflict-resolution.md`)가 아니라, **터지는 양 자체를 브랜치 운영으로 줄이는 규칙**의 단일 출처다.

> **적용 범위**: 이 문서 전체가 **무거운 경로** 전용이다 (판정: `SKILL.md` §경로 판정 게이트). 경량 경로에는 머지할 브랜치가 없다.

충돌이 생기는 자리는 넷이고, 이 문서는 각각에 규칙 하나씩을 둔다.

| 자리 | 규칙 |
|---|---|
| 여러 작업이 같은 파일에 append 한다 | §2 hot-spot 분리 |
| epic HEAD가 움직여 in-flight worktree가 뒤처진다 | §3 base drift 전파 |
| 통합 방식이 섞여 충돌 해결 정책과 어긋난다 | §4 rebase 후 ff |
| main이 움직여 epic이 뒤처진다 | §5 역방향 흡수 |

분해 시점의 충돌 최소화(수직 슬라이스·선행 task)는 이 문서가 아니라 `SKILL.md §분해는 충돌 경계로 쪼갠다`가 단일 출처다 — 항상 로드돼야 판정 전에 걸린다.

---

## 1. 브랜치 네이밍 (단일 출처)

```
epic/<name>                       ← 메인 에이전트가 점유
epic/<name>/t<task-id>-<slug>     ← sub-agent worktree 브랜치
```

`isolation: "worktree"` 는 브랜치를 자동 생성하지만 그 이름은 **agent 식별자**라 작업 단위와 연결되지 않는다. dispatch prompt에 **첫 커밋 전에 지정한 이름으로 전환**하라고 명시한다:

```
작업 시작 시 `git switch -c epic/<name>/t<task-id>-<slug>` 로 브랜치를 만들고 그 위에서만 커밋하라.
```

이름을 규약으로 고정하면 얻는 것:

- **머지 후보 수집이 결정적이 된다** — `git branch --list 'epic/<name>/t*'`. sub-agent 결과 텍스트에 브랜치명이 실려 오기를 기다리지 않아도 되고, 결과를 유실한 agent의 작업물도 찾을 수 있다
- **Task ↔ 브랜치가 연결된다** — `t<task-id>` 가 `TaskGet` 의 id와 같은 값이라 어느 브랜치가 어느 Task 것인지 조회가 필요 없다
- **고아 브랜치를 식별할 수 있다** — 중단·실패한 런이 남긴 브랜치가 Task 목록과 대조된다

`<slug>` 는 영소문자·하이픈, 3~5 단어. 커밋 메시지 규약(`.claude/rules/git-workflow.md`)과 달리 type prefix를 붙이지 않는다 — 브랜치는 epic 안에서만 살고 PR 타이틀이 되지 않는다.

---

## 2. hot-spot 파일 — disjoint 판정에서 떼어낸다

**hot-spot**: 작업들의 의도는 겹치지 않는데 **같은 라인 근처에 항목을 추가**하게 되는 파일. 내용 충돌이 아니라 위치 충돌이라 거의 100% 재현된다.

전형적인 것:

- 의존성 lock 파일 (`Cargo.lock`, `package-lock.json`, `go.sum`)
- 모듈 re-export (`mod.rs`, `index.ts`, `__init__.py`)
- 라우트·DI·플러그인 등록 지점
- 마이그레이션 시퀀스 번호·순번이 있는 파일
- i18n 리소스, 전역 상수 테이블, 피처플래그 목록

**전형 사례이지 고정 목록이 아니다** — 판정은 위 정의(의도는 안 겹치는데 위치가 겹치는가)로 한다. 목록에 없다고 hot-spot이 아닌 것이 아니고, 레포마다 다른 것이 여기 걸린다.

### 판정 절차

`worktree-lifecycle.md §충돌 위험 사전 분석`으로 구한 **겹치는 파일 집합 S** 를 그대로 순차 판정에 넣지 않는다:

```
겹치는 파일 집합 S
  │
  ├─ S 가 비었다 ──────────────────→ 병렬
  │
  ├─ S 가 전부 hot-spot ───────────→ 병렬 유지
  │                                  + S 를 통합 task 1개로 분리 (아래)
  │
  └─ S 에 hot-spot 아닌 파일이 있다 → 순차 (delegation-patterns.md §병렬 vs 순차 결정 트리의 순차 규칙 그대로)
```

**이 분기가 없으면 hot-spot 파일 하나 때문에 fan-out 전체가 순차로 떨어진다.** 실제로 겹치는 것은 등록 한 줄뿐인데 병렬 이득을 통째로 잃는 것이 현재 판정의 실패 모드다.

### 통합 task 처리

1. **병렬 작업 prompt에 hot-spot 편집 금지 + 출구를 함께 준다** (금지에 출구를 짝짓는 형태 — `delegation-patterns.md §필수 포함 요소` 10번):

```
다음 파일은 수정하지 마라: <hot-spot 목록>.
등록·추가가 필요하면 직접 쓰지 말고, 필요한 항목을
`SendMessage({to:"main", ...})` 로 "파일 → 추가할 내용" 형태로 보고하라.
```

2. **모든 병렬 결과를 머지한 뒤**, 보고된 항목을 모아 **hot-spot 전용 task 1개**를 순차로 dispatch한다. 충돌 지점이 1곳·1회로 모인다.

3. **lock 파일은 편집이 아니라 재생성이다** — 통합 task에서 패키지 매니저 명령 1회(`cargo build`, `npm install` 등)로 만든다. 수동 병합은 금지: 텍스트로는 그럴듯하게 합쳐져도 해석 불가능한 lock을 만든다.

---

## 3. base drift 전파 (in-flight 재동기화)

**epic HEAD가 움직이는 순간, 아직 돌고 있는 모든 worktree의 base는 stale이다.** dispatch 시점의 base 확인(`delegation-patterns.md §필수 포함 요소` 9번)은 dispatch **이후**에 생기는 이 drift를 커버하지 못한다.

머지 정책은 두 가지이고, **기본은 A**다.

### A. 배치 머지 (기본)

**in-flight sub-agent가 하나라도 남아 있으면 머지하지 않는다.** 같은 fan-out 배치의 모든 sub-agent가 완료된 뒤 머지 순서를 정해 순차 머지한다.

- drift 자체가 발생하지 않으므로 전파도 필요 없다
- 적용: 배치가 몇 분~수십 분에 끝나는 보통의 fan-out — 즉 대부분
- 비용: 첫 완료분의 머지가 마지막 완료까지 지연된다. 게이트(리뷰·QA)는 지연되지 않는다 — 완료 즉시 돌리고 머지만 미룬다

### B. 즉시 머지 + rebase 전파

배치가 길거나 후속 작업이 앞 결과를 base로 기다려야 할 때만 쓴다.

- 머지 직후 **in-flight worktree 전부**에 재동기화를 지시한다:

```
SendMessage({to: <agentId 또는 name>,
  summary: "rebase onto new epic HEAD",
  message: "epic/<name> 이 <sha> 로 갱신됐다. 지금 작업을 커밋하거나 stash 한 뒤
            epic/<name> 위로 rebase 하고 이어서 진행하라.
            충돌이 나면 직접 풀지 말고 SendMessage 로 보고하라."})
```

- **전파 대상을 겹침 추정으로 좁히지 않는다.** 겹침은 사전 추정이라 틀릴 수 있고, 겹치지 않는 worktree의 rebase는 무비용이다. 전부 보낸다
- **`SendMessage` 가 없으면 B는 성립하지 않는다** — team 비가용 판정(`SKILL.md §진입 시 체크 4`)이면 A로 고정한다. 전파 수단 없이 즉시 머지하는 것은 아래 금지에 해당한다

### 금지: 머지해놓고 전파하지 않기

**B의 절반만 하는 것이 A보다도 나쁘다.** drift한 worktree가 옛 코드 기준으로 작업을 이어가면, 좋게 끝나야 충돌이고 나쁘면 **이미 머지된 수정을 되돌린 결과가 다음 머지에서 조용히 통과**한다. 후자는 테스트가 잡지 못하는 경우가 많다 — 되돌려진 쪽의 테스트도 함께 되돌아가기 때문이다.

어느 정책을 골랐는지와 근거는 진입 보고와 decision log에 남긴다.

---

## 4. 머지 방식 — rebase 후 fast-forward (확정)

worktree 브랜치 → epic 브랜치 통합은 **rebase 후 ff**로 고정한다. 판정하지 않는다.

```bash
git -C <worktree> rebase epic/<name>          # 충돌은 sub-agent 에 위임
git merge --ff-only epic/<name>/t<id>-<slug>  # epic 브랜치의 메인 working tree 에서
```

**`--ff-only` 인 이유**: rebase가 실제로 끝났는지를 명령이 검증한다. 그냥 `git merge` 면 rebase를 빠뜨려도 머지 커밋으로 조용히 통과해서, 이 절의 규칙이 지켜졌는지 사후에 알 수 없다.

**rebase 로 고정하는 이유 셋**:

1. 충돌 해결 정책의 단일 출처(`git` skill `references/conflict-resolution.md`)가 **rebase 전제**다 — 그 문서의 ours/theirs 방향은 merge와 반대이므로, 통합 방식을 섞으면 그 문서가 절반의 경우에 **틀린 지침**이 된다
2. epic 히스토리가 선형이라 `git log --oneline epic/<name>` 이 그대로 task 단위 진행 기록이 된다
3. 최종 통합 검증 게이트가 red일 때 revert·이분 탐색의 단위가 task와 일치한다

**주의**:

- 이미 push된 worktree 브랜치를 rebase하면 히스토리가 바뀐다 — 이때 push 정책은 `git` skill `§force-push 정책`이 단일 출처다
- **epic 브랜치 자체는 rebase하지 않는다** — sub-agent worktree들이 base로 삼는 공유 브랜치라 히스토리를 바꾸면 in-flight worktree가 전부 깨진다 (§5도 같은 이유로 merge다)

---

## 5. epic ← main 역방향 drift

`epic → main` 머지는 이 스킬 범위 밖이지만(`SKILL.md §토폴로지`), **main이 움직여 epic이 뒤처지는 것**은 런 안에서 처리한다. 안 하면 최종 통합 검증 게이트가 green이어도 main 기준으로는 깨진 상태로 완료를 선언하게 된다.

```bash
git fetch origin <default-branch>
git rev-list --count epic/<name>..origin/<default-branch>
```

- **확인 시점**: 최종 통합 검증 게이트 **직전 1회** (`merge-coordinator.md §최종 통합 검증 게이트`)
- **0이 아니면 main을 epic에 머지한다** — rebase가 아니다 (§4 주의의 두 번째 항목과 같은 이유)
- **머지 후 전체 스위트를 다시 돌린다.** 게이트의 green은 main을 흡수한 뒤의 HEAD 기준이어야 의미가 있다. 보고하는 HEAD sha도 흡수 후의 것이다
- **충돌 위임 패킷은 §4의 것을 그대로 쓰지 않는다.** 통합 경로의 기본 위임(`merge-coordinator.md §충돌 시 위임`)은 "해결 후 epic 위로 rebase"를 지시하는 **rebase 전제**인데, 이 흡수는 merge이고 epic은 rebase하면 안 되는 브랜치다. 패킷에 **"이것은 merge다 — epic 브랜치를 rebase하지 말 것"** 을 명시하고, ours/theirs 방향은 merge 기준으로 읽으라고 못 박는다 (방향 자체는 `git` skill `references/conflict-resolution.md`가 단일 출처)
- 도메인 의미 충돌이면 자율 모드라도 에스컬레이션(`autonomous-driving.md §에스컬레이션`)

**루프 중간에는 하지 않는다.** main을 흡수할 때마다 in-flight worktree 전부에 §3의 전파가 필요해져 비용이 곱해진다. 런이 길어 중간 흡수가 불가피하면 **배치 경계**(§3 A안의 머지 시점)에서만 한다.

---

## 6. 충돌 반복 = 재분해 신호

같은 파일에서 **2회 이상** 충돌이 나면 개별 작업의 실패가 아니라 **분해가 틀린 것**이다. 재위임을 반복하면 예산만 태우고 같은 자리로 돌아온다.

```
같은 파일 충돌 1회 → 정상. 충돌 해결 위임 (merge-coordinator.md §충돌 시 위임)
같은 파일 충돌 2회 → 재위임 중단. 그 파일을 hot-spot 으로 재분류(§2)하고
                    관련 task 들을 순차로 내린다
같은 파일 충돌 3회 → 에스컬레이션. 분해 자체를 사용자에게 보고
```

- **이 카운터는 `max_redispatch_per_task` 예산과 별개다** — 예산은 task 단위, 이것은 **파일 단위**다. task를 갈아치워도 파일이 같으면 카운트는 이어진다. 그래서 task 예산만으로는 이 패턴이 잡히지 않는다
- **2·3은 기본값이다.** `max_redispatch_per_task` 와 같은 성격의 예산이므로, 런의 성격상 공유 파일을 여러 task가 정당하게 건드릴 수밖에 없으면 메인이 진입 시 다른 상한을 정하고 근거를 자율 계약·decision log에 남긴다. 조정 가능하다는 것이지 생략 가능하다는 뜻은 아니다
- 재분류·직렬화·에스컬레이션은 전부 decision log에 남긴다 (`autonomous-driving.md §의사결정 기록`)

---

## 안티패턴

1. **자동 생성 브랜치명 방치**: `isolation: "worktree"` 가 만든 agent 식별자 이름을 그대로 둠 → 머지 후보 수집이 결과 텍스트에만 의존하고, 결과를 유실한 agent의 작업물은 찾을 방법이 없다 (§1).
2. **hot-spot 때문에 전부 순차**: 겹치는 파일이 lock/re-export뿐인데 순차로 내림 → 병렬 이득을 통째로 잃는다 (§2).
3. **hot-spot 을 병렬 agent 들이 각자 편집**: 등록 라인을 각자 append → 매 머지마다 충돌. 금지 계약 + 통합 task로 모은다 (§2).
4. **머지 후 전파 없음**: 즉시 머지해놓고 in-flight worktree를 그대로 둠 → 머지된 수정을 되돌린 결과가 조용히 통과할 수 있다. B의 절반만 하느니 A를 쓴다 (§3).
5. **rebase 없이 머지 커밋으로 통합**: `git merge` 로 통합 → 충돌 해결 정책(rebase 전제)과 어긋나고, rebase 누락이 검증되지 않는다. `--ff-only` 로 강제한다 (§4).
6. **epic 브랜치 rebase / force push**: 공유 base의 히스토리를 바꿈 → in-flight worktree 전부가 깨진다 (§4·§5).
7. **반복 충돌을 재위임으로만 대응**: 같은 파일이 계속 터지는데 task 예산만 태움 → 분해가 틀렸다는 신호를 놓친다 (§6).

---

## 체크리스트

병렬 dispatch 전:

- [ ] 겹치는 파일 집합을 hot-spot / 비-hot-spot 으로 갈랐는가? (전부 hot-spot이면 병렬 유지)
- [ ] hot-spot 편집 금지 + 보고 형식(출구)을 각 prompt에 넣었는가?
- [ ] 각 prompt에 `epic/<name>/t<task-id>-<slug>` 브랜치 전환 지시를 넣었는가?
- [ ] 머지 정책(A 배치 / B 즉시+전파)을 정하고 근거를 기록했는가? `SendMessage` 비가용이면 A인가?

머지 단계:

- [ ] (B 선택 시) 매 머지 직후 in-flight worktree **전부**에 rebase 전파를 보냈는가?
- [ ] 통합을 `rebase → merge --ff-only` 로 했는가? (`--ff-only` 실패는 rebase 누락 신호)
- [ ] hot-spot 통합 task를 병렬 결과 머지 **뒤에** 순차로 돌렸는가? lock 파일은 재생성했는가?
- [ ] 같은 파일 충돌 횟수를 파일 단위로 세고 있는가? (2회 → hot-spot 재분류, 3회 → 에스컬레이션)

완료 선언 전:

- [ ] 최종 게이트 직전에 `epic..origin/<default-branch>` 를 확인하고, 뒤처졌으면 main을 머지한 뒤 스위트를 다시 돌렸는가?
