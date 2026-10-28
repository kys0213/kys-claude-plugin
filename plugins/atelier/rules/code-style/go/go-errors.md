---
name: go-errors
description: Go 에러 반환·in-band 금지·에러 문자열·처리 흐름·errors.Is/As 판별, 반환한 에러의 중복 로깅 금지
paths:
  - "**/*.go"
---

# Go 에러

출처: decisions §Returning errors, §In-band errors, §Error strings, §Handle errors, §Indent error flow, best-practices §Logging errors

- DO: `error` 는 마지막 반환값. 에러가 나면 다른 반환값은 (문서에 없는 한) 쓰지 않는다.
- DON'T: 구체 에러 타입 반환 (nil 이 non-nil 이 된다), `-1` 같은 in-band 실패값 (`ok bool` 을 더한다).
- DO: 에러 문자열은 소문자 시작, 끝 구두점 없음 (로그·UI 의 완성 메시지는 예외).
- DO: 에러는 처리·반환·(예외적으로) 종료 중 하나로 다룬다. DON'T: `_` 로 버리기 (실패 불가가 문서화된 경우만, 주석과 함께).
- DO: 에러를 먼저 처리하고 정상 흐름은 들여쓰기 없이. 종료로 끝난 블록 뒤 `else` 금지. 여러 줄에서 쓸 변수는 `if x, err := f();` 밖에서 선언.
- DO: 구분은 `errors.Is`/`As`. DON'T: 메시지 문자열 비교.
- 권장: 검사가 필요하면 `"...: %w"`, 보여주기만 하면 `%v`. 아래 에러가 가진 정보나 "failed" 를 덧붙이지 않는다.
- 권장: 반환하는 에러는 로그하지 않는다.
