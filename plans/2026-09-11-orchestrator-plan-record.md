# orchestrator 런의 plan 기록을 규칙으로 세운다

> **Plan — 이 시점의 결정 기록.** 현재 정책의 신뢰 소스가 아니다.
> 날짜: 2026-09-11 · 브랜치: claude/busy-einstein-5wcdil · 이슈: 없음 · 승인 출처: 사용자 승인

## 요구사항

- orchestrator 모드로 구현을 시작하는 런은 **자율주행이든 아니든** 약속된 경로(`plans/YYYY-MM-DD-<slug>.md`)에 plan 을 남겨야 한다.
- plan 이 task 도출의 근거가 되어야 한다 — 지금은 그 연결이 문서 어디에도 없다.

## 현재 상태 (사이드이펙트 조사)

- **plan 작성 흐름은 `grill` 에만 있었다.** `grill` `SKILL.md §종료와 핸드오프` 가 합의 종료 후 `plans/` 로 저장·커밋하라고 하고, `plans/2026-09-09-plan-record-location.md` 의 결정 2번도 "흐름은 skill 에" 로 `grill` 만 지목했다.
- **orchestrator 에는 그 단계가 없었다.** `SKILL.md §표준 절차`, `references/procedures.md §자율 실행 루프`, `§아키텍트 협의체` 어디에도 plan 파일을 쓰라는 지시가 없다. 협의체 pass → 승인 마커 → task 목록으로 직행한다.
- **승인 마커가 plan 을 조건부로만 요구했다.** `§설계 승인 마커` 의 근거 경로가 "협의체 결과 **또는** 합의 요약이 있는 위치 — grill 합의·사용자 승인이면 커밋된 `plans/…`" 였다. 자율 진입의 기본 경로인 협의체 pass 는 `log_dir`(repo 밖) 경로만으로 마커가 성립하고 dispatch 가 진행된다.
- **규칙이 메인의 plan 작성을 막고 있었다.** `SKILL.md §불변식` 1행의 예외는 "통합 검증·머지·복구·정리 명령과 repo 밖 기록"뿐인데 `plans/*.md` 는 tracked 다. 위임 경로도 정의돼 있지 않아, 규칙을 성실히 따를수록 plan 이 생기지 않았다.
- **핸드오프가 단방향이었다.** 자율주행에서는 메인이 사용자에게 grill 하지 않고 협의체가 대신하므로(`templates/claude-md/CLAUDE.md`) 작성 주체가 사라지는데, orchestrator 쪽에 "plan 이 없으면 멈춰라" 게이트가 없었다.
- 검사 제약: 새 `##` 절은 `scripts/check-decision-graph.sh` 의 구조 heading 화이트리스트에 등재해야 하고, 줄수 상한(SKILL.md · contracts.md · procedures.md · 총합)과 예산 값 유출 검사를 함께 통과해야 한다.

## 검토한 대안

| 대안 | 판단 |
|---|---|
| orchestrator 는 게이트만 두고 작성은 `grill` 에 남긴다 | 버림. 자율주행은 `grill` 을 거치지 않으므로 자율 런이 상시 hard stop 된다 |
| plan 작성을 sub-agent 에 위임한다 (격리 worktree · 문서 게이트 · 머지) | 한때 채택했다가 버림. `spec-write` 선례와 "문서 생성도 위임" 원칙에는 맞지만, Plan Mode 가 이미 **메인**을 plan 의 저자로 두고 있어 위임하면 메인이 이미 가진 결정을 sub-agent 가 다시 쓰게 된다 — 왜곡 축만 늘고 plan 한 장에 전체 파이프라인이 붙는다 |
| 메인이 repo 밖에 초안을 쓰고 `plans/` 로 옮겨 커밋한다 (불변식 1 을 "이동·커밋" 예외로만 연다) | 버림. 저자도 결과물도 같은데 두 스텝으로 나누는 것은 불변식 문구를 피하려는 의식일 뿐이다 |
| **메인이 `plans/` 에 직접 쓰고 커밋한다 + 불변식 1 에 범위를 못박은 예외** | **채택** |

## 결정

1. **작성 주체는 메인**이다. plan 내용은 메인이 이미 보유한 결정이라 반입(불변식 2 의 축)이 없고, 구현 dispatch 이전·epic clean 시점이라 충돌(불변식 4 의 축)도 없다. 두 취지 어느 것도 건드리지 않으므로 예외로 연다.
2. **예외는 네 축으로 못박는다** — 파일 하나(`plans/YYYY-MM-DD-<slug>.md`) · 고정 경로 · 구현 dispatch 이전 · 구현과 분리된 `docs` 커밋. 같은 행에 "그 밖의 문서 산출물(spec·리포트·정리 문서)은 예외가 아니라 위임" 을 박아 `spec-write` 의 위임 규칙까지 삼키지 않게 한다.
3. **순서를 고정한다** — `승인 이벤트 → plan 커밋 → 승인 마커 → task 도출·등록 → 구현 dispatch`. 마커의 근거 경로가 실존하는 plan 을 가리켜야 하므로 plan 이 마커보다 앞선다.
4. **근거 경로를 커밋된 plan 으로 고정**해 협의체 pass 경로도 예외로 두지 않는다. 근거 경로가 실존하는 plan 이 아니면 마커가 성립하지 않고, 불변식 16 이 그대로 구현 dispatch 를 막는다 — 별도 정지 조건을 신설하지 않아도 게이트가 선다.
5. **plan 과 task 는 층이 다르다.** `§task 도출 계약` 의 일곱 필드를 plan 본문에 복제하지 않는다 — 복제하면 task 상태가 바뀔 때마다 plan 을 고쳐야 하는데 plan 은 작성 후 고치지 않는 문서다. 연결은 "plan 의 구현 계획·작업 순서가 도출의 입력" 한 줄로 둔다.
6. **범위는 구현 dispatch 가 있는 런 전부**다. 경량 경로(계획된 tracked 변경이 없는 런)는 구현 dispatch 도 승인 마커도 없고 `§산출 경로 계약` 이 파일 생성을 막으므로 해당하지 않는다.
7. 기록 셋(`log_dir` 결정 로그 / `plans/` plan / `HANDOFF.md`)의 책임 분리를 표로 명시해 한 파일에 섞이지 않게 한다.

## 작업 순서

1. `SKILL.md` — 불변식 1행 예외, `§표준 절차` 에 기록 단계 신설, `§계약과 절차` 표에 plan 기록 노출
2. `references/procedures.md` — `§자율 실행 루프` 에 plan 기록·task 도출 단계 신설(재번호), `§아키텍트 협의체` 4 를 plan 커밋 뒤 마커로 수정
3. `references/contracts.md` — `§plan 기록` 신설, `§설계 승인 마커` 근거 경로 고정, `§task 도출 계약` 입력·복제 금지 명시
4. `scripts/check-decision-graph.sh` — 구조 heading 화이트리스트에 `plan 기록` 등재
5. 이 문서를 `docs` 커밋으로 먼저 남기고, 규칙 변경을 별도 커밋으로 분리
