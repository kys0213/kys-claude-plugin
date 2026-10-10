# external-llm 공유 스크립트를 플러그인 안으로 옮긴다

> **Plan — 이 시점의 결정 기록.** 현재 정책의 신뢰 소스가 아니다.
> 날짜: 2026-10-06 · 브랜치: fix/external-llm-bundle-scripts · 이슈: #914 · 승인 출처: council pass (협의체 2라운드 검증에서 이 작업 단위 pass)

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
- 마켓플레이스에서 설치해 `~/.claude/plugins/cache/kys-claude-plugin/external-llm/<버전>/` 로 복사된 환경에서도 agent·command 가 호출하는 스크립트가 존재해야 해요.

## 사이드이펙트 조사
- 호출 지점 9곳 (8개 파일). 명령: `grep -rn 'CLAUDE_PLUGIN_ROOT}/\.\./' plugins`
  - `plugins/external-llm/agents/code-reviewer-codex.md:39` (call-codex-review.sh)
  - `plugins/external-llm/agents/diff-collector.md:27` (get-diff.sh)
  - `plugins/external-llm/agents/llm-reviewer-gemini.md:27`, `code-reviewer-gemini.md:41`, `commands/invoke-gemini.md:27`, `commands/spec-review.md:110` (call-gemini.sh)
  - `plugins/external-llm/agents/llm-reviewer-codex.md:27`, `commands/invoke-codex.md:27`, `commands/spec-review.md:107` (call-codex.sh)
- 본문 산문 언급 2곳: `plugins/external-llm/commands/invoke-gemini.md:21`, `invoke-codex.md:21` ("common/scripts/call-*.sh 실행")
- 저장소 루트 문서: `README.md:22-25` 구조 트리에 `common/scripts/call-codex.sh`, `call-gemini.sh` 가 나와요.
- 다른 소비자 없음: `grep -rn 'common/scripts' . | grep -v .claude/worktrees` 결과, 플러그인 소비자는 external-llm 뿐이에요 (나머지는 `plans/**` 기록과 `tools/validate/internal/parser/markdown_test.go:132,153` 고정 문자열).
- 스크립트 자기 위치 의존 없음: `grep -n 'dirname\|SCRIPT_DIR\|BASH_SOURCE\|common\|\.\./' common/scripts/{call-codex,call-gemini,call-codex-review,get-diff}.sh` → 출력 없음. 옮겨도 내부 경로가 깨지지 않아요.
- 실행 비트: `ls -l common/scripts` → 4개 모두 `-rwxr-xr-x`. `git mv` 는 모드를 보존해요.
- `render-template.sh`·`test-render-template.sh` 는 external-llm 이 호출하지 않아요 (위 grep). 이번 범위 밖이라 `common/` 에 그대로 둬요.
- 검증 도구 영향: `tools/validate/internal/path/validator.go:204-255` 는 `${CLAUDE_PLUGIN_ROOT}` 를 플러그인 루트로 치환해 존재를 확인해요. `${CLAUDE_PLUGIN_ROOT}/scripts/x.sh` 는 1차 해석(`:207-210`)에서 바로 통과해요. 민감정보 검사 패턴 `**/scripts/*.sh`(`tools/validate/internal/spec/validator.go:125`)도 새 위치를 그대로 잡아요.
- 플러그인 간 의존 규칙: `.claude/rules/plugin-governance.md` "범위 밖" 절이 `common/` 호출을 플러그인 간 의존이 아니라고 명시해요. `dependencies` 선언 변경은 없어요.
- 외부 사실: `facts-external.md` — 캐시 복사는 플러그인 디렉토리 안만 복사하고 `../` 밖은 못 찾아요 (LOAD "In-place and copied plugins"). → 결함 확정.
- 현재 상태 재현(Red): 아래 "결정적 검증" 1번 명령을 현재 main 에서 실행하면 `ESCAPES ...` 4줄이 나와요 (설계자가 직접 실행해 확인).

## 결정과 근거
- **스크립트 4개를 `plugins/external-llm/scripts/` 로 옮기고, 호출을 `${CLAUDE_PLUGIN_ROOT}/scripts/<name>.sh` 로 바꿔요.**
  - 근거 1: 플러그인 디렉토리 안의 파일은 캐시 복사 대상이라는 게 공식 문서의 직접 서술이에요 (추론 아님).
  - 근거 2: 소비자가 external-llm 하나뿐이라 "공유" 위치에 둘 이유가 없어요 (Rule of 3, YAGNI).
  - 근거 3: `..` 경로 자체가 사라져 로컬 제자리 로드(`--plugin-dir`)와 캐시 설치가 같은 경로로 동작해요.
- 산문 언급(`invoke-*.md:21`)은 `scripts/call-*.sh` 로 고쳐요. `README.md` 트리는 `common/scripts/` 아래를 `render-template.sh` 로 바꾸고, external-llm 줄 옆 설명에 스크립트 동봉을 반영해요 (최소 수정).

## 버린 대안
- **플러그인 안 symlink (`plugins/external-llm/scripts -> ../../common/scripts`)**: 공식 공유 방법이지만, 대상이 `plugins/` 밖·레포 루트 안(`common/`)일 때 복사되는지는 문서 예시에 없어요(추론). 확인하려면 실제 git 설치가 필요해 검증 비용이 크고, 소비자가 하나라 얻는 것도 없어요.
- **`../../` 유지 + 마켓플레이스 clone 경로(`~/.claude/plugins/marketplaces/<name>/`) 폴백**: 공식 변수가 없어 하드코딩이에요. Fail Fast 에 어긋나요.
- **`${CLAUDE_PLUGIN_DATA}` 로 설치 시 복사**: 문서상 코드 공유 용도가 아니에요.

## 결정적 검증 명령
1. 캐시 복사 모사 (플러그인 디렉토리 밖을 참조하는지). 수정 후 기대: `ok` 4줄, `ESCAPES`·`MISSING` 0줄.
   ```bash
   R=plugins/external-llm; grep -rhoE '[$][{]CLAUDE_PLUGIN_ROOT[}]/[A-Za-z0-9_./-]+' "$R" | sort -u | while read -r p; do rel="${p#*\}/}"; case "$rel" in *..*) echo "ESCAPES $p";; *) if [ -f "$R/$rel" ]; then echo "ok $p"; else echo "MISSING $p"; fi;; esac; done
   ```
2. `grep -rn 'CLAUDE_PLUGIN_ROOT}/\.\./' plugins` → 출력 없음 (종료 코드 1)
3. `grep -rn 'common/scripts' plugins` → 출력 없음
4. 깨끗한 임시 checkout 에서 `LC_ALL=C.UTF-8 make validate-ci` → `Failed: 0`

## 머지 후 확인 항목 (결정적 검증 아님)
- 버전 범프 감지 (A-F1): `tools/bumpversion/internal/changes/detector.go:42-60` 는 `common/` 변경 시 `common` 을 참조하는 플러그인을 찾아요. 이동 후 HEAD 에는 참조가 없어요. 머지 직후 자동 범프 커밋이 `external-llm` patch 하나만 올렸는지 확인해요: `git fetch origin && git log -1 --format=%s origin/main` → `chore: bump versions (external-llm 0.9.0 → 0.9.1) [skip ci]` 형태, 다른 플러그인 없음.
- 실제 GitHub 설치 후 `ls ~/.claude/plugins/cache/kys-claude-plugin/external-llm/*/scripts` 에 스크립트 4개 존재.

## 범위 밖 (A-F2)
- `tools/validate/internal/path/validator.go:212-217` 이 `strict: false` 플러그인(external-llm 만 해당)의 `../` 경로를 repo 루트 기준으로 통과시켜요. 이번 결함이 검증에 안 잡힌 이유예요. 이번 PR 에서는 고치지 않아요 (YAGNI). 필요하면 후속 이슈로 제안해요.

## PR
- 브랜치: `fix/external-llm-bundle-scripts`
- 타이틀: `fix(external-llm): bundle llm scripts inside plugin`



## 가정

5. **#914 캐시 복사**: 플러그인 디렉토리 안 파일은 캐시에 복사된다 (공식 문서 LOAD 직접 서술). 실제 GitHub 설치 확인은 머지 후 확인 항목이에요. (r1 판정: 수용)
