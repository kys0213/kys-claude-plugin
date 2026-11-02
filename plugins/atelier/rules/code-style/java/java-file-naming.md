---
name: java-file-naming
description: Java 코드를 역할별 클래스 이름에 두고, 새 역할 파일을 만들기 전 같은 역할의 기존 파일을 먼저 읽는 규칙
paths:
  - "**/*.java"
---

# Java 파일 이름

역할별 지침은 파일 이름으로 주입된다 — 그 이름의 파일을 Read 할 때만 로드되고 Write 로는 로드되지 않는다. 그래서 역할 코드는 아래 이름의 클래스(파일)에 두고, 새 역할 파일을 만들기 전에 같은 역할의 기존 파일을 Read 한다.

| 역할 | 파일 이름 |
|---|---|
| 테스트 | `src/test/` 아래, `*Test.java`, `*Tests.java`, `*IT.java` |
| 설정 | `*Config.java`, `*Configuration.java`, `*Properties.java`, `config/` 패키지 아래 |
| 외부 경계 | `*Client.java`, `*Adapter.java`, `*Repository.java`, `*Controller.java`, `*Handler.java`, `*Interceptor.java`, `*Filter.java`, `*Middleware.java` |

- DO: 역할 코드는 위 이름의 클래스에 둔다. 한 클래스에 역할이 섞이면 나눈다.
- DO: 새 역할 파일을 만들기 전에 같은 역할의 기존 파일을 하나 Read 한다. 기존 파일이 없으면 위 이름으로 만든 뒤 Read 한다.
