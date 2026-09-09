# A. Orchestrator 결정 인벤토리 (read-only 조사 결과)

> **09 설계의 입력 자료.** 2026-09-07 시점 orchestrator 스킬(commit `ccd6b94` 기준)의 스냅샷 기록이며 이후 갱신하지 않는다.
> 기계 파서 입력으로 쓰지 않는다 — 근거는 [`09` §9.2](09-orchestrator-graph-restructure.md) 참조.
> **조사 관점**: 스킬 전체를 결정(decision) 단위로 훑어 D01–D58 을 식별하고, 각 결정의 서술 위치·선행 관계·종단과 부록의 절차 11 · 계약 14 를 목록화했다.
> 본문은 조사 원문 그대로다.

조사 대상: `plugins/atelier/skills/orchestrator/` — SKILL.md(230행) + references 10개(총 2,867행).
브랜치 `claude/atelier-plugin-github-issue-tazv6a`, 파일 수정 없음.

- 파일 약칭: `SKILL` = SKILL.md, 나머지는 `references/<name>.md` 를 파일명만으로 표기
  (`delegation` = delegation-patterns.md, `autonomous` = autonomous-driving.md, `monitor` = agent-monitor.md,
  `merge` = merge-coordinator.md, `branch` = branch-strategy.md, `worktree` = worktree-lifecycle.md,
  `advisory` = advisory-consult.md, `council` = architect-council.md, `routing` = model-routing.md,
  `spec-review` = spec-driven-review.md)
- 모든 행 번호는 `awk`/`sed` 로 붙인 실제 행 번호를 확인한 것이다.
- **결정 총 58개** (D01–D58). 절차/계약은 맨 뒤 §부록 A 로 따로 분류.

---

## D01. 스킬 트리거 판정 (오케스트레이터 진입 여부)

- **입력 신호**: 요청이 2개 이상 독립 단위인가 · 병렬 fan-out 가능한가 · 위임/팀/worktree를 명시했는가 · 문서 산출물·리서치인가 · 메인이 Edit/Write 하려는 순간인가 · **규모**(작업 종류가 아님)
- **출력 분기**: 스킬 진입 / 진입 안 함(단일 파일 단순 편집 · 사용자가 메인 처리 명시 · 1턴 단발 조회)
- **서술 위치**:
  - `SKILL:3` (frontmatter description 의 트리거 목록 — 실질적으로 같은 판정을 한 번 더 서술)
  - `SKILL:9-21` (§When to use — 불릿 + "적용 범위는 규모로" + 트리거 금지 케이스)
  - `delegation:176` (출구 조항이 `SKILL §When to use` 의 "트리거하면 안 되는 상황"을 재인용)
- **표현 형태**: frontmatter 산문 목록 + 산문 불릿
- **선행 결정**: 없음 (최상위 진입점)

## D02. 메인 직접 수행 vs 위임 (편집권·조사 착수 경계)

- **입력 신호**: 행위 종류 — 결정적 사실 확인(Read/Bash/git status)인가, 본격 조사·통독인가, Edit/Write/NotebookEdit 인가, 짧고 결정적인 git 검사인가
- **출력 분기**: 메인이 직접 / sub-agent 위임 (실패해도 편집권 회수 금지)
- **서술 위치**:
  - `SKILL:25-36` (§사고 모드 — 해도 되는 일 / 하면 안 되는 일)
  - `SKILL:139` (QA의 테스트 추가도 편집이므로 위임)
  - `SKILL:212` (안티패턴 1 편집권 회수)
  - `autonomous:127-135` (§메인 컨텍스트 격리 — "메인 직접 Read는 결정적 사실로 제한")
  - `autonomous:254` (integration_verify 는 메인이 직접 Bash — 편집 금지 정책의 예외)
  - `autonomous:478` (핸드오프 파일 작성도 sub-agent 위임)
  - `autonomous:514` (안티패턴 12 전문 끌어오기)
  - `monitor:182-183` (폴백: read-only 조각은 메인 직접, 편집 조각은 재위임)
  - `merge:157` / `merge:186` (충돌·회귀 수정은 메인 직접 편집 금지) / `merge:200` (worktree 정리는 메인 직접 가능) / `merge:214` (overlap 검사는 메인 직접)
  - `worktree:39` (worktree 상태 확인은 `git -C` 또는 read-only sub-agent) / `worktree:132` (충돌 사전분석은 메인 직접)
  - `council:150` (안티패턴 1 — 메인이 협의체를 겸하지 않음)
- **표현 형태**: 산문 불릿(주) + 표(monitor 폴백) + 인라인 예외 문장 다수
- **선행 결정**: D01

## D03. 경로 판정 게이트 (경량 vs 무거운) ★위치 최다

- **입력 신호**: 이번 런의 **계획된 산출물에 tracked 파일 변경이 있는가** (git 레포 안인지가 아님)
- **출력 분기**: 경량 경로(체크 1·2·3 · 토폴로지 가드 · 머지 조정 생략) / 무거운 경로(전부) / 경계: 비-git인데 편집 필요 → "격리 불가"로 분류해 순차 dispatch
- **서술 위치**:
  - `SKILL:40` (진입 절차 서두 — 전략 성립 조건) · `SKILL:53-66` (게이트 트리 + 판정 규칙) · `SKILL:99-101` (경로 전환 요지) · `SKILL:118` (표준 절차의 경로별 차이) · `SKILL:217-218` (안티패턴 6·7 "적용 경로는 §경로 판정 게이트") · `SKILL:226` (안티패턴 15 근거 없는 체크 생략)
  - `delegation:51` (prompt 필수요소 3번 — 경량은 산출 경로 계약으로 대체) · `delegation:133` (isolation 표의 경량 주석) · `delegation:166` (병렬/순차 축 변경) · `delegation:193-221` (§경로 판정 경계 케이스 — 유지·생략 표 199-207, 경계 케이스 표 211-217, 산출 경로 계약 219-221) · `delegation:225-235` (경로 전환 5단계) · `delegation:448` (체크리스트)
  - `autonomous:35-37` (자율 계약의 `path` 필드) · `autonomous:60` (`heavy = ...`) · `autonomous:70-73` (isolation 분기) · `autonomous:79-80` (assert_topology 조건) · `autonomous:95` (머지 조건) · `autonomous:245` (토폴로지 가드 적용 범위) · `autonomous:509` (안티패턴 7) · `autonomous:548` (체크리스트)
  - `merge:12` (문서 전체가 무거운 경로 전용) · `branch:12` (동일) · `worktree:12` (동일) · `spec-review:15` (동일)
  - `advisory:101` (토폴로지 가드 적용 범위) · `advisory:120` · `advisory:138` (소집 절차 의사코드의 "무거운 경로만")
- **표현 형태**: ASCII 트리(`SKILL:57-62`) + 표 2개(`delegation:199-207`, `211-217`) + 의사코드(`delegation:227-235`) + 각 파일 상단 인용구 산문
- **선행 결정**: 체크 0(조율 도구 스키마 확보 — 부록 A2)

## D04. 왕복 조율(team) 가용 판정 ★위치 최다

- **입력 신호**: [0] `SendMessage` 스키마 확보 여부 → [1차·권위] spawn한 agent를 `SendMessage`로 다시 지목 가능한가 (`Agent` 스키마의 `name`, 없으면 spawn 결과의 `agentId`) → [2차·보조] `printenv CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS`
- **출력 분기**: 가용(지목자=name) / 가용(지목자=agentId) / 비가용
- **서술 위치**:
  - `SKILL:29` (사고 모드 — SendMessage가 유일한 재지목 수단) · `SKILL:49-51` (체크 0) · `SKILL:73-95` (체크 4 판정 트리 + 정정·시점 규칙) · `SKILL:168` (위임 형태 표의 가용 판정 참조) · `SKILL:177-178` (강제 등급 요지) · `SKILL:183` (team 비가용 = 자문 차단) · `SKILL:222` (안티패턴 11 신호 하나로 단정) · `SKILL:225` (안티패턴 14 deferred 도구)
  - `delegation:21` (team 정의의 name/agentId) · `delegation:329-334` (등급 표의 "team 비가용" 열) · `delegation:344-348` (§Agent team 사용 패턴 전제 인용구 3개) · `delegation:382-383` (지목자·deferred 주의) · `delegation:390-418` (spawn 확인 — D05)
  - `advisory:15-26` (§전제 — team 필수 등급) · `advisory:42-70` (게이트 0 트리) · `advisory:256-260` (안티패턴 9) · `advisory:269` (체크리스트)
  - `council:22-24` (메커니즘 5 + 가용 판정 참조) · `council:73` (`require(team_available)`) · `council:152` (안티패턴 3) · `council:168-169`
  - `autonomous:28-34` (자율 계약의 자문 가용 필드) · `autonomous:146` · `autonomous:159` · `autonomous:526` · `autonomous:533-534`
  - `monitor:37` (agentId 보관) · `monitor:217` (agentId 재개) · `monitor:223` (team 진행 추적)
  - `branch:126` (SendMessage 없으면 정책 B 불가) · `routing:57` · `spec-review:59`, `spec-review:67-69`
- **표현 형태**: ASCII 트리(`SKILL:76-93`) + ASCII 트리(`delegation:396-415`) + 표(`delegation:329-334`) + 인용구 산문(`delegation:344-350`) + ASCII 트리(`advisory:44-63`)
- **선행 결정**: 체크 0

## D05. spawn 확인 (필수 등급 첫 spawn이 실제 teammate인가)

- **입력 신호**: spawn 결과에서 지목자(name/agentId) 확보 여부 · 그 지목자로 보낸 `SendMessage` 1회의 성공/에러 종류(InputValidationError / 수신자 없음)
- **출력 분기**: 왕복 성립 → 진행 / ToolSearch 후 1회 재시도 / [3] 진입 판정 1회 재판정 → 가용(재소집) 또는 비가용(폴백 금지 → 경로별 "team 비가용" 처리)
- **서술 위치**: `delegation:390-418` (단일 출처, 트리 396-415) · `SKILL:91` (정정 규칙 1줄) · `SKILL:178` (필수 등급 가드 요지) · `advisory:133-135` (소집 절차 의사코드) · `advisory:146-150` (§spawn 확인) · `advisory:238-239` (안티패턴 1의 탐지 수단) · `advisory:273` · `council:24` · `council:77-78` · `council:152` · `council:169`
- **표현 형태**: ASCII 트리 + 의사코드 라인 + 산문
- **선행 결정**: D04, D19

## D06. 공유 전제 preflight 판정 (체크 5)

- **입력 신호**: 이번 런의 파이프라인이 실제 거치는 의존만 — "깨졌을 때 fan-out 전체가 죽는 공유 의존인가"
- **출력 분기**: 통과 → dispatch 진행 / 실패 → dispatch 시작하지 않고 해제 방법과 함께 즉시 보고. (작업 하나만 죽이는 의존은 대상 아님 → 재위임·게이트 경로로)
- **서술 위치**: `SKILL:97` (체크 5) · `delegation:180-189` (대상 도출·방법·경계 단일 출처) · `delegation:204` (경량에서 "유지·강화") · `autonomous:532` (체크리스트)
- **표현 형태**: 산문 불릿
- **선행 결정**: D03, 체크 0

## D07. 경로 전환 판정 (경량 → 무거운)

- **입력 신호**: 계획에 없던 tracked 파일 편집이 필요해진 순간
- **출력 분기**: 전환(편집 dispatch 전에 체크 1·2·3을 late gate로) / 유지. **역방향(무거운→경량) 전환 없음**
- **서술 위치**: `SKILL:101` (트리거·역방향 금지) · `delegation:214` (경계 케이스 표의 "조사→구현" 행) · `delegation:223-235` (5단계 절차 단일 출처) · `autonomous:37` (계약 필드 주석) · `autonomous:245` (경량에서 가드 되살리기)
- **표현 형태**: 의사코드 5단계 블록 + 산문
- **선행 결정**: D03

## D08. 협의체 소집 여부 (분해를 아키텍트 협의체에 위임할지)

- **입력 신호**: 요구가 복잡·모호한가 / 여러 서브시스템에 걸치는가 / 작업 경계가 자명한 단순 fan-out인가 / 검증된 spec 입력이 있는가 / (루프 중) 남은 작업의 전제가 무너졌는가
- **출력 분기**: 협의체 소집 / 메인 직접 분해(+ "설계 불필요" 판정을 근거와 함께 기록)
- **서술 위치**: `SKILL:107-108` (표준 절차 1단계) · `SKILL:190` (References 표) · `council:35-43` (§언제 쓰는가 + 생략 기준) · `council:38` (재분해 조건) · `council:138` (마커 적용 예외 = 생략 판정 기록) · `council:154` (안티패턴 5) · `council:163` · `autonomous:65-68` (루프 의사코드 주석) · `autonomous:538` (체크리스트)
- **표현 형태**: 산문 불릿 + 의사코드 주석 + 체크리스트
- **선행 결정**: D01, D04(필수 등급이라 비가용이면 소집 자체가 에스컬레이션)

## D09. 협의체 라운드 종료 판정 (pass / 재심문 / 예산 소진)

- **입력 신호**: `verdict.pass`, `round < max_council_rounds`, `verdict.has_domain_decisions`
- **출력 분기**: break(pass → task 확정) / 다음 라운드(findings로 보강 요청) / 예산 소진 → 자문 tie-break(가용 시) → 에스컬레이션
- **서술 위치**: `council:20-21` (메커니즘 3·4) · `council:70-96` (협의체 루프 의사코드 + 예산·tie-break 불릿) · `council:155-156` (안티패턴 2·6) · `council:166-167` · `advisory:78` (트리거 1) · `autonomous:199` (자문이 재위임 예산을 안 쓴다는 대응 규칙)
- **표현 형태**: 의사코드 + 산문 불릿
- **선행 결정**: D08

## D10. 설계 승인 마커 확인 (implementer dispatch 게이트)

- **입력 신호**: `.orchestrator/<epic>/design-approval.md` 존재 여부 + 해당 task가 `대상 task` 목록에 포함되는지 · 승인 출처(council pass / grill 합의 / 사용자 승인)
- **출력 분기**: dispatch 진행 / dispatch 하지 않고 설계 단계(협의체 또는 grill)로 회귀 / 자명 작업은 마커 면제(단 생략 판정을 기록)
- **서술 위치**: `SKILL:129` (게이트 발동 시점 소유) · `council:118-138` (마커 계약 단일 출처 — 필수 필드 코드블록 129-134, 게이트 동작 136, 자기 승인 금지 137, 적용 예외 138)
- **표현 형태**: 산문 불릿 + 필수 필드 코드블록
- **선행 결정**: D08, D09

## D11. spec 확정 게이트 (미결 판정 — hard stop) ★위치 최다

- **입력 신호**: 입력 spec 문서 스캔의 3신호 — (a) 미결 섹션에 `없음` 아닌 항목(**결정적**) (b) 본문의 TBD/미정/`?` 표기(**후보** — 주변 문장으로 미결/기각 판정) (c) spec 간 모순(**판단**). 루프 중에는 게이트 verdict `spec-unresolved`
- **출력 분기**: 확정(미결 0) → 계약 수립·진입 / 미결 존재 → **hard stop**(계약 미수립, 예산 무관, 미결 목록 + 해소 경로 grill→spec-write 보고)
- **서술 위치**:
  - `SKILL:128` (디스패치 전 게이트) · `SKILL:206` (에스컬레이션 조건) · `SKILL:229` (안티패턴 19) · `SKILL:199` (References 표의 진입 전제)
  - `autonomous:38-41` (계약 `spec_resolved` 필드) · `autonomous:43` (hard_stops 기본 포함) · `autonomous:57-58` (루프 첫 줄) · `autonomous:88-90` (게이트 verdict 분기) · `autonomous:350-391` (§spec 확정 게이트 전체 — 판정 표 358-362, 범위 370-374, 시점 376-379, 동작 381-387, 해제 389-391) · `autonomous:437-439` (에스컬레이션 조건 8) · `autonomous:517` (안티패턴 15) · `autonomous:527` · `autonomous:555`
  - `spec-review:26` (진입 전제) · `spec-review:38-39` (verdict에 `spec-unresolved` 포함) · `spec-review:48-53` (§게이트 중 spec 미결 발견) · `spec-review:81` (루프 분기) · `spec-review:118` (안티패턴 9) · `spec-review:127` · `spec-review:136`
  - `council:43` (생략 기준의 단서) · `council:64-65` (자율 어댑테이션 — 가정 금지) · `council:172`
- **표현 형태**: 표(신호 3종) + 산문 + 의사코드(루프) + 체크리스트
- **선행 결정**: (진입 시) 없음 — 자율 계약보다 앞선다 / (루프 중) D31

## D12. 분해 방식 판정 (수직 슬라이스 · hot-spot 추출 · 선행 task)

- **입력 신호**: 기능 단위로 자를 수 있는가(수직) vs 레이어로 자르는가(수평) · 같은 위치에 항목 추가가 생기는 파일이 있는가 · 여러 작업이 같은 타입·시그니처를 필요로 하는가
- **출력 분기**: 수직 슬라이스 task들 / hot-spot을 별도 통합 task로 뽑음 / 공유 인터페이스를 선행 task로 앞세움
- **서술 위치**: `SKILL:147-153` (§분해는 충돌 경계로 쪼갠다 — 단일 출처) · `SKILL:228` (안티패턴 17) · `branch:23` (분해 원칙은 SKILL이 단일 출처임을 명시) · `branch:50-93` (hot-spot 정의·통합 task 처리 — D13과 공유)
- **표현 형태**: 산문 불릿
- **선행 결정**: D08

## D13. hot-spot 판정

- **입력 신호**: 겹치는 파일 각각이 "의도는 안 겹치는데 **같은 라인 근처에 항목을 추가**하게 되는가" (lock 파일·re-export·라우트/DI 등록·마이그레이션 시퀀스·i18n — 전형 사례이지 고정 목록 아님)
- **출력 분기**: hot-spot(→ 병렬 유지 + 통합 task 분리 + prompt 편집 금지 계약) / hot-spot 아님(→ 순차)
- **서술 위치**: `SKILL:152` (분해 절) · `SKILL:157` (병렬/순차 요지) · `branch:18` (충돌 자리 표) · `branch:50-93` (§2 — 정의 52, 전형 54-62, 판정 트리 68-77, 통합 task 처리 83-93) · `worktree:54-55` (dispatch 사전 검증 의사코드) · `worktree:154` · `delegation:60` (prompt 필수요소 12번) · `delegation:154-155` (병렬/순차 트리의 hot-spot 분기) · `branch:199-200` (안티패턴 2·3) · `branch:212-213`, `branch:221` (체크리스트) · `merge:275` (충돌 2회 → hot-spot 재분류)
- **표현 형태**: 정의 산문 + ASCII 트리(`branch:68-77`) + 코드블록 계약 문구 + 체크리스트
- **선행 결정**: D12, D14의 입력

## D14. 병렬 vs 순차 판정

- **입력 신호**: 작업들의 변경 파일 집합이 disjoint인가 · 의존성이 있는가 · overlap이면 전부 hot-spot인가 · 같은 라인 영역 가능성 · (경량 경로) 외부 리소스·rate limit
- **출력 분기**: 병렬(각자 worktree-isolated) / 순차 / 병렬 유지 + hot-spot 통합 task 분리
- **서술 위치**: `SKILL:110` (표준 절차 3단계) · `SKILL:155-157` (§병렬 vs 순차 판정 요지) · `SKILL:213` (안티패턴 2) · `delegation:142-166` (전체 트리 단일 출처 — 트리 146-159, 판단 근거 161-166) · `delegation:203` (경량의 축 변경) · `branch:68-77` (hot-spot 분기 트리) · `worktree:50-55` (dispatch 사전 검증 의사코드) · `worktree:114-132` (§충돌 위험 사전 분석 4단계) · `worktree:139`, `worktree:153-155` · `merge:65-67`, `merge:204-214` (사후 overlap 재검사) · `merge:266`
- **표현 형태**: ASCII 트리 2개(`delegation:146-159`, `branch:68-77`) + 의사코드(`worktree:50-55`) + 번호 절차(`worktree:118-130`) + 산문 요지
- **선행 결정**: D03, D12, D13

## D15. 조사·감사 작업의 병렬 fan-out 기본값 + 관점 수 결정

- **입력 신호**: 작업이 read-only 조사·감사·원인분석인가(충돌 축에 걸리는 것이 없는가) · 관점이 정말 하나뿐인가 · 1턴 단발 조회인가
- **출력 분기**: 관점별 병렬 fan-out(근거 없으면 3관점 이상) / 단일 agent(판정을 decision log에 기록) / fan-out 대상 아님(1턴 단발)
- **서술 위치**: `SKILL:16` (트리거 절의 read-only 포함 선언) · `SKILL:159-161` (§조사·감사 작업의 기본값은 병렬 fan-out) · `SKILL:230` (안티패턴 18) · `delegation:168-176` (상세 단일 출처 — 출구 판정 176) · `delegation:432-435` (discovery tier — 병렬성이 결과를 좌우한다는 같은 근거)
- **표현 형태**: 산문 불릿
- **선행 결정**: D14(기본값을 뒤집는 결정)

## D16. 근본원인 swarm 축 분해 판정

- **입력 신호**: 원인 불명확한 결함·회귀인가 · 축끼리 **증거가 겹치지 않는가**(같은 증거를 두 agent가 보면 중복)
- **출력 분기**: 축 목록 확정 — 기본 4축(현재 상태 / 변경 이력 / 환경 차이 / 선례)에서 더하거나 뺌
- **서술 위치**: `delegation:277-292` (§근본원인 swarm 1. 축 분해 — 축 표 286-290) · `SKILL:191` (References 표에서 "원인 불명 결함·회귀 조사" 진입 조건)
- **표현 형태**: 표 + 산문
- **선행 결정**: D15

## D17. 가설 랭킹·반증 진행 판정

- **입력 신호**: 축별 지지·반박 증거, 지지 증거가 단일 축에서만 나왔는가, 반증 실험 결과
- **출력 분기**: 최상위 가설의 반증 실험 실행 / 반증되면 다음 순위 / 전부 반증되면 축 분해로 회귀
- **서술 위치**: `delegation:300-314` (§3 가설 랭킹 테이블 + §4 진행 규칙)
- **표현 형태**: 표(빈 헤더) + 산문 불릿
- **선행 결정**: D16

## D18. 위임 형태 결정 (단발 sub-agent vs agent team)

- **입력 신호**: 작업이 단순 1회성이고 결과가 단일인가 · 진행 중 개입(SendMessage)이 필요한가 · review→fix 반복이 예상되는가 · 결과물이 여러 단계로 누적되는가
- **출력 분기**: 단발 sub-agent / agent team (병렬 fan-out도 "단발 여러 개")
- **서술 위치**: `SKILL:110` (표준 절차 3단계) · `SKILL:163-173` (위임 형태 표 165-169 + 격리 주의 171) · `delegation:12-39` (§단발 vs team — 결정 트리 31-37, review→fix 인용구 39) · `delegation:450` (체크리스트) · `autonomous:139-159` (§위임 형태 — 편집은 격리 subagent, 조율은 team) · `autonomous:291` (decision log 기록 시점)
- **표현 형태**: 표(`SKILL:165-169`) + ASCII 트리(`delegation:31-37`) + 산문 인용구
- **선행 결정**: D04

## D19. team mode 강제 등급 판정 (필수 vs 선호)

- **입력 신호**: 두 조건 AND — (1) 경로의 본질이 **왕복 대화**인가(대체 불가) (2) **read-only 조율**인가(비용 없음). 편집이 개입하면 자동으로 선호
- **출력 분기**: 필수(가용인데 단발 대체 = 위반, 비가용이면 폴백 없이 원래 에스컬레이션) / 선호(단발 폴백 허용)
- **서술 위치**: `SKILL:175-178` (§team mode 강제 등급 요지) · `delegation:318-338` (단일 출처 — 등급 기준 322-327, 경로별 표 329-334, 가드 336, 등급 경계 338) · `advisory:15-26` (자문이 필수인 근거) · `council:22-24` (메커니즘 5) · `spec-review:59` (spec 게이트가 선호인 근거) · `spec-review:67-69` (폴백 허용) · `spec-review:138` · `autonomous:146` (review→fix가 선호) · `autonomous:159` · `routing:52`, `routing:57`
- **표현 형태**: 산문 기준 2개 + 표(`delegation:329-334`)
- **선행 결정**: D04, D18

## D20. isolation 유무 결정 ★위치 최다

- **입력 신호**: sub-agent가 **git 저장소 안에서 파일을 수정하는가** / 읽기 전용 분석인가 / 저장소 밖 편집인가 / teammate인가 subagent인가
- **출력 분기**: `isolation:"worktree"` / 없음(현재 브랜치 실행) / 격리 불가(비-git 편집 → 순차 dispatch로 낮추고 보고)
- **서술 위치**:
  - `SKILL:72` (체크 3) · `SKILL:139` (QA 테스트 추가도 격리 위임) · `SKILL:169` (위임 형태 표) · `SKILL:171` (격리는 subagent만 보장)
  - `delegation:55` (prompt 필수요소 7 — 격리 준수) · `delegation:57` (필수요소 9 — base 확인) · `delegation:129-138` (§isolation 결정 — 표 131-134, teammate 제외 138) · `delegation:216` (비-git 경계 케이스) · `delegation:350` (team은 공유 checkout) · `delegation:387` (격리는 subagent가 보장) · `delegation:451-453` (체크리스트)
  - `autonomous:70-73` (루프의 isolation 분기) · `autonomous:141`, `autonomous:145` (§위임 형태) · `autonomous:156` · `autonomous:216` (QA 편집도 격리) · `autonomous:515` (안티패턴 13) · `autonomous:540`
  - `worktree:30-39` (§사용 방식 + EnterWorktree 금지) · `worktree:138` (안티패턴 1) · `worktree:157`
  - `spec-review:63` · `spec-review:112` (안티패턴 3) · `spec-review:134`
- **표현 형태**: 표(`delegation:131-134`) + 산문 + 의사코드(`autonomous:70-73`) + 체크리스트
- **선행 결정**: D03, D18

## D21. worktree dispatch 생성 가드 판정 (assert_worktree_created)

- **입력 신호**: dispatch 전후 `git worktree list --porcelain` 스냅샷 차이(새 worktree 등장) · `git status --short`(메인 clean) · `git rev-parse --show-toplevel`(cwd drift)
- **출력 분기**: 통과 → 다음 dispatch / 새 worktree 미등장 → 후속 dispatch 중단 + agent 정지 + 오염 확인(자율이면 hard stop) / 메인 not clean → 중단 + 변경 보존 후 복구 / cwd drift → 메인 트리 복귀 후 재검증
- **서술 위치**: `worktree:77-96` (단일 출처 — 직렬화 81, 검증 명령 84-88, 위반 처리 90-94, 주의 96) · `worktree:57-68` (병렬 dispatch 패턴 의사코드) · `worktree:144` (안티패턴 7) · `worktree:158`, `worktree:162` · `autonomous:74-76` (루프 주석) · `autonomous:243` (토폴로지 가드 절에서 참조) · `autonomous:509` (안티패턴 7) · `autonomous:546` · `merge:103` (토폴로지 가드가 생성 가드와의 관계를 명시)
- **표현 형태**: 의사코드 + bash 명령 블록 + 산문 불릿 + 체크리스트
- **선행 결정**: D20, D14

## D22. 토폴로지 가드 판정 (branch == epic + clean)

- **입력 신호**: `git branch --show-current` == `epic/<name>` · `git status --short` clean
- **출력 분기**: 통과 → 진행 / 불일치 → 복구 절차(rebase --abort → checkout → pull --rebase → 잘못된 브랜치 삭제, 변경은 `git stash push -u` 로 보존) + **반드시 에스컬레이션**(자율이라도 hard stop)
- **서술 위치**: `merge:101-121` (명령·복구 절차 단일 출처) · `merge:83` (머지 직후 가드 불변식 1) · `merge:251` (안티패턴 7) · `merge:277` · `worktree:100-102` (완료 알림 직후 실행 시점) · `worktree:143` (안티패턴 6) · `worktree:164` · `autonomous:79` (루프 `assert_topology()`) · `autonomous:241-247` (§토폴로지 가드 — 시점·적용 범위·위반 처리) · `autonomous:425-426` (에스컬레이션 조건 2) · `autonomous:509`, `autonomous:547` · `advisory:97` (계약 위반 탐지 수단) · `advisory:101` · `advisory:120`, `advisory:138` (소집 전후) · `advisory:253` (안티패턴 8) · `advisory:276` · `delegation:388` (teammate 위반의 사후 탐지)
- **표현 형태**: bash 명령 블록 + 산문 + 의사코드 호출(`assert_topology()`)
- **선행 결정**: D03(무거운 경로 한정)

## D23. 탐색 예산 결정·소진 처리

- **입력 신호**: 조사·리서치형 dispatch인가 · 작업 규모(기본 30회에서 조정할지) · 예산 소진 여부
- **출력 분기**: 예산 명시 후 dispatch / 예산 없으면 **dispatch 하지 않음** / 소진 시 탐색 중단 → 중간 산출물 3종(발견·계획·미지) 제출 → 메인이 재위임 여부 판단
- **서술 위치**: `SKILL:132` (디스패치 전 게이트 — 발동 시점 소유) · `SKILL:161` (조사 fan-out 절에서 재언급) · `delegation:104-111` (§탐색 예산 단일 출처) · `delegation:175` (조사 fan-out에서 참조)
- **표현 형태**: 산문 불릿
- **선행 결정**: D15, D18

## D24. 테스트 인프라 발견 게이트

- **입력 신호**: 테스트 러너·픽스처·통합 하네스·**유사 기존 테스트 2건 이상**의 `file:line` 인용 확보 여부
- **출력 분기**: 테스트 작성 단계 진입 / 인용 확보 전에는 진입 금지 / 못 찾으면 `NOT FOUND — searched with <명령>` 보고 → 부재 주장이므로 교차 검증(D37) → 메인 판단
- **서술 위치**: `SKILL:130` (디스패치 전 게이트) · `delegation:113-125` (§테스트 인프라 발견 단일 출처) · `autonomous:215` (하네스 미사용 = QA reject 사유, 앞단 참조)
- **표현 형태**: 산문 불릿(발견 항목은 중첩 불릿)
- **선행 결정**: D20(구현 dispatch 전), D30의 앞단

## D25. 계획 우선 게이트 적용 여부 (plan-first)

- **입력 신호**: 편집의 리스크·되돌리기 비용(스키마 변경·광범위 리팩토링·공개 API 변경) vs 단순·저위험
- **출력 분기**: 계획만 받는 read-only subagent → 승인 → 편집 격리 subagent 재위임 / 곧장 편집 위임(저위험은 쓰지 않음) / 계획 승인 실패 → hard stop·에스컬레이션
- **서술 위치**: `delegation:249-273` (§계획 우선 게이트 — 2스텝 의사코드 253-269, 도구 의존 없음 271, 자율 연계 272, 절제 273) · `autonomous:542` (체크리스트) · `council:111` (task 도출 계약의 "위험도"가 이 게이트의 입력)
- **표현 형태**: 의사코드 + 산문 불릿
- **선행 결정**: D18, D20

## D26. 모델 tier 선택 (집행/자문 판정 + 작업유형 시작 tier) ★위치 최다

- **입력 신호**: 이 dispatch가 자문 4트리거에 해당하는가 → 작업 유형(분해·설계 / 일반 구현·리뷰·테스트 / discovery / 단순 변환) → 메인 tier → 사용자의 역할별 모델 제약
- **출력 분기**: 자문(메인보다 상위 허용, team member 필수) / 집행(≤ 메인 tier, 예외 없음) × 시작 tier(최상위 / 중간 / 경량) + 제약 충돌 시 인접 tier 대체(게이트 역할은 인접 상위, 그 외 인접 하위)
- **서술 위치**:
  - `SKILL:180-184` (§모델 라우팅 요지) · `SKILL:221` (안티패턴 10 자문 tier 번짐)
  - `routing:12-40` (§역할 기준 원칙 — 판정 트리 14-34, 근거 36-38) · `routing:42-59` (자문 예외 표 46-52) · `routing:63-71` (§역할별 모델 제약)
  - `delegation:422-439` (§모델 선택 — 작업유형 tier 표 428-433 단일 출처, discovery 근거 435) · `delegation:456-459` (체크리스트)
  - `autonomous:113-123` (§모델 분배 원칙) · `autonomous:223` (게이트 모델) · `autonomous:513` (안티패턴 11) · `autonomous:550`
  - `council:142-144` (§모델 정책) · `council:157` (안티패턴 8) · `spec-review:98-104` (§모델 분배 heuristic)
  - `advisory:126` (`main_picks_supra_tier()`) · `advisory:250-251` (안티패턴 7) · `advisory:279`
- **표현 형태**: ASCII 트리(`routing:14-34`) + 표 2개(`routing:46-52`, `delegation:428-433`) + 산문 원칙 + 체크리스트
- **선행 결정**: D27(자문 판정이 먼저), D18

## D27. 자문 소집 여부 (게이트 0 → 트리거 → 예산)

- **입력 신호**: [게이트 0] team 가용 여부(트리거보다 **먼저** 평가) → 트리거 1~4 중 하나인가(협의체 예산 소진 tie-break / 게이트 reject 재위임 루프 / 되돌리기 비용 큰 결정 / 사용자 명시 요청) → `max_advisory_consults` 잔여
- **출력 분기**: 소집 / 소집도 대체도 없이 원래 에스컬레이션(기록) / 트리거 아님 → 소집 안 함 / 예산 0 → 자문 없이 에스컬레이션
- **서술 위치**: `SKILL:182-183` (§모델 라우팅의 자문 규칙) · `SKILL:219` (안티패턴 8 자문 흉내) · `advisory:37-88` (§언제 소집하는가 — 게이트 0 트리 44-63, 순서 근거 65-70, 트리거 표 76-81, 예산·트리거4 근거 83-87) · `advisory:162` (예산 소모 단위) · `advisory:236-239` (안티패턴 1) · `advisory:244-245` (안티패턴 4 상시 자문) · `advisory:269-271` · `autonomous:28-34` (계약 필드) · `autonomous:199` (재위임 루프 트리거) · `autonomous:450-452` (에스컬레이션과의 관계) · `autonomous:553` · `council:88`, `council:96` (tie-break) · `routing:59`, `routing:61`
- **표현 형태**: ASCII 트리(`advisory:44-63`) + 표(`advisory:76-81`) + 산문
- **선행 결정**: D04, D09/D31/D47(트리거 발생원)

## D28. 자문 관점·인원 결정 (자문 스웜 구성)

- **입력 신호**: 문제의 성격 · 판단이 갈릴 축(성능·보안·마이그레이션 안전성 등) · 기본값 답습 금지
- **출력 분기**: 관점 조합 + 자문자 수(1명 이상, 같은 계약에 관점만 바꿔 병렬 spawn)
- **서술 위치**: `advisory:105-114` (§정책·메커니즘 분리 — 고정/런타임 표 109-113, 스웜 114) · `advisory:122` (`perspectives = main_decides_lenses(question)`) · `advisory:261-263` (안티패턴 10) · `advisory:277` · `routing:58` · `council:28-31` (같은 정책·메커니즘 원리를 협의체 역할 구성에도 적용 — 병렬 서술)
- **표현 형태**: 표 + 의사코드 라인 + 산문
- **선행 결정**: D27

## D29. 자문 권고 처리 판정 (채택 / 부분채택 / 기각 + critical 승격)

- **입력 신호**: 권고 severity(critical / important / optional) · 확신도 · 권고 간 대립 여부 · 계약 위반 발생 여부
- **출력 분기**: 채택 / 부분채택 / 기각(전부 사유 기록) / `critical` → 에스컬레이션 트리거로 승격 / 계약 위반 자문은 채택하지 않고 기록
- **서술 위치**: `advisory:103` (계약 위반 시 미채택) · `advisory:140-141`, `advisory:163` (의사코드 + 대립 기록) · `advisory:215-224` (§메인의 처리 의무) · `advisory:240-241` (안티패턴 2 고무도장) · `advisory:242-243` (안티패턴 3 결정권 부여 금지) · `advisory:278`, `advisory:281-282` · `autonomous:294` (기록 시점) · `autonomous:452` (critical 승격) · `autonomous:553` · `SKILL:220` (안티패턴 9)
- **표현 형태**: 산문 불릿 + 의사코드 라인
- **선행 결정**: D27, D28

## D30. 리뷰어·QA·DBA 게이트 구성 판정 (DB 접촉 → DBA 조건부 추가)

- **입력 신호**: 협의체 task 도출 시의 **DB 접촉 플래그** + 게이트 시점의 **변경 파일 검사**(마이그레이션 디렉토리·`.sql`·스키마 정의·ORM 모델·쿼리 빌더 호출부) 중 하나라도 걸리는가. + 경량 경로면 "쓰기 전 검토 1회"로 축소
- **출력 분기**: reviewer + qa (2차원) / reviewer + qa + dba (3차원) / 경량 → 축소된 1회 검토 / 단발 1회·read-only는 예외
- **서술 위치**: `SKILL:113` (표준 절차 6단계) · `SKILL:135-141` (§작업 케이스마다 검토·QA는 필수) · `SKILL:66` (경량의 축소) · `delegation:205` (유지·생략 표의 "조건부 유지") · `autonomous:202-225` (§리뷰어·QA 게이트 — 역할 표 208-212, DBA 조건부 217, 역할 분리 214) · `autonomous:512` (안티패턴 10) · `autonomous:543` · `spec-review:46` (spec 모드에서도 DBA 생략 없음) · `council:113` (Task 도출 계약의 DB 접촉 여부 필드)
- **표현 형태**: 표(역할별 입력/검증질문/출력) + 산문 불릿
- **선행 결정**: D08(플래그), D20, D03

## D31. 게이트 verdict 판정 및 AND 승급

- **입력 신호**: reviewer / qa / (dba) 각각의 `pass` · `reject` · `spec-unresolved`
- **출력 분기**: 전부 pass → 머지 후보 승급 / 하나라도 reject → findings를 자기완결 prompt에 실어 재위임(`max_redispatch_per_task` 소모) / `spec-unresolved` → 재위임 아님, 예산 미소모, 즉시 hard stop
- **서술 위치**: `SKILL:113` · `SKILL:138` (AND 게이트 + 자기 검증 금지) · `autonomous:87-93` (루프 의사코드 분기) · `autonomous:218` (AND 게이트) · `autonomous:222` (예산 소모) · `autonomous:379` (spec-unresolved의 예산 미소모) · `autonomous:511` (안티패턴 9) · `spec-review:38-39` (verdict 3종 출력 계약) · `spec-review:45` (AND) · `spec-review:48-53` (§게이트 중 spec 미결 발견) · `spec-review:75-87` (continuous 루프 의사코드) · `spec-review:111` (안티패턴 2 OR 게이트화) · `spec-review:118` · `spec-review:133`, `spec-review:136-137`
- **표현 형태**: 의사코드 + 표 + 산문 불릿
- **선행 결정**: D30, D11

## D32. spec-driven 특수화 진입 판정

- **입력 신호**: 자율 계약 입력에 spec 문서(`spec/DESIGN.md`, `spec/concerns/*`, `spec/flows/*` 또는 동등)가 있는가 + 미결 0으로 확정됐는가
- **출력 분기**: `spec-driven-review` 게이트(검토자 spec↔구현 / QA 매니저 spec↔테스트) / 일반 리뷰어·QA 게이트(spec 없어도 생략 없음)
- **서술 위치**: `SKILL:141` (spec 입력 시만 특수화) · `SKILL:199` (References 표) · `autonomous:220` (spec 기반이면 특수화) · `spec-review:23-28` (§언제 진입하는가) · `spec-review:116` (안티패턴 7) · `spec-review:126-127` · `council:43` (spec 입력이면 협의체 생략 + 특수화)
- **표현 형태**: 산문 불릿
- **선행 결정**: D11, D30

## D33. 재위임 판단 기준 (실패 원인 → 액션)

- **입력 신호**: 실패 원인 추정 — 외부 환경 / prompt 결함 / 원인 불명확·도메인 판단 / isolation worktree 실패 / no-op run / idle
- **출력 분기**: 같은 prompt 재위임 / prompt 수정 후 재위임 / 사용자 보고(자동 재시도 금지) / 새 isolation worktree / 취소 후 폴백 · 조건 바꿔 재위임 / agentId로 재개
- **서술 위치**: `monitor:200-217` (§재위임 판단 기준 — 표 204-211, 원칙 213-217) · `monitor:139` (중복 보고 반복 시 회부) · `monitor:156` (no-op 회부) · `monitor:211` (idle 행) · `autonomous:82-85` (루프 `handle_failure`) · `autonomous:189-200` (§재위임 자동 — 의사코드 191-196 + 예산 규칙) · `autonomous:180` (자동 개입 표의 재위임 행) · `autonomous:504` (안티패턴 2) · `autonomous:544` · `delegation:243` (작업 agent의 자기 재위임 금지 — 재위임 판단은 오케스트레이터 몫) · `worktree:108` (결과 수령 후 재위임 판단 위임)
- **표현 형태**: 표 + 의사코드 + 산문 불릿
- **선행 결정**: D34, D35, D38, D45

## D34. idle 판정 (무보고 대기의 한도)

- **입력 신호**: dispatch 시점에 정한 **기대 완료 시간** vs 경과 시간 · 완료 알림/중간 보고 유무 · 모드(자율/HITL) · 취소한 agent가 편집 조각이었는가
- **출력 분기**: 계속 대기 / idle 확정 → (자율) agent 취소 + 폴백 회부 + 머지 후보에서 제외 / (HITL) 보고 후 결정 / 뒤늦은 보고는 새 정보 있을 때만 반영
- **서술 위치**: `SKILL:133` (디스패치 전 게이트 — 기대 완료 시간 없이 dispatch 금지) · `monitor:114` (감지 신호) · `monitor:141-151` (§idle 판정 단일 출처) · `monitor:211` (재위임 표의 idle 행) · `monitor:256` · `monitor:264`
- **표현 형태**: 산문 불릿
- **선행 결정**: D18, D45

## D35. no-op run 감지 판정 (수령 검증)

- **입력 신호**: 편집 목적 agent의 완료 알림 + 해당 worktree의 실제 변경 존재 여부(`git diff --stat <base>...<branch>` 또는 커밋 해시 확인)
- **출력 분기**: 성공 수령 / 변경 0건이면 성공 아님 → **prompt 결함**으로 분류해 검증 기준·범위 보강 후 재위임
- **서술 위치**: `monitor:116` (감지 신호) · `monitor:152-156` (§no-op run 감지) · `monitor:210` (재위임 표) · `monitor:260`
- **표현 형태**: 산문
- **선행 결정**: D20

## D36. 중복 보고 판정 (dedup)

- **입력 신호**: 같은 agent(agentId/name) + 같은 task + 실질적으로 같은 결론인가 · 새 정보가 한 줄이라도 있는가
- **출력 분기**: 첫 수신만 취합(이후 무시) / 새 정보 있으면 "갱신"으로 최신본 대체 / 중복 자체를 이상 신호로 decision log에 남기고 반복되면 prompt 결함으로 회부
- **서술 위치**: `SKILL:133` (디스패치 전·보고 수용 게이트) · `monitor:134-139` (§중복 보고 감지 단일 출처) · `monitor:149` (idle 폴백과의 이중 취합 금지) · `monitor:257`
- **표현 형태**: 산문 불릿
- **선행 결정**: 없음 (보고 수용 시점)

## D37. 증거 계약 수용 판정 (보고 수용 게이트)

- **입력 신호**: 보고에 실행 명령 원문 + `file:line` 근거가 있는가 · 부재 주장(negative claim)인가 · 결론을 좌우하는 고위험 claim인가
- **출력 분기**: 수용 / 증거 없으면 취합 제외 + 증거 계약 명시해 재디스패치(예산 소모) / 부재 주장은 서로 다른 검색 전략 2개 이상으로 교차 검증 후 수용 / 고위험 claim만 메인(또는 검증 전용 read-only agent)이 재실행 검증
- **서술 위치**: `SKILL:131` (디스패치 전·보고 수용 게이트) · `delegation:61` (prompt 필수요소 13번 — agent 쪽 계약) · `delegation:95-102` (§증거 계약 — 메인 수용 규칙 단일 출처) · `delegation:123` (테스트 인프라 NOT FOUND 도 교차 검증) · `delegation:294-298` (근본원인 swarm 반환 계약이 같은 계약의 조사축 적용형)
- **표현 형태**: 산문 불릿
- **선행 결정**: 없음 (보고 수용 시점)

## D38. fan-out 복원력 판정 (체크포인트 · 재시도 · 폴백)

- **입력 신호**: 완료 알림 대비 체크포인트 파일의 존재·형식 · 실패가 504/gateway/API 등 일시적 인프라 오류인가 · 재시도 예산(agent당 기본 3회) 잔여 · 실패 조각이 read-only인가 편집인가
- **출력 분기**: 정상 수령 / 재시도(새 worktree dispatch) / 폴백 — read-only는 메인 직접 분석, 편집은 조건 바꿔 새 agent 재위임 / 그래도 안 되면 "미완(사유 포함)"으로 명시해 취합
- **서술 위치**: `SKILL:143-145` (§병렬 fan-out 복원력 — 필수 규칙 4개) · `monitor:160-196` (단일 출처 — 체크포인트 164-169, 재시도 171-176, 폴백 178-184, 투명 보고 186-196) · `monitor:214` (재시도 예산의 일반 규칙과의 관계) · `monitor:258-259`, `monitor:266` · `delegation:186` (preflight 대상이 아닌 실패는 이 경로가 처리) · `delegation:204` (경량에서도 유지)
- **표현 형태**: 산문 불릿 + 표 스켈레톤(조각별 상태 표)
- **선행 결정**: D14, D34, D33

## D39. 머지 시점 정책 결정 (A 배치 vs B 즉시+전파)

- **입력 신호**: in-flight sub-agent가 남아 있는가 · 배치가 긴가 · 후속 작업이 앞 결과를 base로 기다리는가 · `SendMessage` 가용 여부
- **출력 분기**: A 배치 머지(기본, drift 미발생) / B 즉시 머지 + **in-flight 전부**에 rebase 전파 / SendMessage 비가용이면 A로 고정. **B의 절반만 하는 것은 금지**
- **서술 위치**: `SKILL:227` (안티패턴 16) · `branch:97-132` (§3 base drift 전파 — A 103-109, B 111-126, 금지 128-132) · `branch:215`, `branch:219` (체크리스트) · `merge:52-54` (표준 절차 0단계) · `merge:85-86` (머지 직후 가드 불변식 3) · `merge:253` (안티패턴 9) · `merge:264`, `merge:276` · `autonomous:239` (자율 모드에서 특히 중요) · `worktree:107` (결과 수령 후 바로 머지하지 않음)
- **표현 형태**: 산문 + 코드블록(SendMessage 전파 패킷)
- **선행 결정**: D04, D03

## D40. 머지 순서 결정

- **입력 신호**: [1차] 의존성 없는 작업(leaf) 먼저 → [2차] 변경 파일 수가 적은 것 → [3차] branch/path 알파벳 순
- **출력 분기**: 후보 간 순서 확정
- **서술 위치**: `SKILL:114` (표준 절차 7단계) · `SKILL:196` (References 표) · `merge:26-45` (§머지 순서 결정 — 트리 31-39, 판단 근거 42-45) · `merge:69` (표준 절차 3) · `merge:246` (안티패턴 2) · `merge:267` · `autonomous:99` (decision log 기록) · `autonomous:293`
- **표현 형태**: ASCII 트리 + 산문 근거
- **선행 결정**: D39, D41

## D41. 머지 후보 수집·제외 판정

- **입력 신호**: `git branch --list 'epic/<name>/t*'` 결과 · 각 후보의 변경 유무 · sub-agent 결과의 worktree 경로/브랜치명 대조(고아 브랜치) · `git merge-base` (뒤처진 base) · idle로 취소한 편집 조각인가
- **출력 분기**: 후보 포함 / 제외(변경 없음 → 자동 정리) / 고아 브랜치 식별 / 폐기 대상 표시(idle 취소분) / 뒤처진 base는 최신 HEAD 기준 유효성 재확인
- **서술 위치**: `merge:56-63` (표준 절차 1) · `merge:265`, `merge:269` (체크리스트) · `branch:42` (네이밍 규약이 결정적 수집을 가능하게 함) · `branch:44` (고아 브랜치 식별) · `monitor:148` (idle 취소 agent의 브랜치 제외)
- **표현 형태**: 의사코드 절차 + 산문
- **선행 결정**: D39, D34

## D42. 충돌 처리 판정 (자동 위임 / 보고 / 재분해 / 에스컬레이션)

- **입력 신호**: 충돌 성격(단순 라인 겹침 vs 의미 차이 vs 구조 변경, 도메인 의미 충돌인가) · 모드(자율/HITL) · **같은 파일의 충돌 횟수**(파일 단위 카운터, task 예산과 별개) · 충돌 해결 sub-agent의 성공/실패
- **출력 분기**: 충돌 해결 전담 sub-agent에 자동 위임 / 재시도 1회 / 사용자 보고(옵션 B) / 2회 → 재위임 중단 + hot-spot 재분류 + 관련 task 직렬화 / 3회 → 에스컬레이션(분해 자체 보고) / 도메인 의미 충돌 → 즉시 에스컬레이션
- **서술 위치**: `SKILL:114` (표준 절차 7단계) · `autonomous:183` (자동 개입 표) · `autonomous:227-239` (§머지/충돌 — 사다리 229-237 + 카운터 별개 규칙) · `autonomous:429-430` (에스컬레이션 조건 4) · `branch:179-192` (§6 충돌 반복 — 사다리 183-188, 카운터·기본값 조정 190-191) · `branch:204` (안티패턴 7) · `branch:222` · `merge:77-78` (표준 절차 4) · `merge:163-178` (§충돌 시 위임 — 옵션 A/B) · `merge:182-186` (머지 실패 처리) · `merge:245` (안티패턴 1) · `merge:274-275` · `branch:172` (역방향 흡수의 충돌은 merge 전제로 패킷을 바꿔야 함)
- **표현 형태**: 의사코드 사다리 2개(`autonomous:229-237`, `branch:183-188`) + 산문 옵션 A/B + 표(안티패턴)
- **선행 결정**: D40, D45, D13

## D43. epic ← main 역방향 drift 흡수 판정

- **입력 신호**: `git rev-list --count epic/<name>..origin/<default-branch>` 가 0인가 · 시점이 최종 게이트 직전인가(루프 중간 금지)
- **출력 분기**: 0이면 그대로 / 0이 아니면 main을 epic에 **머지**(rebase 아님) + 전체 스위트 재실행 + 흡수 후 HEAD sha 보고 / 중간 흡수가 불가피하면 배치 경계에서만
- **서술 위치**: `SKILL:45` (토폴로지 — 역방향 drift는 런 안에서) · `branch:160-175` (§5 단일 출처 — 명령 164-167, 규칙 169-175) · `branch:225` (체크리스트) · `merge:145-151` (최종 게이트 절차 2단계) · `merge:254` (안티패턴 10) · `merge:283`
- **표현 형태**: bash 명령 블록 + 산문 불릿
- **선행 결정**: D44 직전 단계

## D44. 최종 통합 검증 게이트 / 완료 선언 판정

- **입력 신호**: epic 최종 HEAD의 `git status` clean 여부 · main 흡수 완료 여부 · 전체 테스트 스위트 1회 실행 결과(부분 실행 금지) · HEAD sha
- **출력 분기**: green → 완료 선언 가능(HEAD sha 명시) / red → 완료 선언 금지 + 회귀 원인 파악 후 수정 위임(메인 직접 편집 금지)
- **서술 위치**: `merge:92-94` (표준 절차 7) · `merge:138-159` (§최종 통합 검증 게이트 — 절차 142-151, 완료 선언 조건 153, 실패 시 157, integration_verify와의 관계 159) · `merge:252` (안티패턴 8) · `merge:284-285` · `branch:225` · `autonomous:167-170` (종료 조건의 검증 가능성 예시 + PR ahead 0)
- **표현 형태**: 번호 절차 블록 + 산문
- **선행 결정**: D43, D42

## D45. 자율 주행 vs HITL 모드 판정 (opt-out)

- **입력 신호**: 사용자가 단계별 확인을 명시했는가("확인받으면서", "단계마다 물어봐", "babysit", "자동으로 머지하지 마") · 검증 가능한 `done_when` 을 세울 수 있는가
- **출력 분기**: 자율 주행(기본) / HITL 전환(자동 개입 금지·보고 후 결정) / 자율 진입 보류(done_when 을 먼저 사용자와 합의)
- **서술 위치**: `SKILL:203` (§사용자 보고 원칙 — 기본은 자율) · `SKILL:208` (opt-out) · `autonomous:10` · `autonomous:12` (우선순위 인용구) · `autonomous:16-20` (§기본 동작과 opt-out) · `autonomous:174-187` (§자동 개입 규칙 — HITL/자율 대비 표 178-183) · `autonomous:506` (안티패턴 4) · `autonomous:525` · `monitor:10`, `monitor:12` (적용 범위 인용구) · `monitor:147` (idle 처리의 모드 분기) · `merge:14` (모드별 차이 인용구) · `merge:173-177` (옵션 B)
- **표현 형태**: 산문 + 표(`autonomous:178-183`)
- **선행 결정**: D01

## D46. SendMessage 명령 주입 허용 판정

- **입력 신호**: 방향 — 메인 → **집행 중인** agent 인가, agent → 메인 보고인가, dispatch prompt의 채널 지시인가 · 상황 — 사용자 지시 / team 내 계획된 단계 전환 / 자문자 반문 / 메인 자체 판단
- **출력 분기**: 허용(사용자 지시·계획된 단계 전환·자문 반문·agent→메인 보고) / 금지(메인 자체 판단으로 정체·실패 agent에 지시 주입) / 자율 모드는 가드레일 안에서 확대 허용
- **서술 위치**: `SKILL:29` (조율 도구) · `monitor:85-106` (§SendMessage — 전제 87-93, 허용 95-98, 금지 100-104, 자문 예외 근거 106) · `monitor:235` (안티패턴 2) · `monitor:255` · `autonomous:181`, `autonomous:187` (자동 개입 표의 "집행 agent" 한정) · `advisory:98` (자문자의 타 teammate 지시 금지) · `delegation:59` (prompt 필수요소 11 — 보고 채널은 이 규칙의 대상 아님)
- **표현 형태**: 산문 인용구 + 허용/금지 불릿
- **선행 결정**: D45, D04

## D47. 에스컬레이션 판정 (8조건)

- **입력 신호**: 8조건 — 되돌리기 어렵거나 외부로 나가는 행위 / 토폴로지 위반 / integration_verify 실패 / 도메인 의미 충돌 / 예산 소진 / 원인 불명확한 반복 실패 / 종료 조건 검증 불가 / spec 미결. + advisor `critical` 승격
- **출력 분기**: 멈추고 보고(현재 상태 + 남은 작업 + 막힌 지점 + 선택지) / 루프 계속
- **서술 위치**: `SKILL:206` (§사용자 보고 원칙 — 진행 중) · `autonomous:102-104` (루프의 hard stop 분기) · `autonomous:414-452` (§에스컬레이션 — 트리 418-443, 미확정 지점 445, 보고 형식 448, 자문 관계 450-452) · `autonomous:247` (토폴로지 위반) · `autonomous:255` (integration_verify) · `autonomous:505` (안티패턴 3) · `autonomous:556` · `advisory:222` (critical 승격) · `council:63` (도메인 의미 결정) · `council:89` · `branch:173` · `merge:121` · `worktree:92`
- **표현 형태**: ASCII 트리(`autonomous:418-443`) + 산문 불릿
- **선행 결정**: D22, D42, D49, D58, D11 등 다수의 출력이 입력으로 모임

## D48. 종료 조건 충족 판정 (done_when)

- **입력 신호**: 매 루프 종료 시 **결정적** 재평가 — 테스트/빌드/lint 실행 결과, git 상태, 게이트 통과 여부, integration_verify 통과, 열린 PR의 원격 최신화(ahead 0)
- **출력 분기**: 완료 선언 / 루프 계속 / 예산 소진 종료. 메인의 주관적 "다 된 것 같다"는 금지
- **서술 위치**: `SKILL:207` (종료 시 보고) · `autonomous:26` (계약 필드) · `autonomous:63` (while 조건) · `autonomous:163-170` (§종료 조건) · `autonomous:225` (게이트 통과 포함) · `autonomous:256` (integration_verify 포함) · `autonomous:296` (기록 시점) · `autonomous:503` (안티패턴 1) · `autonomous:528`, `autonomous:552` · `spec-review:94`, `spec-review:140` (spec 게이트 포함) · `merge:153` (최종 HEAD green 전제)
- **표현 형태**: 산문 불릿 + 의사코드 while 조건
- **선행 결정**: D31, D44, D49, D58

## D49. integration_verify 판정 (인프라 의존 테스트 분리)

- **입력 신호**: worktree sub-agent가 접근 불가한 인프라 의존 테스트가 있는가 · 계약의 `run_at`(before_merge / after_merge) · 실행 결과
- **출력 분기**: 계약에 정의(command + run_at) / 미정의 / 실행 결과 실패 시 — before_merge면 그 머지 중단, after_merge면 후속 루프 중단, 둘 다 hard stop → 에스컬레이션. sub-agent worktree 검증 범위에서는 처음부터 제외
- **서술 위치**: `autonomous:45-47` (계약 필드) · `autonomous:98` (루프 호출) · `autonomous:221` (게이트와의 병행) · `autonomous:249-256` (§통합 검증) · `autonomous:427-428` (에스컬레이션 조건 3) · `autonomous:510` (안티패턴 8) · `autonomous:531`, `autonomous:549` · `merge:159` (최종 게이트와의 관계) · `spec-review:92`
  - 주: **SKILL.md 본문에는 이 결정의 서술이 없다** (요지 절 부재)
- **표현 형태**: 계약 필드 코드블록 + 산문 불릿
- **선행 결정**: D45, D58

## D50. 3분류 종료 판정 (DONE / BLOCKED / NOT-STARTED)

- **입력 신호**: 작업 단위마다 — 검증 증거(테스트·빌드·lint·커밋/머지 링크·push된 HEAD sha)가 있는가 / 정확한 에러 + 해제 조건이 있는가 / 착수하지 못한 이유가 있는가
- **출력 분기**: DONE / BLOCKED / NOT-STARTED (각 판정이 요구하는 정보가 빠지면 "판정이 없는 것과 같다")
- **서술 위치**: `SKILL:207` (종료 보고에 3분류 + 핸드오프 경로) · `autonomous:456-482` (§종료 핸드오프 — 판정 표 462-466, 차이의 근거 468, 위치 470-478, 성립 기준 480-482) · `autonomous:391` (spec hard stop 시 BLOCKED + 해제 조건) · `autonomous:494` (보고 형식) · `autonomous:561`
- **표현 형태**: 표 + 산문
- **선행 결정**: D48, D47

## D51. worktree 정리 판정

- **입력 신호**: 머지 성공 / 머지 실패 + 사용자 보류 결정 / 머지 실패 + 폐기 결정 · 변경 유무
- **출력 분기**: worktree + 브랜치 삭제 / 그대로 둠 / 삭제(폐기) / 변경 없으면 자동 정리
- **서술 위치**: `merge:88-90` (표준 절차 6) · `merge:190-200` (§worktree 정리 — 표 194-198, 수행 주체 200) · `merge:248` (안티패턴 4) · `merge:282` · `worktree:106-107` (결과 수령 후 처리) · `worktree:141` (안티패턴 4) · `worktree:165` · `autonomous:563` (미해결 항목·남은 worktree 정리)
- **표현 형태**: 표 + 산문
- **선행 결정**: D42, D45

## D52. Authorship(committer) 정정 판정

- **입력 신호**: 통합된 커밋의 committer가 오케스트레이터 자신인가 위임한 sub-agent인가 · author(저작자 표시)를 보존할 것인가 — **프로젝트 판단, 문서가 확정하지 않음**
- **출력 분기**: 그대로 / `git commit --amend --no-edit`(committer만) / `--reset-author`(author까지) — 단일 커밋 vs `git rebase --exec`
- **서술 위치**: `merge:84` (머지 직후 가드 불변식 2) · `merge:125-134` (§Authorship 확인) · `merge:255` (안티패턴 11) · `merge:278`
- **표현 형태**: 산문 불릿 + 인라인 명령
- **선행 결정**: D40

## D53. 모호한 seam 처리 판정 (isolate-and-continue vs hard stop vs 에스컬레이션)

- **입력 신호**: 모호함이 **spec이 명시적으로 미결 선언한 항목**인가 / **spec이 침묵하는 엣지**인가 · 경계(seam)는 분명하고 내부 구현·외부 계약만 비었는가 · 도메인 의미 결정인가 · 되돌리기 어려운 외부 행위인가
- **출력 분기**: contract/interface 로 격리 + **loud stub**(inert Noop 또는 throw NotImplemented) 두고 전진 + 깃발 기록 / spec 미결이면 hard stop(D11 우선) / 도메인 의미 결정·비가역 행위면 에스컬레이션. **silent fallback 금지**
- **서술 위치**: `autonomous:395-410` (§모호한 seam — 선행 조건 397, 격리 방법 401-402, 가드 404, 여전히 에스컬레이션 406, 적용 범위 410) · `autonomous:386` (spec 미결은 seam이 아님 — 위반 목록) · `autonomous:516` (안티패턴 14) · `autonomous:517` (안티패턴 15의 단서)
  - 주: **SKILL.md 본문에는 이 결정의 서술이 없다**
- **표현 형태**: 산문 불릿
- **선행 결정**: D11, D47

## D54. Task 시스템 적용 판정

- **입력 신호**: 다중 작업인가 · 의존성이 있는가 · 단발 1회 작업인가
- **출력 분기**: `TaskCreate`/`TaskList`/`TaskGet`/`TaskUpdate`로 분리·추적(항상) / 예외는 단발 1회뿐. ad-hoc 메모 대체 금지
- **서술 위치**: `SKILL:30` (사고 모드) · `SKILL:120-122` (§일감을 Task로 분리·관리하는 것은 메인의 핵심 룰) · `SKILL:139` (같은 예외 규칙 재인용) · `monitor:59-81` (§Task 시스템 — 상세 사용법 단일 출처, 의사코드 63-70) · `delegation:202` (경량에서도 유지) · `autonomous:539` (체크리스트) · `branch:43` (t<task-id> ↔ TaskGet id 연결)
- **표현 형태**: 산문 + 의사코드 예시
- **선행 결정**: D01

## D55. Monitor 도구 사용 판정

- **입력 신호**: 긴 빌드/테스트의 stdout을 line-by-line으로 봐야 하는가 · 특정 패턴 즉시 보고가 필요한가 / 단순 완료 대기인가
- **출력 분기**: `Monitor` 사용 / `run_in_background: true` 만으로 충분(대부분)
- **서술 위치**: `SKILL:29` (조율 도구 목록) · `monitor:43-55` (§진행 상황 추적 — 언제 사용 / 언제 사용 안 함) · `monitor:238` (안티패턴 5 Monitor 남발) · `delegation:273` (isolation·Monitor와 같은 절제 원칙)
- **표현 형태**: 산문(언제/언제 안 함 불릿)
- **선행 결정**: 없음

## D56. 경량 경로 산출 경로 계약 vs 브랜치 컨텍스트 선택

- **입력 신호**: 경로 판정 결과(D03) — 무거운 경로면 base epic 브랜치 이름, 경량이면 산출 경로 계약
- **출력 분기**: prompt 필수요소 3·7·9번(브랜치·격리·base 확인) 포함 / 그 자리에 "결과는 응답 텍스트로 반환, 파일 필요하면 repo 밖 scratchpad로 한정" 계약을 넣음
- **서술 위치**: `delegation:51` (필수요소 3번) · `delegation:219-221` (§경로 판정 경계 케이스 — 산출 경로 계약과 그 부재의 결과) · `delegation:217` (경계 케이스 표의 임시 파일 행) · `delegation:448` (체크리스트) · `SKILL:66` (경량 경로의 유지·생략 요지)
- **표현 형태**: 산문 불릿 + 표 행
- **선행 결정**: D03

## D57. 재분해 트리거 판정 (남은 작업 재계산)

- **입력 신호**: `recompute_remaining()` 의 진전 측정(머지된 브랜치 수·통과 테스트 수·종료 조건 충족 항목 수) · 연속 N 루프 무진전인가 · 남은 작업의 전제가 무너졌는가 · 같은 파일 3회 충돌
- **출력 분기**: 메인이 직접 재분해 / 협의체 재소집(전제가 무너졌을 때만) / no-progress → 에스컬레이션 / 분해 자체를 사용자에게 보고
- **서술 위치**: `autonomous:100` (`remaining_work = recompute_remaining()`) · `autonomous:102-104` (no_progress 분기) · `autonomous:342`, `autonomous:346` (가드레일 표의 no-progress + 진전 측정) · `autonomous:432` (에스컬레이션 조건 5) · `autonomous:552` · `council:38` (재분해는 메인이, 전제가 무너지면 협의체 재소집) · `branch:187` (충돌 3회 → 분해 자체 보고) · `branch:190-191`
- **표현 형태**: 의사코드 + 표 + 산문
- **선행 결정**: D08, D42, D48

## D58. 예산·가드레일 값 결정

- **입력 신호**: 런 규모·리스크 · 각 문서의 기본값(`max_redispatch_per_task` 2~3, `max_advisory_consults` 2/비가용 0, 탐색 예산 30, fan-out 재시도 3, 같은 파일 충돌 2/3, `max_council_rounds` 2) · 사용자·계약의 상한
- **출력 분기**: 기본값 채택 / 조정 + 근거를 자율 계약·decision log에 기록. **조정 가능이지 생략 가능이 아님**
- **서술 위치**: `autonomous:26-47` (자율 계약 블록 — 예산 필드들) · `autonomous:334-346` (§가드레일 표 338-344 + 진전 측정 346) · `autonomous:504` (안티패턴 2) · `autonomous:529`, `autonomous:533` · `delegation:108` (탐색 예산 기본 30 + 조정 근거) · `delegation:99` (재디스패치가 같은 예산 소모) · `monitor:173` (재시도 기본 3회) · `monitor:214` (일반 실패는 1회) · `branch:190-191` (충돌 카운터는 task 예산과 별개 + 기본값 조정) · `council:79`, `council:95` (`max_council_rounds` 기본 2) · `advisory:83` (`max_advisory_consults`) · `spec-review:75`, `spec-review:86` (게이트가 같은 예산 소모)
- **표현 형태**: 계약 코드블록 + 표 + 산문
- **선행 결정**: D45, D04, D03

---

## 요약 표

| # | 결정 이름 | 서술 위치 수 (파일 수) | 표현 형태 | 선행 결정 |
|---|---|---|---|---|
| D01 | 스킬 트리거 판정 | 3 (2) | frontmatter 목록 + 산문 | — |
| D02 | 메인 직접 vs 위임 (편집권 경계) | 14 (7) | 산문 + 표 | D01 |
| D03 | **경로 판정 게이트 (경량/무거운)** | **28 (10)** | ASCII 트리 + 표×2 + 의사코드 + 산문 | 체크0 |
| D04 | **왕복 조율(team) 가용 판정** | **28 (10)** | ASCII 트리×3 + 표 + 산문 | 체크0 |
| D05 | spawn 확인 | 11 (4) | ASCII 트리 + 의사코드 + 산문 | D04, D19 |
| D06 | 공유 전제 preflight | 4 (3) | 산문 | D03 |
| D07 | 경로 전환 (경량→무거운) | 5 (3) | 의사코드 + 산문 | D03 |
| D08 | 협의체 소집 여부 | 9 (3) | 산문 + 의사코드 주석 | D01, D04 |
| D09 | 협의체 라운드 종료 판정 | 7 (3) | 의사코드 + 산문 | D08 |
| D10 | 설계 승인 마커 확인 | 2 (2) | 산문 + 코드블록 | D08, D09 |
| D11 | **spec 확정 게이트 (미결 판정)** | **23 (4)** | 표 + 산문 + 의사코드 | — / D31 |
| D12 | 분해 방식 판정 (수직/hot-spot/선행) | 4 (2) | 산문 | D08 |
| D13 | hot-spot 판정 | 13 (5) | 정의 산문 + ASCII 트리 + 코드블록 | D12 |
| D14 | 병렬 vs 순차 판정 | 13 (5) | ASCII 트리×2 + 의사코드 + 산문 | D03, D12, D13 |
| D15 | 조사·감사 병렬 fan-out 기본값 | 5 (2) | 산문 | D14 |
| D16 | 근본원인 swarm 축 분해 | 2 (2) | 표 + 산문 | D15 |
| D17 | 가설 랭킹·반증 진행 | 1 (1) | 표 + 산문 | D16 |
| D18 | 위임 형태 (단발 vs team) | 7 (3) | 표 + ASCII 트리 + 산문 | D04 |
| D19 | team mode 강제 등급 | 14 (7) | 산문 기준 + 표 | D04, D18 |
| D20 | **isolation 유무 결정** | **21 (5)** | 표 + 산문 + 의사코드 | D03, D18 |
| D21 | worktree dispatch 생성 가드 | 9 (3) | 의사코드 + 명령 블록 + 산문 | D20, D14 |
| D22 | 토폴로지 가드 판정 | 17 (6) | 명령 블록 + 산문 + 의사코드 | D03 |
| D23 | 탐색 예산 결정·소진 처리 | 4 (2) | 산문 | D15, D18 |
| D24 | 테스트 인프라 발견 게이트 | 3 (3) | 산문 | D20 |
| D25 | 계획 우선 게이트 (plan-first) | 3 (3) | 의사코드 + 산문 | D18, D20 |
| D26 | **모델 tier 선택 (집행/자문 + 유형)** | **19 (7)** | ASCII 트리 + 표×2 + 산문 | D27, D18 |
| D27 | 자문 소집 여부 (게이트0→트리거→예산) | 16 (5) | ASCII 트리 + 표 + 산문 | D04 |
| D28 | 자문 관점·인원 결정 | 6 (3) | 표 + 산문 | D27 |
| D29 | 자문 권고 처리 (채택/기각/critical) | 11 (3) | 산문 + 의사코드 | D27, D28 |
| D30 | 리뷰어·QA·DBA 게이트 구성 | 10 (5) | 표 + 산문 | D08, D20, D03 |
| D31 | 게이트 verdict + AND 승급 | 14 (3) | 의사코드 + 표 + 산문 | D30, D11 |
| D32 | spec-driven 특수화 진입 | 7 (4) | 산문 | D11, D30 |
| D33 | 재위임 판단 기준 | 12 (4) | 표 + 의사코드 + 산문 | D34,D35,D38,D45 |
| D34 | idle 판정 | 6 (2) | 산문 | D18, D45 |
| D35 | no-op run 감지 | 4 (1) | 산문 | D20 |
| D36 | 중복 보고 판정 (dedup) | 4 (2) | 산문 | — |
| D37 | 증거 계약 수용 판정 | 5 (2) | 산문 | — |
| D38 | fan-out 복원력 (체크포인트/재시도/폴백) | 8 (3) | 산문 + 표 스켈레톤 | D14, D34 |
| D39 | 머지 시점 정책 (배치 vs 즉시+전파) | 11 (5) | 산문 + 코드블록 | D04, D03 |
| D40 | 머지 순서 결정 | 8 (3) | ASCII 트리 + 산문 | D39, D41 |
| D41 | 머지 후보 수집·제외 | 6 (3) | 의사코드 + 산문 | D39, D34 |
| D42 | 충돌 처리 (위임/보고/재분해/에스컬) | 15 (4) | 의사코드 사다리×2 + 산문 | D40, D45, D13 |
| D43 | epic ← main 역방향 drift 흡수 | 6 (3) | 명령 블록 + 산문 | D44 직전 |
| D44 | 최종 통합 검증 / 완료 선언 | 8 (3) | 번호 절차 + 산문 | D43, D42 |
| D45 | 자율 vs HITL 모드 판정 | 13 (4) | 산문 + 표 | D01 |
| D46 | SendMessage 명령 주입 허용 | 8 (4) | 산문 인용구 + 불릿 | D45, D04 |
| D47 | 에스컬레이션 판정 (8조건) | 14 (7) | ASCII 트리 + 산문 | 다수 |
| D48 | 종료 조건 충족 판정 (done_when) | 13 (4) | 산문 + 의사코드 | D31,D44,D49,D58 |
| D49 | integration_verify (인프라 테스트 분리) | 9 (3) | 계약 블록 + 산문 | D45, D58 |
| D50 | 3분류 종료 판정 (DONE/BLOCKED/NS) | 5 (2) | 표 + 산문 | D48, D47 |
| D51 | worktree 정리 판정 | 8 (3) | 표 + 산문 | D42, D45 |
| D52 | Authorship(committer) 정정 | 4 (1) | 산문 + 명령 | D40 |
| D53 | 모호한 seam 처리 (stub vs stop) | 4 (1) | 산문 | D11, D47 |
| D54 | Task 시스템 적용 판정 | 7 (4) | 산문 + 의사코드 | D01 |
| D55 | Monitor 도구 사용 판정 | 4 (3) | 산문 | — |
| D56 | 경량 산출 경로 계약 vs 브랜치 컨텍스트 | 5 (2) | 산문 + 표 행 | D03 |
| D57 | 재분해 트리거 판정 | 9 (3) | 의사코드 + 표 + 산문 | D08, D42, D48 |
| D58 | 예산·가드레일 값 결정 | 15 (6) | 계약 블록 + 표 + 산문 | D45, D04, D03 |

**서술 위치 3곳 이상인 결정: 48개 / 58개 (83%)** — 산포는 예외가 아니라 이 스킬의 기본 상태다.

---

## 부록 A. 절차/계약으로 분류한 것 (결정 아님)

### A1. 절차 (순서대로 하는 일)

| 항목 | 위치 | 비고 |
|---|---|---|
| 표준 절차 8단계 (진입→분해→위험도→계획→위임→모니터→게이트→머지→보고) | `SKILL:105-116` | 순서 서술. 각 단계 안의 분기가 D03·D08·D14·D18·D30·D40 |
| 진입 절차 전체 흐름 (체크 0→경로 게이트→체크1·2·3→체크4→체크5) | `SKILL:38-101` | 순서. 각 체크의 판정이 D03·D04·D06·D20 |
| 자율 실행 루프 의사코드 | `autonomous:56-107` | while 골격. 안의 분기가 D31·D42·D47·D57 |
| 협의체 루프 골격 | `council:70-93` | 판정은 D09 |
| 자문 소집 절차 의사코드 | `advisory:116-144` | 판정은 D27·D28·D29 |
| 병렬 dispatch 패턴 (스냅샷→dispatch→생성 확인 반복) | `worktree:43-73` | 직렬화는 규칙, 위반 처리 분기는 D21 |
| 머지 표준 절차 8단계 | `merge:49-97` | 순서. 분기는 D40·D41·D42·D44 |
| 최종 통합 검증 게이트 4단계 | `merge:142-151` | 판정은 D44 |
| 토폴로지 가드 복구 절차 (rebase --abort → checkout → pull → 브랜치 삭제 → stash) | `merge:112-119` | 판정은 D22 |
| 충돌 위험 사전 분석 4단계 (Glob→Grep→git log --stat→의존성 그래프) | `worktree:118-130` | 판정은 D14 |
| 체크리스트 전부 | `SKILL` 없음 · `delegation:443-461` · `autonomous:521-564` · `monitor:242-266` · `merge:259-286` · `branch:208-225` · `worktree:148-166` · `advisory:267-283` · `council:161-174` · `spec-review:122-140` | 결정의 사후 검증 도구 |

### A2. 계약 (출력 형식·필수 포함 요소)

| 항목 | 위치 | 비고 |
|---|---|---|
| 조율 도구 스키마 확보(체크 0)의 `ToolSearch` 호출 | `SKILL:49-51` · `delegation:188` · `autonomous:526` · `monitor:87` | "1회만 한다"는 절차. 미확보의 **결과**가 D04의 0단계 |
| Prompt 작성 원칙 — 필수 포함 요소 13개 | `delegation:47-61` | 계약. 단 3번(브랜치 vs 산출 경로)은 D56의 분기 |
| 보고 채널 계약 (`SendMessage({to:"main"})`) + 없을 때의 결과 표 | `delegation:59`, `delegation:63-74` · `SKILL:224` · `monitor:39`, `monitor:247` | 계약 |
| 자율 계약(autonomy contract) 필드 정의 | `autonomous:24-48` | 형식은 계약, 각 필드 **값**의 결정이 D45·D58·D03·D11·D49 |
| decision log 기록 위치·시점·형식 (+ 필수 등급 2필드) | `autonomous:260-330` · `advisory:152-160` | 계약 |
| 종료 핸드오프 기록 위치·성립 기준 | `autonomous:470-482` | 판정 자체는 D50 |
| 자문 입력 패킷 계약 / 출력 계약 | `advisory:167-182` / `advisory:191-211` | 계약 |
| Task 도출 계약 (id·범위·예상 파일·의존성·위험도·검증 기준·DB 접촉) | `council:102-114` | 계약. DB 접촉 필드가 D30의 입력 |
| 설계 승인 마커 필수 필드 | `council:129-134` | 계약. 게이트 동작이 D10 |
| 브랜치 네이밍 규약 `epic/<name>/t<task-id>-<slug>` | `branch:27-46` · `worktree:27` · `merge:57` | 계약 |
| **머지 방식 rebase 후 `--ff-only`** | `branch:136-157` | 원문이 "판정하지 않는다"(`branch:138`)고 명시 → 결정이 아니라 고정 계약 |
| 머지 직후 가드의 불변식 목록 (branch==epic / committer / in-flight rebase) | `merge:80-86` | "새 불변식은 이 목록에 한 줄 추가" — 목록 계약. 각 판정이 D22·D52·D39 |
| 보고 형식 (진입/진행/종료, 머지 결과, 정체·실패 보고, fan-out 상태 표) | `SKILL:201-208` · `autonomous:486-497` · `merge:218-239` · `monitor:119-132` · `monitor:190-193` | 계약 |
| 안티패턴 목록 전부 | `SKILL:210-230` · 각 reference 말미 | 결정의 위반 사례 서술 — 대부분 해당 결정 항목에 위치로 이미 포함 |

---

## 부록 B. 분류가 애매했던 항목과 판단

1. **머지 방식(rebase 후 ff)** — 형태상 "두 갈래 중 선택"처럼 보이나 `branch:138`이 **"판정하지 않는다"**고 못 박아 고정 계약. → 계약으로 분류.
2. **토폴로지 가드 / dispatch 생성 가드** — 이름은 "확인"이지만 위반 시 출력이 2~3갈래(복구+에스컬레이션 / 중단+정지 / cwd 복귀 후 재검증)라 분기가 실재. → **결정**(D21·D22). 명령 블록과 복구 절차 부분만 계약으로 분리.
3. **spawn 확인** — 절차 트리처럼 보이나 최종 출력이 "진행 / 재소집 / 비가용 처리" 3갈래. → **결정**(D05).
4. **체크 0(ToolSearch 스키마 확보)** — 그 자체는 무조건 실행하는 절차이고 분기가 없다. 다만 **결과가 D04의 0단계 입력**. → 절차(부록 A2)로 두고 D04의 선행으로 표기.
5. **자율 계약 필드** — 필드 스키마는 계약이지만 각 필드의 **값**(예산 수치, 경로, spec_resolved, integration_verify 유무)은 판단. → 계약(형식) + D58/D03/D11/D49(값)로 쪼갬.
6. **Prompt 필수 포함 요소 13개** — 계약이지만 3번은 경로에 따라 내용이 갈리므로 그 분기만 D56으로 뽑음. 나머지 12개는 계약.
7. **경로별 유지·생략 표(`delegation:199-207`)** — 결정이 아니라 D03의 **출력 매핑**. D03에 귀속.
8. **Monitor 사용 판정(D55)** — 매우 얕은 분기(2갈래)라 결정으로 셀지 애매했으나, "언제 사용 / 언제 사용 안 함"이 명시적 분기라 포함. 재설계 시 비중은 최하위로 봐도 된다.
9. **D12(분해 방식)과 D13(hot-spot)** — `branch:50-93`이 두 결정에 걸쳐 있다. 분해 시점의 "hot-spot을 별도 task로 뽑는다"(D12)와 병렬 판정 시점의 "이 파일이 hot-spot인가"(D13)는 시점·입력이 달라 분리했다.
10. **D57(재분해 트리거)** — D48(종료 판정)·D42(충돌 3회)·D08(협의체 재소집)에 조각으로 흩어져 어느 하나에 넣기 어려웠다. "루프를 한 번 더 도는가 / 분해로 되돌아가는가"라는 독립 분기이므로 별도 결정으로 세웠다.
11. **`integration_verify`(D49)와 `모호한 seam`(D53)** — 둘 다 **SKILL.md 본문에 요지 절이 없다**. 다른 결정들이 SKILL.md에 요지를 갖는 것과 대비되는 비대칭이라, 재설계 시 "SKILL 요지 누락"으로 별도 표시할 가치가 있다.
