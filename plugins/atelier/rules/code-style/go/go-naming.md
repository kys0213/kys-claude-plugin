---
name: go-naming
description: Go 식별자·패키지·리시버·상수·약어·함수·변수 이름 규칙과 반복·섀도잉 방지
paths:
  - "**/*.go"
---

# Go 이름

출처: guide §MixedCaps, decisions §Underscores, §Constant names, §Package names, §Receiver names, §Initialisms, §Getters, §Repetition, §Variable names, best-practices §Function and method names, §Shadowing

- DO: `MixedCaps`, 지역 변수는 비공개로. DON'T: 밑줄 (테스트 함수·생성 코드 import·cgo 만 예외, 파일명은 무관).
- DO: 상수는 역할로 (`MaxPacketSize`). DON'T: `MAX_LENGTH`, `kMax`, `Twelve`.
- DO: 패키지는 소문자·숫자만 붙여 (`tabwriter`). DON'T: 밑줄·대문자, 흔한 변수와 겹치는 이름 (`count`).
- DO: 리시버는 한두 글자로 모든 메서드에서 같게. DON'T: `this`·`self`·긴 이름·`_` (안 쓰면 생략).
- DO: 약어는 같은 대소문자 (`URL`, `ID`, `xmlAPI`). 소문자 섞인 약어는 첫 글자를 바꿀 때 전체를 맞춘다 (`gRPC`→`GRPC`). DON'T: `Url`, `Id`.
- DON'T: `Get` 접두사 (HTTP GET 예외). DO: 비싸거나 실패하면 `Compute`·`Fetch`.
- 권장: 값을 돌려주면 명사형, 동작하면 동사형. 타입만 다르면 끝에 (`ParseInt64`), 기본판은 생략.
- DON'T: 사용처가 이미 주는 정보 반복 (`widget.NewWidget` → `New`, `Config.WriteConfigTo` → `WriteTo`). 같은 이름끼리 구분할 때만 더한다.
- DO: 길이는 스코프에 비례·사용 횟수에 반비례, 담는 것·쓰임으로 짓는다. DON'T: 타입을 넣거나(`userSlice`; 두 형태 공존 시 `limitRaw` 만) 글자를 빼지(`Sbx`) 않는다.
- 한 글자 이름은 리시버, 루프 인덱스·좌표, 관용 이름(`r`·`w`)에만.
- 권장: 블록 안 `:=` 는 바깥 변수를 가린다 — 바깥 값을 바꾸려면 `=`. 표준 패키지 이름(`url`)을 변수로 쓰지 않는다.
