---
name: worktree-lifecycle
description: isolation worktree 사용 패턴과 dispatch 사전 충돌 검증. 머지/정리 책임은 merge-coordinator로 위임. orchestrator 스킬 내부 참조 문서.
version: 0.1.0
user-invocable: false
---

# Worktree Lifecycle

병렬 작업의 격리와 dispatch 사전 검증을 다룬다. **결과 수령 후 머지/충돌/정리 책임은 `merge-coordinator.md`로 위임한다** (라이프사이클 파일은 격리 패턴까지만 다룸).

## 두 가지 worktree 사용 방식

### 방식 1: Agent isolation (권장)

```
Agent({
  isolation: "worktree",
  run_in_background: true,
  prompt: "..."
})
```

- Agent가 자동으로 worktree를 만들고 그 안에서 작업
- **변경이 없으면 자동 정리** — 메인이 신경 쓸 필요 없음
- **변경이 있으면** 결과에 worktree 경로와 브랜치명이 포함됨
- 병렬 fan-out에 가장 적합

### 방식 2: 명시적 EnterWorktree

```
EnterWorktree({...})  # 메인이 worktree로 진입
# ... 작업 ...
ExitWorktree({...})   # 정리
```

- 메인이 직접 worktree를 다뤄야 할 때 (드물게)
- 오케스트레이터 패턴에서는 거의 안 씀 — 메인은 편집을 안 하니까
- 사용 케이스: 위임된 worktree 상태를 메인이 잠깐 들여다봐야 할 때

**권장**: 방식 1만 사용. 메인은 worktree 진입 자체를 피하고 sub-agent에게 맡김.

---

## 병렬 dispatch 패턴

```
# 사전 검증: 작업들의 변경 파일 집합이 disjoint인가?
files_A = analyze_files(task_A)  # Read/Glob/Grep으로 영향받을 파일 추정
files_B = analyze_files(task_B)
if not disjoint(files_A, files_B):
    → 순차로 전환 (worktree 병렬 X)

# Dispatch
Agent({description: "task A", isolation: "worktree", run_in_background: true,
       prompt: "<자기완결>"})
Agent({description: "task B", isolation: "worktree", run_in_background: true,
       prompt: "<자기완결>"})

# 메인은 다른 일 진행 또는 사용자 응대
# 완료 알림 자동 도착 — sleep/poll 금지
```

---

## 결과 수령 후 처리

worktree agent의 결과는 후속 단계로 위임한다:

- **변경 없음** → 자동 정리됨. 추가 조치 불필요.
- **변경 있음 (성공/실패 무관)** → `merge-coordinator.md`로 이동. 머지 순서 결정, 충돌 처리, 정리 책임이 그쪽에 있다.
- **재위임 판단 기준** → `agent-monitor.md` (외부 환경 vs prompt 결함 vs 불명확).

이 파일은 격리 패턴까지만 다루고, 결과 통합 로직은 의도적으로 가지고 있지 않다.

---

## 충돌 위험 사전 분석

병렬 가능성을 판단하기 위해 메인이 수행할 조사:

```
1. Glob으로 작업 영역 추정
   - "auth 관련 작업" → Glob("src/auth/**", "tests/auth/**")

2. Grep으로 cross-reference 식별
   - 작업 A가 건드릴 함수가 작업 B 영역에서 import되는가?

3. git log --stat <base>..HEAD 로 최근 변경 영역 확인
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

---

## 체크리스트

병렬 dispatch 전:

- [ ] 작업들의 변경 파일 집합을 추정했는가?
- [ ] disjoint가 명확한가? (의심스러우면 순차)
- [ ] 각 sub-agent prompt가 자기완결적인가?
- [ ] `isolation: "worktree"`와 `run_in_background: true`를 켰는가?

dispatch 후:

- [ ] 완료 알림을 기다리는 중에 sleep/poll을 하고 있지 않은가?
- [ ] 각 결과의 worktree 상태(변경 유무)를 파악했는가?
- [ ] 변경 있는 결과를 `merge-coordinator.md` 단계로 넘겼는가? (이 파일의 책임은 여기서 끝)
