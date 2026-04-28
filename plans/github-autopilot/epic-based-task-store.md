# Epic 기반 Task Store 설계

> github-autopilot 의 작업 관리 모델을 "GitHub 이슈 = 작업 큐" 에서 "Epic 브랜치 + 로컬 Task Store" 로 재편하는 설계 문서.
>
> 본 문서는 `/develop` 워크플로우의 Phase 1 (DESIGN) 산출물이다. 구현 전 리뷰/승인이 선행되어야 한다.

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
spec/<spec-paths>/*.md            # 형상관리 (기존, 변화 없음)
<main-worktree>/.autopilot/state.db        # 로컬 task 상태 (gitignored)
<main-worktree>/.autopilot/logs/<epic>/    # 구현 로그 (선택, gitignored)
```

`.autopilot/` 를 **메인 worktree 루트** 에 두고, 자식 worktree 에서는 절대경로로 해석한다.

`.git/` 안에 두지 않는 이유: `.git/` 은 git 의 내부 디렉토리이며 `git gc` / `git fsck` 등 내부 도구가 다루는 공간이다. 커스텀 파일을 두는 것은 관습 위반이며 잠재적 충돌 위험이 있다.

worktree 공유 문제 해결: autopilot 은 worktree 기반 병렬 구현을 쓰므로 자식 worktree 도 메인과 동일 DB 를 봐야 한다. 어느 worktree 에서 실행되든 다음 해석으로 같은 경로를 얻는다:

```bash
common_dir=$(realpath "$(git rev-parse --git-common-dir)")
main_worktree=$(dirname "$common_dir")
state_db="$main_worktree/.autopilot/state.db"
```

`git rev-parse --git-common-dir` 는 어느 worktree 에서 실행해도 메인 `.git/` 을 반환하므로, 그 부모 디렉토리가 메인 worktree 의 루트가 된다. autopilot CLI 가 이 해석을 한 곳(예: `autopilot::paths::state_db()`)에서만 수행하면 모든 호출자가 자동으로 같은 DB 를 보게 된다.

`.autopilot/` 는 `.gitignore` 한 줄로 추적에서 제외한다:

```gitignore
/.autopilot/
```

**제약**: 베어 레포 (working tree 가 없는 형태) 에서는 메인 worktree 가 존재하지 않으므로 autopilot 은 동작하지 않는다. 이는 unsupported 로 명시한다 (autopilot 자체가 작업 디렉토리를 전제로 한 도구이므로 실용적 제약은 없다).

### 3.2 스키마

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

### 3.3 결정적 Task ID

UUID 를 매번 새로 생성하면 spec 재분해 후 매칭이 안 된다. Task ID 는 **spec 내용으로부터 결정적으로 도출**한다.

```
task_id = sha256(epic_name || ":" || section_path || ":" || requirement_slug)[:12]
```

- `section_path`: spec 파일 안 헤딩 경로 (예: `## 인증 / ### 토큰 갱신`)
- `requirement_slug`: 요구사항 항목의 정규화된 슬러그

같은 spec 을 다시 분해하면 같은 task_id 가 나온다. 따라서:

- `git fetch` 후 리모트 브랜치 이름 (`epic/<name>/<task_id>`) 과 1:1 매칭 가능
- 다른 머신에서 `epic-resume` 해도 동일 task 식별 가능

### 3.4 SQLite 선택 근거 (대안 비교)

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

`epic-start` 또는 `epic-resume` 시 다음 절차로 로컬 DB 를 리모트와 동기화한다.

```
1. spec 분해 → 기대 task 목록 (결정적 ID 부여)
2. git fetch origin
3. 리모트 브랜치 스캔: "epic/<name>" 및 "epic/<name>/*"
4. 머지된 PR 스캔: target = "epic/<name>"
5. 매칭하여 status 결정:
   - PR merged                    → done
   - feature 브랜치 + open PR     → wip (검토중)
   - feature 브랜치 + PR 없음    → wip (구현중) 또는 stale 후보
   - 브랜치 없음                  → ready (deps 만족 시) 또는 pending
6. 분해 결과에 없는데 DB 에만 남은 task → orphan 으로 표시 (사람 검토 대상)
7. DB 에 신규 행 insert / 기존 행 update
```

이 절차는 **idempotent** 해야 한다. 동일 epic 에 대해 N번 reconcile 해도 결과가 같아야 하며, 재시도 안전하다.

## 5. 명령 / 에이전트 변화 매트릭스

### 5.1 신규

| 명령 | 역할 |
|------|------|
| `/github-autopilot:epic-start <spec-path>` | epic 브랜치 생성 + spec 분해 + task 적재 + 그 epic 스코프 루프 시작 |
| `/github-autopilot:epic-resume <epic-name>` | 기존 epic 의 상태를 리모트 기준으로 reconcile 후 루프 재개 |
| `/github-autopilot:epic-status [epic-name]` | epic / task 상태 대시보드 (사람용) |
| `/github-autopilot:epic-stop <epic-name>` | 해당 epic 의 루프 종료 (수동) |

### 5.2 변경

| 명령 / 에이전트 | 변경점 |
|----------------|--------|
| `gap-watch` | 발견 시 활성 epic 매칭 → task append. 매칭 실패 시 HITL 이슈 escalate |
| `qa-boost` | 동일 (활성 epic 매칭 → task append, 실패 시 escalate) |
| `ci-watch` | epic 브랜치 / task 브랜치의 CI 실패는 해당 task 의 attempts 증가. main 의 CI 실패는 escalate |
| `build-issues` → `build-tasks` | 입력 소스 변경: `:ready` 라벨 쿼리 → SQLite `WHERE status='ready'` |
| `branch-promoter` | PR target: main → 해당 epic 브랜치 |
| `analyze-issue` | HITL 이슈 분석 후 ready 판정 시: 이슈를 활성 epic 의 task 로 흡수 (이슈 번호를 task 메타에 기록) |
| `merge-prs` | 동작 동일 (`:auto` 라벨 PR 머지). 단 머지 후 task status 업데이트 추가 |

### 5.3 제거 / 폐기

- `:ready` 라벨 — task store 의 status 컬럼이 대체. autopilot 은 더 이상 라벨로 작업을 큐잉하지 않음.
- `:wip` 라벨 — 동일 (status='wip' 가 대체)
- `autopilot issue create` 의 watch-source 호출 경로 — task store insert 로 대체

`:auto`, `:ci-failure` 라벨은 PR / 사람 이슈에서 계속 의미를 가지므로 유지한다.

## 6. HITL Escalation

### 6.1 Escalation 트리거

| 조건 | 동작 |
|------|------|
| watch 가 발견했지만 활성 epic 에 매칭되지 않음 | "어느 epic 에 붙일지 결정해주세요" 이슈 발행 |
| task 가 max_attempts 초과 | "구현 반복 실패, 검토 필요" 이슈 발행 |
| task 의존성 사이클 또는 unresolvable | "의존성 문제 해결 필요" 이슈 발행 |
| 충돌 자동 해결 실패 | 기존 충돌 알림 흐름 유지 |

### 6.2 Escalation 이슈 포맷

```yaml
title: "[autopilot] <짧은 요약>"
labels: ["autopilot:hitl-needed"]   # 새 라벨
body: |
  <!-- autopilot-escalation -->
  <!-- epic: <epic-name or "none"> -->
  <!-- task-id: <id or ""> -->
  <!-- reason: <unmatched|max-attempts|...> -->

  ## 상황
  ...

  ## autopilot 이 시도한 것
  ...

  ## 사람이 결정해야 할 것
  - [ ] ...
```

사람이 이 이슈를 처리하면 (close / 새 epic 시작 / 기존 epic 에 attach) autopilot 은 다음 cycle 에서 status 를 갱신한다.

## 7. Epic 라이프사이클

```
[사람] /epic-start spec/auth.md
  ↓
[autopilot] epic 브랜치 생성 + spec 분해 + task 적재 + 루프 시작
  ↓
[autopilot] task 단위 구현 → PR (target = epic 브랜치) → :auto 라벨 → merge
  ↓ (모든 task done)
[autopilot] notification 설정대로 완료 알림 + 해당 epic 의 루프 종료
  ↓
[사람] epic 브랜치에서 추가 검증 / 수동 보완
  ↓
[사람] epic → main PR 직접 생성 (이 단계는 자동화하지 않음)
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

설계 단계에서 명시적으로 결정을 미룬 사항. Phase 2 (REVIEW) 또는 구현 시 결정한다.

1. **Spec 분해 주체** — 기존 `spec-kit` 플러그인을 재사용할 것인지, github-autopilot 이 자체 분해 에이전트를 둘 것인지.
2. **Stale 브랜치 정책** — feature 브랜치는 있으나 PR 도 없고 일정 시간 변경도 없는 task 를 어떻게 처리할지.
3. **여러 epic 동시 활성** 시 자원 한도 — `max_parallel_agents` 가 epic 간에 어떻게 분배될지.
4. **Schema 마이그레이션** — 향후 스키마 변경 시 사용자 로컬 DB 를 업데이트할 절차 (단순 버전 컬럼 + 마이그레이션 스크립트가 무난).
5. **이벤트 로그 보존 기간** — `events` 테이블 무한 증가 방지 정책 (epic completed 후 N일 후 prune 등).

## 10. 다음 단계

- [ ] 본 문서 리뷰 (Phase 2)
- [ ] 결정 사항 수렴 후 라운드별 구현 계획 (Phase 3 분할)
  - Round 1: SQLite 스키마 + autopilot CLI 의 task store 명령 (DB 단독 동작)
  - Round 2: `epic-start` / 분해 / reconcile
  - Round 3: 기존 watch / build / promote 의 배선 변경
  - Round 4: HITL escalation + epic 완료 알림 + 마이그레이션
