---
name: team-claude:setup
description: Team Claude 환경 설정 - 초기화, 설정 관리, 에이전트 관리, 서버 관리
allowed-tools: ["Read", "Write", "Glob", "Bash", "AskUserQuestion"]
---

# Team Claude Setup

> **먼저 읽기**: `${CLAUDE_PLUGIN_ROOT}/INFRASTRUCTURE.md`

단일 진입점으로 모든 환경 설정을 관리합니다.

## 스크립트 도구

> **중요**: 설정 관리는 결정적 스크립트를 통해 수행합니다. LLM이 직접 YAML을 파싱하지 않습니다.

```bash
# 스크립트 위치
SCRIPTS_DIR="./plugins/team-claude/scripts"

# 설정 초기화
${SCRIPTS_DIR}/tc-config.sh init

# 설정 값 읽기
${SCRIPTS_DIR}/tc-config.sh get project.name
${SCRIPTS_DIR}/tc-config.sh get feedback_loop.mode

# 설정 값 쓰기
${SCRIPTS_DIR}/tc-config.sh set project.language python
${SCRIPTS_DIR}/tc-config.sh set feedback_loop.max_iterations 5

# 전체 설정 보기
${SCRIPTS_DIR}/tc-config.sh show

# 설정 파일 경로
${SCRIPTS_DIR}/tc-config.sh path

# 환경 검증
${SCRIPTS_DIR}/tc-config.sh verify

# 상태 관리
${SCRIPTS_DIR}/tc-state.sh init
${SCRIPTS_DIR}/tc-state.sh check
${SCRIPTS_DIR}/tc-state.sh transition setup

# 서버 관리
${SCRIPTS_DIR}/tc-server.sh install
${SCRIPTS_DIR}/tc-server.sh status
${SCRIPTS_DIR}/tc-server.sh start
```

## 워크플로우

```
/team-claude:setup
        │
        ▼
┌─────────────────────────────────┐
│  Phase 0: 의존성 확인           │
│  yq, jq, git, bun 설치 여부     │
└─────────────────────────────────┘
        │
   ┌────┴────┐
   미설치     설치됨
   │         │
   ▼         │
설치 옵션    │
선택        │
   │         │
   ▼         ▼
.claude/team-claude.yaml 존재?
        │
   ┌────┴────┐
   No        Yes
   │         │
   ▼         ▼
초기화     메인 메뉴
모드       │
   │       ├── 설정 조회
   │       ├── 설정 수정
   │       ├── 에이전트 관리
   │       ├── 서버 관리
   │       └── 종료
   │
   ▼
┌─────────────────────────────────┐
│  Phase 1: 상태 초기화           │
│  tc-state.sh init               │
└─────────────────────────────────┘
        │
        ▼
┌─────────────────────────────────┐
│  Phase 2: 서버 빌드 (필요시)    │
│  tc-server.sh install           │
└─────────────────────────────────┘
        │
        ▼
┌─────────────────────────────────┐
│  Phase 3: 환경 검증             │
│  tc-config.sh verify            │
└─────────────────────────────────┘
        │
        ▼
설정 위자드 → 완료
```

## 실행 절차

### Phase 0: 인프라 전체 진단

setup 시작 전에 전체 인프라 상태를 확인합니다. 이 단계에서 delegate가 정상 동작하기 위한 모든 필수 요소를 검증합니다.

**전체 인프라 체크:**

```bash
# 인프라 전체 상태 확인 (human-readable)
source ./plugins/team-claude/scripts/lib/common.sh
source ./plugins/team-claude/scripts/lib/prerequisites.sh
print_infrastructure_status
```

**JSON 형태로 상태 확인 (프로그래밍용):**

```bash
source ./plugins/team-claude/scripts/lib/common.sh
source ./plugins/team-claude/scripts/lib/prerequisites.sh
check_infrastructure
```

**확인 항목:**

| 항목 | 설명 | 해결 방법 |
|------|------|-----------|
| `yq` | YAML 파싱 | `brew install yq` |
| `jq` | JSON 파싱 | `brew install jq` |
| `git` | 버전 관리 | `xcode-select --install` |
| `curl` | HTTP 통신 | 대부분 기본 설치됨 |
| `bun` | 서버 빌드/실행 | `curl -fsSL https://bun.sh/install \| bash` |
| Server Binary | 컴파일된 서버 | `tc-server install` |
| Server Running | 서버 실행 상태 | `tc-server start` |
| iTerm2 (macOS) | 터미널 자동화 | `brew install --cask iterm2` (선택) |

**의존성 상태만 확인:**

```bash
source ./plugins/team-claude/scripts/lib/common.sh
print_dependency_status

# 누락된 의존성 확인
if ! check_dependencies; then
  echo "일부 의존성이 누락되었습니다."
fi
```

**미설치 시 처리:**

```typescript
AskUserQuestion({
  questions: [{
    question: "누락된 의존성을 설치할까요?",
    header: "Infrastructure Setup",
    options: [
      { label: "자동 설치 (Recommended)", description: "brew를 사용하여 누락된 도구 설치" },
      { label: "수동 설치", description: "설치 명령어를 안내받고 직접 설치" },
      { label: "건너뛰기", description: "일부 기능이 제한될 수 있음" }
    ],
    multiSelect: false
  }]
})
```

**자동 설치 선택 시:**

```bash
source ./plugins/team-claude/scripts/lib/common.sh
install_all_dependencies

# bun 별도 설치 (Homebrew 없이)
if ! command -v bun &>/dev/null; then
  curl -fsSL https://bun.sh/install | bash
fi

# 서버 빌드 및 설치
./plugins/team-claude/scripts/tc-server.sh install
```

**수동 설치 선택 시:**

```
━━━ 수동 설치 가이드 ━━━

1. CLI 도구 (Homebrew 사용):
   brew install yq jq

2. Git (Xcode Command Line Tools):
   xcode-select --install

3. Bun Runtime:
   curl -fsSL https://bun.sh/install | bash
   # 설치 후 터미널 재시작

4. Team Claude Server:
   ./plugins/team-claude/scripts/tc-server.sh install

5. (선택) iTerm2 - 터미널 자동화용:
   brew install --cask iterm2

설치 후 /team-claude:setup을 다시 실행하세요.
```

**Headless 모드 (서버 없이 수동 작업):**

서버 없이도 delegate의 일부 기능을 수동으로 사용할 수 있습니다:

```bash
# Worktree만 생성 (서버 없이)
./plugins/team-claude/scripts/tc-worktree.sh create <checkpoint-id>

# 수동으로 Worker 실행
cd .team-claude/worktrees/<checkpoint-id>
claude --print "CLAUDE.md를 읽고 지시사항을 수행하세요"

# 수동 검증
<validation-command>
```

### Phase 1: 상태 감지

`.claude/team-claude.yaml` 존재 여부 확인 (tc-config.sh 사용):

```bash
# 설정 파일 존재 확인
if ./plugins/team-claude/scripts/tc-config.sh show &>/dev/null; then
  echo "설정 존재 → 메인 메뉴"
else
  echo "설정 없음 → 초기화 모드"
fi
```

- **없음** → [초기화 모드](./reference/setup/init-mode.md) 진입 (`tc-config.sh init` 실행)
- **있음** → 메인 메뉴 표시

### Phase 1.5: 상태 초기화 (초기화 모드에서)

설정 파일 생성 후 워크플로우 상태를 초기화합니다:

```bash
SCRIPTS="${CLAUDE_PLUGIN_ROOT}/scripts"

# 상태 파일 초기화
${SCRIPTS}/tc-state.sh init

# 상태 전이: idle → setup
${SCRIPTS}/tc-state.sh transition setup
```

### Phase 1.6: 서버 빌드 (초기화 모드에서)

서버 바이너리가 없으면 빌드합니다:

```bash
# 서버 바이너리 존재 확인
if [[ ! -f "${HOME}/.claude/team-claude-server" ]]; then
  echo "서버 빌드가 필요합니다."
  ${SCRIPTS}/tc-server.sh install
fi
```

**bun 미설치 시 안내:**

```
bun이 설치되어 있지 않습니다.

설치 방법:
  curl -fsSL https://bun.sh/install | bash

설치 후 '/team-claude:setup'을 다시 실행하세요.
```

### Phase 1.7: 환경 검증 (초기화 모드에서)

초기화 완료 후 환경이 올바르게 구성되었는지 자동 검증합니다:

```bash
# 환경 검증 실행 (cmd_init에서 자동 호출됨)
${SCRIPTS}/tc-config.sh verify
```

**검증 항목:**

| 카테고리 | 검증 내용 |
|---------|----------|
| 설정 파일 | `.claude/team-claude.yaml` 존재 |
| 디렉토리 | sessions, state, hooks, templates, agents |
| Hook 스크립트 | 4개 스크립트 존재 + 실행 권한 |
| 의존성 | yq, jq, git, bun |
| 서버 | `~/.claude/team-claude-server` 바이너리 |

**출력 예시:**

```
━━━ Team Claude 환경 검증 ━━━

📁 설정 파일
  ✓ .claude/team-claude.yaml

📂 디렉토리 구조
  ✓ .team-claude/sessions
  ✓ .team-claude/state
  ✓ .team-claude/hooks
  ✓ .team-claude/templates
  ✓ .claude/agents (프로젝트 에이전트)

🪝 Hook 스크립트
  ✓ on-worker-complete.sh
  ✓ on-validation-complete.sh
  ✓ on-worker-question.sh
  ✓ on-worker-idle.sh

🔧 의존성
  ✓ yq (v4.35.1)
  ✓ jq (jq-1.7)
  ✓ git (2.42.0)
  ⚠ bun (미설치 - 서버 빌드에 필요)

🖥️  서버
  ⚠ team-claude-server (미설치 - tc-server.sh install 실행 필요)

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
⚠ 경고 2개 (선택적 항목)
```

### Phase 2: 메인 메뉴 (설정 존재 시)

```typescript
AskUserQuestion({
  questions: [{
    question: "무엇을 하시겠습니까?",
    header: "Setup",
    options: [
      { label: "인프라 진단", description: "delegate 실행 전 전체 인프라 상태 확인" },
      { label: "현재 설정 보기", description: "전체 설정 조회" },
      { label: "설정 수정", description: "대화형 위자드로 설정 변경" },
      { label: "에이전트 관리", description: "에이전트 생성/수정/삭제/활성화" },
      { label: "서버 관리", description: "서버 설치/시작/중지" },
      { label: "종료", description: "설정 메뉴 종료" }
    ],
    multiSelect: false
  }]
})
```

선택에 따라 해당 reference 파일 참조:

| 선택 | Reference / Action |
|------|-----------|
| 인프라 진단 | `print_infrastructure_status` 실행 (아래 참조) |
| 현재 설정 보기 / 설정 수정 | [config-management.md](./reference/setup/config-management.md) |
| 에이전트 관리 | [agent-management.md](./reference/setup/agent-management.md) |
| 서버 관리 | [server-management.md](./reference/setup/server-management.md) |

**인프라 진단 선택 시:**

```bash
source ./plugins/team-claude/scripts/lib/common.sh
source ./plugins/team-claude/scripts/lib/prerequisites.sh
print_infrastructure_status
```

출력 예시:
```
╔═══════════════════════════════════════════════════════════════╗
║              Team Claude Infrastructure Check                   ║
╚═══════════════════════════════════════════════════════════════╝

━━━ 1. CLI Dependencies ━━━
  [OK] yq: yq version 4.x.x
  [OK] jq: jq-1.7
  [OK] git: git version 2.x.x
  [OK] curl: curl 8.x.x
  [OK] bun: 1.x.x

━━━ 2. Server Binary ━━━
  [OK] Binary: ~/.claude/team-claude-server

━━━ 3. Server Status ━━━
  [OK] Server: http://localhost:7890 (healthy)

━━━ 4. Platform & Terminal ━━━
  [OK] OS: macOS
  [OK] Terminal: iTerm2 (recommended)

━━━ 5. Configuration ━━━
  [OK] Config: .claude/team-claude.yaml
  [OK] State: .team-claude/state/workflow.json

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
✅ 인프라 준비 완료
```

## 설정 파일

### 위치

```
.claude/team-claude.yaml
```

### 스키마

```yaml
version: "1.0"

project:
  name: "{project_name}"
  language: "{detected_language}"
  framework: "{detected_framework}"
  domain: "{selected_domain}"
  test_command: "{test_command}"
  build_command: "{build_command}"
  lint_command: "{lint_command}"

feedback_loop:
  mode: auto                  # auto | semi-auto | manual
  max_iterations: 5
  auto_retry_delay: 5000

validation:
  method: test                # test | script | manual
  timeout: 120000

notification:
  method: system              # system | slack | none
  slack:
    webhook_url: ""
    channel: ""

server:
  port: 7890
  executor: iterm             # iterm | terminal-app | headless

agents:
  enabled:
    - spec_validator
    - test_oracle
    - impl_reviewer
  custom:
    - payment-expert
  overrides:
    spec_validator:
      model: opus
```

## 디렉토리 구조

```
.team-claude/
├── sessions/                # 세션 데이터
├── state/                   # 런타임 상태
├── hooks/                   # Hook 스크립트
├── templates/               # 템플릿
└── agents/                  # 커스텀 에이전트
    ├── payment-expert.md
    └── security-auditor.md

.claude/
└── team-claude.yaml         # 메인 설정
```

## Reference Files

- [init-mode.md](./reference/setup/init-mode.md) - 초기화 모드 (프로젝트 분석, 인터뷰)
- [config-management.md](./reference/setup/config-management.md) - 설정 조회/수정
- [agent-management.md](./reference/setup/agent-management.md) - 에이전트 CRUD (HITL)
- [server-management.md](./reference/setup/server-management.md) - 서버 관리
