# decision graph 검사의 로케일을 고정한다

> **Plan — 이 시점의 결정 기록.** 현재 정책의 신뢰 소스가 아니다.
> 날짜: 2026-10-06 · 브랜치: fix/decision-graph-locale · 이슈: #915 · 승인 출처: council pass (협의체 2라운드 검증에서 이 작업 단위 pass)

후속 이슈 4건(#914 · #915 · #916 · #917)을 각각 독립 PR 로 진행했어요. 이 plan 은 그중 한 작업의 기록이에요.

## 작업 간 파일 분할

| PR | 이슈 | 편집·생성·이동 파일 | 건드리지 않는 것 |
|---|---|---|---|
| A | #914 | `plugins/external-llm/agents/{code-reviewer-codex,code-reviewer-gemini,diff-collector,llm-reviewer-codex,llm-reviewer-gemini}.md`, `plugins/external-llm/commands/{invoke-codex,invoke-gemini,spec-review}.md`, `common/scripts/{call-codex,call-gemini,call-codex-review,get-diff}.sh` → `plugins/external-llm/scripts/` 로 `git mv`, `README.md` (구조 트리) | `common/scripts/render-template.sh`, `test-render-template.sh`, `tools/**`, `plans/**` |
| B | #915 | `scripts/check-decision-graph.sh` | `Makefile`, `plugins/atelier/skills/orchestrator/**` |
| C | #916 | `tools/validate/internal/repofs/glob.go`(신규), `tools/validate/internal/repofs/glob_test.go`(신규), `tools/validate/internal/{path,spec,version,architecture}/validator.go`, `tools/validate/internal/spec/validator_test.go`(테스트 추가) | `Makefile`, `tools/validate/main.go`, `.gitignore` |
| D | #917 | `plugins/atelier/cli/src/git/core/bash_classifier/vars.rs`(신규), `bash_classifier/{lexer,analyzer,mod}.rs` (`mod.rs` 는 `mod vars;` 한 줄), `plugins/atelier/cli/tests/git_core_bash_classifier.rs`, (선택) `plugins/atelier/cli/tests/git_core_guard.rs` | `guard/**` 판정 로직, `mod.rs` 공개 타입, `args.rs` |

- 겹침 점검: A 는 `plugins/external-llm`·`common`·`README.md`, B 는 `scripts/`, C 는 `tools/`, D 는 `plugins/atelier/cli` 만 건드려요.


## 요구사항
- 같은 문서에는 로케일과 무관하게 같은 판정이 나와야 해요. macOS 기본 로케일에서 `make validate-graph` 가 `checks: 8, failed: 0` 이어야 해요.

## 사이드이펙트 조사 (재현 포함)
- 설계자가 현재 main 에서 직접 실행한 결과 (`bash scripts/check-decision-graph.sh plugins/atelier/skills/orchestrator`):
  - 기본 로케일(`LANG=en_US.UTF-8`, `/usr/bin/awk` = `awk version 20200816`): `awk: towc: multibyte conversion failure` 3회 (SKILL.md 레코드 25, contracts.md 121, procedures.md 77) + `(9)` 2건·`(11)` 2건 실패, `checks: 8, failed: 2`
  - `LC_ALL=C`: towc 오류 없음, `checks: 8, failed: 0`
  - `LC_ALL=C.UTF-8`·`ko_KR.UTF-8`: towc 오류가 **여전히 2회** 나지만 결과는 `failed: 0` (우연히 판정에 영향 없는 지점) → UTF-8 로케일 고정은 해법이 아니에요. (횟수는 검증자 실측 `LC_ALL=C.UTF-8 bash scripts/check-decision-graph.sh plugins/atelier/skills/orchestrator 2>&1 | grep -an towc` 기준)
- awk 의 문자 단위 확인: `printf 'AB가나a\n' | LC_ALL=<L> awk '{print index($0,"a"), length($0)}'` 가 en_US.UTF-8·C.UTF-8·C 모두 `9 9` → macOS awk 의 `index/length/substr` 는 바이트 단위예요.
- 원인 가설 (1차 신호: towc 메시지 + `LC_ALL=C` 에서 소멸): 스크립트가 `substr(..., 1)` 로 한 바이트를 잘라낸 뒤(예: `scripts/check-decision-graph.sh:209,225,256,441-442`) UTF-8 로케일의 정규식 매칭이 그 조각을 wide char 로 바꾸다 실패하고, awk 가 그 레코드에서 중단해요. 스캐너(`:196`) 출력이 파일 중간에서 끊기면 heading 개수(11)·예산 절 범위(9)가 어긋나 오탐이 나요.
  - **제2 신호는 구현자가 확보**: 두 로케일에서 스캔 결과(`$SCAN`) 줄 수를 비교해 기본 로케일이 더 짧은지 확인해요 (task B1).
- 스크립트가 쓰는 문자 클래스는 ASCII 뿐이에요: `isword` = `/^[A-Za-z0-9_-]$/` (`:431`). 한글 비교는 전부 문자열 동일성(`==`)이라 바이트 처리여도 판정이 같아요.
- 스크립트는 이미 정렬만 `LC_ALL=C` 로 고정해요 (`:158`). 머리말은 "macOS·Linux 공통" 을 약속해요 (`:12`).
- CI 는 `ubuntu-latest` 에서 `make validate-ci` 실행 (`.github/workflows/validate.yml:14,35`). gawk 는 UTF-8 로케일에서 문자 단위, mawk·macOS awk 는 바이트 단위라 구현마다 단위가 달라요. `LC_ALL=C` 로 고정하면 세 구현 모두 바이트 단위로 같아져요.
- 셸 테스트 하네스: 레포에 셸 블랙박스 테스트 사례는 `common/scripts/test-render-template.sh:1-25` 하나뿐이에요 (`scripts/` 아래 테스트 없음 — `ls scripts` → `check-decision-graph.sh hooks`). 2번째 사례 NOT FOUND — searched with `ls scripts tests`, `grep -rln check-decision-graph .`.

## 결정과 근거
- **스크립트 시작부(`set -u` 다음, `:14` 부근)에 `export LC_ALL=C` 를 넣어 모든 하위 명령(awk·grep·sed·sort)을 바이트 처리로 고정해요.**
  - 근거: 재현에서 `LC_ALL=C` 만 오류·오탐이 모두 사라졌어요. 스크립트 로직은 ASCII 클래스와 바이트 동일성만 써서 의미가 바뀌지 않아요.
  - 머리말 `:12` 에 이유를 한 줄로 적어요 (주석 규칙: why 만). 예: "awk 구현마다 다른 멀티바이트 처리 차이를 없애려 LC_ALL=C 로 고정한다".
  - `:158` 의 `LC_ALL=C sort` 는 전역 고정과 중복이라 `sort` 로 줄여요 (단일 출처).
- 테스트 파일은 추가하지 않아요. 외부 의존 없는 버그 수정이라 CLAUDE.md 테스트 적용 범위 밖이고, Red/Green 은 아래 로케일 매트릭스 명령으로 확인해요.

## 버린 대안
- **`substr` 지점마다 멀티바이트 안전하게 고치기**: awk 구현마다 문자 단위가 달라서(gawk vs mawk/BWK) 고친 뒤에도 구현별 차이가 남아요.
- **Makefile 에서만 `LC_ALL=C` 지정**: 스크립트를 직접 실행하면 그대로 깨지고, 스크립트 머리말의 약속과도 어긋나요.
- **`C.UTF-8` 고정**: macOS 에서 towc 오류가 여전히 2회 나요 (재현).

## 결정적 검증 명령
1. 로케일 매트릭스 — 수정 전(Red): 기본 로케일 줄이 `failed: 2` / 수정 후(Green): 모든 줄 `failed: 0`, `towc` 0건.
   ```bash
   for L in "" en_US.UTF-8 C.UTF-8 C ko_KR.UTF-8; do out=$(env ${L:+LC_ALL=$L} bash scripts/check-decision-graph.sh plugins/atelier/skills/orchestrator 2>&1); echo "[${L:-default}] $(printf '%s\n' "$out" | tail -1) towc=$(printf '%s\n' "$out" | grep -ac towc)"; done
   ```
2. 로케일 지정 없이 `make validate-graph` → 마지막 줄 `checks: 8, failed: 0`
   - towc 메시지에 깨진 바이트가 섞여 macOS grep 이 텍스트로 보지 않아요. 그래서 `grep -ac` 로 세요 (B-F2). 기대 Red: 기본 `towc=3`, C.UTF-8 `towc=2`.
3. (구현 worktree 에서) 탐지력: 임시 사본에 예산 값 유출을 심어 `(9)` 가 잡히는지 확인. 기대: `[FAIL] (9)` 1건 이상, 종료 코드 1.
   ```bash
   T=$(mktemp -d); cp -R plugins/atelier/skills/orchestrator "$T/o"; printf '\n## 표준 절차\n\nmax_council_rounds 2\n' >> "$T/o/SKILL.md"; bash scripts/check-decision-graph.sh "$T/o" | grep '(9)'; echo "exit=$?"; rm -rf "$T"
   ```
   - 심은 줄이 화이트리스트·절 규칙 때문에 다른 검사로 먼저 잡히면, 구현자가 기존 오탐 지점(`contracts.md:114` 형태)을 본떠 비-예산 절에 넣어 다시 확인해요. 검증 기록에 실제 사용한 입력을 남겨요.
4. CI(Linux) 통과: PR 의 `validate` 워크플로 green.

## PR
- 브랜치: `fix/decision-graph-locale`
- 타이틀: `fix(scripts): pin locale in decision graph check`



## 가정

1. **#915 원인 기전**: `substr` 로 자른 한 바이트가 UTF-8 정규식 매칭에서 towc 실패를 일으켜 awk 스캐너가 중단된다 (근거: towc 메시지·`LC_ALL=C` 소멸·바이트 단위 `index/length` 실측). 제2 신호(`$SCAN` 줄 수 비교)는 B1 에서 확보해요. 기전이 달라도 `LC_ALL=C` 고정 결정은 재현 매트릭스로 검증돼요. (r1 판정: 수용)
2. **#915 CI awk**: `ubuntu-latest` 의 `awk` 가 mawk 든 gawk 든 `LC_ALL=C` 에서 바이트 처리로 같은 판정을 낸다 (모델 기억 → 가설 등급, PR CI green 으로 확정). (r1 판정: 수용)
