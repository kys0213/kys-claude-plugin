# 00. Epic 기반 Task Store — 컨셉 문서

> github-autopilot 의 작업 관리 모델을 "GitHub 이슈 = 작업 큐" 에서 "Epic 브랜치 + 로컬 Task Store" 로 재편하는 **컨셉 문서**.
>
> 이 문서는 동기 / 모델 / 큰 그림만 다룬다. 구체적 구현 스펙은 유스케이스 → 아키텍처 → 상세 스펙 → 테스트 순으로 형제 문서에 분할되어 있다:
>
> | 파일 | 다루는 범위 |
> |------|-----------|
> | `00-concept.md` (현재) | 동기, 모델, 데이터 흐름, 라이프사이클 |
> | `01-use-cases.md` | 페르소나별 시나리오. 인터페이스 도출의 출발점 |
> | `02-architecture.md` | 모듈 경계, 의존성 방향, 트랜잭션 / 동시성 정책 |
> | `03-detailed-spec.md` | TaskStore trait, 스키마, 알고리즘 의사코드, CLI 시그니처 |
> | `04-test-scenarios.md` | 블랙박스 테스트 fixture (TDD 원칙용) |
>
> 본 시리즈는 `/develop` 워크플로우의 Phase 1 (DESIGN) 산출물이다. 구현 전 리뷰/승인이 선행되어야 한다.

## 1. 동기

현재 github-autopilot 은 GitHub 이슈를 단일 작업 큐로 사용한다. `gap-watch` / `qa-boost` / `ci-watch` 가 발견한 모든 갭을 이슈로 발행하고, `:ready` 라벨을 통해 `build-issues` 파이프라인이 소비한다.

이 구조의 한계:

1. **이슈 노이즈** — 여러 사람이 같이 개발하는 레포에서, 기계가 발견한 작은 갭들이 사람 이슈 리스트를 오염시킨다. 사람이 "내가 등록한 이슈" 와 "autopilot 이 만든 이슈" 를 구분하기 어렵다.
2. **상태 표현력 부족** — 이슈에는 `:ready`, `:wip`, `:auto` 같은 라벨로 상태를 흉내내고 있어 의존성, 시도 횟수, 재처리 같은 메타가 들어갈 자리가 부족하다.
3. **분해 단위의 부재** — spec 단위로 묶어서 "이 spec 작업이 끝났는지" 를 묻는 자연스러운 단위가 없다. 이슈는 평평하게 흩어져 있다.

목표: **사람-기계 인터페이스(GitHub 이슈)** 와 **기계 내부 작업 큐(로컬 task store)** 를 분리한다.

## 2. 핵심 모델

### 2.1 권한 경계는 Epic

- **Epic = 사람이 부여한 자율 작업 권한의 경계**.
- Epic 은 **사람이 명시적으로 시작 신호를 줄 때만** 생성된다 (`/github-autopilot:epic-start <spec-path>`). 자동 생성은 없다.
- autopilot 은 epic 안에서만 자율적으로 분해/구현/머지를 한다. 어떤 epic 에도 속하지 않는 발견은 사람에게 escalate 한다.

### 2.2 GitHub 이슈의 새 의미

이슈는 더 이상 작업 큐가 아니다. **사람과 autopilot 간의 인터페이스 채널**로 한정한다.

이슈에 남는 것:

- **사람이 직접 등록한 이슈** — 기존 HITL 흐름 유지
- **autopilot 이 escalate 한 이슈** — 사람의 판단이 필요한 상황만:
  - epic 에 매핑되지 않은 watch 발견
  - 반복 실패 (N회 시도 후에도 구현 실패)
  - 의존성 데드락, 충돌 미해결 등 autopilot 이 자체적으로 풀지 못한 상황

### 2.3 진실의 원천 분산

상태는 두 곳에 분산되어 보관된다:

| 원천 | 보관 내용 | 형상관리 |
|------|----------|----------|
| **spec 파일** | "원래 무엇을 해야 했는가" | git (공유) |
| **git remote** | epic 브랜치, task feature 브랜치, merged PR — "어디까지 코드가 올라갔는가" | git (공유) |
| **로컬 SQLite** | task 의 현재 status, 시도 횟수, 의존성, 캐시 | 로컬 (gitignored, 캐시) |

**핵심 성질**: 로컬 SQLite 는 진실의 원천이 아닌 **캐시**다. 날아가도 spec 재분해 + git remote 스캔으로 무손실 복구된다. 이로 인해:

- 동료가 휴가 가도 다른 사람이 같은 epic 을 이어받을 수 있다 (`/epic-resume`)
- 같은 task 를 두 명이 동시에 가져가는 것은 git push reject 에서 자연 차단된다
- 팀원 간 공유 상태(orphan ref 등)를 별도 운영할 필요가 없다

## 3. 저장소 설계

### 3.1 위치

```
spec/<spec-paths>/*.md         # 형상관리 (기존, 변화 없음)
.autopilot/state.db            # 로컬 task 상태 (gitignored)
.autopilot/logs/<epic>/        # 구현 로그 (선택, gitignored)
```

`.autopilot/` 는 autopilot 루프가 실행되는 worktree 의 루트에 위치한다. `.gitignore` 한 줄로 추적에서 제외한다:

```gitignore
/.autopilot/
```

`.git/` 안에 두지 않는 이유: `.git/` 은 git 의 내부 디렉토리이며 `git gc` / `git fsck` 등 내부 도구가 다루는 공간이다. 커스텀 파일을 두는 것은 관습 위반이며 잠재적 충돌 위험이 있다.

### 3.2 DB 접근 주체

DB 의 읽기/쓰기는 **autopilot 메인 루프(부모 에이전트)** 만 수행한다. `build-tasks` 가 worktree 를 만들어 병렬 구현을 띄울 때, 자식 implementer 에이전트는 DB 를 직접 만지지 않는다.

| 주체 | 역할 | DB 접근 |
|------|------|---------|
| 메인 autopilot 루프 | task 큐잉, 상태 전이, 의존성/escalation 판정 | read + write |
| 자식 implementer (worktree) | 받은 task 사양으로 코드 작성 + 브랜치 push | 없음 |

자식이 DB 를 안 만지므로:

- worktree 간 DB 공유 / 락 / 경로 해석 같은 복잡성 불필요
- 자식이 비정상 종료해도 DB 무결성 영향 없음 (부모가 결과를 보고 attempts 증가 / status 갱신)
- 부모는 단일 writer 이므로 SQLite 의 일반적 동시성 케이스 (다중 writer) 도 발생하지 않음

이 단일-writer 가정 덕분에 같은 메인 루프 안에서 cron 으로 도는 여러 sub-loop (`gap-watch`, `build-tasks`, `merge-prs` 등) 만 동시 접근자를 고려하면 된다. 이는 SQLite WAL 모드로 충분히 처리된다.

### 3.3 스키마

```sql
CREATE TABLE epics (
  name           TEXT PRIMARY KEY,        -- 예: "auth-token-refresh"
  spec_path      TEXT NOT NULL,           -- 예: "spec/auth.md"
  branch         TEXT NOT NULL,           -- 예: "epic/auth-token-refresh"
  status         TEXT NOT NULL,           -- active | completed | abandoned
  created_at     TEXT NOT NULL,
  completed_at   TEXT
);

CREATE TABLE tasks (
  id              TEXT PRIMARY KEY,       -- 결정적 ID (3.3 참조)
  epic_name       TEXT NOT NULL REFERENCES epics(name),
  source          TEXT NOT NULL,          -- decompose | gap-watch | qa-boost | ci-watch | human
  fingerprint     TEXT,                   -- watch 류의 중복 검사용 (기존 fingerprint 호환)
  title           TEXT NOT NULL,
  body            TEXT,
  status          TEXT NOT NULL,          -- pending | ready | wip | blocked | done | escalated
  attempts        INTEGER NOT NULL DEFAULT 0,
  branch          TEXT,                   -- "epic/<name>/<task-slug>"
  pr_number       INTEGER,
  escalated_issue INTEGER,                -- escalate 시 GitHub 이슈 번호
  created_at      TEXT NOT NULL,
  updated_at      TEXT NOT NULL
);

CREATE TABLE task_deps (
  task_id     TEXT NOT NULL REFERENCES tasks(id),
  depends_on  TEXT NOT NULL REFERENCES tasks(id),
  PRIMARY KEY (task_id, depends_on)
);

CREATE TABLE events (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  task_id    TEXT,
  epic_name  TEXT,
  kind       TEXT NOT NULL,                -- claimed | started | completed | failed | escalated | ...
  payload    TEXT,                          -- JSON
  at         TEXT NOT NULL
);

CREATE INDEX idx_tasks_epic_status ON tasks(epic_name, status);
CREATE INDEX idx_tasks_fingerprint ON tasks(fingerprint);
```

상태 전이:

```
pending  ─── deps satisfied ──▶ ready
ready    ─── claim (atomic) ──▶ wip
wip      ─── PR merged ───────▶ done
wip      ─── max attempts ────▶ escalated   (GitHub 이슈 발행)
wip      ─── deps fail ───────▶ blocked
blocked  ─── deps recover ────▶ ready
*        ─── manual override ─▶ any         (사람이 CLI 로 강제 변경 가능)
```

**원자적 claim** 은 SQLite 의 행 단위 잠금으로:

```sql
UPDATE tasks
   SET status = 'wip', attempts = attempts + 1, updated_at = ?
 WHERE id = ? AND status = 'ready';
-- changes() == 1 인 경우만 성공
```

### 3.4 결정적 Task ID

UUID 를 매번 새로 생성하면 spec 재분해 후 매칭이 안 된다. Task ID 는 **spec 내용으로부터 결정적으로 도출**한다.

```
task_id = sha256(epic_name || ":" || section_path || ":" || requirement_slug)[:12]
```

- `section_path`: spec 파일 안 헤딩 경로 (예: `## 인증 / ### 토큰 갱신`)
- `requirement_slug`: 요구사항 항목의 정규화된 슬러그

같은 spec 을 다시 분해하면 같은 task_id 가 나온다. 따라서:

- `git fetch` 후 리모트 브랜치 이름 (`epic/<name>/<task_id>`) 과 1:1 매칭 가능
- 다른 머신에서 `epic-resume` 해도 동일 task 식별 가능

### 3.5 SQLite 선택 근거 (대안 비교)

| 항목 | SQLite | 파일 기반 (YAML/JSONL) |
|------|--------|----------------------|
| 원자적 claim | `UPDATE WHERE` 한 줄 | flock + 파일 rename 로직 필요 |
| 동시성 | WAL 모드, 다중 reader/단일 writer | OS 레벨 락 직접 관리 |
| 쿼리 | "ready + deps 완료" 한 SQL | 전체 스캔 후 필터 |
| 스키마 강제 | 컬럼 타입/제약 | 검증 로직 직접 작성 |
| 디버깅 | `sqlite3` CLI 필요 | `cat`/`grep` |
| 머지 | 바이너리 (충돌 시 복구 불가) | 줄 단위 머지 가능 |

팀 공유 요건이 빠진 (= git 머지를 안 거치는) 시점에서 머지 가능성의 가치는 사라지고, 동시성/원자성/쿼리 편의의 가치가 우세해진다. **로컬 캐시 한정** 이라는 전제 하에 SQLite 를 선택한다.

## 4. Reconciliation 프로토콜

Epic 재개 / DB 손실 복구 / 사람의 직접 push 인식 시 사용한다. **Agent 가 정보를 모아 autopilot CLI 에 한 번에 넘긴다** — autopilot 은 받은 plan 을 idempotent 하게 적용할 뿐 git/spec 을 직접 만지지 않는다.

```
[Agent 측]
  1. monitor 호출 → 기대 task 목록 + 의존성 (결정적 ID 부여)
  2. git fetch origin
  3. ls-remote: "epic/<name>" 및 "epic/<name>/*"
  4. PR 스캔: target = "epic/<name>"
  5. 매칭하여 RemoteTaskState 빌드:
     - PR merged                    → done
     - feature 브랜치 + open PR     → wip (검토중)
     - feature 브랜치 + PR 없음    → wip (구현중) 또는 stale 후보
     - 브랜치 없음                  → ready (deps 만족 시) 또는 pending
  6. 분해 결과에 없는 epic/<name>/* 브랜치 목록 → orphan_branches 로 모음

[Agent → Autopilot CLI]
  autopilot epic reconcile --name <name> --plan <jsonl>
    (또는 --tasks-from + --remote-state 분리 옵션)

[Autopilot 측]
  7. plan 검증 (cycle, 알 수 없는 deps, ...)
  8. tasks upsert + deps 갱신 + status 적용 + reconciled 이벤트 기록
```

이 절차는 **idempotent** 해야 한다. 동일 plan 으로 N번 호출해도 결과가 같아야 하며, 재시도 안전하다. attempts 카운터는 보존된다 (DB 손실 시는 0 으로 초기화 — 허용 손실).

## 5. 책임 분리 — Ledger 모델

autopilot 은 **상태 관리 + CLI 표면** 만 책임진다. 탐지 / 결정 / 구현은 **외부 에이전트 (Agent)** 가 수행하고, autopilot 은 그 에이전트가 호출하는 도구로 위치한다.

```
┌─ Agent (Claude Code 세션, GitHub Actions, autodev 등) ─┐
│  - monitor 호출 (gap 탐지)                              │
│  - autopilot CLI 호출 (task 적재 / claim / 결과 보고)   │
│  - 직접 구현 + git/PR 조작                              │
│  - escalation 이슈 발행 / 알림                          │
└────────────────────────────────────────────────────────┘
                        ↓ CLI
┌─ Autopilot ────────────────────────────────────────────┐
│  - state.db 관리 (epics / tasks / events / suppression)│
│  - 결정적 상태 전이 (claim 원자성, deps 재평가, ...)   │
│  - 헬퍼 명령 (worktree, label, issue, watch, ...)      │
└────────────────────────────────────────────────────────┘
```

### 5.1 Autopilot 의 책임 표면

**A. Ledger CLI** (신규 — Round 2 이후 추가)

| 그룹 | 명령 (요지) | 역할 |
|------|------------|------|
| epic | `epic create / list / get / status / complete / abandon / reconcile` | epic 행 CRUD + 라이프사이클 전이 |
| task | `task add / list / get / claim / release / complete / fail / escalate / force-status / find-by-pr` | task 행 CRUD + 상태 전이 (원자성 보장) |
| suppress | `suppress add / check / clear` | escalation fingerprint 억제 |
| events | `events list` | audit 로그 조회 |

**B. 헬퍼 명령** (기존 유지 — 의사결정 없는 순수 도구)

| 명령 | 역할 |
|------|------|
| `worktree` | git worktree CRUD |
| `simhash` | fingerprint 해싱 |
| `labels` | GitHub 라벨 CRUD |
| `issue`, `issue list` | GitHub 이슈 CRUD (gh wrapping) |
| `preflight` | 환경 검증 |
| `stats`, `check` | 읽기 전용 분석 |
| `pipeline idle` | 라벨 기반 idle 판정 (count 기반) |
| `watch run` | 외부 변화를 JSON 이벤트로 stdout 에 emit (long-poll, 의사결정 없음) |

이 헬퍼들은 **에이전트의 도구** 로 분류한다. 에이전트가 필요할 때 호출하고, 출력을 보고 결정한다.

### 5.2 Agent 의 책임 (autopilot 외부)

| 책임 | 설명 |
|------|------|
| **갭 탐지** | monitor (예: `gap-detector`, `qa-boost`) 호출. 결과를 autopilot CLI 로 task 화 |
| **분해 정책** | spec-kit 재사용 / 자체 markdown 파서 / LLM prompt 등 — 에이전트 측 결정 |
| **claim → 구현 → PR** | task claim 후 코드 작성 / 브랜치 push / PR 생성 |
| **결과 보고** | 성공 시 `task complete --pr N`, push reject 시 `task release`, 실패 시 `task fail` |
| **Escalation 발행** | `task fail` 결과가 `Escalated` 면 GitHub 이슈 발행 + `task escalate` |
| **Escalation 해소 감지** | 주기적으로 escalated task 의 issue 상태 폴링 → close 시 `epic reconcile` 또는 `task force-status` |
| **알림** | epic 완료 / escalation 시 사용자 알림 |

### 5.3 라벨 / 호환성

- `:auto`, `:ci-failure` 라벨은 PR / 사람 이슈에서 그대로 의미를 가짐 (변경 없음)
- `:ready`, `:wip` 라벨은 `tasks.status` 컬럼이 의미를 대체. **단 autopilot 자체는 라벨을 큐로 쓰지 않음** — 에이전트가 라벨 기반 흐름을 유지하고 싶으면 별도 매핑 가능
- `autopilot issue create` 의 watch-source 호출 경로 → task store insert 로 대체

## 6. HITL Escalation

### 6.1 Escalation 트리거 (모두 Agent 가 발행)

| 조건 | Agent 동작 |
|------|----------|
| 갭이 어떤 활성 epic 에도 매칭되지 않음 | autopilot `suppress check` 로 중복 차단 검사 → 신규면 이슈 발행 + `suppress add fingerprint reason="unmatched_watch"` |
| `task fail` 결과가 `Escalated` | 이슈 발행 + `task escalate <id> --issue <n>` (의존 task 들은 autopilot 이 자동으로 `blocked`) |
| 의존성 사이클 / unresolvable | epic 적재 시 autopilot 이 거부. agent 가 그 에러를 받고 이슈 발행 |
| 충돌 자동 해결 실패 | agent 의 기존 충돌 알림 흐름 유지 |

### 6.2 Escalation 이슈 포맷

```yaml
title: "[autopilot] <짧은 요약>"
labels: ["autopilot:hitl-needed"]
body: |
  <!-- autopilot-escalation -->
  <!-- epic: <epic-name or "none"> -->
  <!-- task-id: <id or ""> -->
  <!-- reason: <unmatched|max-attempts|...> -->

  ## 상황
  ...

  ## Agent 가 시도한 것
  ...

  ## 사람이 결정해야 할 것
  - [ ] ...
```

사람이 이 이슈를 close 하면, **agent 가 다음 cycle 에서 close 를 감지** 하여:
- 사람이 직접 코드를 푸시했고 PR 머지된 경우 → `epic reconcile` 호출 → task 가 done 으로 인식
- 그렇지 않으면 → `suppress add reason="rejected_by_human"` 로 동일 fingerprint 의 재 escalation 차단

## 7. Epic 라이프사이클

```
[사람]  agent 에게 "spec/auth.md 로 epic 시작" 요청
  ↓
[Agent] monitor 호출 → spec 분해 결과 획득
        autopilot epic create --name auth-token-refresh --spec spec/auth.md
        autopilot task add --epic ... (각 task 별 또는 batch)
        git checkout -b epic/auth-token-refresh && git push
  ↓
[Agent] (cron / loop 마다 반복)
        autopilot task claim --epic ...   → ready task 1건
        구현 → push → PR 생성
        autopilot task complete --id <id> --pr <n>   (또는 fail / release)
  ↓ (모든 task done)
[Agent] autopilot epic status 가 all-done 보고 → epic complete + 사용자 알림
  ↓
[사람]  epic 브랜치 검증 + main 으로 PR 직접 생성 (자동화 안 함)
```

main 으로의 promote 가 자동화되지 않는 이유: epic 단위는 큰 변경이라 사람의 최종 검토가 안전판으로 필요하다.

## 8. 마이그레이션

기존 라벨 기반 사용자가 새 모델로 넘어오는 경로.

### 8.1 단계

1. **공존 기간**: `epic_based: false` (기본값) 설정 시 기존 라벨 기반 동작 유지. `true` 일 때만 새 모델 활성화.
2. **잔여 이슈 처리**: 마이그레이션 시점의 `:ready` 이슈는 두 가지 옵션 제공:
   - (a) 적절한 epic 을 만들고 task 로 흡수 (`autopilot migrate import-issue <#>`)
   - (b) 그대로 두고 사람이 수동 처리
3. **기본값 전환**: 안정화 후 다음 메이저 버전에서 `epic_based: true` 를 기본으로.

### 8.2 설정 신규 항목

```yaml
epic_based: true
epic_branch_prefix: "epic/"
max_attempts: 3                  # task 가 escalated 로 갈 시도 횟수
hitl_label: "autopilot:hitl-needed"
```

### 8.3 호환성 메모

- `:auto`, `:ci-failure` 라벨은 변하지 않음 — 기존 PR 흐름 그대로
- `notification` 설정은 epic 완료 알림에 그대로 사용
- `fingerprint` 는 watch 류의 중복 검사용으로 task 행에 저장하여 동작 호환

## 9. 미해결 / 후속 검토 항목

설계 단계에서 명시적으로 결정을 미룬 사항.

1. **Stale 브랜치 정책** — feature 브랜치는 있으나 PR 도 없고 일정 시간 변경도 없는 task 를 어떻게 처리할지. (Agent 측 정책)
2. **여러 epic 동시 활성** 시 자원 한도 — `max_parallel_agents` 같은 한도는 **agent 측** 의 결정. autopilot 은 임의 다중 epic 을 그냥 수용.
3. **Schema 마이그레이션** — 향후 스키마 변경 시 사용자 로컬 DB 를 업데이트할 절차. V1→V2 자동 적용 패턴 채택 (이미 구현). 다운그레이드는 비지원.
4. **이벤트 로그 보존 기간** — `events` 테이블 무한 증가 방지 정책 (단일 cron `DELETE` 로 N일 prune 권장).
5. **`escalation_suppression` TTL 정리** — 만료된 행을 lazy 또는 주기적으로 cleanup.

> **해소된 항목**: "spec 분해 주체" — Ledger 모델 채택으로 autopilot 의 결정 사항이 아님. Agent (또는 monitor) 가 자유롭게 선택.

## 10. 다음 단계

- [x] PR #646 — 도메인 타입 + 인메모리/SQLite 어댑터 골격
- [x] PR #648 — spec 정제 1차 (트레이트 시그니처)
- [x] PR #649 — TaskStore 트레이트 + V2 schema + conformance suite
- [ ] **현재 PR** — Ledger 모델로 spec rework (orchestration 제거)
- [ ] **Round 2-A**: `cmd/epic.rs` (epic CRUD CLI)
- [ ] **Round 2-B**: `cmd/task.rs` 확장 (claim / complete / fail / escalate)
- [ ] **Round 2-C**: `cmd/suppress.rs` + `cmd/events.rs` + `main.rs` wiring + autopilot.toml
- [ ] **Round 3**: 기존 헬퍼 (`pipeline`, `watch`, ...) 와 새 ledger CLI 의 정합성 점검 / 필요 시 deprecate
