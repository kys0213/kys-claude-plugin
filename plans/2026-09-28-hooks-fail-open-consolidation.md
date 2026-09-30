# hooks 모듈의 fail-open 처리를 한 함수로 모으고 주석 규칙에 판별 문장을 더한다

> **Plan — 이 시점의 결정 기록.** 현재 정책의 신뢰 소스가 아니다.
> 날짜: 2026-09-28 · 브랜치: claude/ecstatic-hawking-e8ov7q · 이슈: 없음 · 승인 출처: 사용자 승인

## 요구사항

1. `plugins/atelier/hooks/register.ts` 의 fail-open(CLI 실패 시 원래 결과를 그대로 돌려줌)을 한 함수로 모은다. 실패를 `undefined` 로 여러 층에 들고 다니지 않고, 실패하면 곧바로 결과를 바꾸지 않는 중립값을 돌려준다.
2. 모은 뒤 파일 상단의 fail-open 주석을 지운다.
3. `.claude/rules/comments.md` 1절에 "흩어진 계약을 주석으로 설명해야 한다면, 먼저 그 로직을 한곳으로 모을 수 있는지 본다. 모을 수 있으면 모으고 주석은 쓰지 않는다" 를 더한다.

## 현재 상태

- 실패 신호 `undefined` 가 세 층을 지난다 — `runOrchestrator`(호출 실패·exit≠0·JSON 파싱 실패 3분기), `spawnCheck`·`compactNote`(응답 모양 불일치), 두 hook(각자 `undefined` 검사 후 원래 결과·`next(e)` 반환).
- 계약이 어느 함수에도 온전히 드러나지 않아 상단 주석이 그것을 메우고 있었다. 사용자가 이를 로직 파편화로 지적했다.
- `claude plugin test` 10건이 바이너리 없음·exit≠0·비 JSON·deny·짝 없는 spawn·compaction main/subagent 를 이미 고정한다.

## 검토한 대안

| 대안 | 판단 |
|---|---|
| 주석 유지 | 버림 — 구조 문제를 설명으로 가린다 |
| `Result` 류 타입으로 실패를 명시해 hook 까지 전달 | 버림 — hook 이 실패를 알 필요가 없다. 모든 실패의 처리가 같으므로 경계에서 중립값으로 흡수하는 편이 분기가 적다 |
| 실패를 경계(`askCli` 류 한 함수)에서 중립값으로 흡수 (채택) | try 하나로 실패 처리가 모이고, hook 에는 도메인 조건만 남는다 |

## 결정

1. 실패 처리를 한 함수로 모은다 — 인자로 서브커맨드, 응답 decoder, 중립값, stdin 을 받는다. 호출 실패·exit≠0·JSON 파싱 실패·decoder 불일치 전부 중립값을 돌려준다.
2. `spawnCheck` 의 중립값은 빈 경고 목록, `compactNote` 는 빈 문구를 뜻하는 값. hook 은 "경고가 있으면 붙인다", "문구가 있으면 붙인다" 만 남긴다. deny·짝 없는 spawn·subagent compaction 분기는 도메인 조건이라 그대로 둔다.
3. 동작 불변 — 기존 `claude plugin test` 10건이 수정 없이 통과해야 한다.
4. 커밋 type 은 `refactor(atelier)`.

## 작업 순서

1. 이 plan 을 `docs` 커밋으로 남긴다.
2. task 1건 — worktree 격리.
3. 게이트 — reviewer 1관점(동작 불변·실패 처리 단일화·남은 주석·규칙 문장).
4. ff-only 머지 → 통합 검증 → push.
