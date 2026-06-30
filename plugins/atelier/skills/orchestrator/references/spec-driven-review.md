---
name: spec-driven-review
description: 자율주행이 spec(DESIGN/concerns/flows)을 입력으로 구현을 시작할 때, 팀 모드로 검토자(spec↔구현 적합성)와 QA 매니저(spec↔테스트 적합성)를 상주시켜 worktree 코드를 계속 리뷰·개선하는 패턴. autonomous-driving.md 리뷰어 게이트의 spec 특수화. orchestrator 스킬 내부 참조 문서.
version: 0.1.0
user-invocable: false
---

# Spec-Driven Review (spec 기반 구현 리뷰 게이트)

자율주행이 **spec 문서를 입력으로 구현을 시작**할 때 적용하는 리뷰 게이트의 특수화다. 구현 sub-agent가 만든 worktree 코드를 두 전용 리뷰 역할로 **계속 검증·개선**한다:

- **검토자 (spec-reviewer)**: worktree 코드가 **spec대로 구현됐는지** (요구사항 충족 ↔ 구현)
- **QA 매니저 (qa-manager)**: **spec대로 테스트 케이스가 작성됐는지** (spec의 각 flow/concern ↔ 테스트 커버리지)

이 문서는 `autonomous-driving.md §리뷰어 게이트`의 일반 게이트를 **spec 맥락으로 특수화**한 것이다 — 예산·재위임·decision log·에스컬레이션·토폴로지 가드 등 공통 규칙은 `autonomous-driving.md`가 단일 소유하며 여기서 중복 정의하지 않는다.

> **spec-review 스킬과의 관계**: `spec-review` 스킬은 *이미 존재하는 spec을 코드와 대조 분석*하는 독립 워크플로우다. 이 문서는 *자율 루프 안에서 머지 전 게이트*로 spec 적합성을 검증한다 — 목적·진입점·수명이 다르다. 게이트가 더 깊은 spec↔code 갭 분석이 필요하다고 판단하면 `spec-review`의 L1→L2→audit 레이어를 빌려올 수 있으나, 기본은 게이트 수준의 pass/reject 판정이다.

---

## 언제 진입하는가

- 자율 계약의 입력에 **spec 문서(`spec/DESIGN.md`, `spec/concerns/*`, `spec/flows/*` 또는 동등한 명세)가 있고**, 그 spec을 구현하는 작업을 dispatch할 때.
- 트리거 발화 예: "spec 대로 구현하면서 검토자랑 QA 매니저 붙여서", "spec 기반 구현 자율주행", "구현이 spec 맞는지 / 테스트가 spec 맞는지 계속 봐줘".
- spec 입력이 없으면(자유 구현·버그 수정 등) 이 게이트 대신 `autonomous-driving.md`의 **일반 리뷰어·QA 게이트**를 쓴다 — 그 게이트도 검토(구현↔요구사항) + QA(테스트↔요구사항) 두 차원을 똑같이 필수로 돌린다. 달라지는 것은 QA의 기준선뿐이다: spec이 있으면 `테스트 ↔ spec flow/concern`으로, 없으면 `테스트 ↔ 요구사항/엣지케이스`로 커버리지를 판정한다.

---

## 두 게이트의 책임 분리

검토자와 QA 매니저는 **서로 다른 차원**을 본다. 한 agent가 둘 다 보게 하지 않는다 (관심사 혼선 → 둘 다 얕아짐).

| 역할 | 입력 | 검증 질문 | 출력 |
|------|------|-----------|------|
| **검토자 (spec-reviewer)** | spec 문서 + worktree diff (epic base 기준) | 구현이 spec의 요구사항/flow/제약을 **빠짐없이·과하지 않게** 충족하는가? 회귀 위험·설계 원칙(SOLID 등) 위반은? | `pass`/`reject` + `spec 항목 ↔ 파일:라인` 매핑, 미충족·초과 구현 목록 |
| **QA 매니저 (qa-manager)** | spec 문서 + worktree의 테스트 코드 | spec의 각 flow/concern/엣지케이스에 **대응하는 테스트가 있는가**? 테스트가 spec 의도를 검증하는가(빈 assert·항상 통과 아님)? | `pass`/`reject` + `spec flow ↔ 테스트:라인` 커버리지 표, 누락 케이스 목록 |

핵심 경계:

- **검토자는 "코드가 spec을 만족하나"**, **QA 매니저는 "테스트가 spec을 만족하나"**. 검토자는 테스트 커버리지의 충분성을 판정하지 않고, QA 매니저는 구현 로직의 정합성을 판정하지 않는다.
- 둘 다 **구현 sub-agent와 다른 agent**다 (자기 코드 자기 리뷰 금지 — `autonomous-driving.md` 안티패턴 #14).
- 두 게이트는 **AND**다 — 둘 다 `pass`여야 머지 후보로 승급한다. 하나라도 `reject`면 재위임.

---

## 팀 구성 (실험 플래그 활성 시)

`CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1`일 때, spec 구현 작업 하나에 **조율 전용 team**을 둔다. team은 공유 checkout이므로 **편집은 절대 teammate가 직접 하지 않고 `isolation:"worktree"` subagent에 위임**한다 (`delegation-patterns.md §Agent team 사용 패턴`, #783).

```
# 구현은 격리 subagent (편집·격리 보장)
impl = Agent({
  description: "<task> 구현",
  isolation: "worktree", run_in_background: true,
  prompt: "<자기완결 컨텍스트 + spec 경로 전문 인용 + base=epic/<name> + 격리 준수>"
})

# 검토자 — read-only 조율 teammate
Agent({
  name: "spec-reviewer", run_in_background: true,
  description: "spec↔구현 적합성 검토",
  prompt: "<spec 문서 경로 + 검토할 worktree diff. 편집하지 말 것.
           구현이 spec 요구사항을 충족하는지 'spec 항목 ↔ 파일:라인'으로 판정.
           pass/reject + 미충족·초과 구현 목록 반환.>"
})

# QA 매니저 — read-only 조율 teammate
Agent({
  name: "qa-manager", run_in_background: true,
  description: "spec↔테스트 적합성 검증",
  prompt: "<spec 문서 경로 + worktree 테스트 코드. 편집하지 말 것.
           spec의 각 flow/concern에 대응 테스트가 있는지, 테스트가 의도를
           실제로 검증하는지 'spec flow ↔ 테스트:라인'으로 판정.
           pass/reject + 누락 케이스 목록 반환.>"
})

# 거부 findings를 team 내부 SendMessage로 전달 → 수정은 다시 격리 subagent로
SendMessage({to: "spec-reviewer", message: "<재검토 요청 또는 범위 조정>"})
```

> teammate의 상세 findings·diff는 teammate 컨텍스트에 남기고, 메인은 두 게이트의 **verdict + 압축 요약(누락/초과 항목)** 만 받는다 (`autonomous-driving.md §메인 컨텍스트 격리`).

### 폴백 — 실험 플래그 없음

플래그가 없으면 team을 쓰지 않고 **단발 격리/read-only subagent 2개**로 같은 두 게이트를 돈다 — 검토자 subagent + QA 매니저 subagent를 병렬 dispatch하고, 거부 시 이전 findings를 새 구현 prompt에 자기완결적으로 실어 재위임한다. 의심스러우면 폴백을 고른다 — 게이트의 본질은 team이 아니라 **구현과 분리된 두 검증 차원**이기 때문이다.

---

## continuous review → improve 루프

worktree 코드를 한 번 보고 끝내지 않고, **머지 전까지 두 게이트가 통과할 때까지** 검증·개선을 반복한다. 이 사이클은 `autonomous-driving.md`의 작업별 리뷰어 게이트 안에 들어가며, **동일한 `max_redispatch_per_task` 예산을 소모**한다 (게이트 전용 새 예산을 만들지 않는다 — 안티패턴 #15).

```
구현 완료 (격리 subagent)
  → 검토자 + QA 매니저 병렬 검증           # 두 차원 동시
      둘 다 pass → 머지 후보로 승급
      하나라도 reject →
        findings(spec 항목/누락 케이스 + 파일:라인)를 자기완결 prompt에 실어
        구현을 격리 subagent로 재위임        # redispatch_count[task] += 1
          → 재구현 완료 → 다시 두 게이트 검증
        max_redispatch_per_task 소진 → hard stop → 에스컬레이션
```

- **병렬 검증**: 검토자와 QA 매니저는 서로 독립이므로 동시에 돌린다 (한쪽 결과를 다른 쪽이 기다리지 않음).
- **재위임 prompt**: 어떤 spec 항목이 미충족인지 / 어떤 flow에 테스트가 없는지를 **`spec 위치 ↔ 코드 위치`로 구체화**해 담는다. "spec을 더 잘 지켜라" 같은 모호한 지시는 금지 (sub-agent는 메인 대화·이전 라운드를 못 봄).
- **decision log**: 게이트 거부 사유와 재위임 판단을 `.orchestrator/<epic>/decisions/`에 append한다 (`autonomous-driving.md §의사결정 기록`).
- **integration_verify와 병행**: 코드/테스트 적합성(이 게이트)과 인프라 의존 동작(`integration_verify`)은 별개 게이트이며 **둘 다 통과해야 머지**한다.

`done_when` 평가에 **"머지된 모든 spec 작업이 검토자·QA 매니저 둘 다 pass"**를 포함한다.

---

## 모델 분배

`autonomous-driving.md §모델 분배` 원칙을 따른다 — 고정 매핑이 아니라 작업 리스크에 맞춰 메인이 정한다. 시작 heuristic:

- **검토자**: 자동 머지의 핵심 안전장치이고 spec↔구현의 미묘한 갭을 봐야 하므로 보통 더 강한 역량을 둘 가치가 있다.
- **QA 매니저**: 커버리지 매핑은 비교적 기계적이라 더 가벼운 tier로 시작할 수 있으나, "빈 assert·항상 통과" 같은 의미 판정이 필요하면 올린다.
- 시작 tier 정의는 `delegation-patterns.md §모델 선택`이 단일 출처다 — 여기서 특정 tier 이름을 고정하지 않는다. 표준 heuristic을 벗어난 선택은 근거와 함께 decision log에 남긴다.

---

## 안티패턴

1. **검토자·QA 차원 혼합**: 한 agent에게 "구현도 보고 테스트도 봐줘" → 둘 다 얕아진다. 차원을 분리한 두 게이트로 둔다.
2. **OR 게이트화**: 한쪽만 pass인데 머지 → spec 미충족 또는 테스트 공백이 통과. 둘 다 pass여야 머지 (AND).
3. **teammate 직접 편집**: 검토자/QA가 발견한 문제를 자기가 고침 → 공유 checkout 오염(#783). 수정은 격리 subagent로 위임, team은 조율·판정만.
4. **자기 코드 자기 리뷰**: 구현 subagent가 검토자/QA를 겸함 → 게이트 무력화. 항상 구현과 다른 agent.
5. **모호한 거부 재위임**: "spec 더 잘 지켜라" → sub-agent가 무엇을 고칠지 모름. `spec 위치 ↔ 코드 위치`로 구체화.
6. **게이트 전용 무한 재위임**: 두 게이트 거부를 예산 밖에서 반복 → 폭주. `max_redispatch_per_task`를 동일하게 소모, 소진 시 에스컬레이션.
7. **spec 없다고 QA 게이트 생략**: spec이 없으면 테스트↔spec 판정은 성립 안 되지만, QA 게이트 자체는 생략하지 않는다 → 일반 게이트의 테스트↔요구사항 QA로 폴백해 검증 테스트는 그대로 추가·검증한다.
8. **메인이 findings 전문 통독**: 두 게이트의 상세 리포트를 메인이 직접 읽음 → 컨텍스트 포화. verdict + 압축 요약만 수령.

---

## 체크리스트

진입 (spec 구현 dispatch 시):

- [ ] 입력에 검증 가능한 spec 문서가 있는가? (없으면 일반 리뷰어 게이트로)
- [ ] 구현 sub-agent prompt에 spec 경로/전문을 자기완결적으로 실었는가?

루프 중 (머지 전):

- [ ] 검토자(spec↔구현)와 QA 매니저(spec↔테스트)를 **구현과 다른 agent**로 두었는가?
- [ ] 두 게이트를 병렬로 돌리고 **둘 다 pass**여야 머지하는가? (AND)
- [ ] 편집(개선)은 teammate가 아니라 `isolation:"worktree"` subagent로 위임했는가?
- [ ] 거부 findings를 `spec 위치 ↔ 코드 위치`로 구체화해 재위임 prompt에 실었는가?
- [ ] 게이트 거부가 `max_redispatch_per_task` 예산을 소모하며 카운트되는가?
- [ ] 실험 플래그가 없으면 단발 subagent 2개 폴백으로 두 차원을 유지하는가?
- [ ] 메인이 verdict + 압축 요약만 받고, 거부 사유를 decision log에 기록하는가?
- [ ] `done_when`에 "모든 spec 작업이 검토자·QA 둘 다 pass"를 포함했는가?
