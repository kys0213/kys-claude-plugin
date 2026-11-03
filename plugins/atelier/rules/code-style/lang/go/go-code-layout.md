---
name: go-code-layout
description: Go import 그룹·리네임, 복합 리터럴, 함수·조건문 줄바꿈, 변수 선언, 문자열 조합
paths:
  - "**/*.go"
---

# Go 코드 배치

출처: decisions §Imports, §Literal formatting, §Function formatting, §Conditionals and loops, §Use %q, best-practices §Variable declarations, §String concatenation

- DO: import 그룹은 표준 → 프로젝트·외부 → 생성 코드 → `_` 순, 빈 줄로 나눈다.
- DON'T: 이유 없는 import 리네임 (충돌 시 더 지역적인 쪽·밑줄 든 패키지·`v1` 만, 같은 패키지는 같은 이름, 변수와 겹치면 `urlpkg`). 라이브러리의 `import _` (`embed` 예외), `import .`.
- DO: 다른 패키지·필드 많은 타입의 구조체 리터럴은 필드명을 쓰고 0값 필드는 생략한다. 닫는 괄호는 여는 줄 들여쓰기에, 여러 줄이면 마지막 값 뒤 쉼표, 반복 타입명·`&T` 생략.
- DO: 함수 시그니처·`if`·`switch` 조건은 한 줄. 길면 지역 변수로 뽑고, 호출 인자를 나눌 땐 의미 단위로. DON'T: 인자 인라인 주석 (option struct·문서로), `"foo" == x`.
- 권장: 0 아닌 초기값은 `:=`, 빈 값은 `var x T`, 원소를 알면 리터럴. 맵은 쓰기 전에 초기화. 크기 힌트는 측정된 경로에서만.
- 권장: 문자열은 `+`, 포맷은 `Sprintf`, 반복은 `strings.Builder`, writer 로는 `Fprintf`, 여러 줄 상수는 백틱. DO: 따옴표 출력은 `%q`.
