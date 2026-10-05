# 위임 prompt 가 레포 규칙을 요약 대신 경로로 지목하게 한다

> **Plan — 이 시점의 결정 기록.** 현재 정책의 신뢰 소스가 아니다.
> 날짜: 2026-10-05 · 브랜치: fix/orchestrator-rule-source · 이슈: #907, #807 · 승인 출처: council pass (협의체 3라운드 검증 pass — 2라운드 한도를 사용자 승인으로 1회 추가)

남은 이슈 5건을 작업 4개(A 규칙 근거 위임 · B 주석 정리 · C 플러그인 거버넌스 · D Stop hook 메시지 전달)로 나눠 각각 독립 PR 로 진행했어요. 이 plan 은 그중 한 작업의 기록이에요.

## 작업 간 파일 분할

| 파일 | 소유 작업 | 근거 |
|---|---|---|
| `plugins/atelier/cli/src/session/mod.rs` | D 전부 (주석 정리 포함) | D 가 `emit()`(L105-110)·모듈 문서(L20-24)를 고쳐요. B 대상 주석 L57·64·71·105·181 도 같은 파일이에요 |
| `plugins/atelier/cli/src/session/commands/simplify.rs` | D 전부 (주석 정리 포함) | D 가 렌더 함수를 더해요. B 대상 주석 L140-141 이 `#725` 를 인용해요. 패턴 밖 이력 주석 L120-121 도 D 가 정리해요 |
| `plugins/atelier/cli/tests/session_cli_simplify.rs` (신규) | D | `emit` 배선 테스트 (D 결정 7) |
| `common/**` | 어느 작업도 편집 안 함 | B 의 `call-*.sh:23` 은 "남긴다" 판정 |
| `plugins/atelier/hooks/suggest-simplify.sh` | B | D 는 shim 을 건드리지 않아요 (shim 은 `exec atelier session simplify-check` 만 해요, `plugins/atelier/hooks/suggest-simplify.sh:13`) |
| `plugins/atelier/skills/orchestrator/**` | A | 다른 작업은 건드리지 않아요 |
| `.claude/rules/plugin-governance.md` (신규) | C | 다른 작업은 건드리지 않아요 |
| `.claude/rules/comments.md` | B (변경 없음으로 결정, §B 결정 4) | — |

- 버린 대안: "B 는 `mod.rs:105-106` 만 빼고 나머지 줄은 정리" — 같은 파일 다른 hunk 라 git 이 자동 병합할 가능성은 높지만, D 의 모듈 문서·`emit` 수정 범위가 구현 중 넓어지면 인접 hunk 충돌이 생겨요. 파일 단위로 나누면 판정이 결정적이에요.



## 요구사항

- #907: 위임 prompt 에 레포 규칙을 **요약해 싣지 않고** "주입되는 규칙 원문이 기준" + 규칙 파일 경로 지목으로 바꿔요. 검토 게이트 agent 가 대상 파일을 **Read 도구로 열게** 해요 (`gh issue view 907` 본문 "무엇을" 표 1~3행).
- #807: `.claude/rules` 가 있는 레포에서 rule 을 기준으로 변경 코드를 대조하는 **명시적 책임 주체**를 둬요 (`gh issue view 807` "제안").
- 두 이슈의 공통 뿌리: 규칙이 "주입은 되지만 아무도 그걸로 판정하지 않는" 상태. 하나의 계약으로 묶어요 — **규칙의 출처는 원문 하나, 원문을 로드시키는 수단은 Read, 원문으로 판정하는 책임은 게이트의 한 관점.**

## 사이드이펙트 조사

- `contracts.md` 의 prompt 필수 포함 요소 표는 1~15번이고 "재배열하지 않는다" 고 적혀 있어요 (`plugins/atelier/skills/orchestrator/references/contracts.md:38`, 마지막 행 `:56`). → 16번으로 끝에 추가해요.
- 번호로 표를 지목하는 외부 참조는 3·7·9번뿐이에요 (`contracts.md:26`, `:32`). 개수("15개")에 의존하는 참조 없음 — `git grep -nE '필수 포함 요소 ?(1[0-9]|[0-9])|15 ?개' -- plugins/atelier spec scripts tools .claude` 결과에 해당 없음.
- 검토 게이트 정의는 `SKILL.md` 표 6 (`plugins/atelier/skills/orchestrator/SKILL.md:137-146`)과 불변식 15 (`SKILL.md:30`)예요. 표 6 첫 문단이 관점 예시를 나열해요 (`SKILL.md:138`).
- 현재 orchestrator 문서에 `.claude/rules` 언급은 브랜치 규약·기록 참고 소스·설정 규약 세 곳뿐이고, 규칙 전달·판정 계약은 없어요 (`grep -rn -i 'claude/rules\|경로 규칙\|paths:' plugins/atelier/skills/orchestrator` → `contracts.md:25`, `:68`, `:188`).
- 문서 규약: `.claude/rules/decision-graph.md` 가 orchestrator 문서를 "불변식·정지 조건·기본값 표·환경 계약·절차" 형식으로 제한하고, `scripts/check-decision-graph.sh` 가 줄 수 상한을 검사해요 — SKILL 200 / contracts 250 / procedures 160 / 총합 600 (`scripts/check-decision-graph.sh:76-79`). 현재 162 / 241 / 101 / 504 (`wc -l`). contracts 여유는 9줄이에요.
- 경로 규칙 로딩 조건: `paths:` 규칙은 Read 도구로 맞는 파일을 열 때만 로드돼요 (repo `CLAUDE.md` "작업 전 Read 필수" 절). #907 이 transcript 로 구현 agent 쪽 자동 주입을 확인했어요.
- orchestrator 스펙 문서 없음 — `ls plugins/atelier/spec` → 없음, `git grep -ln orchestrator -- spec` → 없음.

## 결정과 근거

1. **`contracts.md` §prompt 필수 포함 요소에 16번 행 1줄 추가** — 요소 이름 "레포 규칙 출처", 싣는 형태:
   - "tracked 편집·검토 위임 시 필수. 레포 규칙(`CLAUDE.md`·`.claude/rules/*`)을 요약·재서술해 싣지 않는다 → 대신 '작업 중 주입되는 규칙 원문이 기준이다' 한 줄과 관련 규칙 파일 경로만 싣고, 편집·검토할 파일은 먼저 Read 도구로 열라고 지시한다 (경로 규칙은 Read 로 열 때만 로드된다)."
   - 근거: #907 제안 1·2 를 한 행에 담아요. 금지와 출구를 같은 행에 두므로 10번(금지에는 출구를 함께)을 따로 고치지 않아요. 검토 dispatch 도 "위임" 이라 #907 제안 3(게이트는 Read 로 열 것)도 이 행이 덮어요 — 출처를 한 곳으로 유지해요.
2. **`SKILL.md` 표 6 에서 관점 정의와 의무 조건을 나눠 담아요** — 기존 줄 안에서 고쳐 줄 수 증가 0 (`SKILL.md` 162줄 유지):
   - 관점 예시 문단(`SKILL.md:138`)에는 **정의만** 더해요: "규칙 준수(변경 파일을 Read 로 열어 로드된 `.claude/rules/*` 원문 ↔ 변경, finding 은 규칙 파일·항목 인용)". 의무 표현("반드시")은 넣지 않아요. 같은 문단 끝 "어떤 관점을 세울지는 메인이 분해 시점에 정해 기록한다" 와 부딪히지 않게 하려는 거예요.
   - **의무 조건은 표 6 의 "게이트" 열 행 1·2 에 붙여요.**
     - 행 1 (`SKILL.md:142`, 무거운 경로 · tracked 편집) 게이트 셀 끝에 "레포에 `.claude/rules/` 가 있으면 규칙 준수 관점을 포함" 을 덧붙여요.
     - 행 2 (`SKILL.md:143`, 문서 · 설정) 게이트 셀 끝에도 같은 문구를 덧붙여요.
   - 근거: 기본값 표의 셀은 "조건 → 기본값" 형식이라 의무를 담을 수 있어요(`decision-graph.md` §1 의 기본값 표 형식). 예시 문단은 선택지 목록으로 읽혀요. 세움 조건이 "디렉토리 존재 + 해당 행의 경로" 라 결정적이에요. #807 의 "명시적 책임 주체" 는 새 agent 정의 없이 게이트 관점 하나로 성립해요. 차단력은 불변식 15 가 줘요. 15 는 "세운 관점 전부 pass 여야 승급" 을 강제해요.
   - 행 3·4 (경량)에는 붙이지 않아요. 경량 경로는 tracked 편집 게이트가 아니에요.
   - 어느 규칙이 매칭되는지는 메인이 계산하지 않아요. 게이트 agent 가 파일을 Read 하면 Claude Code 가 매칭 규칙을 주입해요 — 매칭 로직을 문서에 복제하지 않아요.
3. 불변식은 추가하지 않아요. 같은 금지를 불변식과 prompt 표 두 곳에 두면 사본이 어긋나요 (`decision-graph.md` §6 의 "한 곳" 원칙과 같은 이유).

## 버린 대안

| 대안 | 이유 |
|---|---|
| 규칙 전문을 prompt 에 복사 | #907 이 이미 버림 — 토큰 낭비, 이중 출처 |
| `paths:` 제거로 규칙 상시 로드 | 이 레포 작업 규칙이라 플러그인 범위 밖이고, 모든 세션 컨텍스트 비용 증가 |
| #807 의 `rule-convention-reviewer` 전용 agent 파일 신설 | 게이트 관점은 orchestrator 가 dispatch 시 prompt 로 정의해요. agent 정의 파일을 두면 관점 정의가 두 곳이 돼요 (YAGNI) |
| #807 의 implementer DONE_REPORT 에 "적용 rule 목록 + 증명" 강제 | 자기 보고는 증거가 아니에요 — 불변식 15 가 구현 agent 의 게이트 겸임을 금지하는 것과 같은 이유. 판정은 다른 agent 의 규칙 준수 관점이 맡아요 |
| #807 의 code-review convention dimension 추가 | `code-review` 는 Claude Code 내장 스킬이라 이 플러그인이 고칠 수 없어요 |
| #807 의 autopilot 머지 게이트 채널 | autopilot 은 atelier CLI 로 흡수됐고(#768 MERGED, `gh pr view 768 --json state`), 자율 런의 머지 차단은 orchestrator 게이트(불변식 15)가 이미 해요 |
| 표 6 에 규칙 준수를 별도 행으로 추가 | 표 6 의 행은 "산출물·경로" 축이라 관점 행을 끼우면 축이 섞여요. 그래서 기존 행 1·2 의 게이트 셀에 조건을 다는 쪽을 골랐어요 |
| 의무 조건을 관점 예시 문단에 두기 (r1 초안) | 같은 문단의 "메인이 정한다" 와 충돌하고, 예시는 선택 사항으로 읽혀 강제력이 약해요 (검증 F3) |

## PR 타이틀 / 브랜치

- `fix(atelier): delegate repo rules by path, not summary`
- `fix/orchestrator-rule-source`
- type 근거: 위임 결과가 규칙을 어기는 결함 수정이라 `fix`(patch). `plugins/` 변경이라 버전 범프 type 필요 (`.claude/rules/git-workflow.md:55`).
- 머지 시 #907·#807 둘 다 close.



## 가정

- **A1** — 검토 게이트 agent 가 worktree 에서 파일을 Read 하면 그 파일에 매칭되는 `.claude/rules` 가 주입된다. 근거: #907 transcript 관측(구현 agent 에서 확인, 검토 agent 는 전수 미확인), repo `CLAUDE.md` "작업 전 Read 필수"(Claude Code 2.1.285 확인). 틀리면 결정 2 의 규칙 준수 관점이 규칙 없이 돌아요 — 그래서 16번 행이 "관련 규칙 파일 경로" 도 함께 싣게 했어요(보험).
