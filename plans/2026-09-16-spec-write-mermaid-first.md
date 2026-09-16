# spec-write 다이어그램 기준을 mermaid 우선으로 통일한다

> **Plan — 이 시점의 결정 기록.** 현재 정책의 신뢰 소스가 아니다.
> 날짜: 2026-09-16 · 브랜치: claude/awesome-mayer-gmk2jy · 이슈: #897 · 승인 출처: 사용자 승인

## 요구사항

issue #897 이 정한 넷을 그대로 받는다.

1. `skills/spec-write/references/authoring.md` 의 DESIGN.md 템플릿(전체 구조)과 flows 템플릿(흐름 다이어그램) 자리표시자를 mermaid 우선으로 바꾼다 — 컴포넌트 관계는 `flowchart`, 트리거·처리·응답 흐름은 `sequenceDiagram`.
2. `templates/spec/spec-common.md` 의 "ASCII 또는 mermaid" 를 같은 기준으로 맞춰 두 문서가 어긋나지 않게 한다.
3. ASCII 는 mermaid 로 표현이 어색한 경우(한 줄짜리 파이프라인 등)의 예외로만 남긴다.
4. 작성 절차에 문법 검증 한 줄을 추가한다 — mermaid 블록을 `.mmd` 로 추출해 `mmdc` 렌더가 성공하는지 확인.

## 현재 상태 (사이드이펙트 조사)

- **작성 sub-agent 가 따르는 문서는 `authoring.md` 하나다.** `spec-write/SKILL.md §위임 흐름` 이 dispatch prompt 에 `references/authoring.md` 기준을 싣게 하고, 그 템플릿 L48·L117 은 `{ASCII 다이어그램}` 만 말한다. 그래서 결과가 항상 ASCII 다.
- **`spec-common.md` 는 양쪽을 허용한다** — L37 "ASCII 다이어그램 또는 mermaid", L48 "ASCII flow 또는 mermaid sequence", L50 "ASCII 박스 또는 mermaid graph". L51 상태 머신 행은 형식이 비어 있다.
- **`spec-flow.md` 도 ASCII 를 명시한다** — L33 "(ASCII 시퀀스 다이어그램)", L40 "ASCII 코드 블록으로 작성", L52 "ASCII 시퀀스 또는 슈도코드만". issue 가 지목하지 않은 세 번째 출처다.
- **충돌하지 않는 것들**: `.claude/rules/decision-graph.md §4` 의 "mermaid 는 두지 않는다" 와 `scripts/check-decision-graph.sh` check 7 은 `make validate-graph` 가 도는 `skills/orchestrator/` 에만 적용된다. `grill/references/design-generation.md` L156 의 "표 / ASCII / mermaid" 는 대화 중 시각화 수단이지 spec 산출물 형식이 아니다. `tools/validate` 의 architecture 스캔은 `skills/*/SKILL.md`·`commands/*.md`·`agents/*.md` 만 대상이라 `references/`·`templates/` 변경은 검사 밖이다.
- **검증 명령 확인**: `npx -y -p @mermaid-js/mermaid-cli mmdc` 11.17.0 — 정상 diagram 은 exit 0, 깨진 diagram 은 exit 1 로 구분된다.
- **템플릿 중첩**: authoring.md 의 출력 구조는 ```` ```markdown ```` 펜스 안에 있어 그 안에 ```` ```mermaid ```` 펜스를 그대로 넣으면 바깥 펜스가 닫힌다. 자리표시자는 텍스트로 두고 형식 지시는 별도 절로 뺀다.

## 검토한 대안

| 대안 | 판단 |
|---|---|
| 프로젝트별 rules 로 mermaid 강제 | 버림 (issue 의 판단과 같다) — 기본값이 ASCII 인 채로는 rules 없는 프로젝트마다 같은 사후 변환이 반복된다 |
| issue 가 지목한 두 문서만 고친다 | 버림 — `spec-flow.md` 가 flows 작성의 세 번째 출처로 남아 요구사항 2(문서 간 정합)가 실질적으로 미충족 |
| 상태 머신 행은 형식 미지정으로 둔다 | 버림 — 같은 표의 다른 행이 전부 mermaid 종류를 명시하는데 한 행만 비면 다시 갈린다. `stateDiagram-v2` 로 명시 |
| 템플릿 펜스를 4-backtick 으로 바꿔 mermaid 예시를 중첩 | 버림 — 템플릿 형태를 바꾸는 부수 변경. 자리표시자 텍스트 + 별도 "다이어그램 형식" 절로 충분 |
| mmdc 검증을 hook/CLI 로 강제 | 버림 — issue 가 "절차에 한 줄" 로 한정했고, CLI 로 올릴 결정적 변환은 아직 없다 |

## 결정

1. `authoring.md`: `§작성 원칙` 아래에 **다이어그램 형식** 소절을 두고 (a) 대상별 mermaid 종류 표 — 컴포넌트 관계 `flowchart`, 트리거·처리·응답 `sequenceDiagram`, 상태 머신 `stateDiagram-v2`, (b) ASCII 예외 조건, (c) mmdc 검증 한 줄을 적는다. DESIGN.md·flows 템플릿 자리표시자를 `{mermaid flowchart …}` / `{mermaid sequenceDiagram …}` 로 바꾼다.
2. `spec-common.md`: L37·L48·L50·L51 을 mermaid 종류로 통일하고 ASCII 예외 문구를 한 곳(L37)에만 둔다.
3. `spec-flow.md`: L33·L40·L52 를 `sequenceDiagram` 기준으로 맞춘다.
4. `spec-write/SKILL.md §흔한 실수` 에 "ASCII 로 그림 → mermaid 가 기본, ASCII 는 예외" 한 줄을 더한다. 형식 기준의 canonical 은 여전히 authoring.md 다.
5. 커밋 type 은 `fix(atelier)` — plugin 사용자에게 버전 범프로 전달돼야 하는 동작 변경(sub-agent 산출물 형식)이다. `docs` 는 범프가 없어 배포되지 않는다.

## 작업 순서

1. 메인이 이 plan 을 `docs` 커밋으로 남기고 승인 마커를 세운다.
2. task 1건 — 네 파일 편집을 worktree 격리 sub-agent 에 위임(파일 집합이 한 도메인이라 병렬 분해하지 않는다).
3. 게이트 — 요구사항↔산출물 관점 1개, 작성 agent 와 다른 agent. 테스트 관점은 문서 산출물이라 생략.
4. rebase + ff-only 머지 → epic 최종 HEAD 에서 `make build && make test && make validate-ci` → push.
