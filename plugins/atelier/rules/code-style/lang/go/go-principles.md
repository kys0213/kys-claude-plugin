---
name: go-principles
description: Go 가독성 원칙의 우선순위, gofmt·줄 길이, 최소 메커니즘, 지역 일관성과 언어 공통 룰과의 적용 순서
paths:
  - "**/*.go"
---

# Go 원칙

Google Go 스타일 가이드를 옮긴 것이다. decisions 출처는 DO/DON'T, best-practices 출처는 "권장"으로 표기한다.

**언어 공통 룰과 Go 룰이 충돌하면 언어 공통 룰을 따른다.**

출처: guide §Style principles, §Formatting, §Line length, §Least mechanism, §Local consistency, decisions §Non-decisions

- 부딪히면 앞선 것을 고른다: 명료성(쓰기보다 읽기) → 단순성(불필요한 추상화 없음) → 간결성(반복·군더더기 없음) → 유지보수성(결정적 디테일을 숨기지 않음) → 일관성(동점일 때만).
- 한 글자 차이로 의미가 바뀌는 코드(`=`/`:=`, 식 가운데 `!`)는 이름 있는 지역 변수로 풀거나 주석으로 신호를 키운다.
- DO: 모든 소스(생성 코드 포함)는 `gofmt` 출력과 같다. 줄 길이 제한은 없고, 길면 쪼개기 전에 리팩토링한다.
- DON'T: 들여쓰기가 바뀌기 직전(함수 선언·조건문)에서 줄을 나누거나 URL 같은 긴 문자열을 쪼개지 않는다.
- 도구는 언어 기본 구성요소 → 표준 라이브러리 → 기존 의존성 → 새 의존성 순으로 고른다 (집합 검사는 `map[string]bool`). 복잡도를 넣으면 이유를 남긴다.
- DO: 가이드가 말하지 않는 지점은 같은 파일·패키지의 방식을 따른다 (`var i int`/`i := 0`, `&T{}`/`new(T)`, `errors.New`/`fmt.Errorf` 등). DON'T: 그것을 근거로 기존 위반을 넓히지 않는다.
