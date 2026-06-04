# Hook → CLI Migration Design (guard-pr-base 파일럿)

> 상태: **설계 검토 중 (구현 전, 승인 대기)** · **atelier CLI 통합(§4 of `plans/atelier/02-architecture.md`)의 하위 작업**
> 범위: `guard-pr-base` 훅 1종을 **통합 바이너리 `atelier`** 의 `atelier hook guard-pr-base` 서브커맨드로 이전.
> 근거 규칙: `.claude/rules/tool-layer-boundary.md` (훅 로직은 CLI 서브커맨드, 등록 진입점만 thin shim).

---

## 0. ⚠️ 전제 정정 — 타깃 바이너리는 `autopilot`이 아니라 `atelier`

초기 설계는 per-plugin 바이너리 `autopilot`(`plugins/github-autopilot/cli`)을 타깃으로 잡았으나 **이는 틀렸다.** atelier 통합 설계(`plans/atelier/02-architecture.md` §4)는:

- 모든 CLI를 **단일 Rust crate / 단일 바이너리 `atelier`** 로 흡수한다 (`Cargo.toml name = "atelier"`).
- `atelier autopilot <subcmd>` 가 **기존 `autopilot` 바이너리를 대체**하고, 기존 `autopilot`은 `alias autopilot='atelier autopilot'` 로만 호환된다 (§4.4).
- **`atelier hook <subcmd>` 네임스페이스가 이미 계획**되어 있다 (§4.2, git-utils hook 명령 계승).

→ 따라서 훅이 `autopilot`을 바라보면 통합이 지우려는 바이너리에 의존하게 된다. **타깃은 `atelier`, 서브커맨드는 `atelier hook guard-pr-base`, 코드 위치는 `plugins/atelier/cli/`** 다.

### 순서 의존성 (중요)

`plugins/atelier/cli` 는 **아직 코드가 없다**(설계만 존재). 따라서 이 훅→CLI 이전은 **독립 작업이 아니라 atelier CLI 통합의 하위 작업**으로 진행한다. 지금 `autopilot` 바이너리에 단독으로 옮기면 atelier 구현 시 재작업이 발생한다.

```
atelier CLI crate 골격 생성 (plans/atelier §4)
  └─ atelier hook 네임스페이스 확립
       └─ [이 문서] guard-pr-base 로직을 atelier hook 서브커맨드로 이전   ← 여기
```

---

## 1. 배경 & 목표

`guard-pr-base.sh`는 **CLI 호출 없이** config 파싱·PR base 검증·차단을 전부 bash로 수행한다. 이는:

- POSIX/bash 한정 → Windows 네이티브 미지원
- 테스트 부재 (awk/sed/grep 파싱 회귀 위험)
- "결정적 도메인 로직은 CLI에" 원칙 위반

**목표**: 결정 로직을 `atelier hook guard-pr-base` 서브커맨드로 옮겨 크로스플랫폼 + 블랙박스 테스트 가능하게 만들고, 훅 등록은 얇은 진입점만 남긴다. 동작(입출력/exit code)은 **100% 호환** 유지.

이 문서는 파일럿 1종만 다룬다. 검증되면 `protect-stagnation`을 동일 패턴으로 후속 처리한다 (`check-cli-version`은 부트스트랩이라 셸 유지).

---

## 2. 현행 `guard-pr-base.sh` 동작 명세

회귀 없이 옮기려면 현행 계약을 정확히 고정해야 한다. (이 절은 바이너리 이름과 무관하게 유효 — 기존 .sh 동작 그대로.)

### 입력

| 출처 | 내용 |
|------|------|
| env `CLAUDE_PROJECT_DIR` | 프로젝트 루트 (기본 `.`) |
| 파일 `${PROJECT_DIR}/github-autopilot.local.md` | frontmatter의 `work_branch`, `branch_strategy` |
| env `CLAUDE_TOOL_USE_NAME` | 훅이 가로챈 도구 이름 |
| stdin (JSON) | PreToolUse tool input payload |

### EXPECTED_BASE 계산 (우선순위)

```
work_branch 있음                         → work_branch
branch_strategy == "draft-develop-main"  → "develop"
그 외 (draft-main 기본값)                → "main"
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

→ **exit 2 = PreToolUse 차단** 규약. atelier autopilot CLI는 이미 `check stagnation`에서 exit 0/4/5 규약을 쓰므로 선례 일치. 신규 JSON `permissionDecision` 규약으로 바꾸지 **않고** 기존 exit-code 규약을 그대로 재현한다 (호환 우선).

---

## 3. 진입점 결정

현재 `setup.md`는 사용자 settings.json에 **플러그인 상대 경로 스크립트**를 주입한다:

```jsonc
"command": "bash ${CLAUDE_PLUGIN_ROOT}/hooks/guard-pr-base.sh"
```

atelier 통합 후 그 자리에 무엇을 둘지가 결정 포인트다. 세 옵션 (모두 `atelier` 바이너리 기준):

### Option A — thin `.sh` shim 유지

```jsonc
"command": "bash ${CLAUDE_PLUGIN_ROOT}/hooks/guard-pr-base.sh"   // 내부에서 atelier hook 호출
```
- ✅ 경로 견고 (PATH 비결합), 등록 형식 변경 없음, atelier §3.4 경로 재작성과 호환
- ❌ **여전히 POSIX 전용** — shim 자체가 bash라 Windows 불가 → 크로스플랫폼 목표 미달

### Option B — 바레 바이너리 + graceful guard (권고)

```jsonc
"command": "command -v atelier >/dev/null 2>&1 && atelier hook guard-pr-base || exit 0"
```
- ✅ 크로스플랫폼, SRP 부합, 로직은 전부 CLI
- ✅ 미설치 시 `exit 0`으로 graceful skip
- 참고: `atelier`는 통합 plugin의 단일 핵심 바이너리(setup이 설치 보장, §4.4) → 모든 command/agent가 이미 바레 호출. 훅도 동일 관례.

### Option C — 절대 경로 바이너리

```jsonc
"command": "\"$HOME/.local/bin/atelier\" hook guard-pr-base"
```
- ✅ PATH 비결합 + 바이너리 직접
- ❌ 설치 위치 하드코딩, 플랫폼별 확장자(`.exe`) 분기 → 등록 명령 OS 종속

**권고: Option B.** atelier가 plugin의 단일 설치 바이너리이고 모든 호출이 이미 바레 `atelier <subcmd>` 관례를 따르므로, 훅도 동일하게 `atelier hook ...`을 부르는 게 일관적이다. `command -v` 가드가 미설치 구간만 graceful 처리한다.

> **메모(범위 밖)**: 더 근본적으로 setup이 사용자 settings.json에 주입하는 대신 atelier가 hooks를 자체 선언하는 방향은 atelier 통합 차원에서 별도 평가한다.

---

## 4. CLI 설계 (atelier crate 내부)

### 모듈 구조

atelier 단일 crate(`plugins/atelier/cli/`) 안, autopilot 도메인 하위에 배치:

```
plugins/atelier/cli/src/
  cli.rs                 # 최상위 라우터: atelier git|autopilot|hook|setup ...
  hook/
    mod.rs               # HookCommands enum + dispatch (atelier hook <subcmd>)
    guard_pr_base.rs     # 결정 로직
  autopilot/
    config.rs            # github-autopilot.local.md frontmatter 파서 (work_branch/branch_strategy)
```

> `atelier hook` 네임스페이스는 git-utils hook 명령을 계승하는 기존 계획 지점(atelier §4.2)이다. guard-pr-base는 여기에 autopilot-도메인 훅으로 합류한다.

### clap

```rust
// cli.rs 최상위 Subcommand
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
    #[arg(long, env = "CLAUDE_PROJECT_DIR", default_value = ".")]
    project_dir: PathBuf,
    #[arg(long, env = "CLAUDE_TOOL_USE_NAME", default_value = "")]
    tool_name: String,
    #[arg(long, default_value = "github-autopilot.local.md")]
    autopilot_md: String,
}
```

### 입력/파싱

- **stdin**: `serde_json::from_reader` → `struct ToolInput { base: Option<String>, command: Option<String> }`
- **config frontmatter**: `github-autopilot.local.md`의 `work_branch`/`branch_strategy`를 `---` 첫 블록에서 추출하는 작은 파서 (현재 어느 crate에도 없음 → 신규). atelier 흡수 시 autopilot 도메인 config 모듈에 둔다.

### 로직 (순수 함수 → 테스트 용이)

```rust
fn expected_base(work_branch: Option<&str>, strategy: Option<&str>) -> String
fn actual_base(tool_name: &str, input: &ToolInput) -> Option<String>
fn decide(expected: &str, actual: Option<&str>) -> Decision  // Allow | Block{expected, actual}
```

진입점에서 `Decision`을 exit code/stderr로 변환 (Allow→0, Block→stderr+2).

---

## 5. 출력 규약 호환 매핑

| 현행 .sh | atelier hook |
|----------|--------------|
| config 없음 → exit 0 | config 파일 없으면 exit 0 |
| ACTUAL 없음 → exit 0 | `actual_base` == None → Allow → exit 0 |
| 일치 → exit 0 | Allow → exit 0 |
| 불일치 → stderr 3줄 + exit 2 | Block → **동일 문구** stderr + exit 2 |

stderr 문구는 바이트 단위로 동일하게 재현.

---

## 6. 테스트 설계 (TDD — 외부 시스템 상호작용, CLAUDE.md 필수)

`plugins/atelier/cli/tests/hook_guard_pr_base.rs` (assert_cmd + tempfile). **테스트 먼저 → fail → 구현 → pass.**

| # | 시나리오 | 입력 | 기대 |
|---|---------|------|------|
| 1 | 비 autopilot 프로젝트 | config 파일 없음 | exit 0, no stderr |
| 2 | work_branch 일치 (mcp) | work_branch=dev, stdin `{"base":"dev"}`, tool=mcp__github__create_pull_request | exit 0 |
| 3 | work_branch 불일치 (mcp) | 위 + stdin `{"base":"main"}` | exit 2, stderr expected=dev actual=main |
| 4 | draft-develop-main, base=develop | strategy 설정 | exit 0 |
| 5 | 기본 draft-main, base=main | 설정 없음 | exit 0 |
| 6 | Bash `gh pr create --base main` 일치 | tool=Bash | exit 0 |
| 7 | Bash `--base=staging` 불일치 | tool=Bash | exit 2 |
| 8 | Bash인데 `gh pr create` 아님 | command="ls" | exit 0 |
| 9 | 대상 외 도구 | tool=Read | exit 0 |
| 10 | base 필드 없는 mcp 입력 | stdin `{}` | exit 0 |

`expected_base`/`actual_base`/`decide` 순수 함수는 단위 테스트로 추가 커버.

---

## 7. 마이그레이션 / 배포 영향

- **기존 사용자**: settings.json에 옛 `bash .../guard-pr-base.sh` 등록 잔존. **atelier §3.4가 이미 옛 plugin 훅 경로를 atelier 경로로 재작성**하는 절차를 정의 → 그 흐름에 본 훅의 등록 문자열 교체를 포함시킨다 (`/atelier:setup` 재실행).
- **.sh 파일**: Option B → atelier hooks 디렉토리로 옮겨질 때 thin shim화 또는 삭제 (atelier §2 디렉토리 구조의 `hooks/guard-pr-base.sh` 항목과 정합).
- **버전 범프**: atelier crate 변경 → PR type `refactor`/`feat`. CI 자동 범프. 수동 금지.
- **하위호환**: `atelier` 미설치 사용자도 `|| exit 0` graceful skip.

---

## 8. 사이드이펙트 & 리스크

| 항목 | 리스크 | 완화 |
|------|--------|------|
| 타깃 바이너리 혼동 | `autopilot` vs `atelier` | §0 명시 — 항상 `atelier` |
| 순서 의존 | atelier crate 미존재 시 선행 | atelier §4 골격 후 착수 |
| 출력 규약 불일치 | 차단 동작 미세 변화 | §5 1:1 매핑 + §6 테스트 |
| frontmatter 파서 신규 | YAML 엣지케이스 | .sh awk 동작 모사 + 테스트 |
| 기존 사용자 미갱신 | 옛 .sh 경로 잔존 | atelier §3.4 경로 재작성에 편승 |

---

## 9. 구현 순서

**선행 조건**: `plugins/atelier/cli` crate 골격 + `atelier hook` 네임스페이스 존재 (atelier 통합 §4).

1. `plugins/atelier/cli/tests/hook_guard_pr_base.rs` 작성 (10 케이스, 전부 fail 확인)
2. `expected_base`/`actual_base`/`decide` 순수 함수 + 단위 테스트
3. `github-autopilot.local.md` frontmatter 파서 (`work_branch`/`branch_strategy`)
4. `HookCommands::GuardPrBase` clap + `hook/guard_pr_base.rs` 핸들러
5. 진입점에서 exit code/stderr 변환
6. 테스트 green 확인 (`cargo test`), `cargo fmt`/`clippy -D warnings`
7. 진입점 교체 (승인된 옵션대로): setup.md 등록 문자열을 `atelier hook guard-pr-base` 기준으로 + (.sh 삭제/shim)
8. setup.md 검증 문구/훅 표 갱신 (atelier 맥락)

---

## 10. 미결 결정 (사용자 승인 필요)

1. **진입점 형태**: Option B(`atelier hook ...` 바레 + guard, 권고) / A(thin shim) / C(절대경로)
2. **착수 시점**: atelier CLI 통합 본작업에 편입(권고) vs 이 파일럿을 atelier 골격의 첫 시범 케이스로 선행
3. **출력 규약**: 기존 exit-code(0/2) 유지(권고) vs 신규 JSON permissionDecision 현대화

승인되면 §9 순서대로 TDD 구현 진행.
