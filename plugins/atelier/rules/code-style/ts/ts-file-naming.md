---
name: ts-file-naming
description: TypeScript·JavaScript 코드를 역할별 파일명에 두고, 새 역할 파일을 만들기 전 같은 역할의 기존 파일을 먼저 읽는 규칙
paths:
  - "**/*.ts"
  - "**/*.tsx"
  - "**/*.mts"
  - "**/*.cts"
  - "**/*.js"
  - "**/*.jsx"
  - "**/*.mjs"
  - "**/*.cjs"
---

# TypeScript·JavaScript 파일 이름

역할별 지침은 파일 이름으로 주입된다 — 그 이름의 파일을 Read 할 때만 로드되고 Write 로는 로드되지 않는다. 그래서 역할 코드는 아래 이름의 파일에 두고, 새 역할 파일을 만들기 전에 같은 역할의 기존 파일을 Read 한다.

| 역할 | 파일 이름 (`.ts`·`.js`, 테스트는 `.tsx`·`.jsx` 도) |
|---|---|
| 테스트 | `*.test.ts`, `*.spec.ts`, `__tests__/` 아래 |
| 설정 | `config.ts`, `config/` 아래 |
| 외부 경계 | `*.client.ts`, `*.adapter.ts`, `*.repository.ts`, `*.handler.ts`, `*.middleware.ts` (`-client.ts` 처럼 하이픈도 같다) |

- DO: 역할 코드는 위 이름의 파일에 둔다. 역할이 섞이면 나눈다.
- DO: 새 역할 파일을 만들기 전에 같은 역할의 기존 파일을 하나 Read 한다. 기존 파일이 없으면 위 이름으로 만든 뒤 Read 한다.
- DON'T: 도구 설정 파일(`vite.config.ts` 등)을 설정 계층으로 쓴다. 애플리케이션 설정은 `config.ts` 에 모은다.
