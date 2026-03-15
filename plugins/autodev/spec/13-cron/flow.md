# Flow 13: Cron 관리

### 시나리오

daemon이 주기적으로 실행해야 하는 작업을 **스크립트 기반 cron**으로 관리한다.
사용자는 `/cron` slash command로 등록/관리하며, Claw가 스크립트 유효성을 검증한다.

---

### 핵심 원칙

```
Daemon cron engine = 순수 스케줄러 (환경변수 주입 + 스크립트 실행 + 중복 실행 방지)
스크립트 = guard(사전 조건) + autodev agent 호출
autodev agent = claude -p 래핑 (--cwd, 환경변수, 로깅 자동 처리)
```

스크립트 작성자는 guard 조건과 `autodev agent` 호출만 알면 된다.
lock, 경로 설정, 로깅 등 실행 인프라는 daemon과 CLI가 캡슐화한다.

---

### Built-in vs Custom

| | Built-in | Custom |
|---|---|---|
| **생성** | 레포 등록 시 자동 | `/cron add`로 수동 |
| **guard** | 내장 (불필요한 LLM 호출 방지) | 사용자 자유 |
| **제거** | 불가 (pause/resume만) | 자유롭게 추가/제거 |
| **수정** | 스크립트 편집 가능 | 자유 |

### Built-in Cron Jobs

레포 등록 시 자동 생성:

| Job | 유형 | 기본 주기 | Guard | 동작 |
|-----|------|----------|-------|------|
| **claw-evaluate** | per-repo | 60초 | 큐에 pending 있거나 HITL 있을 때 | Claw headless 큐 평가 |
| **gap-detection** | per-repo | 1시간 | active spec 있고 git 변경 있을 때 | 스펙-코드 대조 |
| **knowledge-extract** | per-repo | 1시간 | 미추출 merged PR 있을 때 | 지식 추출 |
| **hitl-timeout** | global | 5분 | 미응답 HITL 있을 때 | 타임아웃 처리 |
| **daily-report** | global | 매일 06시 | 24시간 내 활동 있을 때 | 리포트 생성 |
| **log-cleanup** | global | 매일 00시 | 보관 기간 초과 로그 있을 때 | 로그 정리 |

---

### 스크립트 구조

모든 cron job은 `~/.autodev/crons/` 하위 스크립트 파일로 정의된다.

```
~/.autodev/crons/
├── (built-in, 자동 생성)
│   ├── claw-evaluate.sh
│   ├── gap-detection.sh
│   ├── knowledge-extract.sh
│   ├── hitl-timeout.sh
│   ├── daily-report.sh
│   └── log-cleanup.sh
│
└── (custom, 사용자 작성)
    ├── code-smell-detect.sh
    └── nightly-test.sh
```

### 주입 환경변수

daemon이 스크립트 실행 시 자동 주입:

#### Per-repo (--repo 지정 시)

| 변수 | 설명 | 예시 |
|------|------|------|
| `AUTODEV_REPO_NAME` | 레포 이름 | `org/repo-a` |
| `AUTODEV_REPO_ROOT` | 레포 로컬 경로 | `/Users/me/repos/repo-a` |
| `AUTODEV_REPO_URL` | GitHub URL | `https://github.com/org/repo-a` |
| `AUTODEV_REPO_DEFAULT_BRANCH` | 기본 브랜치 | `main` |
| `AUTODEV_WORKSPACE` | autodev 워크스페이스 | `~/.autodev/workspaces/org-repo-a` |

#### Global (항상)

| 변수 | 설명 | 예시 |
|------|------|------|
| `AUTODEV_HOME` | autodev 홈 | `~/.autodev` |
| `AUTODEV_DB` | DB 경로 | `~/.autodev/autodev.db` |
| `AUTODEV_CLAW_WORKSPACE` | Claw 워크스페이스 | `~/.autodev/claw-workspace` |

---

### 스크립트 예시

#### Built-in: claw-evaluate.sh

```bash
#!/bin/bash
set -euo pipefail

# Guard: 큐에 pending 아이템이 있을 때만
PENDING=$(autodev queue list --repo "$AUTODEV_REPO_NAME" --json | jq 'length')
HITL=$(autodev hitl list --repo "$AUTODEV_REPO_NAME" --json | jq 'length')

if [ "$PENDING" = "0" ] && [ "$HITL" = "0" ]; then
  echo "skip: $AUTODEV_REPO_NAME 큐 비어있고 HITL 없음"
  exit 0
fi

autodev agent --repo "$AUTODEV_REPO_NAME" -p "큐를 평가하고 다음 작업을 결정해줘"
```

스크립트는 guard + `autodev agent` 호출만 작성한다.
`claude -p --cwd`를 직접 호출하지 않고, `autodev agent`가 경로/환경변수/로깅을 캡슐화한다.

#### Custom: code-smell-detect.sh (재사용 가능)

```bash
#!/bin/bash
set -euo pipefail

# Guard: 변경사항 있을 때만
if git -C "$AUTODEV_REPO_ROOT" diff --quiet HEAD~1; then
  echo "skip: $AUTODEV_REPO_NAME 변경사항 없음"
  exit 0
fi

autodev agent --repo "$AUTODEV_REPO_NAME" -p "코드 스멜을 감지하고 개선해줘"
```

```bash
# 같은 스크립트를 여러 레포에 등록
autodev cron add --name code-smell --repo org/repo-a --interval 3600 \
  --script ~/.autodev/crons/code-smell-detect.sh
autodev cron add --name code-smell --repo org/repo-b --interval 3600 \
  --script ~/.autodev/crons/code-smell-detect.sh
# → daemon이 레포별로 환경변수 주입하여 동일 스크립트 실행
# → autodev agent가 --repo에서 Claw 워크스페이스 경로를 자동 결정
```

---

### /cron 등록 플로우 (유효성 검증)

```
> /cron add code-smell --repo repo-a --interval 3600 \
    --script ./code-smell.sh

Claw: 스크립트를 검증합니다...

  ✅ shebang (#!/bin/bash) 존재
  ✅ $AUTODEV_REPO_ROOT 사용 (하드코딩 경로 없음)
  ✅ guard 로직 존재 (git diff)
  ⚠️ claude -p를 직접 호출하고 있습니다
     → autodev agent --repo "$AUTODEV_REPO_NAME" -p "..." 로 변경하시겠어요?

> 응 바꿔줘

  ✅ 수정 완료. cron 등록합니다.
  → org/repo-a에 code-smell (매 1시간) 등록됨
```

### 검증 항목

| 항목 | 검증 내용 |
|------|----------|
| shebang | `#!/bin/bash` 또는 `#!/bin/sh` 존재 |
| 실행 권한 | `chmod +x` 여부 |
| 환경변수 사용 | 하드코딩 경로 대신 `$AUTODEV_*` 사용 권장 |
| guard 존재 | LLM 호출 전 사전 조건 체크 존재 권장 |
| agent 호출 | `autodev agent` 사용 여부 (`claude -p` 직접 호출 경고) |

검증은 Claw가 스크립트를 읽고 자연어로 판단. 경고는 제안이며 강제는 아님.

---

### Cron 관리 명령어

#### Slash Command (Claw 세션)

```
/cron list                              전체 목록
/cron add <name> --repo <r> --interval <s> --script <path>
/cron pause <name> [--repo <r>]         일시정지
/cron resume <name> [--repo <r>]        재개
/cron trigger <name> [--repo <r>]       즉시 실행
/cron remove <name> [--repo <r>]        제거 (custom만)
```

#### CLI (인프라 도구)

```bash
autodev cron list --json
autodev cron add --name <n> --repo <r> --interval <s> --script <path>
autodev cron add --name <n> --repo <r> --schedule "<cron-expr>" --script <path>
autodev cron update <name> [--repo <r>] --interval <s>
autodev cron pause <name> [--repo <r>]
autodev cron resume <name> [--repo <r>]
autodev cron remove <name> [--repo <r>]
autodev cron trigger <name> [--repo <r>]
```

---

### Daemon Cron Engine

```
매 초:
  등록된 cron 목록 순회
    IF 이전 실행이 아직 running → skip (내부 상태로 관리)
    IF 주기 도달:
      환경변수 주입 (global + per-repo)
      스크립트 실행 (subprocess)
      exit code + stdout/stderr 로그 기록
      실행 완료 → running 상태 해제
```

daemon은 스크립트 내용을 모른다. 주기와 경로만 알고 실행한다.

**중복 실행 방지**: daemon이 job별 실행 상태를 in-memory로 관리한다.
이전 실행이 완료되지 않은 job은 다음 틱에서 자동 skip된다.
스크립트가 별도로 lockfile이나 guard를 구현할 필요가 없다.

```rust
// cron engine 내부 구조 (개념)
struct CronEngine {
    running: HashMap<JobKey, JoinHandle<()>>,
}

// 매 틱 판단
if self.running.contains_key(&job_key) && !handle.is_finished() {
    continue; // skip: 이전 실행 진행 중
}
```
