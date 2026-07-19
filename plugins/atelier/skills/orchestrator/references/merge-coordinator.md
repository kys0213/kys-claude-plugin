---
name: merge-coordinator
description: 병렬 위임 결과를 통합하는 머지 순서 결정과 충돌 처리 패턴, worktree 정리 책임. orchestrator 스킬 내부 참조 문서.
version: 0.1.0
user-invocable: false
---

# Merge Coordinator

병렬 sub-agent들이 worktree에 결과를 남긴 뒤, 그 결과를 epic 브랜치로 안정적으로 통합하는 단계. **메인은 직접 머지/충돌 해결을 하지 않고**, 순서를 결정하고 충돌은 `git` skill 의 `references/conflict-resolution.md` 전략에 위임한다.

> **모드별 차이**: 오케스트레이터 **기본 동작은 자율 주행**이라 충돌 없는 머지는 보고 없이 자동 진행하고, 충돌은 `git-resolve` sub-agent 에 자동 위임한다(`autonomous-driving.md §머지/충돌`). 사용자가 **HITL 로 opt-out** 한 경우에만 머지 전 보고 후 결정을 받는다(옵션 B). 단 도메인 의미 충돌·토폴로지 위반은 두 모드 모두 에스컬레이션이다.

## 머지 대상: epic 브랜치

이 단계의 머지 target은 **현재 epic 브랜치**다. main 브랜치가 아니다. 각 worktree(sub-agent 브랜치)는 epic 브랜치로 합류하고, epic 브랜치에서 main으로의 머지는 사용자 결정 / 별도 release 절차로 이 스킬 범위 밖이다.

따라서 아래 절차에서 `base`로 표기된 곳은 모두 **현재 epic 브랜치**를 의미한다.

## 머지 순서 결정 (기본 규칙)

기본 순서 (위에서 아래로 우선):

```
1. 의존성이 없는 작업 먼저
   - 다른 작업이 결과를 기다리지 않는 것부터 머지
   - 의존성 체인의 잎(leaf)부터 처리

2. 변경 파일 수가 적은 것부터
   - 충돌 영향 범위 최소화
   - 큰 변경이 나중에 들어오면 작은 변경의 충돌 위험을 흡수

3. 알파벳 순 (재현성)
   - 위 두 기준이 동률이면 brnach/path 알파벳 순
   - 같은 입력에 같은 결과 → 디버깅 쉬움
```

이 순서는 **충돌 시 사람이 처리할 양을 최소화**하는 휴리스틱이다.

---

## 표준 절차

```
1. 머지 후보 수집
   - 각 sub-agent 결과에서 worktree 경로 + 브랜치명 추출
   - 변경 없음 → 후보에서 제외 (자동 정리됨)

2. 의존성 그래프 구성
   - 메인이 dispatch 단계에서 알고 있는 의존성을 활용
   - 추가로 변경 파일 집합의 overlap을 검사

3. 머지 순서 결정 (위 규칙 적용)

4. 순차 머지 시도
   - epic 브랜치(base)에 후보 브랜치를 머지/리베이스 — 메인은 epic 브랜치 working tree에 그대로 머무름
   - PR 머지는 `gh pr merge <N> --squash --delete-branch` 사용 (--delete-branch로 서버측 + 로컬 브랜치 모두 정리)
   - 충돌 없음 → 다음 후보로
   - 충돌 발생 → 위임 (아래 참조)

5. 토폴로지 가드 (매 머지 직후, 생략 금지)
   - assert `git branch --show-current` == epic 브랜치
   - 불일치 시 즉시 복구 + 에스컬레이션 (아래 "머지 후 토폴로지 가드" 참조)

6. 머지 완료 후 worktree 정리
   - 머지된 worktree 삭제
   - 폐기된 worktree도 사용자 확인 후 삭제

7. 사용자에게 결과 요약 보고
```

---

## 머지 후 토폴로지 가드

실사례에서 `gh pr merge --squash` 후 메인 working tree의 current branch가 sub-agent 브랜치로 의도치 않게 switch된 사고가 발생했다 (#783). 메인은 항상 epic 브랜치에 있어야 하므로, **매 머지 직후** 다음 1줄 가드를 실행한다:

```bash
[ "$(git branch --show-current)" = "epic/<name>" ] || echo "TOPOLOGY VIOLATION"
```

불일치 발견 시 복구 절차:

```bash
git rebase --abort 2>/dev/null   # rebase 진행 중이면 중단
git checkout epic/<name>
git pull --rebase origin epic/<name>
git branch -D <잘못 switch된 sub-agent 브랜치>   # 로컬에 남았으면 정리
```

복구 후 **반드시 에스컬레이션** — 어떤 명령 직후 발생했는지, working tree가 clean했는지를 사용자에게 보고한다. 자율 모드라도 이 가드 실패는 hard stop이다 (`autonomous-driving.md`).

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
- 성공: branch_A → main, branch_B → main
- 실패: branch_C (충돌 — <파일 목록>, 성격: 단순 라인 겹침 / 의미 차이 / 구조 변경)
- 보류: branch_D (사용자 결정 대기)

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
7. **머지 후 가드 생략**: `gh pr merge` 후 current branch 확인 없이 다음 git 명령 진행 → 메인이 sub-agent 브랜치 위에서 작업하는 토폴로지 위반을 뒤늦게 발견 (#783). 매 머지 직후 가드 필수.

---

## 체크리스트

머지 단계 진입 전:

- [ ] 메인이 여전히 epic 브랜치 + 메인 working tree에 있는가?
- [ ] 후보 브랜치 목록을 수집했는가?
- [ ] 의존성 + 변경 파일 overlap을 파악했는가?
- [ ] 머지 순서를 결정했는가? (의존성 없는 것 → 적은 변경 → 알파벳)
- [ ] base(=epic 브랜치)를 최신화했는가?

머지 진행 중:

- [ ] 충돌 발생 시 직접 편집하지 않고 위임/보고했는가?
- [ ] PR 머지에 `--squash --delete-branch` 옵션을 사용했는가?
- [ ] 매 머지 직후 `git branch --show-current` == epic 브랜치를 확인했는가?

머지 종료 후:

- [ ] worktree를 정리했는가?
- [ ] 사용자에게 결과 요약을 보고했는가?
