---
name: orchestrator
description: Use this skill for any multi-unit work delegated to sub-agents, agent-teams, or worktrees — parallel fan-out, sequential pipelines, long-running agent teams, autonomous self-driving runs (decompose→dispatch→merge without human intervention), document deliverables (writing reports, specs, or write-up docs is also delegated Write work), multi-branch research and investigation (codebase surveys, side-effect analysis, comparing options — read-only work counts too), or any moment the main agent is about to use Edit/Write directly (delegate instead). Scope is set by scale, not by kind. Triggers include "자율주행", "자율주행모드", "자율 모드", "자율 주행으로", "알아서 끝까지", "여러 작업 병렬로", "동시에 처리", "에이전트 나눠서", "worktree로 분리", "위임해서", "팀으로 작업", "리포트 작성", "보고서로 정리", "스펙 문서 작성", "분석 결과 문서화", "조사해줘", "리서치", "코드베이스 파악", "영향 범위 분석", "사이드이펙트 조사", "원인 분석", "감사해줘", "다시 검토해줘", "여러 방안 비교", "autonomous mode", "self-driving", "hands-off run", "delegate", "parallel agents", "fan-out", "agent team", "sub-agent", "dispatch multiple", "split into tasks", "run in parallel", "write a report", "draft a spec", "write up findings", "research", "investigate", "survey the codebase", "analyze impact", "root cause", "audit", "re-review", "compare approaches".
version: 0.1.0
---

# Orchestrator Skill
여러 단위로 벌어지는 일을 분해 · 위임 · 통합한다. **적용 범위는 작업의 종류가 아니라 규모로 갈리고, 사용자가 명시한 위임 요구가 규모 판정보다 앞선다** — 구현이냐 문서냐 조사냐로 가르면 read-only 리서치가 부당하게 빠진다.
한 턴에 닫히는 단일 파일 단순 편집은 열지 않는다 — 다만 열지 않기로 한 근거를 한 줄 남긴다(근거 없는 생략은 판정이 아니다).
판단은 모델이 한다. 이 문서가 소유하는 것은 넷뿐이다 — 불변식 · 정지 조건 · 환경 계약 · 기본값.

## 불변식
위반하면 조용히 오염되고 사후에 되돌리기 어려운 것들. 한 행 = 금지 → 대신.
| # | 금지 | 대신 |
|---|---|---|
| 1 | 메인이 tracked 파일을 직접 편집 | 위임한다. 예외는 통합 검증·머지·복구·정리 명령과 repo 밖 기록뿐 |
| 2 | 원문·전체 diff·리뷰 전문을 메인 컨텍스트로 반입 | 작업 id·변경 파일 목록·verdict·기록 경로만 받는다. 산출물이 응답 텍스트인 경량 런은 그 보고 본문만 예외 |
| 3 | 메인이 worktree 또는 다른 브랜치로 진입 | worktree 경로를 지정해 읽거나 read-only 로 위임한다 |
| 4 | 저장소 안 tracked 편집을 격리 없이 위임 | 언제나 worktree 격리. 공유 checkout 을 쓰는 teammate 가 편집하면 격리 subagent 로 형태 전환 |
| 5 | 편집 도구의 파일 경로에 부모 repo 절대경로 | worktree 절대경로만 — 어긋나면 worktree 경로로 재지정 |
| 6 | 격리 위임을 한 메시지에 여러 건 싣거나 생성 확인 전에 다음 건을 싣는 것, 가드 명령을 worktree 안에서 실행 | 한 건씩 싣고 worktree 목록에 등장했는지 확인한 뒤 진행. 가드 세 명령(`references/procedures.md §직렬 dispatch 와 토폴로지`)은 메인 working tree 에서 |
| 7 | 브랜치·tree 가 어긋난 채로 진행 | 복구 절차를 먼저 돌리고(변경은 보존한다) 그다음 정지·보고 |
| 8 | 머지 직후 가드 생략 | 매 머지 직후 넷을 확인 — 브랜치가 epic 이고 tree clean · HEAD 가 방금 통합한 후보의 tip · author 방침 판정을 거침(방침 없음이면 기본값 적용 사실을 기록) · in-flight worktree 가 epic 최신 HEAD 기준 |
| 9 | epic 브랜치를 rebase, 또는 작업 브랜치를 그냥 merge | 통합은 rebase 후 fast-forward 전용 머지로 고정. 역방향 drift 흡수도 merge 로 하고 흡수 커밋 메시지에 merge 임을 명시 |
| 10 | 메인이 충돌을 직접 편집, task 를 갈아치워 충돌 카운터를 리셋 | 전담 위임(해결 전략의 단일 출처는 `git` skill 의 충돌 해결 문서). 카운터는 파일 기준으로 누적 |
| 11 | 개별 worktree·중간 브랜치의 green 으로 완료 선언, red 를 메인이 직접 고쳐 green | epic 최종 HEAD 에서 integration_verify 계약 명령 green(tracked 변경이 없는 런은 해당 없음으로 기록) + 그 HEAD sha 명시. 회귀 원인은 수정 위임 |
| 12 | 계획 밖 tracked 편집을 선-dispatch 후-체크 | 편집 dispatch 전에 토폴로지·격리를 late gate 로 먼저 세우고 전환 사유를 기록 |
| 13 | 빈 자리를 silent fallback 으로 메움 | inert 구현 또는 미구현 예외로 드러내고, 미결 seam 을 결정 기록과 종료 보고에 깃발로 남긴다 |
| 14 | 증거 없는 claim 을 취합, 같은 전략 반복을 교차 검증으로 인정 | 모든 claim 에 실행 명령 원문과 `file:line`, 없으면 `NOT FOUND — searched with <명령>`. 부재 주장은 축을 바꾼 둘 이상의 전략으로 |
| 15 | 한 관점만 pass 인데 승급, 구현 agent 가 게이트를 겸임 | 세운 모든 관점이 pass 여야 승급. 관점마다 다른 agent 를 병렬로. reject 는 `요구사항·spec 위치 ↔ 파일:라인` 을 지목한 findings 를 실어 재위임. 사용자가 단일 agent 를 명시했으면 검토 관점 생략을 사용자 결정으로 받아 기록한다(자율이면 보고 후 진행) |
| 16 | 승인 이벤트 없이 implementer dispatch, 마커만 만드는 자기 승인 | 협의체 pass·심문 합의·사용자 승인 중 하나를 만들거나, 자명한 작업이면 면제 판정을 근거와 함께 기록 |
| 17 | 자기 설계를 자기가 심문, 협의체를 소집한 경우 검증 pass 전 dispatch | 검증자와 생성자를 다른 agent 로. 두 자세를 모두 거친 뒤 dispatch |
| 18 | 자문 권고를 게이트로 사용, 갈린 권고를 평균 | 결정권은 메인에 100% — `critical` 은 에스컬레이션 트리거로 승격, 대립 지점을 기록하고 메인이 고른 뒤 채택·부분채택·기각을 사유와 함께 남긴다 |
| 19 | 편집 목적 dispatch 의 성공을 보고만으로 수령, 격리 worktree 재사용 | diff 또는 커밋 해시 실존으로 실제 변경 확인(변경 0건은 성공이 아니라 prompt 결함). 재위임은 언제나 새 격리 dispatch |
| 20 | 완료 대기에 sleep·폴링 루프, 메인의 체감을 완료 판정 입력으로 사용 | 도착하는 완료 알림으로 진행. 완료 판정 입력은 명령 결과와 산출물 수용뿐 |

## 정지 조건
신호가 관측되면 예산이 남아 있어도 멈춘다. 정지 보고는 `현재 상태 + 남은 작업 + 막힌 지점 + 선택지` 를 한 번에 담고, 종료로 이어지면 3분류 판정과 핸드오프를 남긴다.
| # | 조건 | 신호 | 행동 | 출구 |
|---|---|---|---|---|
| 1 | **spec 확정 게이트** | 미결 섹션에 `없음` 아닌 항목 / 본문의 미확정 표지가 기각되지 않음 / 게이트 verdict 가 spec 미해소 | hard stop — 예산과 무관. 계약도 세우지 않고, 입력 spec 집합을 좁혀 부분 진입하지 않는다 | 심문으로 미결 0 → spec 반영 → 같은 판정 재실행 후 재진입. 재위임 예산 미소모 |
| 2 | 계획 밖 tracked 편집 | 경량으로 판정한 런에서 계획에 없던 tracked 편집이 필요해짐 | 편집 dispatch 를 시작하지 않는다 | 토폴로지·격리를 late gate 로 세운 뒤 무거운 경로로 전환(역방향 없음) |
| 3 | 토폴로지 위반 | 메인의 브랜치가 확보한 epic 과 다름 / tree dirty / dispatch 직후 worktree 미등장 / HEAD 가 통합 후보 tip 과 다름 | 복구 후 정지. 후속 dispatch 중단 | 복구 성공 + 발생 지점을 붙여 보고 → 사용자 결정 |
| 4 | 되돌리기 어렵거나 epic 경계 밖 행위 | 강제 push · 기본 브랜치 머지 · 배포 · 외부 서비스 호출 · 데이터 삭제 / 의도가 갈리는 충돌 / 도메인 의미 결정 / 계획 승인 불가 / authorship 방침을 지금 처리로 지킬 수 없음 | 즉시 정지·보고 — 예산이 남아 있어도. 자문 권고가 멈추는 판단을 대체하지 않는다 | 사용자 승인 또는 결정. 부분 머지 상태를 함께 보고 |
| 5 | 통합 검증 실패 | 계약이 지정한 시점의 검증이 red / 최종 HEAD integration_verify red | 머지 전 시점이면 그 머지를, 머지 후 시점이면 후속 루프를 중단. 완료 선언 금지 | 원인 파악 → 수정 위임 → 최종 HEAD 재실행, HEAD sha 와 함께 보고 |
| 6 | 예산 소진 / 무진전 | 루프·시간·턴 상한 도달 / 연속 무진전이 한도 도달(진전 = 머지된 브랜치 수·통과 테스트 수·종료 조건 충족 항목 수) / 재위임 예산 소진 | 예산 소진 종료 — 진전 없는 재분해 반복 금지 | 사용자가 예산을 올리거나 범위를 좁힌다. 진전 측정값과 남은 작업 제출 |
| 7 | 협의체 라운드 소진 | 라운드 예산 소진 후에도 심문 verdict 가 pass 아님 / verdict 에 도메인 의미 결정 포함 | 검증되지 않은 분해로 dispatch 하지 않는다 — 예산 소진은 통과가 아니라 정지다 | 사용자 결정 또는 tie-break 자문 1회 후 그 결과를 근거로 보고 |
| 8 | 공유 전제 실패 (외부 환경) | 위임·머지 경로가 실제로 거치는 공유 의존(CLI 인증·필수 외부 서비스 도달)의 read-only 확인 실패 | dispatch 를 시작하지 않고 즉시 보고. 부분 dispatch 로 밀어붙이지 않는다 | 무엇을 확인하다 실패했는지 + 해제 방법을 함께 내고, 전제 복구 후 재진입 |
| 9 | 같은 원인으로 연속 실패 (외부 환경) | 여러 dispatch 가 동일 외부 원인(gateway 오류·API 오류·rate limit)으로 반복 실패 | 루프를 멈춘다. 추정으로 prompt 를 고쳐 다시 던지지 않는다 | 환경 복구 확인 후 재개, 또는 사용자가 다른 경로를 지시 |
| 10 | 계획과 다른 치명적 결함 → HITL 전환 | 자율 런 중 spec·설계의 전제를 무너뜨리는 결함 발견(도메인 의미 오염 · 데이터 손상 위험 · 아키텍처 전제 붕괴) | 자율을 중단하고 HITL 로 전환. 남은 계획을 자율 재량으로 재설계하지 않는다 | 결함·영향 범위·산출물 상태·선택지를 한 번에 보고 → 사용자가 계획 갱신·재개 승인 |

## 표준 절차
```
0. 진입      도구 스키마 확보 → 경로 · 모드 판정 → epic 브랜치 확보 → preflight
1. 분해      복잡 · 모호하면 아키텍트 협의체, 그 밖(확정 spec · 단순 요청)은 메인이 직접
2. 계획      병렬 · 순차, 위임 형태, 격리 여부, 리뷰 관점을 기본값 표로 정한다
3. 위임      한 건씩 — 격리 dispatch 는 생성 확인 전에 다음 건을 싣지 않는다
4. 모니터    기대 완료 시간을 정해 두고 도착하는 완료 알림으로 진행한다
5. 게이트    세운 모든 관점이 pass 여야 승급, 관점마다 다른 agent
6. 머지      한 후보씩 rebase 후 fast-forward 전용 머지, 매 머지 직후 가드
7. 최종      epic 최종 HEAD integration_verify 계약 명령 green(tracked 변경 없으면 해당 없음) + HEAD sha 명시
8. 보고      종료 사유 · 3분류 건수 · 핸드오프 · 머지 결과 · 결정 요약
```

경로 · 모드에 따른 단계의 유지 · 생략은 기본값 표가 정한다.

## 기본값
매번 판단하지 않고 표에서 읽는다. 표를 벗어난다고 판단한 경우에만 근거를 기록한다. 표는 전부 `references/contracts.md §기본값 표` 에 있다.
| 무엇을 정하는가 | 표 |
|---|---|
| 어느 tier 로 dispatch 하는가 · 자문을 부르는가 | 표 1 — 집행 tier |
| 왕복 조율이 필수인가 선호인가 / 병렬인가 순차인가 | 표 2 — team 등급 / 표 3 — 병렬 · 순차 |
| 단발인가 team 인가 / worktree 로 격리하는가 | 표 4 — 위임 형태 / 표 5 — 격리 여부 |
| 게이트를 몇 관점으로 세우는가 | 표 6 — 검토 게이트 |
| 자율인가 HITL 인가 · Task 로 등록하는가 · 무거운 경로인가 | 표 7 — 모드 · 등록 · 경로 |

## 계약과 절차
| 지금 무엇이 필요한가 | 파일 |
|---|---|
| 도구 확보 · 브랜치 규약 · prompt 필수 요소 · 기록 형식 · 예산 값 · 보고 형식 · 기본값 표 | `references/contracts.md` |
| 진입 · 자율 루프 · 직렬 dispatch · 복구 · worktree 정리 · 머지 · 최종 게이트 · 협의체 · 자문 | `references/procedures.md` |

진입 시 여는 것은 `contracts.md` 와 `procedures.md §진입 절차` 다 — 나머지 절차 절은 도달한 순간 읽는다.
