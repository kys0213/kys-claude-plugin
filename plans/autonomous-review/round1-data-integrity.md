# Round 1: 데이터 정합성

> **Type**: `fix(autonomous)`
> **Priority**: P0
> **Depends on**: 없음 (기반 작업)

## 요약

DB 스키마와 큐 삽입 로직에서 데이터 정합성 문제를 수정합니다. UNIQUE 제약 조건 부재, exists 체크의 TOCTOU 레이스, done/failed 아이템 재큐잉 불가 문제, status 업데이트 시 필드 손실 문제를 해결합니다.

---

## 발견된 이슈

### #1 UNIQUE 제약 조건 부재 (P0)

**파일**: `queue/schema.rs`

**현재 문제**:

`issue_queue`, `pr_queue` 테이블에 `(repo_id, github_number)` 조합의 UNIQUE 제약이 없습니다:

```sql
-- schema.rs:24-41
CREATE TABLE IF NOT EXISTS issue_queue (
    id              TEXT PRIMARY KEY,
    repo_id         TEXT NOT NULL REFERENCES repositories(id),
    github_number   INTEGER NOT NULL,
    -- ... UNIQUE(repo_id, github_number) 없음
);
```

코드에서 `issue_exists()` / `pr_exists()` 로 삽입 전 체크하지만, 이는 TOCTOU (Time-of-Check-Time-of-Use) 레이스 조건에 취약합니다. Round 2에서 병렬화가 도입되면 실제 중복 삽입 가능성이 높아집니다.

**변경 방향**:

스키마에 UNIQUE 인덱스 추가:
```sql
CREATE UNIQUE INDEX IF NOT EXISTS idx_issue_queue_unique
    ON issue_queue(repo_id, github_number)
    WHERE status NOT IN ('done', 'failed');

CREATE UNIQUE INDEX IF NOT EXISTS idx_pr_queue_unique
    ON pr_queue(repo_id, github_number)
    WHERE status NOT IN ('done', 'failed');
```

> **partial unique index** 를 사용하는 이유: done/failed 상태의 아이템이 있어도 같은 이슈/PR을 다시 큐에 추가할 수 있어야 합니다 (재스캔 시 업데이트된 이슈를 다시 처리).

**사이드이펙트**:
- 기존 DB 파일에 이미 중복 데이터가 있으면 인덱스 생성 실패 → 마이그레이션 로직 필요
- `issue_insert` / `pr_insert`가 중복 시 SQLite UNIQUE violation 에러 반환 → 호출부에서 처리 필요

---

### #2 exists 체크가 done/failed도 포함하여 재큐잉 불가 (P0)

**파일**: `queue/repository.rs:191-198`, `queue/repository.rs:289-296`

**현재 문제**:

```rust
// repository.rs:192-197
fn issue_exists(&self, repo_id: &str, github_number: i64) -> Result<bool> {
    let exists: bool = self.conn().query_row(
        "SELECT COUNT(*) > 0 FROM issue_queue WHERE repo_id = ?1 AND github_number = ?2",
        // ← status 조건 없음: done/failed도 매칭
```

이슈가 한번 처리되면 (done/failed), 같은 이슈가 GitHub에서 업데이트되어도 재스캔 시 `exists=true`로 건너뜁니다.

**변경 방향**:

```rust
fn issue_exists(&self, repo_id: &str, github_number: i64) -> Result<bool> {
    let exists: bool = self.conn().query_row(
        "SELECT COUNT(*) > 0 FROM issue_queue WHERE repo_id = ?1 AND github_number = ?2 AND status NOT IN ('done', 'failed')",
        rusqlite::params![repo_id, github_number],
        |row| row.get(0),
    )?;
    Ok(exists)
}
```

PR도 동일하게 수정.

**사이드이펙트**:
- 이미 처리 완료(done)된 이슈가 업데이트되면 다시 큐에 들어감 → 의도적 동작 (업데이트된 이슈는 재분석 필요)
- queue_clear로 정리하지 않으면 같은 이슈의 히스토리가 여러 행으로 남음 → 허용 가능 (로그 성격)

---

### #3 StatusFields 업데이트 시 필드 손실 (P1)

**파일**: `queue/repository.rs:222-243`

**현재 문제**:

```rust
fn issue_update_status(&self, id: &str, status: &str, fields: &StatusFields) -> Result<()> {
    if let Some(ref worker_id) = fields.worker_id {
        // worker_id만 SET
    } else if let Some(ref report) = fields.analysis_report {
        // analysis_report만 SET
    } else {
        // 아무 필드도 SET하지 않음
    }
}
```

if/else if 구조로 인해 `worker_id`와 `analysis_report`를 동시에 업데이트 불가. 또한 worker_id를 설정할 때 analysis_report는 무시됩니다.

**변경 방향**:

동적 SQL 빌더 패턴으로 변경:

```rust
fn issue_update_status(&self, id: &str, status: &str, fields: &StatusFields) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    let conn = self.conn();

    let mut sets = vec!["status = ?2", "updated_at = ?3"];
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = vec![
        Box::new(id.to_string()),
        Box::new(status.to_string()),
        Box::new(now),
    ];
    let mut idx = 4;

    if let Some(ref worker_id) = fields.worker_id {
        sets.push(&format_placeholder("worker_id", idx));
        params.push(Box::new(worker_id.clone()));
        idx += 1;
    }
    if let Some(ref report) = fields.analysis_report {
        sets.push(&format_placeholder("analysis_report", idx));
        params.push(Box::new(report.clone()));
        idx += 1;
    }

    let sql = format!(
        "UPDATE issue_queue SET {} WHERE id = ?1",
        sets.join(", ")
    );
    // ...
}
```

**대안 (더 단순한 접근)**: 모든 optional 필드를 항상 SET하되 COALESCE 사용:

```rust
fn issue_update_status(&self, id: &str, status: &str, fields: &StatusFields) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    self.conn().execute(
        "UPDATE issue_queue SET \
         status = ?2, \
         worker_id = COALESCE(?3, worker_id), \
         analysis_report = COALESCE(?4, analysis_report), \
         updated_at = ?5 \
         WHERE id = ?1",
        rusqlite::params![id, status, fields.worker_id, fields.analysis_report, now],
    )?;
    Ok(())
}
```

**권장: COALESCE 접근** — 코드가 가장 단순하고 모든 필드를 안전하게 보존.

PR, merge에도 동일 패턴 적용.

---

### #4 queue_reset_stuck에 시간 기반 보호 없음 (P1)

**파일**: `queue/repository.rs:579-604`

**현재 문제**:

```rust
fn queue_reset_stuck(&self) -> Result<u64> {
    // 중간 상태 (analyzing, processing, reviewing 등) 인 모든 아이템을 pending으로 리셋
    // → 시간 기반 필터 없음
}
```

데몬 시작 시 호출되는데, 실제로 지금 막 처리 시작한 아이템도 리셋될 수 있습니다. 단일 데몬이라 현재는 문제없지만, 데몬 재시작이 빠르게 반복되는 경우 위험합니다.

**변경 방향**:

`updated_at` 기준으로 일정 시간(예: 30분) 이상 중간 상태인 아이템만 리셋:

```rust
fn queue_reset_stuck(&self) -> Result<u64> {
    let now = Utc::now().to_rfc3339();
    let threshold = (Utc::now() - chrono::Duration::minutes(30)).to_rfc3339();
    let conn = self.conn();
    let mut total = 0u64;

    let stuck_states = [
        ("issue_queue", &["analyzing", "processing", "ready"] as &[&str]),
        ("pr_queue", &["reviewing"]),
        ("merge_queue", &["merging", "conflict"]),
    ];

    for (table, states) in &stuck_states {
        let placeholders: Vec<String> = states.iter().map(|s| format!("'{s}'")).collect();
        let in_clause = placeholders.join(",");
        let affected = conn.execute(
            &format!(
                "UPDATE {table} SET status = 'pending', worker_id = NULL, error_message = NULL, updated_at = ?1 \
                 WHERE status IN ({in_clause}) AND updated_at < ?2"
            ),
            rusqlite::params![now, threshold],
        )?;
        total += affected as u64;
    }

    Ok(total)
}
```

---

### #5 merge_queue에 중복 방지 메커니즘 없음 (P1)

**파일**: `queue/repository.rs:376-386`, `queue/schema.rs:61-75`

**현재 문제**:

- `merge_queue`에는 `merge_exists(repo_id, pr_number)` 같은 체크 함수가 없음
- 스키마에도 `(repo_id, pr_number)` UNIQUE 제약이 없음
- 현재 merge consumer를 trigger하는 로직이 미구현이라 당장 문제 없지만, 구현 시 중복 방지 필요

**변경 방향**:

1. 스키마에 partial unique index 추가:
```sql
CREATE UNIQUE INDEX IF NOT EXISTS idx_merge_queue_unique
    ON merge_queue(repo_id, pr_number)
    WHERE status NOT IN ('done', 'failed');
```

2. `MergeQueueRepository`에 `merge_exists` 추가:
```rust
fn merge_exists(&self, repo_id: &str, pr_number: i64) -> Result<bool>;
```

---

## 구현 순서

1. `schema.rs` — partial unique index 3개 추가
2. `repository.rs` — `issue_exists`, `pr_exists` 에 status 필터 추가
3. `repository.rs` — `merge_exists` 메서드 추가
4. `repository.rs` — `issue_update_status`, `pr_update_status`, `merge_update_status` COALESCE 패턴 적용
5. `repository.rs` — `queue_reset_stuck` 시간 기반 필터 추가
6. 테스트 추가/수정

## 테스트 계획

- [ ] 기존 42개 테스트 전체 통과
- [ ] `issue_exists` — done 상태 아이템이 있어도 새 삽입 가능 테스트
- [ ] `pr_exists` — 동일
- [ ] `merge_exists` — 기본 동작 테스트
- [ ] `issue_update_status` — worker_id + analysis_report 동시 설정 테스트
- [ ] `queue_reset_stuck` — 최근 아이템(30분 미만)은 리셋 안 됨 테스트
- [ ] UNIQUE 제약 위반 시 에러 반환 확인 테스트

## 영향 범위

| 파일 | 변경 유형 |
|------|----------|
| `queue/schema.rs` | UNIQUE index 추가 |
| `queue/repository.rs` | exists 로직 수정, COALESCE 패턴, merge_exists 추가, stuck 리셋 보강 |
| `tests/repository_tests.rs` | 재큐잉, 동시 필드 업데이트 테스트 추가 |
