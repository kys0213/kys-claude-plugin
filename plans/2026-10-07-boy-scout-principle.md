# 편집하는 파일을 규칙에 맞게 정리하는 보이스카웃 원칙을 넣는다

> **Plan — 이 시점의 결정 기록.** 현재 정책의 신뢰 소스가 아니다.
> 날짜: 2026-10-07 · 브랜치: claude/beautiful-gates-610wax · 이슈: 없음 · 승인 출처: council pass (협의체 2라운드 검증 pass, 입력 spec 은 사용자 승인)

## 배경

규칙이 생기기 전에 쓰인 코드 위에 기능을 더할 때, 에이전트가 그 파일을 규칙에 맞게 정리하지 않고 기존 패턴을 그대로 따라 해요. 사용자는 보이스카웃 원칙(손댄 곳은 더 낫게 남긴다)을 기본으로 원해요.

원인은 지침이 한쪽으로 기울어 있어서예요.

- grill 설계 생성 지침이 "기존 패턴을 따른다", "무관한 리팩터링은 제안하지 않는다"를 조건 없이 말해요 (`plugins/atelier/skills/grill/references/design-generation.md:108`, `:110`).
- orchestrator 표 6 의 reviewer 관점이 "초과 구현"을 지목하고, 규칙 준수 관점은 변경 줄만 대조해요 (`plugins/atelier/skills/orchestrator/SKILL.md:139`).
- 이 레포의 `.claude/rules/comments.md:56` 은 "손대지 않는 줄을 규칙 때문에 일괄 수정하지 않는다" 예요.
- 규칙과 기존 패턴이 부딪힐 때 어느 쪽이 이기는지 정한 곳이 없어요.

## 요구사항

- 입력 spec: `plugins/atelier/spec/concerns/boy-scout.md` (사용자 승인 원문, 미결 없음). 정책 7항 · 적용 지점 6행 · 실패 처리 2항 · 제약 2항을 구현해요.
- 대화에서 확정한 결정:
  - D1 정책 본문의 단일 출처는 템플릿 CLAUDE.md (`plugins/atelier/templates/claude-md/CLAUDE.md`, setup 이 `~/.claude/CLAUDE.md` 의 `[coding-style]` 블록으로 주입). orchestrator · grill 은 가리키기만 해요.
  - D2 정리 범위는 편집하는 파일 전체 (사용자 선택. "확장 지점만" 안은 버림).
  - D3 정리는 기능과 분리된 선행 task · 별도 커밋.
  - D4 `.claude/rules/comments.md` §기존 코드를 편집 파일 안의 주석은 정리 대상이 되도록 고쳐요 (사용자 결정).
  - D5 spec 은 concern 단독으로 저장하고, `related_paths` 는 `[]` 로 저장한 뒤 구현이 끝나면 실제 변경 경로로 채워요 (사용자 결정).

## 사이드이펙트 조사

- 템플릿 내용을 고정하는 테스트는 없어요. drift 테스트는 인메모리 픽스처를 쓰고 (`plugins/atelier/cli/tests/drift_mocks.rs:29`), 실제 파일을 읽는 테스트는 orchestrator 표 1 만 파싱해요 (`plugins/atelier/cli/tests/orchestrator_tier_table.rs`). 그래서 템플릿 검증은 heading grep 으로 해요.
- 이미 설치한 사용자는 템플릿이 바뀌면 drift 가 `DRIFTED` 가 되고 `/atelier:update` 로 맞춰져요. 의도된 동작이에요.
- `tools/validate` 는 `plugins/**/*.md` 의 마크다운 링크만 검사해요. spec 원문에는 링크가 없어요.
- orchestrator 문서는 `.claude/rules/decision-graph.md` 규약과 `scripts/check-decision-graph.sh` 줄 수 상한을 따라요. 편집 전 163 / 242 / 101, 편집 후 예상 164 / 244 / 101 (총 509) 로 상한 안이에요. 새 `##` 절과 수치는 넣지 않아요.
- 템플릿 `:164` 와 루트 `CLAUDE.md` 의 코드 품질 게이트("내가 변경하지 않은 부분도 lint·test 실패면 수정")가 새 제약 "편집하지 않는 파일은 바꾸지 않는다"와 나란히 놓여요. 관계를 템플릿 새 절에 명시해요 (가정 A6).
- 확인한 사실: subagent 는 project CLAUDE.md 를 주입받아요 (2026-10-07 subagent 로 확인). user 레벨 `~/.claude/CLAUDE.md` 는 이 환경에 파일이 없어 확인하지 못했어요 (가정 A1).
- 이 변경의 편집 대상 파일에는 정리할 규칙 위반이 없어요. 선행 정리 task 는 0개예요.

## 결정과 근거

1. **템플릿 CLAUDE.md 에 `## 보이스카웃 원칙 (Boy Scout)` 절을 새로 둬요** — `## 코드 품질 게이트` 뒤, `## 작업 마무리 습관` 앞. 정책 본문은 여기 한 곳에만 있어요. bullet 은 우선순위 · 범위 · 범위 밖 · 동작 고정 테스트 · 순서 · 프로젝트 규칙 우선 · YAGNI와 구분 · 편집하지 않는 파일 · 비적용(저장소 파일을 편집하지 않는 작업) 순이에요.
   - 템플릿 첫 줄(YAGNI)과 `## 작업 마무리 습관` 은 새 절을 가리키는 구절만 덧붙여요. 마무리 정리의 범위는 편집한 파일로 묶어요.
   - 근거: 보이스카웃은 "어떻게 일하는가"라 rules 계층이고, 템플릿은 orchestrator 가 열리지 않는 단발 편집까지 모든 세션에 적용돼요. 프로젝트 규칙으로 덮을 수도 있어요.
2. **grill 설계 생성 지침은 가리키기만 해요** — "기존 패턴을 따르되, 규칙과 어긋나는 패턴은 보이스카웃 원칙", 편집 대상 파일의 규칙 위반을 설계에 정리 항목으로 넣는 한 줄 추가, "무관한 리팩터링은 제안하지 않는다" 원문 유지 + "편집 대상 파일의 규칙 정리는 무관한 리팩터링이 아니다", "가차없는 YAGNI" 에 관계 가리킴.
3. **orchestrator 는 실행 장치만 가져요**
   - `SKILL.md` 표 6: reviewer 의 "초과 구현"에서 선행 정리 task 와 범위 안 정리를 제외해요. 규칙 준수 관점은 편집한 파일 **전체**를 규칙 원문(CLAUDE.md · `.claude/rules/*`)과 대조하고, 범위 밖으로 보고된 항목을 뺀 위반은 reject 해요. 기존 `문서 · 설정` 행 뒤에 `선행 정리 task` 행을 더해요 (행 1·2 보다 우선. 공개 계약 변화 없음 + 규칙 준수, 실행 코드는 기존 테스트 green, 문서·설정은 요구사항↔산출물).
   - `procedures.md` 자율 실행 루프 3(분해): 편집 대상 파일마다 규칙 위반을 조사해 선행 정리 task 를 도출한다는 문장을 더해요.
   - `contracts.md`: §task 도출 계약에 선행 정리 task 를 파일마다 하나 만들어 그 파일을 편집하는 task 의 의존성에 거는 독립 문단을 더해요. §prompt 필수 포함 요소 4번(범위) 행에 편집 파일 목록을 정리 범위로 싣고 범위 밖은 보고, 정리·기능 커밋 분리를 싣게 해요. §종료 핸드오프 미해결 항목에 범위 밖 위반을 넣어요.
   - 근거: 정책을 다시 쓰지 않고 분해 · 게이트 · 보고라는 오케스트레이션 고유 장치만 둬요. 실패 처리 1(정리 task red → reject · 재위임, 기능 task 미시작)은 의존성과 게이트 reject 라는 기존 기제로 충족돼요.
4. **`.claude/rules/comments.md` §기존 코드** — 편집하는 파일 안의 주석 전체에 적용하고, 편집하지 않는 파일의 주석은 수정하지 않으며, 정리는 기능과 다른 커밋으로 나눈다고 고쳐요. 이 레포 규칙은 atelier 를 설치하지 않은 기여자도 읽으므로 템플릿을 가리키지 않고 자기완결 문구로 써요.
5. **spec 파일** — 승인 원문을 그대로 저장하고, 구현 머지 뒤 `related_paths` 만 실제 변경 경로로 채워요. 레포 운영 규칙(`.claude/rules/`)과 `plans/` 는 코드 영역 힌트가 아니라서 빼요 (가정 A5).

작업 순서: spec 저장 · 템플릿+주석 규칙 · grill · orchestrator 는 파일이 겹치지 않아 병렬로 돌고, `related_paths` 채우기는 마지막에 순차로 돌아요. 최종 검증은 epic HEAD 에서 `bash scripts/check-decision-graph.sh && make validate-ci` 예요.

## 버린 대안

| 대안 | 이유 |
|---|---|
| 정리 범위를 "확장 지점"(이번 변경이 기대거나 확장하는 코드)으로 한정 | 사용자가 편집 파일 전체를 골랐어요 |
| 정책 본문을 orchestrator contracts 에 새 절로 두기 (첫 설계안) | orchestrator 가 열리지 않는 단발 편집에는 적용되지 않아요. 원칙은 rules 계층인 템플릿으로 |
| 정책 본문을 orchestrator · grill 에 복제 | 사본끼리 어긋나요 (D1) |
| 표 6 새 행을 `문서 · 설정` 행 앞에 두기 | 새 행이 행 2 가 돼서 "행 1·2 보다 우선"이 자기 자신을 가리켜요 |
| 범위 · 출구를 prompt 필수 포함 요소 16번 행에 싣기 | 같은 셀의 "규칙을 요약·재서술하지 않는다"와 모순돼요. 4번 "범위"가 맞는 자리예요 |
| 문서 · 설정 파일도 테스트 하네스가 없으니 범위 밖 | atelier 산출물 대부분이 markdown 이라 원칙이 무력해져요 (가정 A3) |
| 코드 품질 게이트 bullet 도 함께 고치기 | 관계를 두 곳에 쓰면 사본이 생겨요. 새 절 한 곳에만 둬요 |
| orchestrator 에 새 `##` 절 | heading 화이트리스트를 바꿔야 하고 판단 대본이 될 위험이 있어요 |
| 규칙 준수 관점을 CLAUDE.md 만 있는 레포에도 세우기 | 승인 spec 범위 밖이에요. 후속 후보로 남겨요 |
| 루트 레포 `CLAUDE.md` 에 원칙 추가 | 이 레포 기여 지침이 아니라 atelier 제품 변경이에요 (D1) |

## 가정

- **A1** — user 레벨 `~/.claude/CLAUDE.md` 도 위임 agent 에 주입된다. project 레벨 주입만 확인했어요. 틀려도 contracts 4번 행이 범위 · 출구 · 커밋 분리를 prompt 로 실어요. atelier setup 이 템플릿을 주입한 환경을 전제해요.
- **A3** — 동작 고정 테스트 전제는 실행 코드에만 걸려요. 테스트 대상이 아닌 문서 · 설정 파일은 고정할 동작이 없어 그 전제 없이 정리 범위 안이에요. 단, hooks 설정이나 `plugin.json` 처럼 동작에 영향을 주는 설정은 "동작을 바꾸는 정리는 범위 밖" 조항이 막아요. spec 해석이라 종료 보고에 표시해요.
- **A5** — `related_paths` 에서 `.claude/rules/comments.md` 를 빼요. related_paths 는 코드 영역 힌트이기 때문이에요.
- **A6** — lint · format · test 실패 수정은 코드 품질 게이트 몫이고 보이스카웃 정리가 아니에요. "편집하지 않는 파일은 바꾸지 않는다"는 자동 검증이 잡지 않는 규칙 정리에 대한 제한이에요. spec 해석이라 종료 보고에 표시해요.

## PR 타이틀 / 브랜치

- `feat(atelier): add boy scout cleanup principle`
- `claude/beautiful-gates-610wax`
- type 근거: `plugins/` 변경이라 버전 범프 type 이 필요하고, 사용자가 받는 작업 원칙이 늘어나는 기능 추가라 `feat`(minor) 예요 (`.claude/rules/git-workflow.md` 버전 범프 표).
