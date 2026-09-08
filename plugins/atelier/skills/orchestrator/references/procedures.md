# Orchestrator 절차

결정적 순서라 모델 추론에 맡길 수 없는 것만 둔다. 각 절차 안의 분기는 계약과 기본값 표가 정한다.

## 진입 절차

1. 조율 도구 스키마를 확보한다 — 다른 모든 단계보다 먼저 온다 (`references/contracts.md §도구 확보`).
2. 경로를 판정한다 — 무거운 경로면 이후 전부, 경량 경로면 브랜치·토폴로지·격리 단계를 건너뛴다 (`references/contracts.md §기본값 표`).
3. 모드를 판정한다 — 토폴로지 확인이 모드를 입력으로 쓰므로 그 앞에 온다 (`references/contracts.md §기본값 표`).
4. **epic 브랜치를 확보한다** — 현재 브랜치가 epic 이면 그것을 이 런의 epic 으로 쓴다. 아니면 하네스·PR 이 지정한 작업 브랜치를 접두 형태와 무관하게 그대로 `<epic>` 으로 삼는다. 지정 브랜치도 없으면 자율은 새 epic 을 만들고 HITL 은 어느 epic 으로 진입할지 사용자 결정을 받는다. 확보하지 않은 채 다음 단계로 가는 것은 금지한다 — 토폴로지·머지·기록 경로가 모두 이 이름을 전제한다. 확보한 이름과 확보 방식을 진입 보고와 기록에 남긴다 (`references/contracts.md §브랜치 규약`).
5. 메인이 worktree 가 아니라 메인 working tree 의 epic 브랜치 위에서 clean 한지 확인한다 — 진입 이후에도 런 내내 같은 가드가 돈다.
6. 왕복 조율 가용을 판정하고 공유 전제 preflight 를 돌린다 — 실패하면 dispatch 를 시작하지 않는다 (`references/contracts.md §왕복 조율 가용 판정과 spawn 확인` · `SKILL.md §정지 조건`).
7. 각 단계의 판정 결과와 사용한 신호를 진입 보고 한 줄과 기록에 남긴다 — 근거 없는 생략은 판정이 아니다 (`references/contracts.md §사용자 보고 형식` · `references/contracts.md §기록 위치와 형식`).

## 자율 실행 루프

1. spec 확정 게이트 — spec 입력 런이면 미결 스캔부터. 미결이 있으면 계약도 세우지 않는다 (`SKILL.md §정지 조건`).
2. 예산·가드레일 값을 정해 자율 계약을 세우고 시작 보고를 한 번 낸다 (`references/contracts.md §예산과 가드레일 값` · `references/contracts.md §사용자 보고 형식`).
3. 분해 — 복잡·모호하면 아키텍트 협의체로, 미결 0 확정 spec 이면 메인이 직접. 도출 결과와 근거를 기록한다 (`references/contracts.md §task 도출 계약`).
4. 실행 계획 — 병렬과 순차, 위임 형태, 격리 여부, 세울 게이트 관점을 표에서 읽는다 (`references/contracts.md §기본값 표`).
5. 위임과 진행 추적 — 완료 알림 수령으로 진행한다. 무거운 경로면 dispatch 직후 생성 가드, 완료 알림 직후 토폴로지 가드.
6. 결과 처리와 게이트 — 실패면 재위임 판정, 아니면 세운 관점이 전부 pass 여야 승급한다. 게이트가 spec 미결을 드러내면 재위임이 아니라 hard stop (`SKILL.md §정지 조건`).
7. 머지 조정(무거운 경로만) 후 계약에 통합 검증이 있으면 지정된 시점에 실행한다.
8. 남은 작업을 재계산해 종료 조건을 재평가한다 — 미충족이고 예산이 남으면 3 으로 되돌아가고, 충족되거나 예산이 소진되면 3분류 판정과 핸드오프를 남기고 종료한다 (`references/contracts.md §종료 핸드오프`).

어느 단계에서든 정지 신호에 닿으면 그 즉시 종료 단계로 간다. 완료를 기다릴 때 `sleep` 이나 polling 은 쓰지 않는다.

## 직렬 dispatch 와 토폴로지

worktree 는 항상 epic 브랜치 위의 격리 수단이고, 메인은 epic 브랜치의 메인 working tree 에 머문다.

```
main
  └─ <epic>                                ← 메인 (읽기 · dispatch · 보고)
       ├─ worktree A → <epic>-t<id>-<slug>    (base = <epic>)
       ├─ worktree B → <epic>-t<id>-<slug>    (base = <epic>)
       └─ worktree C → <epic>-t<id>-<slug>    (base = <epic>)
```

1. 진입 확인 — `git branch --show-current` 가 epic 브랜치이고 `git rev-parse --show-toplevel` 이 메인 working tree 인가.
2. `git worktree list --porcelain` 으로 스냅샷을 뜬다.
3. 격리 위임을 **한 건만** 이번 메시지에 싣는다 — 작업 브랜치는 `<epic>-t<id>-<slug>`, prompt 에 실을 것은 `references/contracts.md §prompt 필수 포함 요소`.
4. 생성 가드로 worktree 등장을 확인한다 — 통과 전에는 다음 건을 싣지 않는다. 남은 건이 있으면 2 로 돌아간다.
5. 메인은 epic 브랜치에서 다음 일을 진행하고, 완료 알림 수신 직후 토폴로지 가드를 돌린다.

## 토폴로지 복구

메인 로컬을 원격에 맞추는 절차이지 epic 히스토리를 재작성하는 절차가 아니다.

1. 메인 working tree 로 복귀한다 — 복구 명령을 worktree 안에서 실행하면 엉뚱한 체크아웃을 고치게 된다.
2. 의도치 않은 변경이 있으면 `git stash push -u` 로 보존한다 — worktree 로 갔어야 할 산출물일 수 있어 버리지 않는다.
3. 진행 중인 rebase 가 있으면 `git rebase --abort` 로 중단한 뒤 복구를 시작한다.
4. `git checkout <epic>` — 이름은 진입 절차가 확보한 것을 그대로 쓴다.
5. `git pull --ff-only origin <epic>`.
6. 5 가 non-ff 로 거부되면 hard stop 후 보고한다 — 갈라짐 자체가 토폴로지 위반의 증거이므로 epic 히스토리를 다시 쓰지 않고 갈라진 상태 그대로 보고한다 (`SKILL.md §정지 조건`).
7. 잘못 switch 된 브랜치가 로컬에 남았으면 `git branch -D <브랜치>` 로 지운다.
8. 어떤 명령 직후 발생했는지와 working tree 가 clean 했는지를 붙여 보고한다 (`references/contracts.md §사용자 보고 형식`).

## worktree 정리

| 상태 | 조치 |
|---|---|
| 변경 없음 | 추가 조치 없음 — 회수 시점에 정리되므로 삭제 명령을 따로 돌리지 않는다 |
| 변경 있고 머지·폐기 미결정 | 머지 후보로 인계한다 — 아직 도착하지 않은 위임이 남아 있으면 여기서 바로 머지하지 않는다 |
| 결정 끝 · 자율 | worktree 디렉토리와 머지된 브랜치를 정리한다 |
| 결정 끝 · HITL | 정리 여부를 보고하고 결정을 받는다 — 보류면 그대로 두고 임의 삭제는 금지한다 |

## 머지 표준 절차

1. 머지 시점을 확인한다 — 지금 통합해도 되는 시점인가 (`references/contracts.md §머지 대상: epic 브랜치`).
2. 후보를 수집하고 제외한다 — `git branch --list '<epic>-t*'` 로 작업 브랜치를 모으고, 변경 파일 overlap 을 `git diff --name-only <epic>...<branch>` 로 대조해 순서 판정의 입력으로 넘긴다.
3. 머지 순서를 정한다 (`references/contracts.md §기본값 표`).
4. 순서대로 한 후보씩 통합한다 — 후보 브랜치를 `<epic>` 위로 rebase 한 뒤 `git merge --ff-only` 로 넘긴다. 충돌은 충돌 카운터를 따라 재분류하거나 에스컬레이션한다 (`references/contracts.md §예산과 가드레일 값` · `SKILL.md §정지 조건`).
5. 매 머지 직후 가드를 돌린다 — 생략하지 않는다.
6. 정리 대상 worktree 를 `§worktree 정리` 로 인계하고, 성공·실패·보류 후보와 남은 worktree 를 담아 머지 결과를 보고한다 (`references/contracts.md §사용자 보고 형식`).

## 최종 통합 검증 게이트

1. epic 최종 HEAD 에서 `git status` clean 확인 — 미커밋 변경이나 untracked 잔여물이 있으면 먼저 정리하고 재확인한다.
2. 원격 기본 브랜치와의 격차를 확인해 흡수한다 — `git fetch origin <default-branch>` → `git rev-list --count <epic>..origin/<default-branch>`.
3. 전체 테스트 스위트를 한 번 실행한다 — 변경 파일 한정이나 부분 실행은 하지 않는다. 인프라 의존 테스트는 이 게이트와 별개의 검증 대상이다 (`SKILL.md §정지 조건`).
4. `git rev-parse HEAD` 로 흡수 후 HEAD sha 를 기록한 뒤 green·red 판정과 완료 선언 여부를 보고한다 (`references/contracts.md §사용자 보고 형식`).

## 아키텍트 협의체

1. 생성 자세를 주입해 설계를 만든다 — 자세의 출처는 심문 스킬 `grill` 이고, 주입할 때 치환 규칙을 함께 싣는다 (`references/contracts.md §대화 스킬의 자율 어댑테이션`).
2. 검증 자세는 생성과 다른 agent 가 맡는다 — 자기 설계를 자기가 심문하지 않는다. 두 자세 모두 read-only 분석이라 격리가 필요 없다.
3. 라운드를 돈다 — 검증이 pass 가 아니면 findings 를 생성 쪽에 돌려 보강받고, 라운드 상한에 닿으면 멈추고 보고한다 (`references/contracts.md §예산과 가드레일 값`).
4. pass 를 받으면 승인 이벤트를 기록하고 task 목록을 넘긴다 — 검증 pass 전 dispatch 는 하지 않는다 (`references/contracts.md §설계 승인 마커` · `references/contracts.md §task 도출 계약`).

## 자문 소집

1. 왕복 조율 가용 판정을 확인한다 — 진입 시 확정한 것을 그대로 쓰고 트리거 시점에 다시 판정하지 않는다 (`references/contracts.md §왕복 조율 가용 판정과 spawn 확인`).
2. 관점을 정하고 관점마다 자문자를 하나씩 배경으로 소집한다 — 이름은 세션 안에서 유니크하게, 역량은 메인보다 상위로 지정한다. 패킷은 `references/contracts.md §자문 계약`, 사용자 제약이 있으면 `references/contracts.md §역할별 모델 제약` 이 앞선다.
3. 첫 자문자가 뜬 직후 spawn 확인을 한다 — 왕복이 성립하지 않으면 한 번 재판정하고, 그래도 아니면 폴백 없이 자문을 생략한 뒤 원래 에스컬레이션으로 간다.
4. 권고를 모아 채택 판단을 내리고 기록한다 — 자문자는 게이트가 아니라 입력이다. 무거운 경로는 소집 전후로 토폴로지 가드를 돌린다 (`references/contracts.md §기록 위치와 형식`).
