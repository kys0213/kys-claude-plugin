# 플러그인 간 의존 선언 규칙을 정한다

> **Plan — 이 시점의 결정 기록.** 현재 정책의 신뢰 소스가 아니다.
> 날짜: 2026-10-05 · 브랜치: docs/plugin-dependency-rule · 이슈: #738 · 승인 출처: council pass (협의체 3라운드 검증 pass — 2라운드 한도를 사용자 승인으로 1회 추가)

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

- #738 남은 축: (2) 묵시적 의존 확인 · 의존 메커니즘 결정(공식 vs 자체) · (5) 신규 플러그인 컨벤션 문서. (1) dead plugin 은 정리 끝 (#764·#768 머지 — `gh pr view 764 --json state`, `gh pr view 768 --json state` → MERGED), (3)(4) 중복 책임·테마 묶기는 같은 통합 작업에서 atelier 로 흡수됐어요 (`git log --format=%s -- .claude/rules` 의 `refactor(plugins): remove plugins absorbed into atelier (#839)`).

## 사이드이펙트 조사

- **플러그인 간 묵시적 의존: 없음.** 축을 바꾼 두 전략으로 확인했어요.
  - 전략 1 (호출 형태): 각 플러그인 디렉토리에서 다른 플러그인의 `name:x`·`/name:x`·`plugins/name/` 참조 grep → 6개 플러그인 모두 0건 (`git grep -nE "(<others>):[a-z]|/(<others>):|plugins/(<others>)/" -- plugins/<p>`).
  - 전략 2 (이름 단어): 다른 플러그인 디렉토리에서 각 플러그인 이름 단어 grep → 0건 (`git grep -lw -- <name> -- plugins | grep -v "^plugins/<name>/"`).
  - 심볼릭 링크 0건 (`find plugins common -type l`).
- `dependencies`/`requires` 필드 0건 (`git grep -n '"dependencies"\|"requires"' -- '*plugin.json' .claude-plugin/marketplace.json`).
- 공식 메커니즘: `plugin.json` 의 `dependencies` 배열, bare name 은 같은 마켓플레이스, 버전 범위는 `<plugin>--v<version>` git tag 필요 (facts-external.md "plugin dependencies").
- 이 레포에 `<plugin>--v<version>` tag 없음 (`git tag -l '*--v*'` → 0건, 전체 tag 257개).
- 플러그인 → 공유 코드 결합은 1건 있어요 (플러그인 간 의존 아님): external-llm 의 agent·command 가 `${CLAUDE_PLUGIN_ROOT}/../../common/scripts/call-*.sh` 를 호출해요 (`plugins/external-llm/agents/code-reviewer-codex.md:39`, `llm-reviewer-codex.md:27`, `code-reviewer-gemini.md:41`). marketplace 에서 external-llm 만 `strict: false` 이고 (`jq -c '.plugins[] | {name, strict}' .claude-plugin/marketplace.json`), `tools/validate` 가 `strict: false` 플러그인의 `../../` 경로를 repo root 기준으로 허용해요 (`tools/validate/internal/path/validator.go:214-217`). 즉 의도된 레포 내 컨벤션이에요.
- 신규 플러그인 등록 형식은 `tools/validate` 가 CI 로 검사해요 (`tools/validate/main.go`, `internal/{path,version,spec}` 가 `plugin.json`·`marketplace.json` 을 다룸 — `git grep -ln 'marketplace.json\|plugin.json' -- tools/validate`).

## 결정과 근거

1. **의존 메커니즘은 공식 `plugin.json` `dependencies` 를 쓴다. 자체 컨벤션은 만들지 않는다.** 공식 기능이 자동 설치·비활성화까지 해 주므로 자체 필드를 둘 이유가 없어요.
2. **지금은 어떤 `dependencies` 필드도 추가하지 않는다.** 실제 의존이 0건이에요.
3. **`.claude/rules/plugin-governance.md` 를 짧게 신설**해 다음 교차 호출이 묵시적으로 생기는 것을 막아요 (#738 이 epic 을 연 이유가 "verify 가 또 하나의 묵시적 의존을 추가" 였어요).
   - `paths:` — `plugins/*/.claude-plugin/plugin.json`, `.claude-plugin/marketplace.json`, `plugins/*/skills/**`, `plugins/*/commands/**`, `plugins/*/agents/**` (교차 호출은 스킬·커맨드·에이전트 본문에서 생겨요).
   - 내용 (4줄 안팎): 다른 플러그인의 skill·command·agent 를 호출하면 호출하는 쪽 `plugin.json` 에 `"dependencies": ["<plugin>"]` 을 선언한다 / bare name 만 쓰고 버전 범위는 쓰지 않는다(이 레포는 `<plugin>--v<version>` tag 를 만들지 않음) / 의존을 추가·제거하는 PR 은 설명에 이유를 남긴다.
   - 등록 형식(필수 파일·필드)은 적지 않아요 — `tools/validate` 가 CI 로 집행하므로 문서로 복제하면 사본이 어긋나요.
4. **#738 은 이 PR 로 close.** (1)(3)(4) 는 완료 근거(#764·#768·#839)를 PR 본문에 인용해요.

## 버린 대안

| 대안 | 이유 |
|---|---|
| 자체 `requires` 필드·마켓플레이스 컨벤션 | 공식 기능이 있어요 (facts-external.md). 자체 필드는 아무도 해석하지 않아 조용히 무시돼요 |
| 문서 없이 이슈만 close | 다음 교차 호출 작성자가 선언 의무를 알 경로가 없어요. 규칙 파일은 매칭 경로를 Read 할 때만 로드돼 평소 비용이 없어요 |
| 교차 호출 미선언을 잡는 CI 검사 추가 | 현재 의존 0건이라 검사할 대상이 없어요 (YAGNI, Rule of 3) |
| 버전 범위 사용 + `claude plugin tag --push` 도입 | release 파이프라인에 tag 단계를 새로 넣어야 해요. 상대경로 플러그인은 마켓플레이스 현재 사본을 쓰므로 같은 레포 안에서는 범위가 필요 없어요 |
| external-llm 의 `../../common` 결합을 이 PR 에서 정리 | 플러그인 간 의존이 아니라 공유 스크립트 배포 문제라 #738 범위 밖이에요. 아래 에스컬레이션 E1 로 넘겨요 |
| 신규 플러그인 등록 체크리스트 문서화 | `tools/validate` 가 집행하는 내용의 복제예요 |

## PR 타이틀 / 브랜치

- `docs(rules): add plugin dependency rule`
- `docs/plugin-dependency-rule`
- type 근거: `plugins/`·`common/` 변경 없음 → 버전 범프 불필요, 문서라 `docs` (`.claude/rules/git-workflow.md:23`, `:33` 플러그인 외 변경은 디렉토리 이름 scope).



## 가정

- **C1** — 상대경로(`./plugins/...`) 플러그인 간 bare name 의존은 마켓플레이스 현재 사본을 쓰므로 버전 범위 없이 충분하다. 근거: facts-external.md "relative-path plugin 은 marketplace 현재 사본 사용".
