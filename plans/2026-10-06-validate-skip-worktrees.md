# validate 가 .claude/worktrees 를 건너뛰게 한다

> **Plan — 이 시점의 결정 기록.** 현재 정책의 신뢰 소스가 아니다.
> 날짜: 2026-10-06 · 브랜치: fix/validate-skip-worktrees · 이슈: #916 · 승인 출처: council pass (협의체 2라운드 검증에서 이 작업 단위 pass)

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
- 레포 루트에서 `./bin/validate --skip-versions .` 를 돌릴 때 `.claude/worktrees/` 아래 파일은 검사 대상에서 빠져야 해요 (이슈의 최소 기대).

## 사이드이펙트 조사
- 파일 수집은 전부 `doublestar.Glob(os.DirFS(repoRoot), pattern)` 이에요 (`grep -rn 'doublestar\|Glob(' tools/validate --include='*.go' | grep -v _test.go`):
  - `tools/validate/internal/path/validator.go:38,52,66,93` (`**/` 패턴 — worktree 매칭됨), `:111` (`plugins/**/*.md` — 루트 고정이라 worktree 미매칭)
  - `tools/validate/internal/spec/validator.go:49,81,92,103,114,127` (`**/` 패턴 — 매칭됨. `:125` 민감정보 패턴 포함)
  - `tools/validate/internal/version/validator.go:39` (`**/plugin.json` — 매칭됨. `--skip-versions` 면 미실행)
  - `tools/validate/internal/architecture/validator.go:103` (`plugins/*/...` — 루트 고정, 미매칭)
  - `tools/validate/internal/path/validator.go:420` 은 `filepath.Glob` 으로 선언된 경로만 확인 — 탐색 아님
  - `tools/validate/internal/extraction/validator.go:102` 는 manifest 의 `search_roots` 만 `filepath.Walk` — 탐색 범위가 고정이라 영향 없음
- 이슈 근거의 실패 2건(command 파일 누락, marketplace 미등록)은 spec 검사의 `**/plugin.json` 수집(`spec/validator.go:49`)에서 나와요.
- `.gitignore:34` = `.claude/worktrees/` (`sed -n 30,40p .gitignore`). 현재 로컬에 worktree 5개 존재 (`ls .claude/worktrees`).
- Go 테스트 하네스 사례: `tools/validate/internal/spec/validator_test.go:234-249` (`TestValidatePluginMarketplaceSync` — `t.TempDir()` + `os.MkdirAll` 로 플러그인·마켓플레이스 구성), `tools/validate/internal/version/validator_test.go:9-10` (`t.TempDir()`).

## 결정과 근거
- **새 패키지 `tools/validate/internal/repofs` 에 수집 진입점 하나를 둬요.**
  ```go
  // Glob matches pattern under repoRoot, leaving out directories that hold
  // other checkouts of the repository.
  func Glob(repoRoot, pattern string) []string
  ```
  - 내부: `doublestar.Glob(os.DirFS(repoRoot), pattern)` 결과에서 `.claude/worktrees/` 접두사 경로를 걸러요. 제외 목록은 패키지 안 상수 하나 (`[]string{".claude/worktrees/"}`).
  - 기존 호출부는 에러를 이미 버리고 있어요(`_`). 시그니처는 그 사용 방식 그대로 `[]string` 만 반환해요 (동작 계약 유지).
  - 위 Glob 호출 13곳을 모두 `repofs.Glob` 으로 바꿔요. 루트 고정 패턴(`path:111`, `architecture:103`)도 함께 바꿔 "수집은 한 곳" 을 지켜요 (변경 이유가 하나로 모임).
- 제외 범위를 `.claude/worktrees/` 하나로 둬요. 이슈가 요구한 최소치이고, 다른 gitignore 대상 중 플러그인 파일 형태를 띠는 것이 지금은 없어요 (YAGNI).

## 버린 대안
- **`git ls-files` 로 대상 한정**: 아직 `git add` 하지 않은 새 파일이 검사에서 빠져, 작업 중 검증이 조용히 통과해요. 또 순수 파일 도구에 git 실행 의존이 생겨요.
- **`.gitignore` 파싱**: 파서 의존성이나 자체 구현이 필요해요. 지금 문제는 한 디렉토리뿐이라 과해요.
- **CLI 플래그 `--exclude` + Makefile 전달**: 호출자마다 기억해야 하고 CI·로컬 동작이 갈려요.
- **fs.FS 래퍼로 탐색 단계에서 가지치기**: 성능상 이점은 있지만(worktree 아래 `target/` 순회 생략) 지금 느리다는 보고가 없어요. 필요해지면 `repofs.Glob` 내부만 바꾸면 돼요.

## 결정적 검증 명령
1. `cd tools/validate && go test ./... && go vet ./...` → 통과
2. 실제 레포(worktree 존재 상태)에서: `make build && ./bin/validate --skip-versions . | grep -c '.claude/worktrees'` → `0`
3. 같은 기준 커밋에서 수정 전·후 비교 (C-F1) — 검사 대상이 줄지 않았음을 확인. 구현 worktree(작업 브랜치)에서 실행해요.
   ```bash
   B=$(mktemp -d); git worktree add --detach "$B" origin/main && (cd "$B" && make build >/dev/null && ./bin/validate --skip-versions . | grep -E 'Passed|Failed') ; make build >/dev/null && ./bin/validate --skip-versions "$B" | grep -E 'Passed|Failed'; git worktree remove --force "$B"
   ```
   - 기대: 두 줄의 `Passed`·`Failed` 가 같고 `Failed: 0`. 첫 줄은 기준 바이너리, 둘째 줄은 수정 바이너리가 같은 깨끗한 트리를 검사한 결과예요.
4. 깨끗한 임시 checkout 에서 `LC_ALL=C.UTF-8 make validate-ci` → `Failed: 0`

## PR
- 브랜치: `fix/validate-skip-worktrees`
- 타이틀: `fix(tools): skip claude worktrees in validate`



## 가정

3. **#916 제외 범위**: `.claude/worktrees/` 외 gitignore 대상에 플러그인 형태 파일이 없다 (검증자 실측 `git status --ignored --porcelain` 으로 확인). (r1 판정: 수용)
