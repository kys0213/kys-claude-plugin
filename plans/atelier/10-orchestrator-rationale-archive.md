# orchestrator 재구조화 — 배경 문장 아카이브와 대조 기록

**이 문서는 plan 계층이다** (CLAUDE.md §문서 계층). 2026-09-08 의 orchestrator 스킬 결정그래프 재구조화에서, 재구조화 **전** 문서에 있었으나 규칙이 아니라 "왜 그렇게 결정했는가"의 배경·관찰이라 새 구조에 자리가 없던 문장의 **원문 보존소**다.
현재 오케스트레이터가 어떻게 동작해야 하는지는 여기서 판단하지 않는다 — 그 신뢰 소스는 `plugins/atelier/skills/orchestrator/` 의 코드(문서)와 `.claude/rules/decision-graph.md` 다. 이 문서와 현실이 어긋나도 고치지 않는다.
인용문은 전부 **재구조화 전 원문**이며, 그 시점의 표기·용어·행 번호를 그대로 둔다. 지금의 규칙 문안과 다른 것이 정상이다.

**관련 문서**

| 문서 | 성격 |
|---|---|
| `plans/atelier/09-orchestrator-graph-restructure.md` | 설계 — 3안 비교·버린 이유·모순 13건 판단·심문 findings 처리·구현 순서 |
| `plans/atelier/09a-orchestrator-decision-inventory.md` | 조사 A 원문 (D01–D58 과 선행 결정) |
| `plans/atelier/09b-duplication-map.md` | 조사 B 원문 (참조 그래프·중복 31군·모순 X1–X13) |
| `plans/atelier/09c-rationale-inventory.md` | 조사 C 원문 (근거 92·안티패턴 91·체크리스트 142 이관 판정) |
| `plans/atelier/09d-external-contracts.md` | 조사 D 원문 (외부 참조 28·validator 제약) |
| `.claude/rules/decision-graph.md` | rules — 결정그래프 작성 규약 (현재형 지침, 이 문서와 역할이 다르다) |

---

## 1. 이 아카이브가 담는 것

설계 §6.3 은 재구조화 전 문서의 근거·관찰 문장에 세 플래그를 붙였다. `RULE`(규칙 요소로 흡수) · `LINE`(소유 항목에 근거 1줄로 축약) · `PLAN`(원문을 아카이브로). 한 문장이 두 플래그를 동시에 갖는 것이 정상이며, `PLAN` 이 "흔적 없는 삭제"가 되지 않도록 writer 는 `LINE` 동반 검토 기록을 함께 남겼다.

§2 는 그 `PLAN` 문장 **49건 전부**를 출처와 함께 담는다. 각 항목은 `A##` 로 번호를 매기고, "배경이 설명하는 현재 항목"에 그 배경이 뒷받침하던 규칙의 **현재 소유 id** 를 적는다 — 원문을 읽을 때 지금 어느 항목의 뒷이야기인지 찾아가라는 색인이지, 그 항목의 내용을 이 문서가 정의한다는 뜻이 아니다.

---

## 2. PLAN 인용 아카이브 (49건)

### 2.1 색인

출처 파일은 전부 **재구조화 전** 경로다. `delegation-patterns.md` · `branch-strategy.md` · `advisory-consult.md` · `spec-driven-review.md` 네 파일은 T16(`0b7969a`)에서 삭제됐고, 나머지는 같은 이름으로 개편됐다. 원문과 행 번호는 **재구조화 전 커밋 `a9349aa` 트리**에서 확인한다 (해체 4파일은 재구조화 중 편집되지 않았으므로 `0b7969a` 직전 트리까지 내용이 동일하다).

인용은 flags 산출물이 옮겨 적은 형태를 그대로 따른다 — 굵게·불릿·따옴표 등 표기가 원문 마크다운과 미세하게 다를 수 있으며, 여러 줄 인용은 평문화해 실었다.

| # | 출처 (재구조화 전) | 배경이 설명하는 현재 항목 | 기록한 writer |
|---|---|---|---|
| A01 | `references/delegation-patterns.md:63-74` | C03 (보고 채널 계약) | T3 |
| A02 | `references/delegation-patterns.md:80-83` (본문 `:82`) | D20 (isolation 유무) — R7 | T3 |
| A03 | `references/delegation-patterns.md:84-94` | C02 10번 (금지에는 출구를) | T3 |
| A04 | `references/delegation-patterns.md:161-164` | D14 (병렬 vs 순차 판정) | T3 |
| A05 | `references/delegation-patterns.md:245` | D18 (위임 형태 결정) | T3 |
| A06 | `references/delegation-patterns.md:346` | D04 (왕복 조율 가용 판정) | T4 |
| A07 | `references/architect-council.md:10` | D08 (협의체 소집 여부) · 파일 스코프 | T5 |
| A08 | `references/architect-council.md:14` | P04 (협의체 루프 골격) · D08 | T5 |
| A09 | `references/architect-council.md:22` (뒷 절반) | D19 (team mode 강제 등급) | T5 |
| A10 | `references/delegation-patterns.md:100` | D37 (증거 계약 수용 판정) | T6 |
| A11 | `references/delegation-patterns.md:172` (O13) | D15 (조사·감사 병렬 fan-out 기본값) | T6 |
| A12 | `references/model-routing.md:38` | D26 (집행 tier 선택) | T7 |
| A13 | `references/advisory-consult.md:28-33` | D19 · D27 (자문 소집 여부) | T7 |
| A14 | `references/advisory-consult.md:112` | D28 (자문자 관점 분할) | T7 |
| A15 | `references/advisory-consult.md:187` (뒷 문장) | C15 (자문 계약 — 입력 패킷) | T7 |
| A16 | `references/advisory-consult.md:219-220` | D29 (자문 권고 처리 판정) | T7 |
| A17 | `references/advisory-consult.md:254-255` (O28) | C15 (역할 제한) | T7 |
| A18 | `references/advisory-consult.md:256-260` (O15) | D04 | T7 |
| A19 | `references/worktree-lifecycle.md:79` (O1) | D21 (worktree dispatch 생성 가드) | T8 |
| A20 | `references/worktree-lifecycle.md:81-82` | P06 4·5 · D21 (A19 의 종결부 맥락) | T8 |
| A21 | `references/worktree-lifecycle.md:143` (O6) | D22 (토폴로지 가드) | T8 |
| A22 | `references/worktree-lifecycle.md:144` (O4) | D21 | T8 |
| A23 | `references/agent-monitor.md:23-26` (O25) | D46 (명령 주입 · 정체 시 반응 판정) | T9 |
| A24 | `references/agent-monitor.md:136` (뒷 문장, O22) | D36 (중복 보고 판정) | T9 |
| A25 | `references/agent-monitor.md:143` (뒷 문장, O23) | D34 (idle 판정) | T9 |
| A26 | `references/agent-monitor.md:217` (뒷 문장) | D33 (재위임 판단 기준) | T9 |
| A27 | `references/spec-driven-review.md:19` | D31 · D32 (게이트 범위) | T10 |
| A28 | `references/spec-driven-review.md:27` | D32 (spec 입력 특수화 진입 판정) | T10 |
| A29 | `references/spec-driven-review.md:69` | P11 (spec-driven review→improve 루프) | T10 |
| A30 | `references/delegation-patterns.md:115` (앞, O16) | D24 (테스트 인프라 발견 게이트) | T10 |
| A31 | `references/merge-coordinator.md:140` | D44 (최종 통합 검증 / 완료 선언 판정) | T11 |
| A32 | `references/branch-strategy.md:150` | C11 (머지 방식 — rebase 후 `--ff-only`) | T11 |
| A33 | `references/branch-strategy.md:151` | C11 | T11 |
| A34 | `references/autonomous-driving.md:129` (O26) | D02 (편집권 경계) · C05 | T12 |
| A35 | `references/autonomous-driving.md:239` (뒷 문장) | D39 (머지 시점 정책 판정) | T12 |
| A36 | `references/autonomous-driving.md:243` (O2) | D21 | T12 |
| A37 | `references/autonomous-driving.md:283` (뒷 절반) | C05 (의사결정 기록) | T12 |
| A38 | `references/autonomous-driving.md:366` (O18) | D11 (spec 확정 게이트) | T12 |
| A39 | `references/autonomous-driving.md:408` (O19) | D53 (모호한 seam 처리 판정) | T12 |
| A40 | `references/autonomous-driving.md:445` | D47 (에스컬레이션 판정) | T12 |
| A41 | `references/autonomous-driving.md:468` | D50 (3분류 종료 판정) | T12 |
| A42 | `references/autonomous-driving.md:509` (O3) | D21 · D22 | T12 |
| A43 | `references/autonomous-driving.md:167-168` | D48 (종료 조건 충족 판정) | T12 |
| A44 | `references/autonomous-driving.md:352` (앞 문장) | D11 | T12 |
| A45 | `references/autonomous-driving.md:141` | D18 · D20 | T12 |
| A46 | `SKILL.md:19` | D01 (트리거 판정) | T13 |
| A47 | `SKILL.md:149` | D12 (분해 방식 판정) · D14 | T13 |
| A48 | `SKILL.md:171` | D18 · D20 | T13 |
| A49 | `SKILL.md:32` | C01 (조율 도구 스키마 확보) | T13 |

**가정**: 지시서는 원문을 색인 표의 한 열로 요구했으나, 인용 중 여럿이 여러 줄(표 4행·코드블록·인용구)이라 마크다운 표 셀에 원형 그대로 들어가지 않는다. 색인 표(위)와 원문 본문(아래)으로 나눴다. 색인 행 수 = 원문 항목 수 = 49 로 일치한다.

### 2.2 원문 — T3 (`references/dispatch-planning.md` writer)

**A01** — `delegation-patterns.md:63-74` → C03. 규칙판은 C03 의 근거 1줄이 갖는다. 표 4행과 "소집해놓고 대화가 안 되는" 문단이 여기 남는다.

> ### 보고 채널을 빼면 생기는 일 (11번의 근거)
>
> 최종 완료 알림에는 agent의 마지막 출력이 실려 오므로 **짧은 단발 작업은 채널이 없어도 성공한 것처럼 보인다.** 그래서 이 결함은 조용하다. 유실되는 것은 완료 이전의 전부다:
>
> | 상황 | 채널 없음 | 채널 있음 |
> |------|-----------|-----------|
> | 중간 진행 보고 | 유실 — 메인에는 침묵으로 보임 | 도착 |
> | 막혔을 때의 질문 | 유실 → agent가 임의로 가정하고 진행 | 도착 → 메인이 결정 |
> | 부분 결과 (긴 작업) | 완료까지 아무것도 안 옴 | 점진 도착 |
> | 왕복 조율 (필수 등급 경로) | **성립 불가** | 성립 |
>
> 특히 필수 등급 경로(자문·협의체)는 왕복이 실질이므로, 상대에게 답신 채널을 주지 않으면 **소집해놓고 대화가 안 되는** 상태가 된다 — team이 가용한데도 결과적으로 단발과 같아진다. 이때의 증상은 에러가 아니라 "agent가 답을 안 한다"이므로 원인을 찾기 어렵다.

**A02** — `delegation-patterns.md:80-83` (본문은 `:82`) → D20 종단 계약(R7). 앞 두 문장은 R7 로 승격했고, 마지막 문장이 배경으로 남는다.

> **Edit 절대경로 트랩**: worktree 격리 sub-agent라도 Edit tool의 file_path가 부모 repo의 절대경로를 가리키면 격리를 우회해 메인 working tree가 직접 수정된다. Bash cwd가 worktree여도 Edit는 별개 경로 판정이므로, prompt에 worktree 격리 준수(위 7번)를 반드시 명시한다. 부모 repo가 변형되면 메인 branch switch까지 이어질 수 있다.

**A03** — `delegation-patterns.md:84-94` → C02 10번의 배경. LINE 검토 결과 D25·C02 어느 근거 슬롯에도 맞지 않았다(위임 사례이지 판정 근거가 아니다).

> ### 적용 사례 — 산문 → 블록 변환 위임
>
> 위 10번(금지에는 출구를)이 실제로 걸리는 자리 하나. 산문은 모호한 채로 존재할 수 있지만 블록(트리·우선순위 블록)은 그럴 수 없어, **변환 행위 자체가 확정을 강요**한다. 따라서 "모호하면 확정하지 마라"는 금지에 붙일 출구를 함께 준다 — 블록 밖 "미확정" 항목:
>
> ```
> - **<조건/항목>의 미확정 지점**: 원문은 …인데, 이것이 A인지 B인지는 원문이 정하지 않았다.
>   여기서 확정하지 않는다.
> ```

**A04** — `delegation-patterns.md:161-164` → D14. 비대칭 손해 한 줄로 축약했고 3불릿 원문이 남는다.

> 판단 근거:
> - **병렬의 이득**: 시간 단축, 독립 컨텍스트
> - **병렬의 비용**: 머지 시 충돌 → 사람 개입 필요
> - **기본 규칙**: 의심스러우면 순차. 병렬은 disjoint가 명백할 때만.

**A05** — `delegation-patterns.md:245` → D18. LINE 검토했으나 D18 근거 슬롯 1개를 단발/team 판정 근거가 이미 점유했다.

> 근거: 구현 agent가 스스로 중첩 재위임 체인을 만들면 오케스트레이터가 위임 트리 전체를 파악하지 못해 상태 추적·개입 경로를 잃는다. 위임 깊이를 평평하게 유지해야 오케스트레이터가 각 agent를 직접 추적·중단할 수 있다.

### 2.3 원문 — T4 (`references/entry-gates.md` writer)

**A06** — `delegation-patterns.md:346` → D04 agentId 종단 계약. 규칙판("`name` 부재를 비가용으로 처리하는 것은 금지한다 — 반환값의 `agentId` 를 보관해 같은 왕복을 세운다")은 D04 가 갖는다.

> **`name`이 없는 런타임에서도 team은 성립한다.** `Agent` 스키마에 `name`이 없으면 `run_in_background: true`로 띄우고 **spawn 결과의 `agentId`(`a...` 형식)를 지목자로 쓴다** — 그 agent에 `SendMessage`를 보내면 직전까지의 transcript에서 재개되므로 라운드 간 맥락이 유지된다. 이름이 있으면 이름이 낫다(완료된 agent에도 계속 유효하고, 읽기 쉽다). 없다고 비가용으로 처리하면 필수 등급 경로가 이유 없이 죽는다.

### 2.4 원문 — T5 (`references/architect-council.md` writer)

**A07** — `architect-council.md:10` → 파일 스코프 1줄 + D08. LINE 채택("분해를 세우고 검증하는 국면의 항목을 소유한다").

> 오케스트레이터 표준 절차의 **1단계 분해(Decompose)를 특수화**하는 패턴이다. 복잡하거나 모호한 요구를 메인이 혼자 쪼개는 대신, **설계를 생성하는 자세**와 **설계를 심문하는 자세**를 서로 다른 sub-agent에 주입해 **검증된 task 목록**을 도출한다. CLAUDE.md의 설계 최우선 원칙(요구사항 정리 → 사이드이펙트 조사 → 설계)을 스웜의 분해 단계에 강제하는 장치다.

**A08** — `architect-council.md:14` → P04 서두 + D08 근거. 앞 절반은 P04 규칙으로, 근거 절반은 D08 근거 1줄로 남겼다.

> 과정의 **골격은 고정**하고(Template Method), 그 자리에 들어갈 **역할은 문제에 따라 런타임 주입**한다(Strategy/DI). 골격을 고정하는 이유는 검증 없는 분해가 스웜 전체를 잘못된 방향으로 밀기 때문이고, 역할을 고정하지 않는 이유는 어떤 관점이 필요한지가 문제마다 다르기 때문이다.

**A09** — `architect-council.md:22` (뒷 절반) → D19. 단일 출처가 `delegation:324-325`(= D19)라 이 파일에 근거 1줄을 남기면 3중 사본의 3번째가 된다. LINE 미채택.

> 심문은 라운드 간 맥락이 이어져야 성립하는데 단발 subagent는 직전 라운드를 기억하지 못해 상호 반박 자체가 불가능하고(대체 불가), 두 자세 모두 read-only 분석이라 강제 비용도 없기 때문이다(아래 §두 자세)

### 2.5 원문 — T6 (`references/investigation.md` writer)

**A10** — `delegation-patterns.md:100` → D37. 종단 계약에는 "검색 축을 바꾼다(패턴 검색 → 이름 기반 탐색 → 추적 목록 조회)"로 일반화해 남겼고, 구체 명령 예시가 여기 남는다.

> - **부재 주장(negative claim)은 교차 검증 후에만 수용한다.** "X가 없다"는 보고는 검색 명령 하나가 틀리면(깨진 glob 등) 통째로 뒤집힌다. 서로 다른 검색 전략 2개 이상으로 재확인한 뒤에만 수용한다 — 예: `rg <패턴>` (glob 없이) → `find`(이름 기반) → `git ls-files | grep`. 같은 전략의 반복은 교차 검증이 아니다.

**A11** — `delegation-patterns.md:172` (O13) → D15. 규범은 D15 종단 계약("기록 없이 순차 탐색으로 시작하는 것은 금지한다")이 갖는다.

> "일단 훑어보자"로 메인이 순차 탐색을 시작하면 사용자가 중단하고 병렬로 재지시하는 반복 마찰이 생긴다

### 2.6 원문 — T7 (`references/model-routing.md` writer)

**A12** — `model-routing.md:38` → D26. 규칙 절반("모델명 대신 역량 수준만 쓴다")은 D26 종단 계약에 남았다.

> **모델명을 안 박는 근거**: 세대가 바뀌어도 이 원칙이 그대로 성립해야 한다. 역량 수준 ↔ 실제 모델명 매핑은 dispatch 시점 판단에 맡긴다.

**A13** — `advisory-consult.md:28-33` → D19 · D27. LINE 미채택 — `delegation:324-325` 가 등급 기준의 단일 출처라 중복이 된다.

> 자문은 본질이 왕복이다 — 권고 → 메인 반문 → 재답변. 단발 subagent 는 매 왕복마다 패킷을 재합성해야 하고 직전 라운드를 기억하지 못한다. 협의체 tie-break 로 투입할 때는 더 분명해서, 생성자·검증자와 **같은 대화 공간**에 있어야 상호 반박이 성립한다. team 의 격리 특성과 spawn 패턴은 `delegation-patterns.md §Agent team 사용 패턴`이 단일 출처다.

**A14** — `advisory-consult.md:112` → D28. 뒤 절반만 D28 근거 1줄로 축약했다.

> 이게 흔들리면 자문이 실질 오케스트레이터가 된다 / 어떤 관점이 필요한지는 문제마다 다르다

**A15** — `advisory-consult.md:187` (뒷 문장) → C15 입력 패킷. 앞 절반("긴 근거는 경로로")은 규칙으로 남았다.

> 메인이 전문을 패킷에 복사하면 메인 컨텍스트가 포화된다

**A16** — `advisory-consult.md:219-220` → D29. "고무도장 금지"는 D29 근거 1줄로 유지했다.

> **기각도 정당한 결과다** — 상위 tier 라는 이유로 자동 채택하면 메인이 고무도장이 된다

**A17** — `advisory-consult.md:254-255` (O28) → C15. C15 근거가 "없는 보장을 있다고 적지 않는다"로 이미 규칙화했다.

> '정의 파일로 도구를 제한했다'는 문장을 문서에 쓰는 것도 같은 실패다 — 없는 보장을 기록하면 다음 사람이 그것을 믿고 가드를 생략한다

**A18** — `advisory-consult.md:256-260` (O15) → D04. 가용 판정은 D04 소유라 이 파일에 근거 자리가 없었다.

> **신호 하나로 비가용 단정**: `printenv` 가 비었다거나 `Agent` 스키마에 `name` 이 없다는 이유만으로 자문 경로를 닫음 → Bash 는 메인과 다른 프로세스라 env 의 false negative 가 구조적으로 발생하고, `name` 부재는 애초에 비가용의 근거가 아니다(`agentId` 로 같은 왕복을 한다). 그 한 번의 오판이 세션 내내 자문을 죽인다. 권위 신호는 **`SendMessage` 로 다시 지목 가능한지**다 (`SKILL.md §진입 시 체크 4`).

### 2.7 원문 — T8 (`references/worktree-lifecycle.md` writer)

**A19** — `worktree-lifecycle.md:79` (O1) → D21. D21 근거 1줄("완료 알림 시점의 토폴로지 가드는 유출이 이미 커밋된 뒤라 늦다")로 축약했다.

> 동일 tool-call batch에서 worktree-isolated agent를 2개 이상 동시 생성하면, worktree 생성이 직렬화되지 않아 한쪽이 누락되는 race가 알려져 있다. worktree를 받지 못한 agent는 **메인 working tree에서 직접 편집·커밋**해 격리 계약이 깨지고, 메인 shell cwd가 다른 agent의 worktree로 drift하는 부수효과도 관찰됐다. 완료 알림 시점의 토폴로지 가드로는 늦다 — 유출이 이미 커밋된 뒤다. 그래서 dispatch 시점에 두 가지를 지킨다:

**A20** — `worktree-lifecycle.md:81-82` → P06 4·5 · D21 입력 신호. A19 문장의 종결부가 가리키는 두 항목이라 맥락 보존용으로 함께 남긴다. 규칙 본문은 P06 과 D21 이 갖는다.

> 1. **직렬화**: worktree dispatch는 **한 메시지에 하나씩**. 이전 dispatch의 worktree 생성을 확인한 뒤에야 다음을 dispatch한다. 병렬성은 잃지 않는다 — `run_in_background: true` agent는 dispatch가 직렬이어도 실행은 병렬이다.
> 2. **생성 검증**: 매 dispatch 직후 메인이 직접 Bash로 확인한다:

**A21** — `worktree-lifecycle.md:143` (안티패턴 6, O6) → D22. 같은 관찰의 3번째 사본이라 근거 슬롯이 이미 차 있었다.

> **완료 알림 후 가드 생략**: sub-agent 완료 직후 메인 branch/status 확인 없이 다음 단계 진행 → sub-agent의 격리 이탈(Edit 절대경로 트랩 등)로 변형된 메인 state 위에서 후속 작업이 진행됨.

**A22** — `worktree-lifecycle.md:144` (안티패턴 7, O4) → D21. 같은 관찰의 4번째 사본.

> **단일 batch 다중 worktree dispatch**: 한 메시지(tool-call batch)에 worktree-isolated dispatch를 2개 이상 → 생성 race로 한쪽 worktree가 누락되고 그 agent의 편집이 메인 트리로 유출. 직렬 dispatch + 생성 가드 필수 (§dispatch 생성 가드).

### 2.8 원문 — T9 (`references/agent-monitor.md` writer)

**A23** — `agent-monitor.md:23-26` (O25) → D46. LINE 미채택 — 같은 근거가 `:104`·`:106` 에 압축돼 있어 D46 근거 1줄과 중복이다. `monitor:100-104` 앞 문장("금지의 근거는 둘이다")도 이 관찰의 사본이라 여기에 함께 접는다.

> 자동 개입을 피하는 이유: LLM 판단으로 sub-agent에 명령을 주입하면 폭주/루프 위험 / 사용자가 의도와 다른 방향으로 진행되는 걸 모를 수 있음 / 보고 위주가 안전하고 추적 가능

**A24** — `agent-monitor.md:136` (뒷 문장, O22) → D36. 앞 절반이 이미 D36 근거 1줄("같은 내용을 두 번 취합하면 수치·상태가 이중 계산된다")을 차지했다.

> 동일 보고 재전송이 idle 알림 중복과 겹치면 조사 사이클 하나가 통째로 낭비된다

**A25** — `agent-monitor.md:143` (뒷 문장, O23) → D34. 앞 절반이 D34 근거 1줄을 차지했다.

> 감으로 판정하면 폴백 판단이 늦어, 대기 시간만 소모한 끝에 결국 메인이 직접 분석하게 된다

**A26** — `agent-monitor.md:217` (뒷 문장) → D33. 앞 절반(`agentId` 재개)은 D33 공통 종단 계약으로 남았다.

> 직전 실패 맥락이 이미 그 agent에 남아 있으면 새 자기완결 prompt를 처음부터 짜는 비용을 던다

### 2.9 원문 — T10 (`references/review-gates.md` writer)

**A27** — `spec-driven-review.md:19` → D31 · D32 의 범위 배경. LINE 동반 검토했으나 D31/D32 판정에 분기를 주지 않아 근거 슬롯을 쓰지 않았다.

> **범위**: 이 문서는 *자율 루프 안에서 머지 전 게이트*로 spec 적합성을 검증하는 것만 다룬다. 판정은 **게이트 수준의 `pass`/`reject`/`spec-unresolved`**이며, 그 이상의 전수 갭 분석(spec 전체 ↔ 코드베이스 전체 대조)은 이 게이트의 일이 아니다 — 필요하면 별도 작업으로 분리해 위임한다.

**A28** — `spec-driven-review.md:27` → D32. 발화 예는 근거가 아니라 사례라 LINE 미채택.

> 트리거 발화 예: "spec 대로 구현하면서 검토자랑 QA 매니저 붙여서", "spec 기반 구현 자율주행", "구현이 spec 맞는지 / 테스트가 spec 맞는지 계속 봐줘".

**A29** — `spec-driven-review.md:69` → P11. 같은 근거의 두 사본 중 `:59` 쪽이 P11 근거 1줄을 차지했다.

> team이 비가용이면 **단발 격리/read-only subagent 2개**로 같은 두 게이트를 돈다 — 검토자 subagent + QA 매니저 subagent를 병렬 dispatch하고, 거부 시 이전 findings를 새 구현 prompt에 자기완결적으로 실어 재위임한다. 이 게이트 안에서는 의심스러우면 폴백을 고른다 — 게이트의 본질은 team이 아니라 두 검증 차원이기 때문이다.

**A30** — `delegation-patterns.md:115` (앞, O16) → D24. 압축분이 D24 근거 1줄로 남았고, 구체 실패 사례가 여기 남는다.

> 레포에 테스트 하네스가 이미 있어도 구현 agent가 발견하지 못하면 ad-hoc 검증(예: SQL 문자열 assertion)으로 흘러가고, 가장 비싼 시점(QA 게이트)에서야 reject로 걸린다.

### 2.10 원문 — T11 (`references/merge-coordinator.md` writer)

**A31** — `merge-coordinator.md:140` → D44. 같은 근거의 두 문장 중 `:153` 이 D44 근거 1줄을 차지했다.

> 모든 머지 후보가 epic 브랜치로 통합된 뒤에도, 개별 worktree가 각자 green이었다는 사실이 머지 결합 후 회귀가 없음을 보장하지는 않는다.

**A32** — `branch-strategy.md:150` → C11. rebase 고정의 부수 이득이라 계약 본문에는 남기지 않았다.

> epic 히스토리가 선형이라 `git log --oneline epic/<name>` 이 그대로 task 단위 진행 기록이 된다

**A33** — `branch-strategy.md:151` → C11. 같은 사유.

> 최종 통합 검증 게이트가 red일 때 revert·이분 탐색의 단위가 task와 일치한다

### 2.11 원문 — T12 (`references/autonomous-driving.md` writer)

**A34** — `autonomous-driving.md:129` (O26) → D02 · C05. 대응 규칙("메인은 압축 요약만 수령")은 재구조화 중 소유자가 없다가 T13 이 D02 종단 계약으로 흡수했다. LINE 미채택.

> 긴 자율 루프에서 메인이 매 작업의 파일 내용·전체 diff·리뷰 전문을 자기 컨텍스트에 쌓으면, 루프가 길어질수록 메인 컨텍스트가 포화되어 조율 판단 품질이 떨어진다. 자율 모드에서 메인은 **조율에 필요한 최소 상태만** 보유한다.

**A35** — `autonomous-driving.md:239` (뒷 문장) → D39. 대응 규칙(배치 머지 기본값)이 D39/D41 소유라 여기 근거를 남기면 재서술이 된다. LINE 미채택.

> 자율 모드는 in-flight가 남아 있어도 보고 없이 진행하므로 **전파 누락이 조용히 누적된다** — 배치 머지 기본값이 여기서 특히 중요하다.

**A36** — `autonomous-driving.md:243` (O2) → D21. O1 의 LINE 은 T8(`worktree:79`)이 소유하므로 LINE 미채택.

> 여기에 더해 **매 worktree dispatch 직후**에는 생성 가드(worktree 등장 + 메인 clean + cwd)를 실행한다 — 동일 batch 다중 dispatch race로 worktree를 못 받은 agent가 메인 트리에서 편집하는 유출은 완료 알림 시점 가드로는 늦게 잡히기 때문이다.

**A37** — `autonomous-driving.md:283` (뒷 절반) → C05. 규칙(파일 구성)이 이미 형태를 정한다. LINE 미채택.

> 파일 구성: append-only 단일 로그 `decisions/log.md` 또는 결정별 개별 파일 `decisions/NNNN-<slug>.md`. 결정적 파일명으로 재현성을 확보한다.

**A38** — `autonomous-driving.md:366` (O18) → D11. LINE 채택 — "후보는 주변 문장으로 판정한다" 규칙만 D11 입력 신호 3번에 남겼다.

> **본문 표기는 후보다.** 표기가 있다는 사실만으로 미결로 치면 과잉 발화한다 — 상태 기계 spec의 `미확정 주문`은 주문의 한 상태를 가리키는 도메인 용어이고, 플로우 문서의 의문형 제목 끝 `?`는 절 제목의 형식이지 열린 결정이 아니다. 그래서 표기를 **전부 수집한 뒤** 각각의 주변 문장을 읽고, 그 문장이 결정을 열어 둔 것인지(**미결**) 단어를 도메인 용어·수사로 쓴 것인지(**기각**)를 판정한다.

**A39** — `autonomous-driving.md:408` (O19) → D53. 규칙(`:401-402`)이 이미 형태를 지정한다. LINE 미채택.

> 같은 패턴 선례: 미확정 외부 의존을 `interface` 로 격리하고 그 자리에 명백히 inert 한 `Noop` 구현(스캐너·알림 등)을 두는 contract-격리 + Noop stub 구성. seam 만 고정하고 구현은 비워둔 채 골격을 끝까지 가져간다.

**A40** — `autonomous-driving.md:445` → D47. LINE 채택 — D47 종단 계약에 "토폴로지 위반이면 복구 후 즉시 보고"만 남기고 미확정 서술은 제거했다. (A03 이 기록한 "미확정 지점" 출구 패턴이 실제로 쓰인 자리다.)

> **조건 2의 미확정 지점**: 원문은 토폴로지 위반에만 "복구 후 즉시 보고"를 덧붙였는데, 이것이 보고 *순서*만 규정하는지 공통 보고 포맷을 대체하는지는 원문이 정하지 않았다. 여기서 확정하지 않는다.

**A41** — `autonomous-driving.md:468` → D50. 중첩 판정 — §3.1 이 근거 1줄을 강제하므로 LINE 으로도 유지했다.

> 세 판정의 차이는 상태 이름이 아니라 **다음 행동을 정하는 데 필요한 정보가 다르다**는 것이다. 그 정보가 빠진 항목은 판정이 없는 것과 같다.

**A42** — `autonomous-driving.md:509` (O3, 안티패턴 7) → D21 · D22. O1 LINE 은 T8 소유. race 4중 복제의 마지막 사본.

> **토폴로지 가드 생략** (무거운 경로): 완료 알림/머지/dispatch 직후 확인 없이 연속 진행 → 오염된 HEAD 위에서 다음 dispatch의 worktree base가 잘못 잡히거나, worktree를 못 받은 agent의 유출을 커밋된 뒤에야 발견. 같은 batch에 worktree dispatch를 2개 이상 싣는 것 자체가 이 유출의 알려진 원인이다. **(앞부분만)**

**A43** — `autonomous-driving.md:167-168` → D48. LINE 미채택 — D48 판정 질문이 같은 판정을 질문형으로 이미 담는다.

> ✅ 검증 가능: "모든 작업이 리뷰 통과 후 머지 완료 + `cargo test` green + `cargo fmt --check`/`clippy -D warnings` 통과" / ❌ 검증 불가: "코드가 좋아 보이면", "대충 다 되면"

**A44** — `autonomous-driving.md:352` (앞 문장) → D11. LINE 채택 — D11 근거 한 줄로 유지하고 문서 계층 인용만 이관했다.

> spec은 "제품이 무엇이어야 하는가"의 신뢰 소스인데(CLAUDE.md 문서 계층), 미결이 남은 spec은 아직 규격이 아니다.

**A45** — `autonomous-driving.md:141` → D18 · D20. 규칙은 D18 소유라 이 파일에서 삭제됐고 배경 문장만 남긴다.

> 자율 루프는 본질적으로 **구현 → 리뷰 → 수정**을 반복하는 구조다. 두 책임을 분리한다: **편집·격리는 `isolation:"worktree"` subagent가**(하베스트 보장), **조율은 team이**(공유 checkout).

### 2.12 원문 — T13 (`SKILL.md` 라우터 writer)

출처는 재구조화 전 `SKILL.md`(230줄 판). 라우터에서 사라지지만 어느 항목의 판정 규칙도 아닌 배경·서사 문장이다.

**A46** — `SKILL.md:19` → D01. 앞 문장은 D01 근거 1줄로 축약했고 예시 열거가 배경으로 남는다.

> **적용 범위는 작업의 종류가 아니라 규모로 정한다.** 구현이냐 문서냐 조사냐로 가르지 않는다 — 여러 단위로 쪼개지거나, 병렬로 벌릴 수 있거나, 메인 컨텍스트를 크게 먹으면 위임 대상이다.

**A47** — `SKILL.md:149` → D12 · D14.

> 병렬/순차 결정 트리는 **이미 쪼개진** 작업을 거르는 사후 필터다. 분해(1단계)가 충돌을 만들어 놓으면 트리는 그것을 전부 순차로 떨어뜨릴 수밖에 없다 — **병렬 이득은 판정이 아니라 분해에서 결정된다.**

**A48** — `SKILL.md:171` → D18 · D20.

> **격리는 subagent만 보장** — teammate는 공유 checkout. 편집은 `isolation:"worktree"` subagent, team은 조율 전용 **(앞부분만)**

**A49** — `SKILL.md:32` → C01.

> 위 조율 도구는 대부분 deferred tool이다 — `ToolSearch`로 스키마를 확보하기 전에는 호출할 수 없고(§진입 시 체크 0), 확보를 건너뛴 세션은 명시적 에러 없이 team 경로 전체를 조용히 잃는다.

### 2.13 건수 대조

writer 별 `PLAN` 건수는 세 가지로 셀 수 있고, 세 값이 서로 다르다. 아카이브가 담는 단위는 **원문 인용 블록 1건 = 색인 1행**이다.

| flags 파일 | (a) writer 자기 집계 | (b) 표의 `PLAN` 행 (기계 계수) | (c) 아카이브 인용 건 = 색인 행 |
|---|---:|---:|---:|
| `T3.md` | 4 | 5 | 5 |
| `T4.md` | 1 | 1 | 1 |
| `T5.md` | 3 | 3 | 3 |
| `T6.md` | 2 | 2 | 2 |
| `T7.md` | 7 | 7 | 7 |
| `T8.md` | 3 | 3 | 4 |
| `T9.md` | 5 | 4 | 4 |
| `T10.md` | 4 | 4 | 4 |
| `T11.md` | 3 | 3 | 3 |
| `T12.md` | 12 | 9 | 12 |
| `T13.md` | 4 | 0 | 4 |
| **합** | **48** | **41** | **49** |

(b) 계수 규칙: 각 표의 **`플래그` 열** 셀이 `PLAN` 토큰을 포함하는 행. 셀 안의 부가 주석(`RULE (PLAN 검토함 …)` · `PLAN (O18)` · `PLAN — 원문: "…"` 같은 꼬리말)은 허용한다. 첫 열이 `플래그` 인 표 — 각 파일 말미의 "건수 요약" — 는 제외한다.

이 규칙으로 재계수하면 **합은 41 로 재현되지만, 파일별로 상쇄되는 기계 오차가 2건** 있다.

| 파일 | 기계 계수 | 표의 실제 PLAN 행 | 차이의 원인 |
|---|---:|---:|---|
| `T4.md` | 2 | 1 | `delegation:186` 의 플래그 셀이 `RULE (PLAN 검토함 — 규범 문장으로 흡수)` 다. `LINE`/`PLAN` 을 **검토했다는 기록**(설계 §6.3 요구)이지 `PLAN` 지정이 아니다 → 과계수 1 |
| `T6.md` | 1 | 2 | `delegation:100` 행의 인용에 `git ls-files \| grep` 의 `\|` 가 들어 있어 표 열이 한 칸 밀린다 → 미계수 1 |

두 오차가 +1 / −1 로 맞물려 합계만 우연히 일치한다. 위 표의 (b) 열은 **사람이 정정한 값**(T4 1 · T6 2)이며, 기계 계수 그대로는 T4 2 · T6 1 이다. 재계수 스크립트는 세션 scratchpad 산출물이라 저장소에 남지 않는다.

차이의 출처:

- **T3**: 자기 집계가 1 적다. 실제 `PLAN` 표시 행은 `delegation:63-74` · `:80-83` · `:84-94` · `:161-164` · `:245` 의 5행이다 (T14 가 확인).
- **T8**: 표 행은 3(`:79` · `:143` · `:144`)이나 이관 원문 절에 `:81-82` 인용 블록이 하나 더 있다. `:79` 문장의 종결부가 가리키는 두 항목이라 A19/A20 으로 나눠 실었다.
- **T9**: 자기 집계가 1 많다. 표의 `PLAN` 행은 4이고, `monitor:100-104` 행(플래그 RULE, 비고에 "O25 와 중복 → PLAN")이 다섯 번째로 세어진 것으로 보인다. 그 문장은 A23(O25)에 접어 넣었다.
- **T12**: 표의 `PLAN` 행은 9이나 이관 원문 절이 12건을 인용한다. 나머지 3건(`:167-168` DEL · `:352` LINE · `:141` MOVE)은 표에서 다른 플래그를 받았지만 writer 가 배경 인용으로 함께 넘겼다. 그대로 실었다.
- **T13**: 표에는 `PLAN` 행이 없고 별도의 "PLAN 이관 후보" 목록 4건이 있다.

**T14 가 T15 에 넘긴 수치는 "45건(표 행 기준)"이었다.** 그 45 는 자기 집계 합 44 에 T3 정정분 1 을 더한 값이며, T9·T12 의 자기 집계 오차는 반영되지 않은 값이다. 위 (b)(c) 가 실측이다.

---

## 3. T14 대조 요약

원본은 `scratchpad/impl/T14-crosscheck.md`(313줄, 세션 산출물이라 저장소에 없다). 대상 커밋 `771563f`, 조사자 read-only. 아래는 결론만 압축한 것이다.

### 3.1 구간 커버리지 (해체 4파일 → 새 references)

| 소스 파일 (줄수) | 미배정 구간 | 판정 |
|---|---|---|
| `delegation-patterns.md` (461) | 0건 — 1-461 전 구간에 목적지 | OK |
| `branch-strategy.md` (226) | `:193-195`(여백) · `:196-207`(안티패턴 7) · `:208-226`(체크리스트 9) | flags 스코프 밖일 뿐. 설계 §2.4 의 삭제 판정 + 항목별 전수 확인으로 소유 공백 아님 |
| `advisory-consult.md` (283) | `:1-14`(서두) | `model-routing.md` 스코프 2줄이 대체함을 확인 |
| `spec-driven-review.md` (140) | 0건 | OK |

전제 확인: 새 파일들의 `## D##.`/`## C##.` heading 중복 0건, dangling 참조는 D01·D02·P02 3개뿐이며 전부 `SKILL.md`(T13) 소유. 항목 수 D 58 / C 16 / P 11 = 85 로 설계 §2.1 과 일치.

### 3.2 R1~R7 회수 (설계 §6.1)

**7/7 회수.** 미회수 0건.

| # | 원문 | 회수 위치 |
|---|---|---|
| R1 | `advisory:263` 자문자 수 상한 | D28 판정 노드 2개 + 근거 |
| R2 | `SKILL:225` deferred 도구 `InputValidationError` | C01 1번 + 한 줄 근거 |
| R3 | `autonomous:516` silent stub | D53 종단 계약 |
| R4 | `worktree:153` "변경 파일 집합을 추정했는가" | D14 입력 신호 첫 줄 |
| R5 | `worktree:165` 결과별 worktree 상태 | D51 입력 신호 |
| R6 | `worktree:166` 변경 있는 결과를 머지로 | D51 종단 계약 |
| R7 | `delegation:76-83` O5 Edit 절대경로 트랩 | D20 종단 계약 |

### 3.3 §6.4 "유지 권장 9곳"

**9/9 RULE 로 흡수.** 근거 자리로 내려간 것 0건 (§6.4 요구 충족). 흡수처: D26 입력 신호 · D27 종단 계약 · D19 판정 노드 2개 · D42 입력 신호 + C04 각주 · D11 종단 계약 · D21 입력 신호 · D13 과 D16 입력 신호 말미 · C12 항목 + 한 줄 근거 · D28 판정 노드(= R1).

### 3.4 검사 3b — `needs:` ↔ 09a "선행 결정"

D03~D58 중 54개가 09a 와 일치. 차이 2건 + T13 대기 2건.

| id | 차이 | 성격 |
|---|---|---|
| D36 | `<!-- needs: -->` 주석 자체가 없음 | 내용(선행 없음)은 일치. "선행 없는 결정도 빈 needs 를 둔다" 규약 위반 → A1 |
| D55 | 같음 | 같음 → A1 |
| D01 · D02 | T13 미작성 시점이라 대조 불가 | 이후 확인 |

부수 관찰(blocking 아님): `needs:` 에 C-id 를 쓴 항목 3개(D03·D04·D06 — 09a 의 "체크 0" 치환) · 파일 단위 `needs:` 유무가 6:4 로 갈림 · `merge-coordinator.md:8` 의 `owns:` 만 `|` 구분자를 쓰지 않음.

### 3.5 검사 8 — 안티패턴·체크리스트 전수 소유자

대상 98항목(해체 4파일의 안티패턴 28 + 체크리스트 51 + `SKILL.md §안티패턴` 19).

| 묶음 | 결과 |
|---|---|
| `SKILL.md §안티패턴` 19 | 17 소유자 확인 · 1 T13 대기(§안티 1 → D02) · **1 소유 공백(§안티 4)** |
| `delegation:76-83` 안티 2 | 2/2 |
| `branch:196-207` 안티 7 | 7/7 |
| `advisory:234-266` 안티 10 | 10/10 |
| `spec-review:108-121` 안티 9 | 9/9 |
| 체크리스트 51 (15 + 9 + 15 + 12) | 51/51 |

**소유 공백 G1** — `SKILL.md:215`(§안티패턴 4) "**Reference 일괄 로드**: 시작하자마자 모든 reference 를 Read 금지 — 단계별로 필요할 때만." 새 10파일 어디에도 소유자가 없었다(설계 §5 는 반대 방향인 과소 로드만 다룬다). **해소**: T13 이 라우터 `## 필수 로드` 표 아래 금지+출구 1행으로 흡수했다 (새 id 를 발급하지 않았다 — §4-(7)).

### 3.6 잔여 산문과 후속 권고

T14 §6 은 새 파일의 6요소 밖 잔여 산문 3건(`model-routing.md:37-42` 7번째 요소 · `:37` 소유 선언 · `:13` 소유 선언 뒷절반)을 지적했다. 기계 확인으로 `**후속**` 이후의 산문은 10파일 전수 0건.

이 3건은 최종적으로 **후속 권고 A1~A8** 로 정리됐다. 아래가 정본 목록이며, 앞의 "잔여 산문 3건"은 A2·A4·A5 에 흡수된다.

| # | 대상 | 조치 | 이 런에서의 처분 |
|---|---|---|---|
| A1 | `agent-monitor.md` D36 · D55 | 빈 `<!-- needs: -->` 추가 (검사 3b) | T14-fix writer |
| A2 | `model-routing.md` P05 8단계 | `→ C05` 포인터로 축약 (C05 와 축자 중복, 값 열거가 이미 갈렸다) | T14-fix writer |
| A3 | `autonomous-driving.md:146` | `단일 출처다 —,` 문장부호 수정 | T14-fix writer |
| A4 | `model-routing.md:13` `:37` | 소유 선언 산문 2건 제거 | T14-fix writer |
| A5 | `model-routing.md:37-42` | D26 의 7번째 요소를 6요소 안으로 접기 | T14-fix writer |
| A6 | `merge-coordinator.md:8` | `owns:` 구분자를 `\|` 로 통일 | T14-fix writer |
| A7 | 설계 §10 T16 (c) | grep 범위를 `plugins/` 로 한정 | 채택 (§4-(8)) |
| A8 | 설계 §2.4 `advisory-consult.md` heading 수 "21" | 실제 16(H1 제외)/17(H1 포함). 기록 오차 | plan 이므로 고치지 않고 여기 남긴다 |

### 3.7 삭제 안전성 판정

**T16(해체 4파일 삭제) 가능 — 조건부.** 전 구간에 목적지가 있고 R1~R7 · §6.4 · 98항목 전수 대조(소유 공백 1 · T13 대기 1 포함)가 끝났으며, 4파일 고유 자산 3건(`spec-review:36-39` 검증 질문 표 → P11 표 원형 보존 / `spec-review:48-56` `spec-unresolved` → D31 hard stop 종단 / `delegation:422-442` D26 tier 표 → `model-routing.md` tier 목록)도 보존을 확인했다.

선행 조건 2개: (1) T13 이 `SKILL.md` 의 해체 4파일 참조 34건을 제거할 것 — 마크다운 링크가 아니라 CI 가 못 잡는다. (2) T16 검증 기준 (c) 의 grep 범위를 `plugins/` 로 한정할 것. 둘 다 충족한 뒤 `0b7969a` 에서 삭제했다.

---

## 4. 설계 대비 이탈 기록

설계 09 본문과 다르게 확정된 표기·범위 결정이다. 설계 문서(plan)는 고치지 않으므로 여기에 모은다. 각 항목의 "결정 시점"은 런 중 decision log 의 순서다. 아래의 "영향 파일" 서술은 전부 커밋 `12995ec` 기준이다.

**(1) cross-file 참조는 마크다운 링크 없이 bare `→ D##`**
설계 §3.5 는 "파일을 가로지르는 참조는 마크다운 링크로 건다"였다. T2 파일럿(`5add3ea`) 이후 메인이 bare id 로 통일했다. 근거: 사용자 결정 E5 — 스킬이 플러그인 묶음 밖 **개별 설치 형태**로도 쓰이므로 상대 경로 링크가 설치 위치에 따라 깨진다. E5 가 외부 참조에 대해 링크 전환(F-1)을 폐기했으므로 내부 참조도 같은 표기로 맞췄다. id → 파일 매핑은 라우팅 표 하나가 갖고, 실존은 스크립트 검사 (4) 가 보장한다.
영향: 새 references 10파일 · `SKILL.md` · `.claude/rules/decision-graph.md` 전부.

**(2) C07 = 역할별 모델 제약 / C15 = 자문 계약 (설계 §2.2 와 스왑)**
설계 §2.2 는 C07 = "자문 입력 패킷 · 출력 계약", C15 = "자문 역할 제한(도구 보장 아님) · 수명" 이었다. 구현은 C07 = **역할별 모델 제약**(`model-routing.md:153`), C15 = **자문 계약 — 역할 제한 · 패킷 · 출력 · 수명**(`:166`) 이다. 메인 지시의 오기가 T7 산출물로 굳었고, 자문 계약 4요소를 한 항목으로 묶는 쪽이 낫다고 판단해 **구현 기준으로 확정**했다. 결정 시점: T7·T10 완료 직후.
영향: `model-routing.md` · `SKILL.md` 라우팅 표 · 09a 대조 시 이 두 id 는 이름이 다르다.

**(3) `owns:`/`needs:` 는 frontmatter 가 아니라 HTML 주석**
WRITER-COMMON 지시는 frontmatter 필드였으나 검사 스크립트가 HTML 주석을 파싱하고 `.claude/rules/decision-graph.md` §원칙 3 도 주석을 규정한다. **T4 가 rule 을 따랐고**(`af2d4a6` 의 `entry-gates.md:8`, decision log 에 "WRITER-COMMON 은 frontmatter 라고 썼으나 스크립트가 HTML 주석 파싱"으로 기록), T7 의 frontmatter 판은 reviewer 경유로 되돌렸다. 확정 표기: 파일 상단 `<!-- owns: D… | C… | P… -->`, 각 결정 블록 heading 다음 줄 `<!-- needs: … -->`. 선행이 없는 결정도 빈 주석을 두기로 확정했다 (규약 본문은 rules 소유).
영향: 10파일 전부 + `SKILL.md`. 잔여 nit 는 A6(구분자).

**(4) mermaid 엣지 인라인 선언 허용**
설계 §3.1 은 노드 선언과 엣지를 분리한 표기를 예시로 들었다. T3 이 260줄 상한을 맞추려 인라인 엣지로 압축했고, sonnet tester 6시나리오 재검증에서 6/6 정답·보완 지식 0 이 나와 표기를 확정했다. 두 표기 모두 허용하며, 검사 5(엣지 계수)는 줄당 엣지 1개를 전제로 센다.
영향: `dispatch-planning.md` 이후 모든 그래프.

**(5) 구조 heading 화이트리스트 W1~W6**
심문 findings R-1(화이트리스트 충돌)에 대해 design-v3 가 항목 heading(`## D##.` 등) 외에 허용되는 구조 heading 6종을 열거했다. 검사 6(금지 heading)이 이 목록을 기준으로 판정한다.
영향: T19 스크립트 · `SKILL.md`(`## 필수 로드` · `## 결정 라우팅` · `## 디스패치 전 게이트 인덱스`) · `merge-coordinator.md`(`## 머지 대상: epic 브랜치`).

**(6) T19 스크립트 상시 검사는 10종 (설계의 "9종" 표기는 낡음)**
설계 §9.2 · §12 는 "상시 9종"이라고 적는다. E5 결정으로 **검사 11(외부 계약 표면 실존)** 이 추가돼 실제로는 10종이다(설계 §8.2 는 이미 10종으로 적혀 있어 문서 안에서도 갈렸다). 현행 `scripts/check-decision-graph.sh` 는 `checks: 10` 을 보고한다. 1회성 이행 검사 2종(3b · 8)은 별개이며 T14 에서 사람이 수행했다.
영향: `scripts/check-decision-graph.sh` · `Makefile`(`validate-graph`, `validate-ci`) · `.claude/rules/decision-graph.md`(검사 개수를 문서에 적지 않고 스크립트가 소유하도록 결정).

**(7) "Reference 일괄 로드 금지"는 라우터 `## 필수 로드` 아래 1행이 소유 (신규 id 미발급)**
T14 가 발견한 소유 공백 G1 의 해소 방식이다. D01–D58 · C01–C16 · P01–P11 어디에도 자리가 없었고, 새 id 를 발급하는 대신 라우터가 `## 필수 로드` 표 바로 아래 금지+출구 1행으로 직접 갖는다. 결과적으로 항목 수 85 는 변하지 않는다.
영향: `SKILL.md`.

**(8) T16 검증 기준 (c) 의 grep 범위를 `plugins/` 로 한정**
설계 §10 T16 (c) 는 "해체 4파일 이름 grep → 0건"을 `.` 전체에 요구했으나 원리적으로 달성 불가다. 설계 §6.5 가 `plans/atelier/09a~09d` 를 커밋하기로 했고, 그 문서들이 해체 4파일을 `파일:행` 으로 인용하는 것이 존재 이유이기 때문이다(이 문서 §2 도 마찬가지다). 그래서 검증은 `plugins/` 한정으로 읽는다.
영향: T16 수용 기준 · 이후 같은 grep 을 도는 사람.

**(9) 필수 로드 표의 3열은 `근거` 가 아니라 `항목` id**
설계 §4.1 스케치는 `## 필수 로드` 3행의 마지막 열을 근거 서술로 뒀으나, 라우터에서 근거를 서술하면 §3.3(다른 항목의 판정 규칙 재요약 금지)에 걸린다. 3열을 **항목 id** 로 바꿔 라우팅만 하게 했다. 근거: T13 산출물(`SKILL.md` 의 `## 필수 로드` 표)에서 확인.
영향: `SKILL.md`.

---

## 5. 실측

### 5.1 줄수

| | 재구조화 전 (`a9349aa`) | 재구조화 후 (`12995ec`) |
|---|---:|---:|
| `SKILL.md` | 230 | 124 |
| `references/*.md` | 2,637 (10파일) | 2,251 (10파일) |
| **합** | **2,867** | **2,375** |

감량 492줄(-17.2%). 설계 §12 의 목표 구간은 1,760~2,015 였으므로 **목표를 360~615줄 초과**한다. 상한 준수(파일당 260줄·결정 블록당 35줄)와 R1~R7·§6.4 회수를 우선한 결과다.

재구조화 후 파일별: `agent-monitor` 257 · `architect-council` 259 · `autonomous-driving` 255 · `dispatch-planning` 252 · `entry-gates` 237 · `investigation` 188 · `merge-coordinator` 259 · `model-routing` 199 · `review-gates` 177 · `worktree-lifecycle` 168.

**정정**: T15 지시서와 일부 중간 기록은 재구조화 후 references 를 "9파일"로 적는다. 실제는 **10파일**이다 — 설계가 지정한 신설·개편 9개에 `merge-coordinator.md`(제자리 개편)를 더한 수다. 해체돼 사라진 것은 `delegation-patterns.md` · `branch-strategy.md` · `advisory-consult.md` · `spec-driven-review.md` 4개다.

### 5.2 검사 스크립트

- `scripts/check-decision-graph.sh` — 상시 검사 10종. `bash scripts/check-decision-graph.sh plugins/atelier/skills/orchestrator` → `checks: 10, failed: 0`, exit 0.
- `Makefile` — `validate-graph` 타깃이 위 스크립트를 돌리고, `validate-ci` 가 그것을 호출한다. 해체 4파일이 남아 있는 동안은 red 였으므로 T19 머지를 T16 뒤로 미뤘다.
- 1회성 이행 검사 2종(3b · 8)은 스크립트에 넣지 않았다. 09a 를 기계 파서의 입력으로 쓰면 CI 를 green 으로 만들려고 plan 을 갱신하게 되어 "plan 은 고치지 않는다"가 깨지기 때문이다(설계 §6.5·§9.2). T14 가 사람으로서 1회 수행했고 결과는 §3.4·§3.5 에 있다.

### 5.3 이 수치의 유효 범위

위 실측은 커밋 `12995ec` 시점의 스냅샷이다. 이후 T21 수정 라운드(T18 실런 결과 반영)가 진행되어 최종 줄수는 달라질 수 있다 — **최종 수치는 코드가 진실이며, 이 문서는 갱신하지 않는다.**

---

## 6. 이 문서를 쓰며 세운 가정

1. **인용 표는 색인 + 원문 본문 2단으로 나눴다** (§2.1 말미). 여러 줄 인용을 마크다운 표 셀에 원형 보존할 수 없기 때문이다. 색인 행 수와 원문 항목 수는 49 로 일치한다.
2. **`PLAN` 건수의 단위는 "원문 인용 블록 1건"으로 잡았다** (§2.13 (c) 열). 표 행 기준(41)·writer 자기 집계(48)와 모두 다르며, 세 값과 그 차이의 출처를 §2.13 에 남겼다. T14 가 넘긴 "45"는 자기 집계 기반이라 실측과 다르다.
3. **"배경이 설명하는 현재 항목" 열은 flags 가 적은 목적지 id 를 그대로 옮기고, 그 id 의 heading 실존만 현행 트리에서 확인했다.** 원문과 현재 규칙 문안의 의미 대조는 하지 않았다 — 그것은 spec·코드의 영역이다.
4. **§3 의 T14 요약은 원본 313줄을 압축한 것이다.** 판정 근거의 세부(파일:행 증거 열)는 생략했다. 원본은 세션 scratchpad 산출물이라 저장소에 남지 않는다.
5. **§4 의 "영향 파일"은 결정이 실제로 반영된 파일이다.** 결정 시점 이후 그 파일이 더 바뀌었는지는 확인하지 않았다.
6. **§5 의 후 측정은 커밋 `12995ec` 기준이다.** T14 후속 권고 A1~A6 의 반영분은 이 시점에 아직 들어오지 않았을 수 있다.
