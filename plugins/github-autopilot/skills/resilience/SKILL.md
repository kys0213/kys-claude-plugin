---
name: resilience
description: 유사 세대진화(stagnation) 감지 시 lateral thinking persona를 적용하여 새로운 접근으로 이슈를 생성하는 가이드. GitHub issue body의 simhash 기반 — gap-watch는 ledger-only로 전환되며 잠정 미사용 (ledger 기반 stagnation은 follow-up).
version: 1.0.0
---

# Resilience: Stagnation Detection & Lateral Thinking

## 개요

같은 gap이 반복 감지되고 유사한 접근이 반복 실패할 때, CLI가 simhash 기반으로 stagnation을 감지한다.
이 스킬은 감지된 패턴에 맞는 **lateral thinking persona**를 선택하여 근본적으로 다른 접근의 이슈를 생성하도록 안내한다.

## Stagnation 판단 기준

CLI `autopilot check diff`가 exit 4를 반환하면 stagnation이다.
출력 JSON에 `pattern_type`과 `recommended_persona`가 포함된다.

```json
{
  "status": "stagnation",
  "stagnation": {
    "detected": true,
    "pattern_type": "spinning",
    "recommended_persona": "hacker",
    "current_simhash": "0xA3F2...",
    "similar_count": 3,
    "candidates": [
      {"simhash": "0x...", "distance": 1, "category": "gap-analysis", "timestamp": "..."}
    ]
  }
}
```

## Pattern → Persona 매핑 (Deterministic)

CLI가 패턴을 분류하고 persona를 결정적으로 추천한다. `recommended_persona`가 있으면 그대로 사용한다.

| pattern_type | Persona | 상황 |
|---|---|---|
| `spinning` | Hacker | 같은 해시 반복 (distance ≤ 3) |
| `oscillation` | Architect | A↔B 교대 패턴 |
| `no_drift` | Researcher | 전부 유사하나 개선 없음 |
| `diminishing_returns` | Simplifier | 점진적 개선이나 여전히 유사 |
| (all exhausted) | Contrarian | fallback — 위 4패턴에 해당하지 않거나 모든 persona 소진 시 |

> `recommended_persona`가 없는 경우(하위 호환): 기존 방식대로 candidates의 distance 분포를 읽고 아래 5가지 persona 중 선택한다.

## Persona 상세

아래 5가지 persona 중 `recommended_persona` 또는 상황에 맞는 것을 사용한다.

### 1. Hacker

**철학**: 제약을 우회한다. 불가능을 거부한다.

**적합한 상황**: 같은 에러가 반복 (distance ≤ 3인 이슈가 3개 이상)

**접근 지침**:
1. 명시적/암시적 제약 조건을 나열한다
2. 각 제약이 실제로 필수인지 의심한다
3. 제약을 우회하여 다른 경로로 문제를 해결한다
4. "잘못된 것"을 시도했을 때 무엇이 깨지는지 탐색한다

**탐색 질문**:
- 우리가 당연시하는 가정 중 사실이 아닌 것은?
- 이 문제를 완전히 우회하면 어떻게 되는가?
- 코드 대신 데이터로 해결할 수 있는가?

---

### 2. Researcher

**철학**: 정보 부족이 근본 원인이다. 추측을 멈추고 증거를 찾아라.

**적합한 상황**: 진전 없음 (다양한 접근을 시도했지만 모두 실패)

**접근 지침**:
1. 알 수 없는 것(지식 갭)을 정의한다
2. 에러 메시지를 다시 주의 깊게 읽는다
3. 공식 문서에서 정확한 케이스를 확인한다
4. 증거 기반 가설을 세운다

**탐색 질문**:
- 이 문제를 해결하기 위해 부족한 정보는 무엇인가?
- 에러 메시지를 정말 주의 깊게 읽었는가?
- 최근 변경된 것 중 이 문제를 유발했을 수 있는 것은?

---

### 3. Simplifier

**철학**: 복잡성은 진전의 적이다. 제거할 수 있는 것을 찾아라.

**적합한 상황**: 점점 복잡해지는 시도 (distance가 점차 증가하지만 계속 실패)

**접근 지침**:
1. 관련된 모든 컴포넌트를 나열한다
2. 각 컴포넌트가 정말 필수인지 도전한다
3. "동작하는 가장 단순한 것"을 찾는다
4. YAGNI: 지금 필요하지 않은 것은 제거한다

**탐색 질문**:
- 핵심 가치를 잃지 않고 제거할 수 있는 것은?
- 이 복잡성이 그 비용을 정당화하는가?
- 기능의 절반을 제거하면 어떻게 되는가?

---

### 4. Architect

**철학**: 아키텍처와 싸우고 있다면, 아키텍처가 잘못된 것이다.

**적합한 상황**: A→B→A 진동 (distance가 교대로 크고 작은 패턴)

**접근 지침**:
1. 구조적 증상을 식별한다 (반복 버그, 강한 커플링)
2. 현재 구조를 매핑한다 (추상화, 책임, 데이터 흐름)
3. 근본적 불일치를 찾는다 (처음부터 잘못된 가정)
4. 최소한의 구조 변경을 제안한다 (전체 재작성이 아닌)

**탐색 질문**:
- 아키텍처와 싸우고 있는가, 함께 일하고 있는가?
- 어떤 추상화가 누수(leak)되고 있는가?
- 처음부터 다시 설계한다면 이렇게 할 것인가?

---

### 5. Contrarian

**철학**: 모든 가정을 검증한다. 위대한 진리의 반대도 종종 또 다른 진리이다.

**적합한 상황**: 위 4가지 persona가 모두 이미 시도되었거나, 패턴이 불분명할 때 (fallback)

**접근 지침**:
1. 모든 가정을 나열한다 (명시적 + 암시적)
2. 각 가정의 반대를 고려한다
3. 문제 자체를 의심한다
4. "아무것도 하지 않으면?"을 질문한다
5. "당연한" 해결책의 반대를 시도한다

**탐색 질문**:
- 우리 가정의 반대가 사실이라면?
- 방지하려는 것이 실제로 일어나야 하는 것이라면?
- 올바른 문제를 풀고 있는가?
- 아무것도 하지 않으면 어떻게 되는가?

---

## 이슈 생성 가이드

stagnation 감지 시, 호출자(예: 향후 ledger 기반 stagnation reader)는 다음 구조로 task body를 생성한다:

```markdown
## 요구사항
[기존과 동일한 gap 설명]

## 과거 시도 이력
- #{이슈번호} ({상태}): [접근 요약] — [실패 사유]
- #{이슈번호} ({상태}): [접근 요약] — [실패 사유]

## 새로운 접근 ({Persona} Persona)
[선택된 persona의 관점에서 작성한 새로운 구현 방향]

### 탐색 질문
- [persona별 질문 중 이 상황에 적합한 것 2-3개]

## 영향 범위
[기존과 동일]
```

## Persona 선택 우선순위

1. candidates를 distance 순으로 읽고 과거 이슈 본문을 확인한다
2. 과거 이슈에서 이미 사용된 persona가 있으면 제외한다
3. 패턴에 맞는 persona를 선택한다:
   - 같은 해시 반복 → Hacker
   - 진전 없음 → Researcher
   - 복잡도 증가 → Simplifier
   - A↔B 진동 → Architect
   - 위 모두 해당 없음 또는 소진 → Contrarian
4. 모든 persona가 소진되면 이슈 본문에 "모든 자동 접근이 소진됨 — 사람의 검토 필요"를 명시한다
