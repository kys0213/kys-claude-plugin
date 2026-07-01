# 이슈 기반 작업 — 상태 표시 규칙

GitHub 이슈(또는 외부 트래커 이슈)를 가져다 작업할 때, **작업 진행 상태를 이슈에 라벨로 드러내는** 규칙.
이슈를 여러 사람·에이전트가 동시에 볼 때 "지금 누가 이걸 잡고 있나 / 어디까지 왔나"가 이슈만 봐도 보이게 한다.

## 도구 경계 (왜 CLI 가 아닌가)

라벨 추가/제거는 `gh` 가 이미 **결정적**이라(`gh issue edit --add-label` 은 동일 입력 → 동일 결과) `atelier git` CLI 로 감싸지 않는다 — 커밋·브랜치·PR 을 plain `gh` 로 실행하는 것과 같은 이유(`cli-reference.md §도구 경계`).

경계는 이렇게 갈린다:

| 무엇 | 성격 | 누가 |
|---|---|---|
| **어느** 이슈를 작업할지 / **언제** 상태를 올릴지 | 컨텍스트·우선순위 판단 | **스킬(에이전트)** |
| 라벨 add/remove 실행 | 결정적 상태 전이 | **plain `gh`** |

즉 스킬이 "이 이슈를 지금 작업한다"를 판단하고, 그 결정을 plain `gh` 명령으로 반영한다. 라벨 이름·전이 규칙은 아래 고정 컨벤션을 따른다.

---

## 상태 라벨 컨벤션

| 라벨 | 의미 | 언제 |
|---|---|---|
| `status: in progress` | 누군가 작업 착수 | 이슈 작업 **시작 직전** |
| `status: in review` | PR 올라감, 리뷰 대기 | PR 생성 직후 (선택) |
| (라벨 제거 + 이슈 close) | 완료 | PR 머지로 이슈 자동 close |

핵심 규칙은 **하나**다: **특정 이슈를 작업하기 전에 `status: in progress` 라벨을 추가한다.** 나머지(`in review`, 완료 처리)는 흐름에 맞춰 따라온다.

---

## 흐름

### 1. 작업 시작 전 — in progress 표시

```bash
[ -f ~/.git-workflow-env ] && source ~/.git-workflow-env   # GH_HOST 등 (git skill 공통)

gh issue edit <issue-number> --add-label "status: in progress"
```

- **작업 브랜치를 만들기 직전**에 실행한다 — 착수 사실을 이슈에 먼저 못 박아 중복 착수를 막는다.
- 라벨 추가는 멱등하다. 이미 붙어 있으면 그대로 통과한다.
- 라벨이 repo 에 없어 실패하면(`could not add label`) 한 번 생성하고 재시도한다:
  ```bash
  gh label create "status: in progress" --color FBCA04 --description "작업 진행 중" 2>/dev/null || true
  gh issue edit <issue-number> --add-label "status: in progress"
  ```

### 2. 작업 → 브랜치·커밋·PR

브랜치 생성·커밋·PR 은 `cli-reference.md §B` 의 컨벤션을 그대로 따른다.
이슈와 PR 을 잇기 위해 PR 본문에 **`Closes #<issue-number>`** 를 넣어 머지 시 이슈가 자동 close 되게 한다.

### 3. PR 생성 후 — in review 로 전환 (선택)

```bash
gh issue edit <issue-number> --remove-label "status: in progress" --add-label "status: in review"
```

- 리뷰 단계를 이슈에서 구분하고 싶을 때만. 안 쓰면 `in progress` 를 완료까지 유지해도 된다.

### 4. 완료 — 라벨 정리

PR 이 `Closes #<n>` 로 머지되면 이슈는 **자동 close** 된다. 열린 상태 라벨은 정리한다:

```bash
gh issue edit <issue-number> --remove-label "status: in progress" --remove-label "status: in review"
```

- 이슈가 이미 닫혔으면 라벨 제거는 생략해도 무방하다(닫힌 이슈의 in-progress 라벨은 노이즈일 뿐 상태 오도는 아님). 깔끔히 하려면 위 명령으로 제거한다.

---

## 판단 기준 (스킬이 정하는 것)

라벨 명령 자체는 결정적이지만, **호출 여부·시점**은 스킬이 판단한다:

- **어느 이슈**: "뭐부터?" 우선순위 판단은 이 스킬의 이슈 우선순위 추천 흐름(SKILL.md)이 담당한다. 그 판단 결과로 고른 이슈에 라벨을 붙인다.
- **언제 in progress → in review/완료**: 완료 판정(테스트 green·리뷰 통과)은 **판단**이다. CLI/자동화에 맡기지 않고, 스킬이 상태를 보고 전이를 결정한 뒤 `gh` 명령을 실행한다.
- **자율 모드 연동**: orchestrator 자율 루프가 이슈를 구동하면, claim 시점에 `in progress`, 머지 시점에 라벨 정리를 루프 단계에 끼워 넣는다. 내부 작업 상태(Task 시스템)와 별개로, **이슈에도 외부 가시성**을 남기는 것이 이 문서의 목적이다.

---

## 안티패턴

1. **라벨을 CLI 로 감싸기**: `gh issue edit` 은 이미 결정적 → `atelier git` 서브커맨드로 만들지 않는다. plain `gh` 로 충분.
2. **작업 다 하고 나서 라벨 붙이기**: in-progress 는 **착수 전** 신호다. 사후에 붙이면 중복 착수 방지 효과가 사라진다.
3. **완료 판정을 라벨 자동화에 위임**: "PR 머지되면 자동으로 done" 같은 판정 로직을 도구에 넣지 않는다. 전이 결정은 스킬 몫(CLAUDE.md 책임 경계).
4. **상태를 이슈 라벨에만 의존**: 라벨은 외부 가시성용이다. 자율 루프의 결정적 작업 추적은 Task 시스템/decision log 가 담당한다 — 둘을 혼동하지 않는다.
