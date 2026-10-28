---
name: go-file-naming
description: Go 코드를 역할별 파일명에 두고, 새 역할 파일을 만들기 전 같은 역할의 기존 파일을 먼저 읽는 규칙과 역할별 핵심
paths:
  - "**/*.go"
---

# Go 파일 이름

역할별 지침은 파일 이름으로 주입된다 — 그 이름의 파일을 Read 할 때만 로드되고 Write 로는 로드되지 않는다. 그래서 역할 코드는 아래 이름의 파일에 두고, 새 역할 파일을 만들기 전에 같은 역할의 기존 파일을 Read 한다.

| 역할 | 파일 이름 | 어디서나 지킬 핵심 |
|---|---|---|
| 테스트 | `*_test.go` | 표준 `testing`, 메시지는 got/want |
| 진입점 | `main.go`, `cmd/**` | 플래그·`import _`·`log.Fatal`·`os.Exit` 는 진입점에만 |
| 에러 정의 | `errors.go`, `error.go`, `*_errors.go` | 가를 에러는 sentinel·타입으로. `panic` 은 에러 처리에 쓰지 않고 `MustXxx` 는 초기화 시점에만 |
| 설정 | `config.go`, `options.go`, `config/**` | 긴 인자 목록은 option struct 로 |
| 외부 경계 | `*_client.go`, `*_adapter.go`, `*_repository.go`, `*_handler.go`, `*_middleware.go` | 인터페이스는 소비자가 작게. PII 를 로그하지 않는다 |
| 동시성 | `*_worker.go`, `*_pool.go`, `*_scheduler.go` | 고루틴은 종료 조건을 함께 정한다. 기본은 동기 API |
| 패키지 문서·공개 API | `doc.go` | doc 주석은 이름으로 시작하는 문장. 전역 상태 API 금지 |
