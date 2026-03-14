# DESIGN v4: 컴포넌트 구조

> 12개 flow에서 도출된 컴포넌트 전체 구조.
> 각 컴포넌트의 상세 설계는 해당 spec 디렉토리의 `design.md` 참조.

---

## 핵심 원칙

```
Daemon = 토큰 안 쓰는 인프라 (수집, 상태 관리, 실행, cron)
Claw   = 토큰 쓰는 판단 전부 (큐 평가, 스펙 분해, gap 탐지, HITL)
```

Daemon 자체는 **LLM을 호출하지 않는다**. GitHub API 스캔, DB 상태 관리, ready Task 실행 트리거, cron job 스케줄링만 수행한다. 토큰을 소비하는 판단은 전부 Claw(headless 또는 interactive)가 담당하며, Task 실행 시의 토큰은 개별 Task(AnalyzeTask, ImplementTask 등)가 사용한다.

### Claw의 두 가지 실행 모드

```
Interactive (autodev agent):
  → 사용자가 직접 실행, 대화형
  → /hitl, /board, /status 등 slash command
  → 사용자가 원할 때 실행

Headless (daemon 틱에서 자동 호출):
  → daemon이 주기적으로 claude -p --cwd claw-workspace 실행
  → Claw가 자동으로 큐 평가 → advance/skip/hitl 판단
  → claw.schedule_interval_secs 주기 (기본 60초)
  → force_claw_next_tick 시 즉시 호출
```

**둘 다 같은 claw-workspace를 사용**한다. 같은 CLAUDE.md, rules, skills.
사용자가 `autodev agent`를 안 띄워도 daemon이 headless로 Claw를 호출하여 자율 루프가 동작한다.

### Daemon 이벤트 루프: Heartbeat + Cron

Daemon은 **heartbeat**(틱)과 **cron**(스케줄) 두 가지 루프를 운영한다.

```
Heartbeat (tick_interval_secs, 기본 10초):
  → 가볍고 빠른 반복
  1. Collector.collect() → DB에 저장 (수집)
  2. Queue에서 "ready" 상태 작업 꺼내서 실행 (실행)
  3. 실행 결과 → DB에 저장 (저장)

Cron (사용자 정의 스케줄):
  → 무거운 작업을 주기적으로 실행
  → 등록된 cron job 목록을 관리
```

### 기본 Cron Jobs

cron job은 **global**(레포 무관)과 **per-repo**(레포 컨텍스트 필요)로 구분된다.
per-repo job은 `claude -p --cwd <repo-path>`로 실행되므로 레포 정보가 필수.

#### Global (레포 무관)

| Job | 기본 주기 | 동작 |
|-----|----------|------|
| **hitl-timeout** | 5분 | 미응답 HITL 타임아웃 확인 (DB만 조회) |
| **daily-report** | 매일 06시 | 전체 레포 집계 리포트 |
| **log-cleanup** | 매일 00시 | 오래된 로그 정리 |

#### Per-repo (레포 컨텍스트 필요)

| Job | 기본 주기 | 동작 |
|-----|----------|------|
| **claw-evaluate** | 60초 | Claw headless 호출 (큐 평가 → advance/skip/hitl) |
| **gap-detection** | 1시간 | 스펙-코드 대조 (레포 코드 읽기 필요) |
| **knowledge-extract** | 1시간 | merged PR에서 지식 추출 |

레포 등록 시 per-repo 기본 cron이 자동 등록된다.

### Cron = 스크립트 실행

cron job은 **스크립트 파일**을 주기적으로 실행한다.
guard 로직(사전 조건 체크)은 스크립트 안에 포함되어, daemon의 cron engine은 순수 스케줄러.

```
~/.autodev/crons/
├── (built-in, 레포 등록 시 자동 생성)
│   ├── claw-evaluate.sh        ← guard + claude -p
│   ├── gap-detection.sh        ← guard + claude -p
│   ├── knowledge-extract.sh    ← guard + autodev CLI
│   ├── hitl-timeout.sh         ← guard + autodev CLI
│   ├── daily-report.sh         ← guard + claude -p
│   └── log-cleanup.sh          ← guard + autodev CLI
│
└── (user-defined, /cron add로 등록)
    ├── code-smell-detect.sh    ← 사용자 작성
    └── nightly-test.sh         ← 사용자 작성
```

### 스크립트 구조 (guard + 실행)

```bash
#!/bin/bash
# ~/.autodev/crons/claw-evaluate.sh (built-in 예시)

REPO="org/repo-a"

# Guard: 큐에 pending 아이템이 있을 때만
PENDING=$(autodev queue list --repo "$REPO" --json | jq '.[] | select(.phase=="Pending") | length')
HITL=$(autodev hitl list --repo "$REPO" --json | jq 'length')

if [ "$PENDING" = "0" ] && [ "$HITL" = "0" ]; then
  echo "skip: 큐 비어있고 HITL 없음"
  exit 0
fi

# 실행
claude -p "큐를 평가하고 다음 작업을 결정해줘" --cwd ~/.autodev/claw-workspace
```

```bash
#!/bin/bash
# ~/.autodev/crons/code-smell-detect.sh (사용자 커스텀 예시)

REPO="org/repo-a"
REPO_PATH="/path/to/repo-a"

# Guard: 변경사항 있을 때만
if git -C "$REPO_PATH" diff --quiet HEAD~1; then
  echo "skip: 변경사항 없음"
  exit 0
fi

# 실행: Claw에게 코드 스멜 감지 요청
claude -p "/코드스멜감지를 실행하여 개선해줘" --cwd "$REPO_PATH"
```

### 기본 cron vs 커스텀 cron

| | 기본 cron (built-in) | 커스텀 cron (user-defined) |
|---|---|---|
| **스크립트** | 레포 등록 시 자동 생성 | 사용자가 직접 작성 |
| **guard** | 내장 (불필요한 LLM 호출 방지) | 사용자 자유 (없어도 됨) |
| **제거** | 불가 (pause/resume만) | 자유롭게 추가/제거 |
| **수정** | 스크립트 편집으로 guard/명령 커스터마이즈 가능 | 자유 |

### Cron Engine 역할

```
daemon cron engine:
  주기 도달 → 환경변수 주입 → 스크립트 실행 → exit code 확인 → 로그 기록
```

daemon은 **환경변수 주입 + 스크립트 실행기**. guard도 명령도 전부 스크립트 안에 있다.

### 주입 환경변수

daemon이 cron 스크립트 실행 시 자동으로 주입하는 환경변수:

#### Per-repo (--repo 지정 시)

| 변수 | 설명 | 예시 |
|------|------|------|
| `AUTODEV_REPO_NAME` | 레포 이름 | `org/repo-a` |
| `AUTODEV_REPO_ROOT` | 레포 로컬 경로 | `/Users/me/repos/repo-a` |
| `AUTODEV_REPO_URL` | GitHub URL | `https://github.com/org/repo-a` |
| `AUTODEV_REPO_DEFAULT_BRANCH` | 기본 브랜치 | `main` |
| `AUTODEV_WORKSPACE` | autodev 워크스페이스 경로 | `~/.autodev/workspaces/org-repo-a` |
| `AUTODEV_CLAW_WORKSPACE` | Claw 워크스페이스 경로 | `~/.autodev/claw-workspace` |

#### Global (항상)

| 변수 | 설명 | 예시 |
|------|------|------|
| `AUTODEV_HOME` | autodev 홈 디렉토리 | `~/.autodev` |
| `AUTODEV_DB` | DB 경로 | `~/.autodev/autodev.db` |
| `AUTODEV_CLAW_WORKSPACE` | Claw 워크스페이스 경로 | `~/.autodev/claw-workspace` |

### 환경변수 활용 예시

같은 스크립트를 여러 레포에 재사용:

```bash
#!/bin/bash
# ~/.autodev/crons/code-smell-detect.sh
# 어떤 레포에 등록해도 동작하는 재사용 가능한 스크립트

# Guard: 변경사항 있을 때만
if git -C "$AUTODEV_REPO_ROOT" diff --quiet HEAD~1; then
  echo "skip: $AUTODEV_REPO_NAME 변경사항 없음"
  exit 0
fi

# 실행
claude -p "/코드스멜감지를 실행하여 개선해줘" --cwd "$AUTODEV_REPO_ROOT"
```

```bash
# 동일한 스크립트를 여러 레포에 등록
autodev cron add --name code-smell --repo org/repo-a --interval 3600 \
  --script ~/.autodev/crons/code-smell-detect.sh
autodev cron add --name code-smell --repo org/repo-b --interval 3600 \
  --script ~/.autodev/crons/code-smell-detect.sh
# → daemon이 레포별로 AUTODEV_REPO_ROOT 등을 주입하여 실행
```

built-in cron도 동일한 환경변수를 사용:

```bash
#!/bin/bash
# ~/.autodev/crons/claw-evaluate.sh (built-in)

# Guard
PENDING=$(autodev queue list --repo "$AUTODEV_REPO_NAME" --json | jq 'length')
if [ "$PENDING" = "0" ]; then
  echo "skip: $AUTODEV_REPO_NAME 큐 비어있음"
  exit 0
fi

# 실행
claude -p "큐를 평가하고 다음 작업을 결정해줘" --cwd "$AUTODEV_CLAW_WORKSPACE"
```

### Cron 관리

Claw 세션에서 `/cron` slash command로 관리하거나, CLI로 직접 관리.

#### Slash Command (Claw 세션)

```
> /cron list
Global:
  hitl-timeout      5분      active
  daily-report      매일06시  active
  log-cleanup       매일00시  active

Per-repo (org/repo-a):
  claw-evaluate     60초     active
  gap-detection     1시간    active
  knowledge-extract 1시간    paused

Per-repo (org/repo-b):
  claw-evaluate     60초     active
  gap-detection     1시간    active

> /cron pause gap-detection --repo repo-a
✅ org/repo-a의 gap-detection을 일시정지했습니다.

> /cron add --name "nightly-test" --repo repo-a --schedule "0 2 * * *" \
    --command "cargo test"
✅ org/repo-a에 nightly-test cron을 등록했습니다.
```

#### CLI (인프라 도구)

```bash
# 전체 cron 목록
autodev cron list --json

# 커스텀 cron 추가 (스크립트 파일 기반)
autodev cron add --name "code-smell" --repo org/repo-a \
  --interval 3600 --script ~/.autodev/crons/code-smell-detect.sh

# cron expression도 지원
autodev cron add --name "nightly-test" --repo org/repo-a \
  --schedule "0 2 * * *" --script ~/.autodev/crons/nightly-test.sh

# 수정 / 정지 / 재개 / 제거 / 즉시 실행
autodev cron update <name> [--repo <r>] --interval <s>
autodev cron pause <name> [--repo <r>]
autodev cron resume <name> [--repo <r>]
autodev cron remove <name> [--repo <r>]
autodev cron trigger <name> [--repo <r>]    # 스크립트 즉시 실행
```

### Claw Headless 호출 (cron: claw-evaluate)

```
claw-evaluate 실행 시:
  claude -p "큐를 평가하고 다음 작업을 결정해줘" --cwd claw-workspace
  → Claw가 autodev queue list --json 호출
  → 판단 후 autodev queue advance/skip 호출
  → 결과가 DB에 반영됨
  → 다음 heartbeat에서 ready 작업 실행
```

### force 트리거

특정 이벤트 발생 시 `claw-evaluate` cron을 즉시 실행:

```
force 트리거:
  - 스펙 등록 (autodev spec add)
  - Task 실패 완료
  - 스펙 연관 Task 완료
  - HITL 응답 수신
```

---

## 전체 아키텍처

```
┌──────────────────────────────────────────────────────────────────┐
│  사용자                                                          │
│                                                                  │
│  레포 Claude 세션          Claw 세션              터미널          │
│  ├─ /add-spec             ├─ /status              autodev       │
│  └─ /update-spec          ├─ /board               dashboard     │
│       │                   ├─ /hitl                    │          │
│       │                   ├─ /spec                    │          │
│       │                   └─ /decisions               │          │
└───────┼───────────────────────┼───────────────────────┼──────────┘
        │                       │                       │
        ▼                       ▼                       ▼
┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐
│  Plugin Commands │  │  Claw Agent      │  │  TUI Dashboard   │
│  (SSOT)          │  │  (Claude Code in │  │  (읽기 전용)      │
│                  │  │  claw-workspace) │  │                  │
│  /add-spec       │  │  CLAUDE.md       │  │  BoardRenderer   │
│  /update-spec    │  │  rules/          │  │  trait            │
│                  │  │  skills/         │  │                  │
│                  │  │  commands/       │  │                  │
└────────┬─────────┘  └────────┬─────────┘  └────────┬─────────┘
         │                     │                      │
         │      ┌──────────────┘                      │
         ▼      ▼                                     ▼
┌──────────────────────────────────────────────────────────────────┐
│  autodev CLI (인프라 도구)                                        │
│                                                                  │
│  autodev spec add/list/show/status/update/pause/resume           │
│  autodev hitl list/show/respond                                  │
│  autodev repo add/list/show/update/remove                        │
│  autodev queue list/show/advance/skip                            │
│  autodev claw init/rules/edit                                    │
│  autodev agent / dashboard / start / stop                        │
└────────┬─────────────────────────────────────────────────────────┘
         │
         ▼
┌──────────────────────────────────────────────────────────────────┐
│  Daemon (토큰 0 — LLM 호출 없음)                                  │
│                                                                  │
│  ┌────────────────────────────────────────────────────────┐      │
│  │  Heartbeat (tick_interval_secs, 기본 10초)             │      │
│  │                                                        │      │
│  │  ┌─ Collector (OCP)        외부 소스 스캔 → DB 저장    │      │
│  │  │  ├─ GitHubCollector     이슈/PR 라벨 기반 감지      │      │
│  │  │  └─ (향후 확장)                                    │      │
│  │  │                                                    │      │
│  │  ├─ State Store (DB)       큐/스펙/HITL/로그 관리      │      │
│  │  │                                                    │      │
│  │  └─ Task Executor          ready 작업 꺼내서 실행      │      │
│  │     → claude 호출 (토큰은 Task가 소비)                │      │
│  │     → 실행 결과 → DB 저장                             │      │
│  └────────────────────────────────────────────────────────┘      │
│                                                                  │
│  ┌────────────────────────────────────────────────────────┐      │
│  │  Cron Engine               등록된 job 주기 실행        │      │
│  │  ├─ claw-evaluate (60초)   Claw headless 호출         │      │
│  │  ├─ gap-detection (1시간)  스펙-코드 대조              │      │
│  │  ├─ knowledge-extract      merged PR 지식 추출        │      │
│  │  ├─ daily-report           일간 리포트                │      │
│  │  ├─ hitl-timeout (5분)     미응답 HITL 확인           │      │
│  │  └─ log-cleanup            오래된 로그 정리           │      │
│  └────────────────────────────────────────────────────────┘      │
│                                                                  │
│  ┌────────────────────────────────────────────────────────┐      │
│  │  Notifier (OCP)            이벤트 발생 시 알림         │      │
│  │  ├─ GitHubCommentNotifier                             │      │
│  │  └─ WebhookNotifier                                   │      │
│  └────────────────────────────────────────────────────────┘      │
└──────────────────────────────────────────────────────────────────┘
```

### Claw의 큐 조작 흐름

```
Claw: autodev queue list --json     # 큐 상태 확인
Claw: (판단)                         # "42는 진행, 43은 대기"
Claw: autodev queue advance 42      # 42를 다음 phase로
                                     # → DB 상태 변경

Daemon 다음 틱:
  → 42가 ready 상태 → Task 실행
  → 43은 아직 pending → 대기
```

---

## 컴포넌트 목록

### Layer 1: 사용자 인터페이스

| 컴포넌트 | 설명 | 상세 설계 |
|----------|------|----------|
| **Plugin Commands** | /add-spec, /update-spec — 레포 Claude 세션에서 실행, SSOT | [03](./03-spec-registration/) [07](./07-feedback-loop/) |
| **Claw Agent** | autodev agent — claw-workspace의 Claude Code 세션 | [10](./10-claw-workspace/) |
| **TUI Dashboard** | autodev dashboard — 읽기 전용 칸반 보드 | [06](./06-kanban-board/) |
| **Claw Commands** | /status, /board, /hitl 등 — Claw 세션 전용 | [12](./12-cli-reference/) |

### Layer 2: autodev CLI (Claw가 조작하는 도구)

| 컴포넌트 | 설명 | 상세 설계 |
|----------|------|----------|
| **Spec Manager** | spec CRUD + 검증 + spec_issues 관리 | [03](./03-spec-registration/) |
| **HITL Manager** | HITL 이벤트 생성/조회/응답/타임아웃 | [05](./05-hitl-notification/) |
| **Queue Manager** | 큐 조회 + **상태 전이** (advance/skip) — Claw가 호출 | [04](./04-spec-priority/) |
| **Repo Manager** | 레포 등록/설정/제거 | [01](./01-repo-registration/) |
| **Claw Workspace** | init/rules/edit — 워크스페이스 관리 | [10](./10-claw-workspace/) |

### Layer 3: Daemon (수동적 인프라)

Daemon은 **판단하지 않는다**. 수집, 저장, ready 실행만 수행.

| 컴포넌트 | 설명 | 상세 설계 |
|----------|------|----------|
| **Collector** (trait, OCP) | 외부 소스 스캔 → DB 저장 | [01](./01-repo-registration/) |
| **State Store** | DB 기반 상태 관리 (큐, 스펙, HITL, 로그) | [DESIGN.md](./DESIGN.md) |
| **Task Executor** | ready 상태 작업을 꺼내 실행 | v3 Task trait 유지 |
| **Notifier** (trait, OCP) | HITL/Task 이벤트 알림 디스패치 | [05](./05-hitl-notification/) |

### Layer 4: Storage

| 컴포넌트 | 설명 | 상세 설계 |
|----------|------|----------|
| **SpecRepository** (trait) | specs + spec_issues CRUD | [03](./03-spec-registration/) |
| **HitlRepository** (trait) | hitl_events + hitl_responses CRUD | [05](./05-hitl-notification/) |
| **기존 Repository** | RepoRepository, ScanCursorRepository, ConsumerLogRepository, TokenUsageRepository | 유지 |

---

## OCP 확장점

| trait | 구현체 | 확장 방법 |
|-------|--------|----------|
| **Collector** | GitHubCollector | 새 소스 = 구현체 추가 |
| **Notifier** | GitHubCommentNotifier, WebhookNotifier | 새 채널 = 구현체 추가 |
| **BoardRenderer** | TuiBoardRenderer | 새 렌더러 = 구현체 추가 |

---

## 신규 DB 테이블

```sql
-- 스펙
specs (id, repo_id, title, body, status, source_path,
       test_commands, acceptance_criteria, created_at, updated_at)
spec_issues (spec_id, issue_number, created_at)

-- HITL
hitl_events (id, repo_id, spec_id, work_id, severity, situation, context, options, status, created_at)
hitl_responses (id, event_id, choice, message, source, created_at)

-- Claw 판단
claw_decisions (id, repo_id, spec_id, decision_json, created_at)
```

---

## 기존 코드 변경 범위

| 변경 유형 | 대상 |
|----------|------|
| **유지** | Task trait, TaskRunner, AnalyzeTask, ImplementTask, ReviewTask, ImproveTask, ExtractTask, StateQueue |
| **단순화** | Daemon — drain_queue_items/MechanicalScheduler 제거, 순수 수집+실행으로 축소 |
| **리팩토링** | GitHubTaskSource → GitHubCollector (scan만, drain 제거) |
| **확장** | config/models.rs (ClawConfig), queue/schema.rs (테이블 추가), CLI (queue advance/skip) |
| **신규** | spec/ CLI, hitl/ CLI, Plugin commands, claw-workspace 파일 |

---

## 구현 순서 제안

```
Phase 1: 설계 확정
  → spec/DESIGN.md 확정 (지금)
  → 각 항목 design.md 작성

Phase 2: Storage + CLI
  → DB 테이블 추가 (specs, hitl_events, hitl_responses, claw_decisions)
  → SpecRepository, HitlRepository trait + impl
  → autodev spec/hitl/queue CLI 구현

Phase 3: Daemon 단순화
  → GitHubTaskSource → GitHubCollector (drain 제거)
  → Daemon에서 drain_queue_items 제거
  → "ready" 상태만 실행하는 단순 executor로 축소
  → Notifier trait + GitHubCommentNotifier

Phase 4: Claw Workspace + Plugin Commands
  → claw-workspace 구조 생성 (CLAUDE.md, rules/, skills/, commands/)
  → /add-spec, /update-spec plugin commands
  → autodev agent 구현

Phase 5: TUI Dashboard
  → BoardRenderer trait + TuiBoardRenderer
  → BoardState Builder

Phase 6: Convention Engine
  → 기술 스택 감지
  → 대화형 부트스트랩
  → 피드백 기반 자율 정제
```
