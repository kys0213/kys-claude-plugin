---
name: closed-value-sets
description: 닫힌 값 집합은 전용 타입과 망라 분기로 다루고, 밖에서 온 값은 경계에서 해석해 모르는 값은 실패시킨다
paths:
  - "**/*.go"
  - "**/*.rs"
---

# 닫힌 값 집합과 빨리 실패

- DO: 상태·종류·모드는 전용 타입으로 두고 술어·문자열 변환을 같은 모듈에 모은다. 값이 늘 때 고칠 곳이 하나가 된다.
- DO: 외부 표현을 값으로 바꿀 때 모르는 값은 오류다. 기본값·기본 분기로 조용히 넘기지 않는다.
- DO: 분기는 누락을 빌드가 잡는 망라 switch/match 로 쓴다. "나머지 전부" 분기로 검사를 끄지 말고, 같게 처리할 값도 나열한다.

```
DON'T  parse("archived") → Active
DO     parse("archived") → error("unknown status: archived")
```
