---
name: architect-council
description: 오케스트레이터 스웜 진입 시 분해(Decompose)를 특수화하는 아키텍트 협의체 패턴. brainstorm 아키텍트가 자율로 상황을 분석·설계하고 grill 아키텍트가 심문·검증한 뒤 검증된 task 목록을 도출한다. orchestrator 스킬 내부 참조 문서.
version: 0.1.0
user-invocable: false
---

# Architect Council (아키텍트 협의체)

오케스트레이터 표준 절차의 **1단계 분해(Decompose)를 특수화**하는 패턴이다. 복잡하거나 모호한 요구를 메인이 혼자 쪼개는 대신, **brainstorm 아키텍트**(발산→수렴 상황 분석·설계)와 **grill 아키텍트**(심문·검증) 두 sub-agent에 위임해 **검증된 task 목록**을 도출한다. CLAUDE.md의 설계 최우선 원칙(요구사항 정리 → 사이드이펙트 조사 → 설계)을 스웜의 분해 단계에 강제하는 장치다.

---

## 언제 쓰는가

- 요구사항이 **복잡·모호**하거나, 분해 방식 자체에 설계 판단이 필요할 때 (여러 서브시스템에 걸침, 사이드이펙트 범위 불명, 접근법이 갈림)
- 자율 루프(`autonomous-driving.md`)의 첫 분해 — 이후 루프의 재분해(`recompute_remaining`)는 첫 협의체 산출물을 기준으로 메인이 직접 수행하고, **남은 작업의 전제가 무너졌을 때만** 협의체를 다시 소집한다

생략하는 경우 (메인 직접 분해):

- 작업 경계가 자명한 단순 fan-out (예: "이 3개 파일에 같은 패턴 적용")
- 이미 검증된 spec/설계 문서가 입력인 경우 — 분해는 spec을 따르고, 게이트는 `spec-driven-review.md`로 특수화

---

## 역할 (두 아키텍트)

| 역할 | 자세의 출처 | 입력 | 하는 일 | 출력 |
|------|------------|------|---------|------|
| **brainstorm 아키텍트** | `brainstorm` 스킬의 발산→수렴 자세 | 요구사항 원문 + 코드베이스 | 상황 분석(현 구조·제약·사이드이펙트 조사) → 접근법 2–3개 비교 → 추천 설계 + task 후보 도출 | 상황 분석 요약 + 설계 + task 후보 목록 + 명시적 가정 목록 |
| **grill 아키텍트** | `grill` 스킬의 심문 자세 | brainstorm 아키텍트의 산출물 + 코드베이스 | 설계·분해를 무너뜨려 본다 — 빈틈·미검증 가정·암묵적 trade-off·누락된 사이드이펙트·task 간 의존/충돌을 공격 | `pass` / `reject` + findings (가정별 검증 결과 포함) |

두 아키텍트는 read-only 분석 sub-agent다 (`isolation` 불필요 — 편집하지 않는다). 서로 다른 agent여야 한다 — 자기 설계 자기 심문 금지 (게이트의 자기 검증 편향 방지와 동일 원리).

### 대화 스킬의 자율 어댑테이션

`brainstorm`/`grill` 스킬은 원래 **사용자와의 대화**로 진행된다. 협의체에서는 사용자가 없으므로 다음으로 치환한다:

- **사용자 질문 → 코드베이스 탐색**: 질문이 코드·git 이력·spec 문서로 답해지면 직접 탐색해 해소한다 (grill 스킬의 원칙 그대로).
- **탐색으로 답할 수 없는 질문 → 명시적 가정**: "가정: X (근거: Y)" 형태로 산출물에 남기고, grill 아키텍트가 그 가정의 위험도를 판정한다.
- **도메인 의미 결정 → 에스컬레이션 후보**: 틀리면 데이터/의미가 오염되는 결정은 가정으로 덮지 않고 에스컬레이션 항목으로 표시한다 (`autonomous-driving.md §에스컬레이션`).
- brainstorm 스킬의 HARD-GATE(승인 전 구현 금지)는 협의체에서 "grill pass 전 dispatch 금지"로 대응된다.

---

## 협의체 루프

```
round = 0
design = Agent({description: "상황 분석·설계", model: <아키텍트 tier>,
                prompt: "<요구사항 원문 + brainstorm 자세 + 자율 어댑테이션 규칙 + 출력 계약>"})
while round < max_council_rounds:                  # 계약에 고정 (기본 2)
    verdict = Agent({description: "설계 심문", model: <아키텍트 tier>,
                     prompt: "<design 전문 + grill 자세 + findings 형식>"})
    if verdict.pass: break
    round += 1
    design = Agent({..., prompt: "<이전 design + grill findings를 자기완결적으로 포함해 보강>"})

if not verdict.pass or verdict.has_domain_decisions:
    escalate()                                     # 미해소 findings/도메인 결정과 함께 보고
tasks = design.tasks                               # 검증된 task 목록 → TaskCreate + dispatch
log_decision("협의체 분해", design, verdict)        # decision log (autonomous-driving.md §의사결정 기록)
```

- 라운드는 `max_council_rounds` 예산을 소모한다 — 소진 시 hard stop 후 미해소 findings와 함께 에스컬레이션 (무한 심문 금지).
- 협의체 산출물 전문은 decision log에 남기고, 메인은 **task 목록 + 가정/에스컬레이션 요약**만 컨텍스트에 유지한다 (§메인 컨텍스트 격리).

---

## Task 도출 계약 (출력 형식)

협의체가 확정하는 task 목록의 각 항목은 dispatch prompt의 재료가 되므로 다음을 반드시 포함한다:

```
- id / 목적: 무엇을 달성하는가
- 범위: 무엇을 하고 무엇을 하지 않는가
- 예상 변경 파일: 병렬/순차 결정(disjoint 판정)의 입력
- 의존성: 선행 task (TaskCreate의 blocked-by 입력)
- 위험도: 모델 배분·계획 우선 게이트 적용 판단의 입력
- 검증 기준: 완료를 판정할 결정적 명령/조건
- DB 접촉 여부: 스키마·마이그레이션·쿼리·ORM 모델 변경 포함 여부 (DBA 게이트 트리거 — autonomous-driving.md §리뷰어·QA 게이트)
```

---

## 모델 정책

협의체 아키텍트의 모델은 **`delegation-patterns.md §모델 선택`의 fable 배분 정책(고정 제약)**을 따른다 — 요구사항 분해·검증의 품질이 다운스트림 전체를 좌우하므로 두 아키텍트는 최상위 tier(floor)를 유지하고, 협의체 외 모든 역할은 최상위 tier를 쓰지 않는다(ceiling). tier 정의와 정책 전문은 그 문서가 단일 출처다 — 여기서 중복 정의하지 않는다.

---

## 안티패턴

1. **메인이 협의체를 겸함**: 메인이 직접 brainstorm/grill을 수행 → 분석 전문이 메인 컨텍스트를 포화시키고 자기 설계 자기 심문이 된다. 두 역할 모두 sub-agent로 위임한다.
2. **grill pass 전 dispatch**: 심문이 끝나기 전에 task 후보를 먼저 실행 → 검증 안 된 분해로 스웜 전체가 잘못된 방향으로 진행. pass 또는 에스컬레이션 전에는 구현을 시작하지 않는다.
3. **가정을 조용히 삼킴**: 탐색으로 답 못 한 질문을 가정 표시 없이 설계에 녹임 → 사후에 "왜"를 복원 불가. 가정은 명시하고 grill이 위험도를 판정한다.
4. **모든 요구에 협의체 강제**: 자명한 단순 fan-out까지 협의체 왕복 → 오버헤드만 증가. 생략 기준(위 §언제 쓰는가)을 따른다.
5. **협의체 예산 없는 왕복**: brainstorm↔grill을 pass까지 무한 반복 → 폭주. `max_council_rounds` 소진 시 에스컬레이션.
6. **아키텍트 tier 강등**: 비용 절약으로 협의체를 가벼운 모델에 맡김 → 잘못된 분해 비용이 절약분을 압도. floor는 정책이며 재평가 대상이 아니다.

---

## 체크리스트

- [ ] 협의체 소집/생략 판단을 기준(복잡·모호 vs 자명·spec 입력)에 따라 내렸는가?
- [ ] brainstorm·grill 아키텍트가 서로 다른 sub-agent인가?
- [ ] 두 아키텍트 모델이 fable 배분 정책의 floor를 지키는가?
- [ ] 탐색으로 해소 못 한 질문이 명시적 가정 또는 에스컬레이션 항목으로 남았는가?
- [ ] task 목록이 출력 계약(파일·의존성·위험도·검증 기준·DB 접촉 여부)을 채웠는가?
- [ ] grill pass(또는 에스컬레이션 처리) 후에만 dispatch를 시작했는가?
- [ ] 협의체 산출물 전문을 decision log에 남기고 메인은 요약만 유지하는가?
