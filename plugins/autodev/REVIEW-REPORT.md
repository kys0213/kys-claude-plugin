# Autodev Plugin Code Review Report

> **Date**: 2026-02-22
> **Reviewer**: Claude (Automated Code Review)
> **Scope**: `plugins/autodev/cli/src/` 전체 소스 코드 + 테스트
> **Version**: 0.2.3

---

## 1. Executive Summary

autodev 플러그인은 GitHub 이슈 분석, PR 리뷰, 머지를 자동화하는 Rust 기반 데몬입니다.
Clean Architecture와 SOLID 원칙을 잘 따르고 있으며, trait 기반 DI로 테스트 격리가 우수합니다.

그러나 프로덕션 배포 전에 해결해야 할 **Critical 4건**, **High 7건**, **Medium 8건**, **Low 6건**의 개선 사항이 식별되었습니다.

### Rating

| Category | Rating | Notes |
|----------|--------|-------|
| Architecture | **Good** | Clean layers, proper DIP |
| Code Quality | **Good** | Consistent style, idiomatic Rust |
| Test Coverage | **Good** | Comprehensive, edge cases 포함 |
| Error Handling | **Medium** | 일관성 부족, 일부 에러 무시 |
| Security | **Medium** | SQL interpolation, path panic |
| Robustness | **Good** | Recovery 메커니즘 우수 |
| Portability | **Low** | Linux-only PID check |

---

## 2. Architecture Overview

```
CLI (main.rs) → Client / Daemon / TUI
                    ↓
    Pipeline (issue, pr, merge)
        ↓                   ↓
  Components             Scanner
(workspace, notifier,   (issues, pulls)
 reviewer, merger,
 verdict)
        ↓
  Infrastructure (gh, git, claude)
        ↓
  Queue (Database + SQLite)
```

### Strengths

- **Trait 기반 DI**: `Gh`, `Git`, `Claude`, `Env` 모든 외부 의존성이 trait으로 추상화
- **Mock 구현체**: 모든 인프라 trait에 대한 Mock 제공으로 테스트 격리 완벽
- **상태 머신**: `pending → analyzing → ready → processing → done/failed` 명확한 상태 전이
- **멱등성**: `INSERT ON CONFLICT DO UPDATE WHERE status = 'done'`으로 중복 삽입 방지
- **3중 복구**: Stuck reset, Auto-retry, Orphan WIP recovery
- **Config Deep Merge**: Raw `serde_json::Value` 레벨에서 글로벌 + 레포별 YAML 딥머지

---

## 3. Issues by Priority

### 3.1 Critical (프로덕션 배포 전 필수 수정)

#### C-01. `repo_remove`에 트랜잭션 없음

- **File**: `queue/repository.rs:84-121`
- **Impact**: 중간 단계 실패 시 DB 불일치 (orphan queue items)
- **Description**: 6개의 DELETE 문이 트랜잭션 없이 순차 실행. 3번째 DELETE 실패 시 이슈 큐는 삭제되었지만 PR 큐와 머지 큐는 남아있는 상태가 됨.
- **Fix**:
  ```rust
  fn repo_remove(&self, name: &str) -> Result<()> {
      let conn = self.conn();
      let tx = conn.unchecked_transaction()?;
      // ... all deletes using tx ...
      tx.commit()?;
      Ok(())
  }
  ```

#### C-02. `git/real.rs` Path Panic

- **File**: `infrastructure/git/real.rs:15, 39, 60`
- **Impact**: Non-UTF-8 경로에서 프로세스 크래시
- **Description**: `.to_str().unwrap()` 사용으로 non-UTF-8 경로에서 panic 발생.
- **Fix**: `.to_string_lossy()` 사용 또는 `anyhow::bail!` 반환.
  ```rust
  let dest_str = dest.to_str()
      .ok_or_else(|| anyhow::anyhow!("invalid UTF-8 path: {}", dest.display()))?;
  ```

#### C-03. Schema Migration Race Condition

- **File**: `queue/schema.rs:106-146`
- **Impact**: 두 데몬 동시 시작 시 중복 삭제 또는 인덱스 생성 실패
- **Description**: `migrate_unique_constraints`가 트랜잭션 없이 DELETE + CREATE INDEX 실행. 동시 실행 시 데이터 손실 가능.
- **Fix**:
  ```rust
  conn.execute_batch("BEGIN EXCLUSIVE")?;
  // ... migration logic ...
  conn.execute_batch("COMMIT")?;
  ```

#### C-04. `issue_insert` 반환값 오류 (Upsert 시)

- **File**: `queue/repository.rs:181-195`
- **Impact**: ON CONFLICT UPDATE 경로에서 잘못된 ID 반환
- **Description**: `INSERT ON CONFLICT DO UPDATE` 시 새로 생성한 UUID를 반환하지만, 실제로는 기존 행이 업데이트됨. 반환된 ID는 DB에 존재하지 않는 값.
- **Fix**: upsert 후 실제 ID를 조회하거나, `RETURNING id` 사용.
  ```rust
  let actual_id: String = self.conn().query_row(
      "SELECT id FROM issue_queue WHERE repo_id = ?1 AND github_number = ?2",
      rusqlite::params![item.repo_id, item.github_number],
      |row| row.get(0),
  )?;
  Ok(actual_id)
  ```

---

### 3.2 High (V1.0 전 수정 권장)

#### H-01. TUI `handle_skip`이 Repository 레이어 우회

- **File**: `tui/mod.rs:123-151`
- **Impact**: 아키텍처 레이어 위반, SQL 직접 실행
- **Description**: TUI의 skip 핸들러가 `db.conn()`으로 직접 SQL 실행. `format!`으로 테이블명 보간.
- **Fix**: `QueueAdmin` trait에 `queue_skip(id: &str) -> Result<bool>` 메서드 추가.
  ```rust
  // queue/repository.rs
  pub trait QueueAdmin {
      // ... existing methods ...
      fn queue_skip(&self, id: &str) -> Result<bool>;
  }
  ```

#### H-02. SQL String Interpolation 패턴

- **Files**: `queue/repository.rs:663-664`, `tui/mod.rs:136-139`
- **Impact**: 현재는 하드코딩된 값이지만, 이 패턴이 복사되면 SQL injection 가능
- **Description**: `format!("'{s}'")` 형태로 SQL 값을 문자열 보간. 테이블명도 `format!`으로 보간.
- **Fix**: 파라미터화된 쿼리 사용 또는 테이블명은 enum으로 관리.
  ```rust
  enum QueueTable { Issue, Pr, Merge }
  impl QueueTable {
      fn as_str(&self) -> &'static str {
          match self {
              Self::Issue => "issue_queue",
              Self::Pr => "pr_queue",
              Self::Merge => "merge_queue",
          }
      }
  }
  ```

#### H-03. `#![allow(dead_code, unused_imports)]` 블랭킷 억제

- **Files**: `main.rs:1`, `lib.rs:1`
- **Impact**: 미사용 코드 감지 불가
- **Description**: 프로덕션 코드에서 dead_code 경고를 전역 억제하면 불필요한 코드가 축적됨.
- **Fix**: 이 지시문을 제거하고, 필요한 곳에만 `#[allow(dead_code)]` 적용.

#### H-04. PR 리뷰 결과가 GitHub에 미게시

- **File**: `pipeline/pr.rs:108-118`
- **Impact**: 리뷰가 생성되지만 실제로 GitHub에 댓글로 게시되지 않음
- **Description**: `Reviewer`가 `ReviewOutput`을 반환하지만, `process_pending`에서 이를 DB에만 저장하고 GitHub 댓글로 게시하지 않음.
- **Fix**: `notifier.post_issue_comment()`로 리뷰 결과를 GitHub에 게시하는 로직 추가.

#### H-05. `worktree_remove` 실패 무시 (`git/real.rs`)

- **File**: `infrastructure/git/real.rs:58-66`
- **Impact**: 실패한 worktree가 디스크에 잔류, 후속 작업 충돌 가능
- **Description**: `worktree_remove`가 exit code를 확인하지 않음.
- **Fix**:
  ```rust
  let status = tokio::process::Command::new("git")
      .args(["worktree", "remove", "--force", ...])
      .current_dir(base_dir)
      .status()
      .await?;
  if !status.success() {
      anyhow::bail!("git worktree remove failed");
  }
  ```

#### H-06. Verdict를 String으로 관리

- **File**: `infrastructure/claude/output.rs:26-27`, `pipeline/issue.rs:156-239`
- **Impact**: 오타 시 컴파일 에러 없이 잘못된 분기로 진입
- **Description**: `verdict` 필드가 `String` 타입. `"implement"`, `"wontfix"` 등을 문자열로 비교.
- **Fix**: Rust enum으로 변경:
  ```rust
  #[derive(Debug, Clone, Deserialize, PartialEq)]
  #[serde(rename_all = "snake_case")]
  pub enum Verdict {
      Implement,
      NeedsClarification,
      Wontfix,
  }
  ```

#### H-07. DESIGN.md와 구현 불일치 (Startup Reconciliation)

- **File**: `DESIGN.md` (bounded reconciliation), `daemon/mod.rs`
- **Impact**: 설계 문서와 실제 동작이 다름
- **Description**: DESIGN.md에는 "bounded reconciliation" (24시간 윈도우 복구) 메커니즘이 명시되어 있지만, 실제 코드는 incremental scan만 수행.
- **Fix**: DESIGN.md 업데이트 또는 bounded reconciliation 구현.

---

### 3.3 Medium (V2.0 전 수정 권장)

#### M-01. `db.conn()` Public 노출

- **File**: `queue/mod.rs:25-27`
- **Impact**: Repository 패턴 우회, 임의 SQL 실행 가능
- **Description**: `pub fn conn(&self) -> &Connection`이 raw SQLite 커넥션을 노출. TUI, 테스트 등에서 직접 SQL 실행.
- **Fix**: `conn()` 접근 제한 (`pub(crate)`) 또는 필요한 쿼리를 Repository trait에 추가.

#### M-02. Worktree Orphan 누적

- **File**: `pipeline/issue.rs:101-110`, `daemon/mod.rs`
- **Impact**: 디스크 공간 낭비
- **Description**: 데몬 재시작 시 이전 세션의 worktree가 정리되지 않음. `process_ready`는 worktree를 재생성하지만 기존 것을 정리하지 않음.
- **Fix**: 데몬 시작 시 활성 큐에 없는 worktree를 정리하는 로직 추가.

#### M-03. Views 모듈의 SQL 중복

- **File**: `tui/views.rs:113-231`
- **Impact**: 스키마 변경 시 두 곳 수정 필요
- **Description**: `query_active_items()`와 `query_label_counts()`가 `repository.rs`의 SQL 로직을 중복 구현.
- **Fix**: Repository trait에 전용 메서드 추가.

#### M-04. PR Scanner에 `since` 파라미터 미사용

- **File**: `scanner/pulls.rs:43-49`
- **Impact**: 매 스캔마다 모든 open PR을 다시 가져옴 (비효율)
- **Description**: Issues scanner는 `since` 파라미터를 API에 전달하지만, PR scanner는 cursor를 조회만 하고 사용하지 않음. GitHub Pulls API는 `since`를 지원하지 않으므로, 정렬된 결과에서 early-break 추가.
- **Fix**: 이미 처리된 PR 이후 항목은 `break`로 스캔 중단.
  ```rust
  if let Some(ref s) = latest_updated {
      if pr.updated_at <= *s {
          break; // sorted desc, so all remaining are older
      }
  }
  ```

#### M-05. `Workspace::repo_base_path`와 `client::repo_config` 경로 불일치

- **File**: `components/workspace.rs:21`, `client/mod.rs:94`
- **Impact**: `repo config` 명령어가 잘못된 경로를 표시
- **Description**: `workspace.rs`는 `repo_name.replace('/', "-")`으로 경로 생성하지만, `client.rs`는 raw `name`으로 `join`.
- **Fix**: 경로 변환 로직을 공유 함수로 추출.
  ```rust
  // config/mod.rs
  pub fn sanitize_repo_name(name: &str) -> String {
      name.replace('/', "-")
  }
  ```

#### M-06. `LabelCounts.skip`이 항상 0

- **File**: `tui/views.rs:225-231`
- **Impact**: TUI에서 의미 없는 데이터 표시
- **Description**: `skip` 카운트가 하드코딩된 0. `error_message = 'skipped via dashboard'`인 항목을 카운트하거나, 표시를 제거해야 함.
- **Fix**: 스킵된 항목 카운트 구현.
  ```rust
  let skip: i64 = conn.query_row(
      &format!("SELECT COUNT(*) FROM {table} WHERE status = 'done' AND error_message = 'skipped via dashboard'"),
      [], |row| row.get(0),
  ).unwrap_or(0);
  counts.skip += skip;
  ```

#### M-07. `StatusFields` COALESCE로 필드 초기화 불가

- **File**: `queue/repository.rs:252-270`
- **Impact**: 필드를 명시적으로 NULL로 설정할 수 없음
- **Description**: `COALESCE(?3, worker_id)` 패턴은 `None` 전달 시 기존값을 유지. `queue_reset_stuck`은 이 패턴을 우회하여 직접 `worker_id = NULL` 설정.
- **Fix**: `ClearableField<T>` enum 도입 또는 별도의 clear 메서드 추가.

#### M-08. Config에 `deny_unknown_fields` 미적용

- **File**: `config/models.rs`
- **Impact**: YAML 오타가 무시됨 (예: `scan_interval_secsss`)
- **Description**: `#[serde(default)]`로 인해 알 수 없는 필드가 무시됨.
- **Fix**: 경고 레벨의 unknown field 감지 추가. (`deny_unknown_fields`는 deep merge와 충돌할 수 있으므로 로딩 후 검증 방식 권장)

---

### 3.4 Low (개선 권장)

#### L-01. PID Check가 Linux 전용

- **File**: `daemon/pid.rs:19-26`
- **Impact**: macOS, Windows에서 동작하지 않음
- **Description**: `/proc/{pid}` 존재 여부로 프로세스 생존 확인.
- **Fix**: `libc::kill(pid, 0)` 또는 `nix` 크레이트 사용.

#### L-02. `truncate` 함수의 비표준 동작

- **File**: `infrastructure/claude/real.rs:55-65`
- **Impact**: 잘린 로그에 표시 없음
- **Description**: 프롬프트를 80자로 자르지만 "..." 접미사 없음. Non-ASCII 문자에서 예상치 못한 결과.
- **Fix**: 잘림 표시 추가.

#### L-03. `bar()` 함수 스케일링 안됨

- **File**: `tui/views.rs:579-584`
- **Impact**: 큰 카운트에서 모든 바가 동일 길이
- **Description**: `count`를 직접 바 길이로 사용. 최대값 기준 정규화 필요.
- **Fix**: `len = (count * max_width) / total_max` 형태로 정규화.

#### L-04. `client::repo_add` URL 파싱이 취약

- **File**: `client/mod.rs:46-55`
- **Impact**: 비표준 URL에서 잘못된 repo name 추출
- **Description**: `rsplit('/')` + `take(2)` 방식. `.git/` suffix, 중첩 경로 등에서 실패 가능.
- **Fix**: `url` 크레이트 사용.

#### L-05. `merger::resolve_conflicts` 프롬프트가 불충분

- **File**: `components/merger.rs:75-77`
- **Impact**: Claude가 실제 충돌 마커를 보지 못해 해결 실패율 높음
- **Description**: `"Resolve merge conflicts for PR #{}"` 프롬프트만 전달. 충돌 파일 목록이나 마커 내용 미포함.
- **Fix**: 충돌 파일 목록과 diff를 프롬프트에 포함.

#### L-06. `LogTailer::initial_load` 대용량 파일에서 OOM 위험

- **File**: `tui/events.rs:52-66`
- **Impact**: 100MB+ 로그 파일에서 메모리 초과
- **Description**: 전체 파일을 메모리에 읽은 후 마지막 N줄 추출.
- **Fix**: 파일 끝에서부터 역방향 읽기 또는 `BufReader`로 라인 수 제한.

---

## 4. Test Coverage Assessment

### Strengths

| Test File | Coverage Target | Quality |
|-----------|----------------|---------|
| `repository_tests.rs` | DB CRUD, 상태 전이, 커서, 로그 | Excellent |
| `pipeline_e2e_tests.rs` | Issue/PR 성공·실패, batch limit | Good |
| `issue_verdict_tests.rs` | Verdict 분기 (wontfix, clarification, confidence) | Excellent |
| `daemon_scan_tests.rs` | 스캔, dedup, cursor, label 필터 | Good |
| `daemon_recovery_tests.rs` | Orphan WIP 복구 | Good |
| `queue_admin_tests.rs` | Stuck reset, auto-retry, max retries | Excellent |
| `notifier_tests.rs` | GitHub 상태 확인 | Good |
| `config_loader_tests.rs` | YAML 로딩, deep merge, malformed | Good |
| `tui_tests.rs` | Active items, label counts, retry/skip | Good |
| `component_tests.rs` | Component 통합 테스트 | Good |
| `cli_tests.rs` | CLI 통합 테스트 | Good |

### Gaps

- **Infrastructure unit tests**: `real.rs` 구현체에 대한 테스트 없음 (환경 의존적이므로 이해 가능)
- **Concurrent access**: 동시 접근 시나리오 테스트 없음
- **Error path coverage**: `clone_should_fail`, `worktree_should_fail` 등 Mock의 실패 시나리오 테스트 부족
- **Config validation**: 잘못된 값 범위 (예: `confidence_threshold: 2.0`) 테스트 없음

---

## 5. SOLID Principles Assessment

| Principle | Score | Notes |
|-----------|-------|-------|
| **SRP** | 8/10 | `repository.rs` (699줄)과 `views.rs` (585줄)이 과대. 분리 필요 |
| **OCP** | 9/10 | Trait 기반 확장 우수. 새 큐 타입 추가 시 기존 코드 수정 필요 |
| **LSP** | 10/10 | Mock/Real 구현체가 동일한 계약 충족 |
| **ISP** | 9/10 | Trait 인터페이스가 적절히 분리됨. `QueueAdmin`에 skip 메서드 추가 필요 |
| **DIP** | 10/10 | 코어 로직이 추상화에만 의존. 구현체는 외부 주입 |

---

## 6. Improvement Roadmap

```
Phase 1: Critical Fixes (1주)
├── C-01: repo_remove 트랜잭션 추가
├── C-02: git/real.rs path panic 수정
├── C-03: schema migration 트랜잭션 추가
└── C-04: issue_insert 반환값 수정

Phase 2: High Priority (2주)
├── H-01: TUI skip을 QueueAdmin으로 이동
├── H-02: SQL interpolation을 enum 기반으로 전환
├── H-03: dead_code allow 제거
├── H-04: PR 리뷰 결과 GitHub 게시
├── H-05: worktree_remove exit code 확인
├── H-06: Verdict를 enum으로 전환
└── H-07: DESIGN.md 동기화

Phase 3: Medium Priority (2주)
├── M-01: db.conn() 접근 제한
├── M-02: worktree orphan 정리
├── M-03: views SQL 중복 제거
├── M-04: PR scanner early-break
├── M-05: 경로 변환 공유 함수
├── M-06: skip 카운트 구현
├── M-07: StatusFields 초기화 패턴
└── M-08: config 검증

Phase 4: Low Priority (1주)
├── L-01: Portable PID check
├── L-02: truncate 표시 개선
├── L-03: bar() 스케일링
├── L-04: URL 파싱 개선
├── L-05: conflict resolver 프롬프트 개선
└── L-06: LogTailer 대용량 파일 처리
```

---

## 7. Files Index

| File | Lines | Issues |
|------|-------|--------|
| `main.rs` | 170 | H-03 |
| `lib.rs` | 11 | H-03 |
| `active.rs` | 29 | - |
| `infrastructure/gh/mod.rs` | 53 | - |
| `infrastructure/gh/real.rs` | 159 | - |
| `infrastructure/gh/mock.rs` | 112 | - |
| `infrastructure/git/mod.rs` | 28 | - |
| `infrastructure/git/real.rs` | 68 | C-02, H-05 |
| `infrastructure/git/mock.rs` | 95 | - |
| `infrastructure/claude/mod.rs` | 32 | - |
| `infrastructure/claude/real.rs` | 66 | L-02 |
| `infrastructure/claude/mock.rs` | 72 | - |
| `infrastructure/claude/output.rs` | 56 | H-06 |
| `components/workspace.rs` | 81 | M-02, M-05 |
| `components/notifier.rs` | 71 | - |
| `components/reviewer.rs` | 51 | - |
| `components/merger.rs` | 102 | L-05 |
| `components/verdict.rs` | 38 | - |
| `pipeline/mod.rs` | 35 | - |
| `pipeline/issue.rs` | 357 | C-04 |
| `pipeline/pr.rs` | 134 | H-04 |
| `pipeline/merge.rs` | 141 | - |
| `queue/mod.rs` | 29 | M-01 |
| `queue/models.rs` | 213 | - |
| `queue/schema.rs` | 147 | C-03 |
| `queue/repository.rs` | 699 | C-01, C-04, H-02, M-07 |
| `daemon/mod.rs` | 119 | H-07 |
| `daemon/pid.rs` | 31 | L-01 |
| `daemon/recovery.rs` | 72 | - |
| `scanner/mod.rs` | 83 | - |
| `scanner/issues.rs` | 127 | - |
| `scanner/pulls.rs` | 107 | M-04 |
| `config/mod.rs` | 33 | - |
| `config/loader.rs` | 77 | - |
| `config/models.rs` | 149 | M-08 |
| `tui/mod.rs` | 156 | H-01 |
| `tui/views.rs` | 585 | M-03, M-06, L-03 |
| `tui/events.rs` | 245 | L-06 |
| `client/mod.rs` | 206 | M-05, L-04 |
