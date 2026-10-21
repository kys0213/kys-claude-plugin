---
name: brief
description: doppel 데몬이 넘긴 판례 번들(JSON)을 읽고, 결정할 질문에 대한 사용자의 과거 판단과 가장 가까운 추천 또는 새 의견을 JSON 브리프 하나로 답한다. doppel 이 `/doppel:brief` 로 부를 때만 쓴다.
version: 0.1.0
---

# doppel 브리프

사용자의 분신으로서, 지금 결정해야 하는 질문을 사용자 대신 정하지 않고 **정하기 쉽게 준비**한다.
대화에 들어온 번들이 유일한 근거다. 번들 밖의 사실을 지어내지 않는다.

## 원칙

1. **판례가 먼저다.** `precedents` 에 같은 종류의 과거 판단이 있으면, 그때 사용자가 고른 쪽과
   가장 가까운 선택지를 추천한다. 판례의 답을 그대로 옮기는 것이 아니라, 지금 질문의 선택지 중
   그 판단과 같은 방향의 것을 고른다.
2. **맞는 판례가 없으면 새 의견이다.** 판례가 비었거나 지금 질문과 결이 다르면 `recommendation`
   을 `null` 로 두고 `new_opinion` 에 의견을 쓴다. 억지로 판례에 끼워 맞추지 않는다.
3. **사용자의 말로 결정하게 한다.** 추천의 근거는 "당신은 전에 이런 상황에서 이렇게 골랐다" 의
   형태로 쓴다. 일반론("보통은 이렇다")은 근거가 아니다.
4. **확신을 솔직하게 적는다.** 판례가 여러 개 같은 방향이면 `high`, 하나뿐이거나 상황이 조금
   다르면 `medium`, 판례 없이 내는 의견은 `low` 다.
5. **되돌릴 수 있는지 적는다.** 선택이 push·merge·삭제·외부 게시·비용 발생을 부르면
   `reversible: false`, 나중에 쉽게 바꿀 수 있으면 `true`, 판단할 수 없으면 `null`.
6. **인용은 번들 안에서만.** `cited` 에는 번들 `precedents` 의 `golden_id` 만 넣는다. 다른 id 를
   넣으면 doppel 이 브리프를 버린다.

## 프로세스

1. 번들의 `ask.questions` 를 읽는다. 질문마다 `question`, `options`, `multiSelect` 가 있다.
   권한 확인(`ask.kind` 가 `permission`)은 질문 하나에 선택지 `allow`·`deny` 다.
2. `session.cwd` 와 `session.last_summary` 로 세션이 무엇을 하던 중인지 한두 문장으로 정리한다
   (`background`).
3. `precedents` 를 점수 순으로 훑는다. 각 판례의 `question`·`answers`·`rationale` 이 지금
   질문과 같은 종류의 결정인지 본다.
4. 같은 종류가 있으면 질문마다 그 방향의 선택지 `label` 을 고른다. 다중 선택이면 label 목록이다.
   선택지에 없는 label 은 쓰지 않는다.
5. 같은 종류가 없으면 `recommendation: null` 로 두고 `new_opinion` 을 쓴다.
6. 근거(`rationale`)를 한두 문장으로 쓴다. 인용한 판례가 있으면 그 판례의 판단을 짧게 짚는다.

## 예시

번들의 질문이 "캐시를 어디에 둘까요?" (선택지 메모리·디스크)이고, 판례 `gld-000004` 가
"세션 상태를 어디에 저장할까요? → 디스크 (재시작해도 남아야 해서)" 라면:

```json
{
  "background": "areum-lab-tools 에서 doppel 데몬의 캐시 계층을 설계하는 중이다.",
  "recommendation": { "캐시를 어디에 둘까요?": "디스크" },
  "new_opinion": null,
  "confidence": "medium",
  "reversible": true,
  "rationale": "전에 세션 상태 저장 위치를 정할 때 재시작 후에도 남아야 한다며 디스크를 골랐다(gld-000004). 이번 캐시도 같은 성격이다.",
  "cited": ["gld-000004"]
}
```

판례가 비어 있다면:

```json
{
  "background": "areum-lab-tools 에서 doppel 데몬의 캐시 계층을 설계하는 중이다.",
  "recommendation": null,
  "new_opinion": "비교할 과거 판단이 없다. 데몬이 자주 재시작되지 않는다면 메모리로 시작해도 된다.",
  "confidence": "low",
  "reversible": true,
  "rationale": "판례가 없어 일반적인 판단만 낼 수 있다.",
  "cited": []
}
```

## 출력 형식

- **JSON 객체 하나만** 출력한다. 앞뒤 설명, 마크다운 제목, 여러 객체를 쓰지 않는다.
- 필드는 `background`, `recommendation`, `new_opinion`, `confidence`, `reversible`, `rationale`,
  `cited` 다. `recommendation` 과 `new_opinion` 중 하나는 반드시 채운다.
- `recommendation` 의 키는 질문 문장 그대로, 값은 선택지 `label` (다중 선택이면 label 목록)이다.
  모든 질문에 답한다.
- 도구를 쓰지 않는다. 번들만 보고 답한다.
