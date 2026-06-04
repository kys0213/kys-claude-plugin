# Hook → CLI Migration Design (guard-pr-base 파일럿)

> 상태: **설계 검토 중 (구현 전, 승인 대기)**
> 범위: github-autopilot `guard-pr-base` 훅 1종을 `autopilot hook guard-pr-base` CLI 서브커맨드로 이전하는 파일럿.
> 근거 규칙: `.claude/rules/tool-layer-boundary.md` (훅 로직은 CLI 서브커맨드, 등록 진입점만 thin shim).

---

## 1. 배경 & 목표

`guard-pr-base.sh`는 **CLI 호출 없이** config 파싱·PR base 검증·차단을 전부 bash로 수행한다. 이는:

- POSIX/bash 한정 → Windows 네이티브 미지원
- 테스트 부재 (awk/sed/grep 파싱 회귀 위험)
- "결정적 도메인 로직은 CLI에" 원칙 위반 (같은 plugin에 `autopilot` CLI가 이미 존재)

**목표**: 결정 로직을 `autopilot hook guard-pr-base` 서브커맨드로 옮겨 크로스플랫폼 + 블랙박스 테스트 가능하게 만들고, 훅 등록은 얇은 진입점만 남긴다. 동작(입출력/exit code)은 **100% 호환**을 유지한다.

이 문서는 파일럿 1종만 다룬다. 검증되면 `protect-stagnation`을 동일 패턴으로 후속 처리한다 (`check-cli-version`은 부트스트랩이라 셸 유지).

---

## 2. 현행 `guard-pr-base.sh` 동작 명세

회귀 없이 옮기려면 현행 계약을 정확히 고정해야 한다.

### 입력

| 출처 | 내용 |
|------|------|
| env `CLAUDE_PROJECT_DIR` | 프로젝트 루트 (기본 `.`) |
| 파일 `${PROJECT_DIR}/github-autopilot.local.md` | frontmatter의 `work_branch`, `branch_strategy` |
| env `CLAUDE_TOOL_USE_NAME` | 훅이 가로챈 도구 이름 |
| stdin (JSON) | PreToolUse tool input payload |

### EXPECTED_BASE 계산 (우선순위)

```
work_branch 있음            → work_branch
branch_strategy == "draft-develop-main" → "develop"
그 외 (draft-main 기본값)   → "main"
```

### ACTUAL_BASE 추출 (도구별)

| TOOL_NAME | 추출 방법 |
|-----------|----------|
| `mcp__github__create_pull_request` | JSON `.base` 필드 |
| `Bash` | command에 `gh pr create` 포함 시 `--base <val>` 또는 `--base=<val>` |
| 기타 | 없음 (빈 값) |

### 출력 / exit code 계약 (★ 호환 핵심)

| 조건 | stderr | exit |
|------|--------|------|
| config 파일 없음 (비 autopilot 프로젝트) | — | `0` (skip) |
| ACTUAL_BASE 추출 불가 (대상 도구/패턴 아님) | — | `0` (allow) |
| ACTUAL_BASE == EXPECTED_BASE | — | `0` (allow) |
| 불일치 | `BLOCKED: PR base branch mismatch` + expected/actual + 안내 | `2` (block) |

→ **exit 2 = PreToolUse 차단** 규약. CLI는 이미 `check stagnation`에서 exit 0/4/5 규약을 쓰므로 선례 일치. 신규 JSON `permissionDecision` 규약으로 바꾸지 **않고** 기존 exit-code 규약을 그대로 재현한다 (호환 우선).

---

## 3. 진입점 결정 (★ "왜 settings.json에 autopilot이?" 에 대한 답)

현재 `setup.md`는 사용자 프로젝트 settings.json에 다음을 주입한다:

```jsonc
"command": "bash ${CLAUDE_PLUGIN_ROOT}/hooks/guard-pr-base.sh"
```

즉 **지금은 `autopilot`이 아니라 플러그인 상대 경로 스크립트**를 가리킨다. 리팩터하면 그 자리에 무엇을 둘지가 결정 포인트다. 세 옵션:

### Option A — thin `.sh` shim 유지

```jsonc
"command": "bash ${CLAUDE_PLUGIN_ROOT}/hooks/guard-pr-base.sh"   // 내부에서 autopilot hook 호출
```
- ✅ 경로 견고 (PATH 비결합), 등록 형식 변경 없음
- ❌ **여전히 POSIX 전용** — shim 자체가 bash라 Windows 불가 → 크로스플랫폼 목표 미달

### Option B — 바레 바이너리 + graceful guard (권고)

```jsonc
"command": "command -v autopilot >/dev/null 2>&1 && autopilot hook guard-pr-base || exit 0"
```
- ✅ 크로스플랫폼, SRP 부합, 로직은 전부 CLI
- ✅ 미설치 시 `exit 0`으로 graceful skip
- ⚠️ settings.json이 **전역 PATH의 `autopilot` 바이너리에 결합** ← 사용자가 지적한 지점

### Option C — 절대 경로 바이너리

```jsonc
"command": "\"$HOME/.local/bin/autopilot\" hook guard-pr-base"   // 또는 ${CLAUDE_PLUGIN_ROOT}/cli/target/release/autopilot
```
- ✅ PATH 비결합 + 바이너리 직접
- ❌ 설치 위치 하드코딩, 플랫폼별 확장자(`.exe`) 분기 필요 → 등록 명령이 OS 종속

### 결정의 본질

"settings.json에 `autopilot`이 들어가는 게 맞나?"의 핵심은 **그 결합이 정당한가**다.

- `autopilot` 바이너리는 이 plugin의 **핵심 의존성**이다 — 없으면 plugin 전체가 무의미하다 (`check-cli-version`/`setup`이 이미 설치를 보장).
- 따라서 훅이 바이너리를 직접 부르는 건 *새 결합*이 아니라 **이미 존재하는 의존을 명시**하는 것이다. `.sh`도 결국 내부에서 `autopilot`을 부른다(`protect-stagnation.sh`가 그 예).

**권고: Option B.** `command -v` 가드로 미설치를 graceful 처리하면 결합의 유일한 리스크(미설치/PATH)가 해소된다. 크로스플랫폼·SRP·테스트성을 모두 만족한다.

> **대안 검토(범위 밖, 메모)**: 더 근본적으로는 setup.md가 사용자 settings.json에 주입하는 대신 **plugin이 hooks를 자체 선언**(plugin 레벨 hooks + `${CLAUDE_PLUGIN_ROOT}`)하면 사용자 settings.json 오염 자체를 없앨 수 있다. 이는 별도 과제로 분리한다.

**→ 이 결정은 사용자 승인이 필요하다 (§9 미결 결정).**

---

## 4. CLI 설계

### 모듈 구조

```
src/cmd/hook/
  mod.rs            # HookCommands enum + dispatch
  guard_pr_base.rs  # 결정 로직
src/cmd/mod.rs      # Commands::Hook 추가
src/main.rs         # 디스패치 연결
```

### clap

```rust
// src/cmd/mod.rs — Commands enum에 추가
/// Claude Code hook entry points (deterministic, stdin JSON in)
Hook {
    #[command(subcommand)]
    command: HookCommands,
},

#[derive(Subcommand)]
pub enum HookCommands {
    /// PreToolUse: block PR creation whose base ≠ configured base.
    /// Exit: 0 allow/skip, 2 block.
    GuardPrBase(GuardPrBaseArgs),
}

#[derive(clap::Args)]
pub struct GuardPrBaseArgs {
    /// Project root (default: $CLAUDE_PROJECT_DIR or ".")
    #[arg(long, env = "CLAUDE_PROJECT_DIR", default_value = ".")]
    project_dir: PathBuf,
    /// Intercepted tool name (default: $CLAUDE_TOOL_USE_NAME)
    #[arg(long, env = "CLAUDE_TOOL_USE_NAME", default_value = "")]
    tool_name: String,
    /// Config markdown filename (reuse 기존 --autopilot-md 패턴)
    #[arg(long, default_value = "github-autopilot.local.md")]
    autopilot_md: String,
}
```

### 입력/파싱

- **stdin**: `serde_json::from_reader`로 tool input 읽기. 필요한 필드만:
  ```rust
  struct ToolInput { base: Option<String>, command: Option<String> }
  ```
  (mcp PR 생성은 `base`, Bash는 `command`)
- **config frontmatter**: 현재 `config.rs`는 `autopilot.toml`만 파싱하고 `github-autopilot.local.md`의 `work_branch`/`branch_strategy`는 **파서가 없다**. → 작은 frontmatter 파서를 추가 (`src/config.rs`에 `LocalMdConfig` 또는 `cmd/hook` 내부 헬퍼). `---` 사이 첫 frontmatter 블록에서 두 키만 추출.

### 로직 (순수 함수 → 테스트 용이)

```rust
fn expected_base(work_branch: Option<&str>, strategy: Option<&str>) -> String
fn actual_base(tool_name: &str, input: &ToolInput) -> Option<String>
fn decide(expected: &str, actual: Option<&str>) -> Decision  // Allow | Block{expected, actual}
```

`main.rs`에서 `Decision`을 exit code/stderr로 변환 (Allow→0, Block→stderr+2).

---

## 5. 출력 규약 호환 매핑

| 현행 .sh | CLI |
|----------|-----|
| config 없음 → exit 0 | `decide` 이전에 config 파일 없으면 exit 0 |
| ACTUAL 없음 → exit 0 | `actual_base` == None → Allow → exit 0 |
| 일치 → exit 0 | Allow → exit 0 |
| 불일치 → stderr 3줄 + exit 2 | Block → **동일 문구** stderr + exit 2 |

stderr 문구는 바이트 단위로 동일하게 재현 (사용자/모델이 보던 메시지 유지).

---

## 6. 테스트 설계 (TDD — 외부 시스템 상호작용, CLAUDE.md 필수)

`cli/tests/hook_guard_pr_base.rs` (assert_cmd + tempfile). **테스트 먼저 작성 → fail → 구현 → pass.**

| # | 시나리오 | 입력 | 기대 |
|---|---------|------|------|
| 1 | 비 autopilot 프로젝트 | config 파일 없음 | exit 0, no stderr |
| 2 | work_branch 일치 (mcp) | config work_branch=dev, stdin `{"base":"dev"}`, tool=mcp__github__create_pull_request | exit 0 |
| 3 | work_branch 불일치 (mcp) | 위 + stdin `{"base":"main"}` | exit 2, stderr expected=dev actual=main |
| 4 | branch_strategy=draft-develop-main, base=develop | strategy 설정, base=develop | exit 0 |
| 5 | 기본 draft-main, base=main | 설정 없음, base=main | exit 0 |
| 6 | Bash `gh pr create --base main` 일치 | tool=Bash, command 포함 | exit 0 |
| 7 | Bash `--base=staging` 불일치 | tool=Bash | exit 2 |
| 8 | Bash인데 `gh pr create` 아님 | tool=Bash, command="ls" | exit 0 (대상 아님) |
| 9 | 대상 외 도구 | tool=Read | exit 0 |
| 10 | base 필드 없는 mcp 입력 | stdin `{}` | exit 0 (추출 불가) |

`expected_base`/`actual_base`/`decide` 순수 함수는 단위 테스트로 추가 커버.

---

## 7. 마이그레이션 / 배포 영향

- **기존 사용자**: settings.json에 옛 `bash .../guard-pr-base.sh` 등록이 남아 있음. Option B/C 채택 시 등록 문자열이 바뀌므로 **`/github-autopilot:setup` 재실행**으로 갱신 필요. `check-cli-version`(SessionStart)이 버전 불일치 안내 시 함께 고지 가능.
- **.sh 파일**: Option B → `hooks/guard-pr-base.sh` 삭제. Option A → thin shim으로 축소.
- **버전 범프**: `plugins/` 코드 변경 → PR type `refactor` (patch). CI 자동 범프. 수동 금지.
- **하위호환**: CLI 미설치 사용자도 graceful skip(`|| exit 0`)으로 안전.

---

## 8. 사이드이펙트 & 리스크

| 항목 | 리스크 | 완화 |
|------|--------|------|
| 출력 규약 불일치 | 훅 차단 동작이 미묘하게 달라짐 | §5 1:1 매핑 + §6 테스트로 고정 |
| frontmatter 파서 신규 | YAML 엣지케이스(따옴표/공백) | .sh의 awk 동작 그대로 모사 + 테스트 |
| settings.json 결합 (Option B) | 미설치/PATH | `command -v` 가드 |
| 기존 사용자 미갱신 | 옛 .sh 경로 잔존 | setup 재실행 안내, .sh를 한 버전 유예 후 삭제 고려 |
| stdin 파싱 차이 | mcp/Bash payload 형태 | serde Option 필드 + 테스트 6~10 |

---

## 9. 구현 순서

1. `cli/tests/hook_guard_pr_base.rs` 작성 (10 케이스, 전부 fail 확인)
2. `expected_base`/`actual_base`/`decide` 순수 함수 + 단위 테스트
3. frontmatter 파서 (`work_branch`/`branch_strategy`)
4. `HookCommands::GuardPrBase` clap + `cmd/hook/guard_pr_base.rs` 핸들러
5. `main.rs` 디스패치 + exit code/stderr 변환
6. 테스트 green 확인 (`cargo test`), `cargo fmt`/`clippy -D warnings`
7. 진입점 교체 (승인된 옵션대로): setup.md 등록 문자열 + (.sh 삭제 또는 shim)
8. setup.md 검증 문구/훅 표 갱신

---

## 10. 미결 결정 (사용자 승인 필요)

1. **진입점 형태**: Option B(바레 + guard, 권고) / A(thin shim) / C(절대경로) 중 택1
2. **.sh 처리**: 즉시 삭제 vs 한 버전 유예(deprecated) 후 삭제
3. **출력 규약**: 기존 exit-code(0/2) 유지(권고) vs 신규 JSON permissionDecision 규약으로 현대화

승인되면 §9 순서대로 TDD 구현 진행.
