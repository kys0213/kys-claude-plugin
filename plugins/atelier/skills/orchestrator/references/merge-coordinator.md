---
name: merge-coordinator
description: 병렬 위임 결과를 통합하는 머지 순서 결정과 충돌 처리 패턴, worktree 정리 책임. orchestrator 스킬 내부 참조 문서.
version: 0.1.0
user-invocable: false
---

# Merge Coordinator

병렬 sub-agent들이 worktree에 결과를 남긴 뒤, 그 결과를 epic 브랜치로 안정적으로 통합하는 단계. **메인은 직접 머지/충돌 해결을 하지 않고**, 순서를 결정하고 충돌은 `git` skill 의 `references/conflict-resolution.md` 전략에 위임한다.

> **적용 범위**: 이 문서 전체가 **무거운 경로** 전용이다 (판정: `SKILL.md` §경로 판정 게이트).
>
> **모드별 차이**: 오케스트레이터 **기본 동작은 자율 주행**이라 충돌 없는 머지는 보고 없이 자동 진행하고, 충돌은 전담 sub-agent 에 자동 위임한다(`autonomous-driving.md §머지/충돌`). 사용자가 **HITL 로 opt-out** 한 경우에만 머지 전 보고 후 결정을 받는다(옵션 B). 단 도메인 의미 충돌·토폴로지 위반은 두 모드 모두 에스컬레이션이다.

## 머지 대상: epic 브랜치

이 단계의 머지 target은 **현재 epic 브랜치**다. main 브랜치가 아니다. 각 worktree(sub-agent 브랜치)는 epic 브랜치로 합류하고, epic 브랜치에서 main으로의 머지는 사용자 결정 / 별도 release 절차로 이 스킬 범위 밖이다.

따라서 아래 절차에서 `base`로 표기된 곳은 모두 **현재 epic 브랜치**를 의미한다.

**언제·어떻게 머지하는가는 이 문서가 정하지 않는다** — 머지 시점 정책(배치 vs 즉시+rebase 전파), 통합 방식(rebase 후 `--ff-only`), 브랜치 네이밍은 `branch-strategy.md`가 단일 출처다. 이 문서는 그 정책 아래에서 **후보 간 순서를 정하고 충돌·정리·검증을 처리**한다.

worktree 브랜치는 PR을 생성하지 않고 epic 브랜치로 수렴 후 삭제된다 — 외부로 나가는 PR은 사용자 요청 1건당 1개가 기본값이며, 그 규칙은 `git` skill `SKILL.md §PR 단위 원칙`이 단일 출처다.

## 머지 순서 결정 (기본 규칙)

기본 순서 (위에서 아래로 우선):

```
머지 순서 결정 — 후보 간 우선순위 비교
  │
  ├─ [1차] 의존성 없는 작업 먼저
  │     다른 작업이 결과를 기다리지 않는 것부터 머지
  │     의존성 체인의 잎(leaf)부터 처리
  │
  ├─ [2차 · 1차 동률 시] 변경 파일 수가 적은 것부터
  │
  └─ [3차 · 1·2차 모두 동률 시] branch/path 알파벳 순
```

판단 근거:
- 변경 파일 수 기준: 충돌 영향 범위 최소화 — 큰 변경이 나중에 들어오면 작은 변경의 충돌 위험을 흡수
- 알파벳 순 기준: 같은 입력에 같은 결과 → 디버깅 쉬움
- 전체: 이 순서는 **충돌 시 사람이 처리할 양을 최소화**하는 휴리스틱이다.

---

## 표준 절차

```
0. 머지 시점 확인 — 지금 머지해도 되는 시점인가?
   - 기본값은 배치 머지라 in-flight sub-agent가 남아 있으면 아직 머지하지 않는다
   - 정책 선택지와 근거는 `branch-strategy.md §base drift 전파`가 단일 출처

1. 머지 후보 수집
   - `git branch --list 'epic/<name>/t*'` 로 수집 — 네이밍 규약이 있으므로 결과 텍스트에
     의존하지 않는다 (`branch-strategy.md §브랜치 네이밍`). 결과를 유실한 agent의 작업물도 여기 잡힌다
   - 각 sub-agent 결과에서 worktree 경로 + 브랜치명을 대조 (목록과 어긋나면 고아 브랜치)
   - 변경 없음 → 후보에서 제외 (자동 정리됨)
   - 각 후보의 merge-base 확인 (`git merge-base epic/<name> <branch>`) — worktree base가 dispatch
     시점 epic 브랜치 HEAD였다는 보장은 없으므로 (`delegation-patterns.md §Prompt 작성 원칙 필수
     포함 요소` 9번), 뒤처진 base 위에서 작업됐다면 통합 전 최신 HEAD 기준으로도 유효한지 확인한다

2. 의존성 그래프 구성
   - 메인이 dispatch 단계에서 알고 있는 의존성을 활용
   - 추가로 변경 파일 집합의 overlap을 검사

3. 머지 순서 결정 (위 규칙 적용)

4. 순차 머지 시도
   - **rebase 후 `--ff-only`** 로 통합 — 방식은 `branch-strategy.md §머지 방식`이 단일 출처다
     (`git merge` 로 머지 커밋을 만들면 rebase 누락이 조용히 통과한다)
   - 메인은 epic 브랜치 working tree에 그대로 머무름. rebase는 worktree 쪽에서 수행·위임한다
   - 로컬 통합으로 수행 후 머지된 브랜치 삭제 — worktree 브랜치는 PR 을 생성하지 않으므로 `gh pr merge` 를 사용하지 않는다 (§머지 대상: epic 브랜치)
   - 충돌 없음 → 다음 후보로
   - 충돌 발생 → 위임 (아래 참조). 같은 파일이 반복 충돌하면 재위임으로 풀지 않는다 —
     대응 사다리는 `branch-strategy.md §충돌 반복`이 단일 출처다

5. 머지 직후 가드 (매 머지 직후, 생략 금지)
   불변식마다 (확인 → 위반 시 처리). **새 불변식이 생기면 이 목록에 한 줄 추가한다** — 단계를
   신설하지 않는다 (단계를 늘리면 번호가 밀려 교차 참조까지 함께 고쳐야 한다).
   - branch == epic 브랜치 + working tree clean → 복구 + 에스컬레이션 (아래 §토폴로지 가드)
   - committer == 오케스트레이터 자신           → 정정 (아래 §Authorship 확인)
   - in-flight worktree가 epic 최신 HEAD 기준  → rebase 전파 (`branch-strategy.md §base drift 전파`.
     배치 머지(기본)면 in-flight가 없으므로 자동 충족)

6. 머지 완료 후 worktree 정리
   - 머지된 worktree 삭제
   - 폐기된 worktree도 사용자 확인 후 삭제

7. 최종 통합 검증 게이트 (아래 "최종 통합 검증 게이트 (final HEAD full-suite)" 참조)
   - epic 브랜치 최종 HEAD clean 확인 후 전체 테스트 스위트 1회 실행
   - green이어야 완료 선언 가능 (HEAD sha 기록)

8. 사용자에게 결과 요약 보고
```

---

## 토폴로지 가드 (명령·복구 절차 단일 출처)

메인은 항상 epic 브랜치의 메인 working tree에 있어야 한다. 머지 명령이나 sub-agent의 격리 이탈로 메인의 current branch가 sub-agent 브랜치로 switch되면, 오염된 HEAD 위에서 후속 dispatch의 worktree base가 잘못 잡히고 머지 경로가 어긋난다. 가드 명령과 복구 절차는 이 절이 단일 출처이며, **언제 실행하는가**는 각 단계 문서가 정한다 — 매 sub-agent 완료 알림 직후(`worktree-lifecycle.md`), 매 머지 직후(위 §표준 절차 5 머지 직후 가드), 자율 루프의 `assert_topology()`(`autonomous-driving.md`). 매 worktree dispatch 직후에는 **생성 검증이 추가된** 별도 가드가 돈다 — 검사 항목과 절차는 `worktree-lifecycle.md §dispatch 생성 가드`가 단일 출처이고, 거기서 오염이 발견되면 복구는 이 절을 따른다.

```bash
git branch --show-current    # epic/<name> 이어야 함
git status --short           # clean 이어야 함 (메인은 편집하지 않으므로)
```

불일치 발견 시 복구 절차:

```bash
git rebase --abort 2>/dev/null   # rebase 진행 중이면 중단
git checkout epic/<name>
git pull --rebase origin epic/<name>
git branch -D <잘못 switch된 sub-agent 브랜치>   # 로컬에 남았으면 정리
```

의도치 않은 변경이 있으면 `git stash push -u` 로 보존한 뒤 보고한다 — 그 변경이 worktree로 갔어야 할 sub-agent 작업물일 수 있으므로 버리지 않는다.

복구 후 **반드시 에스컬레이션** — 어떤 명령 직후 발생했는지, working tree가 clean했는지를 사용자에게 보고한다. 자율 모드라도 이 가드 실패는 hard stop이다 (`autonomous-driving.md`).

---

## Authorship 확인 (통합 후 committer 정정)

sub-agent worktree에서 만든 커밋을 epic 브랜치로 가져오면(머지·cherry-pick·rebase 어떤 경로든), 그 커밋의 committer가 위임한 sub-agent 자신의 git 아이덴티티로 남을 수 있다. 통합을 수행한 것은 오케스트레이터(메인) 자신인데 committer가 위임자로 남으면 통합 이력이 실제 수행자와 어긋나고, 원격에서 서명·계정 매칭이 안 되면 Unverified로 표시된다.

- **확인**: 매 머지/cherry-pick/rebase 직후, 새로 들어온 커밋의 committer가 위임한 sub-agent가 아니라 오케스트레이터 자신인지 확인한다 (`git log --format='%H %cn %ce' -n <N>` 등). 구체 아이덴티티 값(이메일 등)은 이 문서가 규정하지 않는다 — 프로젝트/환경의 git 설정을 따른다.
- **정정**: committer가 위임한 sub-agent로 남아 있으면 정정한다. `--reset-author`는 committer뿐 아니라 **author까지** 현재 사용자로 덮어써 sub-agent의 저작자 표시를 지운다는 점에 주의 — author 보존 여부에 따라 명령이 갈린다.
  - committer만 갱신하고 author(sub-agent 저작자 표시)는 유지: `git commit --amend --no-edit` (단일 커밋) / `git rebase --exec 'git commit --amend --no-edit' <base>` (여러 커밋)
  - author까지 오케스트레이터로 정정: `git commit --amend --no-edit --reset-author` (단일 커밋) / `git rebase --exec 'git commit --amend --no-edit --reset-author' <base>` (여러 커밋)
  - 어느 쪽을 쓸지는 프로젝트 판단이다 — 이 문서는 하나로 확정하지 않는다.
- 실행 시점은 §표준 절차 5(머지 직후 가드)의 한 불변식으로 관리된다 — 토폴로지 가드와 같은 시점이다.

---

## 최종 통합 검증 게이트 (final HEAD full-suite)

모든 머지 후보가 epic 브랜치로 통합된 뒤에도, 개별 worktree가 각자 green이었다는 사실이 머지 결합 후 회귀가 없음을 보장하지는 않는다. 통합이 끝나면 **epic 브랜치 최종 HEAD**에서 전체 테스트 스위트를 1회 더 실행한다.

```
1. epic 브랜치 최종 HEAD에서 `git status` clean 확인
   - 미커밋 변경/untracked 잔여물이 있으면 먼저 정리 후 재확인
2. main 역방향 drift 흡수 (branch-strategy.md §epic ← main 역방향 drift)
   - git fetch origin <default-branch>
   - git rev-list --count epic/<name>..origin/<default-branch> 가 0이 아니면 main을 epic에 머지
     (rebase 아님 — epic은 worktree들의 공유 base다)
3. 전체 테스트 스위트 1회 실행 (변경 파일 한정/부분 실행 금지)
4. HEAD sha 기록 (`git rev-parse HEAD`) — 흡수 후의 HEAD여야 한다
```

**완료 선언 조건**: '작업 완료' 보고는 이 최종 HEAD green을 전제로 한다. 중간 브랜치나 개별 worktree의 green 결과만으로는 완료를 선언하지 않는다 — 각 worktree가 개별적으로 green이어도 머지 결합 지점(인터페이스, 전역 상태, 실행 순서 등)에서 회귀가 발생할 수 있기 때문이다.

보고에는 스위트가 실행된 **HEAD sha**를 반드시 명시한다 (아래 "보고 형식" 참조).

**실패 시**: 완료 선언 금지. 회귀 원인 파악 후 수정을 위임한다 — 메인 직접 편집 금지 원칙은 이 단계에도 동일하게 적용된다 ("충돌 시 위임" 절차 참조).

인프라 의존 테스트(DB, 외부 서비스 등)는 자율 모드 통합 단계에서 메인 working tree 기준 별도 검증 대상이다 (`autonomous-driving.md §통합 검증`). 본 게이트는 인프라 의존 여부와 무관하게, epic 브랜치 최종 HEAD 전체 스위트 실행을 완료 선언의 전제 조건으로 규정하는 상위 규칙이다.

---

## 충돌 시 위임

메인은 충돌 해결을 직접 하지 않는다.

### 옵션 A: 충돌 해결을 sub-agent 에 위임

파일별 충돌 해결 전략(Ours/Theirs/Manual, rebase marker 의미)은 `git` skill 의
`references/conflict-resolution.md` 가 단일 출처다 (git skill 이 로드).
메인은 이 전략을 트리거할 sub-agent 를 호출한다. prompt에는 최소 `base: epic/<name>`, `target: <feature-branch>`, "git skill의 `references/conflict-resolution.md` 전략으로 해결 후 epic 브랜치 위로 rebase, 완료 후 변경 파일·커밋 해시 보고"를 포함한다.

### 옵션 B: 사용자에게 보고

- 충돌이 의미상 판단을 요구하는 경우 (도메인 로직, 의도 충돌)
- 또는 사용자가 직접 처리하길 선호하는 경우

보고 형식은 아래 §보고 형식과 동일하다(단일 출처).

---

## 머지 실패 처리

위임된 충돌 해결이 실패한 경우 부분 머지 상태(이미 머지된 후보/미머지 후보)를 확인하고, 아래 §보고 형식으로 사용자에게 보고한 뒤 결정에 따라 진행한다.

**금지**: 메인이 자체 판단으로 충돌 부분을 직접 편집해 머지를 강제 진행 — 오케스트레이터 원칙 위반.

---

## worktree 정리

머지 완료 후:

| 상태 | 정리 방법 |
|------|----------|
| 머지 성공 | worktree 디렉토리 삭제 + 머지된 브랜치 삭제 (선택) |
| 머지 실패, 사용자가 보류 결정 | 그대로 둠 — 사용자가 나중에 처리 |
| 머지 실패, 사용자가 폐기 결정 | worktree 삭제 + 브랜치 삭제 |

정리는 `atelier git` 또는 `git worktree remove` Bash 호출로 수행. 메인이 직접 해도 되고, sub-agent에 위임해도 된다 (변경이 없으니 위험도 낮음).

---

## 변경 파일 overlap 검사

병렬 위임 시 사전에 disjoint를 검증했더라도, 결과 단계에서 다시 한번 확인하면 안전하다.

```
git diff --name-only epic/<name>...<branch_A>  # A가 변경한 파일 (base = epic 브랜치)
git diff --name-only epic/<name>...<branch_B>  # B가 변경한 파일
→ 교집합이 있으면 충돌 가능성 ↑ → 머지 순서를 신중히
```

이 검사는 메인이 epic 브랜치에서 직접 수행 (Bash) — 짧고 결정적.

---

## 보고 형식

머지 단계 종료 시 사용자에게:

```
머지 결과:
- 성공: branch_A → epic/<name>, branch_B → epic/<name>
- 실패: branch_C (충돌 — <파일 목록>, 성격: 단순 라인 겹침 / 의미 차이 / 구조 변경)
- 보류: branch_D (사용자 결정 대기)

최종 통합 검증 게이트:
- HEAD sha: <sha>
- 전체 스위트 결과: green | red (red면 완료 선언 보류, 원인/재위임 계획 명시)

남은 worktree:
- <경로> (branch_C, 충돌 해결 미완)

다음 액션 제안:
- branch_C 재시도 prompt 작성 (다른 조건으로)
- 해당 브랜치 보류 또는 폐기
- 또는 사용자 직접 처리
```

---

## 안티패턴

1. **메인이 직접 충돌 편집**: 머지 충돌이 발생했을 때 메인이 Edit로 해결 → 오케스트레이터 원칙 위반.
2. **순서 무시한 동시 머지**: 모든 후보를 한꺼번에 머지 시도 → 충돌 시 사람 부담 폭발.
3. **머지 실패 무시**: 한 후보 충돌 → 그냥 건너뛰고 다음 진행 → 누락 발생. 보고 + 결정.
4. **worktree 방치**: 머지 완료 후 정리 안 함 → 디스크/git 상태 오염.
5. **base 미동기화 머지**: 오래된 base 위에 머지 시도 → 무의미한 충돌. 머지 직전 base pull 필수.
6. **main으로 바로 머지**: epic 브랜치를 거치지 않고 sub-agent 결과를 main으로 직접 머지 → epic 브랜치 전략 위반. 이 단계의 target은 항상 epic 브랜치.
7. **머지 후 가드 생략**: 머지 직후 current branch 확인 없이 다음 git 명령 진행 → 메인이 sub-agent 브랜치 위에서 작업하는 토폴로지 위반을 뒤늦게 발견. 매 머지 직후 가드 필수.
8. **조기 완료 선언**: 개별 worktree/중간 브랜치 green만으로 완료 보고 → 머지 결합 후 회귀 가능성을 놓침. epic 브랜치 최종 HEAD 전체 스위트 green과 HEAD sha 명시를 완료 선언의 전제로 한다.
9. **in-flight 방치 머지**: 아직 돌고 있는 sub-agent가 있는데 먼저 끝난 결과를 머지하고 알리지 않음 → 남은 worktree가 옛 base 위에서 계속 작업한다. 기본은 배치 머지이고, 즉시 머지했으면 전부에 rebase를 전파한다 (`branch-strategy.md §base drift 전파`).
10. **완료 선언 전 main drift 미확인**: epic이 main보다 뒤처진 채 최종 스위트를 green으로 보고 → main 기준으로는 깨진 상태로 완료를 선언한다. 게이트 직전에 흡수 후 스위트를 다시 돌린다 (`branch-strategy.md §epic ← main 역방향 drift`).
11. **통합 후 authorship 미확인**: 머지/cherry-pick/rebase로 sub-agent 커밋을 가져온 뒤 committer 확인 없이 다음 단계로 진행 → 통합을 수행한 오케스트레이터가 아니라 위임한 sub-agent가 committer로 남을 수 있다. 매 머지 직후 확인 필수 (§Authorship 확인).

---

## 체크리스트

머지 단계 진입 전:

- [ ] 메인이 여전히 epic 브랜치 + 메인 working tree에 있는가?
- [ ] 머지 시점 정책을 확인했는가? (기본=배치 — in-flight가 남아 있으면 아직 머지하지 않는다)
- [ ] 후보 브랜치 목록을 `git branch --list 'epic/<name>/t*'` 로 수집했는가? (결과 텍스트 의존 X)
- [ ] 의존성 + 변경 파일 overlap을 파악했는가?
- [ ] 머지 순서를 결정했는가? (의존성 없는 것 → 적은 변경 → 알파벳)
- [ ] base(=epic 브랜치)를 최신화했는가?
- [ ] 각 후보 브랜치의 merge-base를 확인했는가? (뒤처진 base 위에서 작업됐다면 최신 HEAD 기준 유효성 재확인)

머지 진행 중:

- [ ] 통합을 `rebase → merge --ff-only` 로 했는가? (`--ff-only` 실패 = rebase 누락 신호)
- [ ] 충돌 발생 시 직접 편집하지 않고 위임/보고했는가?
- [ ] 같은 파일 충돌 횟수를 파일 단위로 세고 있는가? (2회 → hot-spot 재분류, 3회 → 에스컬레이션)
- [ ] (즉시 머지 정책이면) 매 머지 직후 in-flight worktree 전부에 rebase를 전파했는가?
- [ ] 매 머지 직후 `git branch --show-current` == epic 브랜치를 확인했는가?
- [ ] 매 머지 직후 통합 커밋의 committer가 오케스트레이터 자신인지 확인하고, 위임한 sub-agent로 남아있으면 정정했는가?

머지 종료 후:

- [ ] worktree를 정리했는가?
- [ ] epic이 `origin/<default-branch>` 보다 뒤처졌는지 확인하고, 뒤처졌으면 main을 흡수했는가?
- [ ] 최종 HEAD(clean 상태 · main 흡수 후)에서 전체 테스트 스위트를 1회 실행하고 green을 확인했는가?
- [ ] 보고에 스위트 실행 HEAD sha를 명시했는가?
- [ ] 사용자에게 결과 요약을 보고했는가?
