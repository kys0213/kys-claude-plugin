# Autodev Critical Issues 분석 및 수정 계획

## 요구사항 정리

REVIEW-REPORT.md의 Critical 4건을 치명도 순으로 분석·수정한다.

| 우선순위 | ID | 파일 | 문제 | 데이터 영향 |
|---------|-----|------|------|------------|
| 1 | C-04 | `queue/repository.rs:181-195` | upsert 시 phantom UUID 반환 | 존재하지 않는 ID 참조 → 상태 전이 실패 |
| 2 | C-01 | `queue/repository.rs:84-121` | `repo_remove` 6개 DELETE 트랜잭션 없음 | 중간 실패 시 orphan 데이터 |
| 3 | C-03 | `queue/schema.rs:106-146` | 마이그레이션 트랜잭션 없음 | 동시 데몬 시작 시 데이터 손실/중복 인덱스 |
| 4 | C-02 | `infrastructure/git/real.rs:15,39,60` | `.to_str().unwrap()` 3곳 panic | non-UTF-8 경로에서 데몬 크래시 |

> 우선순위 근거: C-04는 정상 운영 중에도 발생 가능(이슈 재스캔), C-01은 repo 삭제 시 발생, C-03은 동시 시작이라는 특수 조건 필요, C-02는 non-UTF-8 경로라는 드문 조건 필요

---

## 사이드이펙트 조사

### C-04: `issue_insert` 반환값 오류

- **영향 범위**: `issue_insert`, `pr_insert`, `merge_insert` 3곳에 동일 패턴
- **현재 호출자 분석**: scanner에서 반환 ID를 사용하지 않으므로 즉시 영향 없음
- **잠재 위험**: 향후 반환 ID를 사용하는 코드 추가 시 silent failure 발생
- **수정 방식**: upsert 후 SELECT로 실제 ID 조회 (쿼리 1회 추가, 성능 영향 무시 가능)
- **기존 테스트**: `issue_duplicate_insert_is_ignored()` — 반환 ID 정확성 미검증

### C-01: `repo_remove` 트랜잭션 누락

- **영향 범위**: `repo_remove()` 1곳
- **사이드이펙트**: `self.conn()`이 `&Connection` 반환 → `unchecked_transaction()` 호출 가능 (rusqlite 표준 패턴)
- **기존 테스트**: 3건 존재하나 부분 실패 시나리오 없음
- **위험**: 트랜잭션 안에서 `?` 사용 시 Transaction Drop이 자동 rollback → 안전

### C-03: Schema migration race condition

- **영향 범위**: `migrate_unique_constraints()` 1곳
- **사이드이펙트**: `BEGIN EXCLUSIVE`는 다른 writer를 블로킹 → 동시 시작 시 한쪽이 대기 후 진행
- **WAL 모드 호환성**: EXCLUSIVE 트랜잭션은 WAL 모드에서도 정상 동작
- **기존 테스트**: `migration_cleans_up_existing_duplicates()` 1건 — 동시성 미검증

### C-02: Path panic

- **영향 범위**: `real.rs`의 `clone`, `worktree_add`, `worktree_remove` 3개 메서드
- **사이드이펙트**: `to_str()` → `ok_or_else()`로 변경 시 에러 반환. 호출자가 이미 `Result` 처리하므로 안전
- **기존 테스트**: `real.rs`에 대한 단위 테스트 없음 (Mock만 존재)

---

## 수정 설계

### Step 1: C-04 — upsert 후 실제 ID 조회

**파일**: `plugins/autodev/cli/src/queue/repository.rs`

`issue_insert`, `pr_insert`, `merge_insert` 3곳 수정:

```rust
fn issue_insert(&self, item: &NewIssueItem) -> Result<String> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    self.conn().execute(
        "INSERT INTO issue_queue (...) VALUES (...) ON CONFLICT(...) DO UPDATE SET ...",
        rusqlite::params![id, ...],
    )?;

    // upsert 후 실제 ID 조회
    let actual_id: String = self.conn().query_row(
        "SELECT id FROM issue_queue WHERE repo_id = ?1 AND github_number = ?2",
        rusqlite::params![item.repo_id, item.github_number],
        |row| row.get(0),
    )?;
    Ok(actual_id)
}
```

동일 패턴을 `pr_insert` (unique key: `repo_id + github_number`), `merge_insert` (unique key: `repo_id + pr_number`)에도 적용.

**테스트 추가**:
- 신규 삽입 → 새 ID 반환 확인
- 중복 삽입 (status=done) → 기존 행의 ID 반환 확인
- 중복 삽입 (status≠done) → INSERT 무시, 기존 ID 반환 확인

### Step 2: C-01 — `repo_remove` 트랜잭션 래핑

**파일**: `plugins/autodev/cli/src/queue/repository.rs`

```rust
fn repo_remove(&self, name: &str) -> Result<()> {
    let conn = self.conn();
    let tx = conn.unchecked_transaction()?;
    let repo_id_query = "(SELECT id FROM repositories WHERE name = ?1)";

    tx.execute(&format!("DELETE FROM issue_queue WHERE repo_id = {repo_id_query}"), rusqlite::params![name])?;
    tx.execute(&format!("DELETE FROM pr_queue WHERE repo_id = {repo_id_query}"), rusqlite::params![name])?;
    tx.execute(&format!("DELETE FROM merge_queue WHERE repo_id = {repo_id_query}"), rusqlite::params![name])?;
    tx.execute(&format!("DELETE FROM scan_cursors WHERE repo_id = {repo_id_query}"), rusqlite::params![name])?;
    tx.execute(&format!("DELETE FROM consumer_logs WHERE repo_id = {repo_id_query}"), rusqlite::params![name])?;
    tx.execute("DELETE FROM repositories WHERE name = ?1", rusqlite::params![name])?;

    tx.commit()?;
    Ok(())
}
```

### Step 3: C-03 — Schema migration EXCLUSIVE 트랜잭션

**파일**: `plugins/autodev/cli/src/queue/schema.rs`

```rust
fn migrate_unique_constraints(conn: &Connection) -> Result<()> {
    conn.execute_batch("BEGIN EXCLUSIVE")?;

    let result = (|| -> Result<()> {
        let tables = [("issue_queue", "github_number"), ...];
        for (table, number_col) in &tables {
            let idx_name = format!("idx_{table}_unique");
            let exists: bool = conn.query_row(...)?;
            if exists { continue; }
            conn.execute(&format!("DELETE FROM {table} ..."), [])?;
            conn.execute(&format!("CREATE UNIQUE INDEX ..."), [])?;
        }
        Ok(())
    })();

    match result {
        Ok(()) => { conn.execute_batch("COMMIT")?; Ok(()) }
        Err(e) => { let _ = conn.execute_batch("ROLLBACK"); Err(e) }
    }
}
```

### Step 4: C-02 — Path panic → Result 반환

**파일**: `plugins/autodev/cli/src/infrastructure/git/real.rs`

3곳의 `.to_str().unwrap()` 변환:

```rust
// Before
dest.to_str().unwrap()

// After
let dest_str = dest.to_str()
    .ok_or_else(|| anyhow::anyhow!("invalid UTF-8 path: {}", dest.display()))?;
```

### Step 5: 테스트 실행 및 검증

```bash
cd plugins/autodev/cli && cargo test
```

### Step 6: 커밋 및 PR

- 커밋: `fix(autodev): resolve critical data integrity issues (C-01~C-04)`
- PR: `docs(autodev): add code review report`
