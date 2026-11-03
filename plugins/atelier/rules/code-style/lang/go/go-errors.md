---
name: go-errors
description: Go 에러 반환 위치·구체 에러 타입 반환 금지·ok bool·에러 문자열 형식·처리 흐름·errors.Is/As·%w 와 %v
paths:
  - "**/*.go"
---

# Go 에러

출처: decisions §Returning errors, §In-band errors, §Error strings, §Handle errors, §Indent error flow, best-practices §Logging errors

- DO: `error` 는 마지막 반환값. 에러가 나면 다른 반환값은 (문서에 없는 한) 쓰지 않는다.
- DON'T: 구체 에러 타입 반환 (nil 이 non-nil 이 된다). 실패를 값으로만 알릴 수 없으면 `ok bool` 이나 `error` 를 더한다.
- DO: 에러 문자열은 소문자 시작, 끝 구두점 없음 (로그·UI 의 완성 메시지는 예외).
- DON'T: 에러를 `_` 로 버린다 (실패 불가가 문서화된 경우만, 주석과 함께).
- DO: 종료로 끝난 블록 뒤 `else` 금지. 여러 줄에서 쓸 변수는 `if x, err := f();` 밖에서 선언.
- DO: 구분은 `errors.Is`/`As`.
- 권장: 검사가 필요하면 `"...: %w"`, 보여주기만 하면 `%v`.
